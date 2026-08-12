use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::Builder;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::{Error, Result, download_cache_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    Zip,
    TarXz,
}

impl ArtifactFormat {
    fn receipt_name(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactSourceKind {
    Official,
    Mirror,
}

impl ArtifactSourceKind {
    fn receipt_name(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Mirror => "mirror",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSource {
    pub id: String,
    pub url: String,
    pub kind: ArtifactSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSpec {
    pub canonical_url: String,
    pub sources: Vec<ArtifactSource>,
    pub sha256: String,
    pub format: ArtifactFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub pinset_home: PathBuf,
    pub tool: String,
    pub version: String,
    pub target: String,
    pub artifact: ArtifactSpec,
    pub strip_components: usize,
    pub required_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub install_dir: PathBuf,
    pub bytes_downloaded: u64,
    pub sha256: String,
    pub source_id: String,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallLimits {
    pub max_download_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_archive_entries: usize,
    pub request_timeout: Duration,
}

impl Default for InstallLimits {
    fn default() -> Self {
        Self {
            max_download_bytes: 1_073_741_824,
            max_unpacked_bytes: 4_294_967_296,
            max_archive_entries: 100_000,
            request_timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug)]
pub struct Installer {
    client: Client,
    limits: InstallLimits,
}

#[derive(Debug)]
struct SelectedArtifact {
    source_id: String,
    source_kind: String,
    source_url: String,
    path: PathBuf,
    bytes_downloaded: u64,
    actual_hash: String,
}

impl Installer {
    pub fn new(limits: InstallLimits) -> Result<Self> {
        let client = Client::builder()
            .timeout(limits.request_timeout)
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        Ok(Self { client, limits })
    }

    pub fn install(&self, request: &InstallRequest) -> Result<InstallOutcome> {
        validate_request(request)?;
        let expected_hash = parse_sha256(&request.artifact.sha256)?;
        let final_dir = request
            .pinset_home
            .join("installs")
            .join(&request.tool)
            .join(&request.version)
            .join(&request.target);
        if final_dir.exists() {
            return existing_install_outcome(&final_dir, request)
                .ok_or(Error::InstallAlreadyExists { path: final_dir });
        }

        let temporary_root = request.pinset_home.join("tmp");
        fs::create_dir_all(&temporary_root).map_err(|source| Error::CreateInstallStaging {
            path: temporary_root.clone(),
            source,
        })?;
        let transaction = Builder::new()
            .prefix("install-")
            .tempdir_in(&temporary_root)
            .map_err(|source| Error::CreateInstallStaging {
                path: temporary_root,
                source,
            })?;
        let staging_dir = transaction.path().join("payload");
        fs::create_dir(&staging_dir).map_err(|source| Error::CreateInstallStaging {
            path: staging_dir.clone(),
            source,
        })?;

        let expected_hex = hex::encode(expected_hash);
        let cache_path = download_cache_path(&request.pinset_home, &expected_hex)?;
        let selected = if self.cached_artifact_is_valid(&cache_path, &expected_hash)? {
            SelectedArtifact {
                source_id: "cache".to_owned(),
                source_kind: "cache".to_owned(),
                source_url: format!("cache:sha256:{expected_hex}"),
                path: cache_path,
                bytes_downloaded: 0,
                actual_hash: expected_hex,
            }
        } else {
            let mut attempted = Vec::with_capacity(request.artifact.sources.len());
            let mut last_retryable_error = None;
            let mut selected = None;
            for (index, source) in request.artifact.sources.iter().enumerate() {
                attempted.push(source.id.clone());
                let download_path = transaction
                    .path()
                    .join(format!("artifact-{index}.download"));
                match self.download_verified(&source.url, &expected_hash, &download_path) {
                    Ok((bytes_downloaded, actual_hash)) => {
                        self.persist_cache_artifact(&download_path, &cache_path, &expected_hash)?;
                        selected = Some(SelectedArtifact {
                            source_id: source.id.clone(),
                            source_kind: source.kind.receipt_name().to_owned(),
                            source_url: redacted_url(&source.url),
                            path: cache_path.clone(),
                            bytes_downloaded,
                            actual_hash,
                        });
                        break;
                    }
                    Err(error) if is_retryable_source_error(&error) => {
                        last_retryable_error = Some(error);
                    }
                    Err(error) => return Err(error),
                }
            }
            selected.ok_or_else(|| Error::ArtifactSourcesExhausted {
                attempted: attempted.join(", "),
                last_error: last_retryable_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "no source was attempted".to_owned()),
            })?
        };
        match request.artifact.format {
            ArtifactFormat::Zip => {
                self.extract_zip(&selected.path, &staging_dir, request.strip_components)?
            }
            ArtifactFormat::TarXz => {
                self.extract_tar_xz(&selected.path, &staging_dir, request.strip_components)?
            }
        }
        validate_required_paths(&staging_dir, &request.required_paths)?;
        write_receipt(&staging_dir, request, &selected)?;

        let final_parent = final_dir
            .parent()
            .expect("install directory always has a parent");
        fs::create_dir_all(final_parent).map_err(|source| Error::CreateInstallParent {
            path: final_parent.to_path_buf(),
            source,
        })?;
        if final_dir.exists() {
            return Err(Error::InstallAlreadyExists { path: final_dir });
        }
        fs::rename(&staging_dir, &final_dir).map_err(|source| Error::CommitInstall {
            staging: staging_dir,
            destination: final_dir.clone(),
            source,
        })?;

        Ok(InstallOutcome {
            install_dir: final_dir,
            bytes_downloaded: selected.bytes_downloaded,
            sha256: selected.actual_hash,
            source_id: selected.source_id,
            reused_existing: false,
        })
    }

    fn cached_artifact_is_valid(&self, path: &Path, expected_hash: &[u8; 32]) -> Result<bool> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(source) => {
                return Err(Error::ReadDownloadCache {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(Error::UnsafeDownloadCacheEntry {
                path: path.to_path_buf(),
            });
        }
        if metadata.len() > self.limits.max_download_bytes {
            fs::remove_file(path).map_err(|source| Error::RemoveDownloadCacheEntry {
                path: path.to_path_buf(),
                source,
            })?;
            return Ok(false);
        }
        let mut file = File::open(path).map_err(|source| Error::ReadDownloadCache {
            path: path.to_path_buf(),
            source,
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|source| Error::ReadDownloadCache {
                    path: path.to_path_buf(),
                    source,
                })?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let actual: [u8; 32] = hasher.finalize().into();
        if actual == *expected_hash {
            return Ok(true);
        }
        fs::remove_file(path).map_err(|source| Error::RemoveDownloadCacheEntry {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(false)
    }

    fn persist_cache_artifact(
        &self,
        source: &Path,
        destination: &Path,
        expected_hash: &[u8; 32],
    ) -> Result<()> {
        let parent = destination
            .parent()
            .expect("cache path always has a parent");
        fs::create_dir_all(parent).map_err(|source| Error::WriteDownload {
            path: parent.to_path_buf(),
            source,
        })?;
        let mut input = File::open(source).map_err(|source| Error::WriteDownload {
            path: destination.to_path_buf(),
            source,
        })?;
        let mut temporary = Builder::new()
            .prefix(".download-cache-")
            .tempfile_in(parent)
            .map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
        io::copy(&mut input, temporary.as_file_mut())
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
        let mut candidate = temporary;
        for _ in 0..3 {
            match candidate.persist_noclobber(destination) {
                Ok(_) => return Ok(()),
                Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                    candidate = error.file;
                    if self.cached_artifact_is_valid(destination, expected_hash)? {
                        return Ok(());
                    }
                }
                Err(error) => {
                    return Err(Error::WriteDownload {
                        path: destination.to_path_buf(),
                        source: error.error,
                    });
                }
            }
        }
        Err(Error::WriteDownload {
            path: destination.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "download cache entry changed repeatedly during atomic commit",
            ),
        })
    }

    fn download_verified(
        &self,
        url: &str,
        expected_hash: &[u8; 32],
        destination: &Path,
    ) -> Result<(u64, String)> {
        let display_url = redacted_url(url);
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::DownloadRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > self.limits.max_download_bytes)
        {
            return Err(Error::DownloadTooLarge {
                url: display_url.clone(),
                limit: self.limits.max_download_bytes,
            });
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|source| Error::DownloadRead {
                    url: display_url.clone(),
                    source,
                })?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .filter(|value| *value <= self.limits.max_download_bytes)
                .ok_or_else(|| Error::DownloadTooLarge {
                    url: display_url.clone(),
                    limit: self.limits.max_download_bytes,
                })?;
            file.write_all(&buffer[..count])
                .map_err(|source| Error::WriteDownload {
                    path: destination.to_path_buf(),
                    source,
                })?;
            hasher.update(&buffer[..count]);
        }
        file.sync_all().map_err(|source| Error::WriteDownload {
            path: destination.to_path_buf(),
            source,
        })?;

        let actual: [u8; 32] = hasher.finalize().into();
        let actual_hex = hex::encode(actual);
        if actual != *expected_hash {
            return Err(Error::ChecksumMismatch {
                expected: hex::encode(expected_hash),
                actual: actual_hex,
            });
        }

        Ok((total, actual_hex))
    }

    fn extract_zip(
        &self,
        archive_path: &Path,
        destination: &Path,
        strip_components: usize,
    ) -> Result<()> {
        let file = File::open(archive_path).map_err(|source| Error::ExtractArchiveEntry {
            entry: "<archive>".to_owned(),
            path: archive_path.to_path_buf(),
            source,
        })?;
        let mut archive = ZipArchive::new(file).map_err(|source| Error::OpenZip {
            path: archive_path.to_path_buf(),
            source,
        })?;
        if archive.len() > self.limits.max_archive_entries {
            return Err(Error::TooManyArchiveEntries {
                actual: archive.len(),
                limit: self.limits.max_archive_entries,
            });
        }

        let mut seen = HashSet::with_capacity(archive.len());
        let mut total_unpacked = 0_u64;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|source| Error::ReadZipEntry { index, source })?;
            let entry_name = entry.name().to_owned();
            let archived_path = entry
                .enclosed_name()
                .ok_or_else(|| Error::UnsafeArchiveEntry {
                    entry: entry_name.clone(),
                })?;
            if !is_safe_relative(&archived_path) || is_special_zip_entry(entry.unix_mode()) {
                return Err(Error::UnsafeArchiveEntry { entry: entry_name });
            }
            let Some(relative) = strip_entry_path(
                &archived_path,
                strip_components,
                entry.is_dir(),
                &entry_name,
            )?
            else {
                continue;
            };
            let collision_key = archive_collision_key(&relative);
            if !seen.insert(collision_key) {
                return Err(Error::DuplicateArchiveEntry { entry: entry_name });
            }

            let output_path = destination.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&output_path).map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source,
                })?;
                continue;
            }

            let remaining = self
                .limits
                .max_unpacked_bytes
                .checked_sub(total_unpacked)
                .ok_or(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                })?;
            let claimed_size = entry.size();
            if claimed_size > remaining {
                return Err(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                });
            }
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)
                .map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                })?;
            let mut limited_entry = (&mut entry).take(remaining.saturating_add(1));
            let copied = io::copy(&mut limited_entry, &mut output).map_err(|source| {
                Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                }
            })?;
            if copied > remaining {
                return Err(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                });
            }
            if copied != claimed_size {
                return Err(Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source: io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "ZIP entry size did not match extracted bytes",
                    ),
                });
            }
            total_unpacked += copied;
            output
                .sync_all()
                .map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                })?;
            set_executable_permissions(&output_path, entry.unix_mode()).map_err(|source| {
                Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source,
                }
            })?;
        }

        Ok(())
    }

    fn extract_tar_xz(
        &self,
        archive_path: &Path,
        destination: &Path,
        strip_components: usize,
    ) -> Result<()> {
        let file = File::open(archive_path).map_err(|source| Error::ExtractArchiveEntry {
            entry: "<archive>".to_owned(),
            path: archive_path.to_path_buf(),
            source,
        })?;
        let decoder = XzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let entries = archive.entries().map_err(|source| Error::ReadTarArchive {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let mut seen = HashSet::new();
        let mut pending_symlinks = Vec::new();
        let mut total_unpacked = 0_u64;
        let mut entry_count = 0_usize;

        for entry in entries {
            entry_count += 1;
            if entry_count > self.limits.max_archive_entries {
                return Err(Error::TooManyArchiveEntries {
                    actual: entry_count,
                    limit: self.limits.max_archive_entries,
                });
            }
            let mut entry = entry.map_err(|source| Error::ReadTarArchive {
                path: archive_path.to_path_buf(),
                source,
            })?;
            let archived_path = entry.path().map_err(|source| Error::ReadTarArchive {
                path: archive_path.to_path_buf(),
                source,
            })?;
            let entry_name = archived_path.to_string_lossy().into_owned();
            let entry_type = entry.header().entry_type();
            let is_directory = entry_type.is_dir();
            let is_symlink = entry_type.is_symlink();
            if (!entry_type.is_file() && !is_directory && !is_symlink)
                || !is_safe_relative(&archived_path)
            {
                return Err(Error::UnsafeArchiveEntry { entry: entry_name });
            }
            let Some(relative) =
                strip_entry_path(&archived_path, strip_components, is_directory, &entry_name)?
            else {
                continue;
            };
            if !seen.insert(archive_collision_key(&relative)) {
                return Err(Error::DuplicateArchiveEntry { entry: entry_name });
            }
            let output_path = destination.join(&relative);
            if is_directory {
                fs::create_dir_all(&output_path).map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source,
                })?;
                continue;
            }
            if is_symlink {
                let link_target = entry
                    .link_name()
                    .map_err(|source| Error::ReadTarArchive {
                        path: archive_path.to_path_buf(),
                        source,
                    })?
                    .ok_or_else(|| Error::UnsafeArchiveEntry {
                        entry: entry_name.clone(),
                    })?
                    .into_owned();
                let resolved_target =
                    resolve_archive_symlink(&relative, &link_target).ok_or_else(|| {
                        Error::UnsafeArchiveEntry {
                            entry: entry_name.clone(),
                        }
                    })?;
                pending_symlinks.push((entry_name, output_path, link_target, resolved_target));
                continue;
            }

            let remaining = self
                .limits
                .max_unpacked_bytes
                .checked_sub(total_unpacked)
                .ok_or(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                })?;
            let claimed_size = entry
                .header()
                .size()
                .map_err(|source| Error::ReadTarArchive {
                    path: archive_path.to_path_buf(),
                    source,
                })?;
            if claimed_size > remaining {
                return Err(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                });
            }
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)
                .map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                })?;
            let mut limited_entry = (&mut entry).take(remaining.saturating_add(1));
            let copied = io::copy(&mut limited_entry, &mut output).map_err(|source| {
                Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                }
            })?;
            if copied > remaining {
                return Err(Error::ArchiveTooLarge {
                    limit: self.limits.max_unpacked_bytes,
                });
            }
            if copied != claimed_size {
                return Err(Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source: io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "TAR entry size did not match extracted bytes",
                    ),
                });
            }
            total_unpacked += copied;
            output
                .sync_all()
                .map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: output_path.clone(),
                    source,
                })?;
            let mode = entry
                .header()
                .mode()
                .map_err(|source| Error::ReadTarArchive {
                    path: archive_path.to_path_buf(),
                    source,
                })?;
            set_executable_permissions(&output_path, Some(mode)).map_err(|source| {
                Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source,
                }
            })?;
        }
        for (entry_name, output_path, link_target, resolved_target) in pending_symlinks {
            if !destination.join(resolved_target).is_file() {
                return Err(Error::UnsafeArchiveEntry { entry: entry_name });
            }
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|source| Error::ExtractArchiveEntry {
                    entry: entry_name.clone(),
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            create_archive_symlink(&link_target, &output_path).map_err(|source| {
                Error::ExtractArchiveEntry {
                    entry: entry_name,
                    path: output_path,
                    source,
                }
            })?;
        }
        Ok(())
    }
}

fn existing_install_outcome(final_dir: &Path, request: &InstallRequest) -> Option<InstallOutcome> {
    let content = fs::read_to_string(final_dir.join(".pinset-install.toml")).ok()?;
    let receipt: ExistingInstallReceipt = toml::from_str(&content).ok()?;
    if !receipt.complete
        || receipt.tool != request.tool
        || receipt.version != request.version
        || receipt.target != request.target
        || receipt.artifact_sha256 != request.artifact.sha256.to_ascii_lowercase()
    {
        return None;
    }
    if validate_required_paths(final_dir, &request.required_paths).is_err() {
        return None;
    }
    Some(InstallOutcome {
        install_dir: final_dir.to_path_buf(),
        bytes_downloaded: 0,
        sha256: receipt.artifact_sha256,
        source_id: receipt.selected_source,
        reused_existing: true,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn parse_sha256(value: &str) -> Result<[u8; 32]> {
    let decoded = hex::decode(value).map_err(|_| Error::InvalidSha256 {
        value: value.to_owned(),
    })?;
    decoded.try_into().map_err(|_| Error::InvalidSha256 {
        value: value.to_owned(),
    })
}

fn validate_request(request: &InstallRequest) -> Result<()> {
    validate_segment("tool", &request.tool)?;
    validate_segment("version", &request.version)?;
    validate_segment("target", &request.target)?;
    if request.strip_components > 8 {
        return Err(Error::InvalidStripComponents {
            value: request.strip_components,
        });
    }
    if request.required_paths.is_empty() {
        return Err(Error::RequiredPathsEmpty);
    }
    if request.artifact.sources.is_empty() {
        return Err(Error::ArtifactSourcesEmpty);
    }
    let mut source_ids = HashSet::with_capacity(request.artifact.sources.len());
    for source in &request.artifact.sources {
        if source.id.is_empty()
            || !source
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(Error::InvalidArtifactSourceId {
                value: source.id.clone(),
            });
        }
        if !source_ids.insert(&source.id) {
            return Err(Error::DuplicateArtifactSourceId {
                value: source.id.clone(),
            });
        }
    }
    for path in &request.required_paths {
        if !is_safe_relative(path) {
            return Err(Error::InvalidRequiredPath { path: path.clone() });
        }
    }
    Ok(())
}

fn strip_entry_path(
    path: &Path,
    strip_components: usize,
    is_directory: bool,
    entry_name: &str,
) -> Result<Option<PathBuf>> {
    let components = path.components().collect::<Vec<_>>();
    if components.len() < strip_components
        || (components.len() == strip_components && !is_directory)
    {
        return Err(Error::UnsafeArchiveEntry {
            entry: entry_name.to_owned(),
        });
    }
    if components.len() == strip_components {
        return Ok(None);
    }
    Ok(Some(
        components
            .into_iter()
            .skip(strip_components)
            .collect::<PathBuf>(),
    ))
}

fn resolve_archive_symlink(link_path: &Path, target: &Path) -> Option<PathBuf> {
    if target.as_os_str().is_empty()
        || target.is_absolute()
        || target.to_string_lossy().contains('\\')
    {
        return None;
    }
    let mut resolved = link_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for component in target.components() {
        match component {
            Component::Normal(value) => resolved.push(value.to_owned()),
            Component::ParentDir => {
                resolved.pop()?;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if resolved.is_empty() {
        return None;
    }
    Some(resolved.into_iter().collect())
}

#[cfg(unix)]
fn create_archive_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn create_archive_symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "archive symbolic links are only supported on Unix targets",
    ))
}

fn is_retryable_source_error(error: &Error) -> bool {
    matches!(
        error,
        Error::DownloadRequest { .. } | Error::DownloadRead { .. }
    )
}

fn validate_segment(field: &'static str, value: &str) -> Result<()> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\', ':']) {
        return Err(Error::InvalidInstallSegment {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(component, Component::Normal(_))
                && component.as_os_str() != "."
                && component.as_os_str() != ".."
        })
}

fn is_special_zip_entry(mode: Option<u32>) -> bool {
    let Some(mode) = mode else {
        return false;
    };
    let file_type = mode & 0o170000;
    file_type != 0 && file_type != 0o100000 && file_type != 0o040000
}

fn archive_collision_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path, _mode: Option<u32>) -> io::Result<()> {
    Ok(())
}

fn redacted_url(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return "<invalid-url>".to_owned();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

fn validate_required_paths(staging: &Path, required_paths: &[PathBuf]) -> Result<()> {
    for relative in required_paths {
        let path = staging.join(relative);
        if !path.is_file() {
            return Err(Error::RequiredPathMissing { path });
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct InstallReceipt<'a> {
    schema: u32,
    complete: bool,
    tool: &'a str,
    version: &'a str,
    target: &'a str,
    canonical_url: &'a str,
    selected_source: &'a str,
    selected_source_kind: &'a str,
    selected_url: &'a str,
    artifact_sha256: &'a str,
    artifact_format: &'a str,
    bytes_downloaded: u64,
}

#[derive(Deserialize)]
struct ExistingInstallReceipt {
    complete: bool,
    tool: String,
    version: String,
    target: String,
    selected_source: String,
    artifact_sha256: String,
}

fn write_receipt(
    staging: &Path,
    request: &InstallRequest,
    selected: &SelectedArtifact,
) -> Result<()> {
    let canonical_url = redacted_url(&request.artifact.canonical_url);
    let receipt = InstallReceipt {
        schema: 1,
        complete: true,
        tool: &request.tool,
        version: &request.version,
        target: &request.target,
        canonical_url: &canonical_url,
        selected_source: &selected.source_id,
        selected_source_kind: &selected.source_kind,
        selected_url: &selected.source_url,
        artifact_sha256: &selected.actual_hash,
        artifact_format: request.artifact.format.receipt_name(),
        bytes_downloaded: selected.bytes_downloaded,
    };
    let serialized =
        toml::to_string(&receipt).map_err(|source| Error::SerializeInstallReceipt { source })?;
    let path = staging.join(".pinset-install.toml");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| Error::WriteInstallReceipt {
            path: path.clone(),
            source,
        })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| Error::WriteInstallReceipt { path, source })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read as _, Write as _},
        net::TcpListener,
        sync::{Arc, Barrier},
        thread,
    };

    use tempfile::tempdir;
    use xz2::write::XzEncoder;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn installs_verified_zip_atomically_from_local_http() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (base_url, server) = serve_once(archive.clone(), archive.len());
        let url = format!("{base_url}?token=must-not-be-recorded");
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, sha256_hex(&archive));

        let outcome = test_installer().install(&request).expect("install");
        server.join().expect("server");

        assert_eq!(
            fs::read(outcome.install_dir.join("bin/node.exe")).expect("runtime"),
            b"fake node"
        );
        let receipt = fs::read_to_string(outcome.install_dir.join(".pinset-install.toml"))
            .expect("install receipt");
        assert!(receipt.contains("artifact.zip"));
        assert!(receipt.contains("https://nodejs.org/dist/"));
        assert!(receipt.contains("selected_source = \"local-mirror\""));
        assert!(!receipt.contains("must-not-be-recorded"));
        assert_eq!(outcome.bytes_downloaded, archive.len() as u64);
        assert_eq!(outcome.source_id, "local-mirror");
        let repeated = test_installer()
            .install(&request)
            .expect("identical install is idempotent");
        assert_eq!(repeated.install_dir, outcome.install_dir);
        assert_eq!(repeated.bytes_downloaded, 0);
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn reuses_verified_content_addressed_cache_without_network() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let hash = sha256_hex(&archive);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, hash.clone());

        let first = test_installer().install(&request).expect("first install");
        server.join().expect("server");
        fs::remove_dir_all(&first.install_dir).expect("remove installed fixture");

        let mut offline_request = request;
        offline_request.artifact.sources[0].url = "http://127.0.0.1:1/offline.zip".to_owned();
        let cached = test_installer()
            .install(&offline_request)
            .expect("cached install");
        assert_eq!(cached.source_id, "cache");
        assert_eq!(cached.bytes_downloaded, 0);
        assert!(cached.install_dir.join("bin/node.exe").is_file());
        assert!(
            root.path()
                .join(format!("downloads/sha256/{hash}.archive"))
                .is_file()
        );
    }

    #[test]
    fn concurrent_cache_commits_leave_one_verified_archive() {
        let root = tempdir().expect("temp root");
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let hash = sha256_hex(&archive);
        let expected: [u8; 32] = hex::decode(&hash)
            .expect("hash")
            .try_into()
            .expect("sha256");
        let destination = download_cache_path(root.path(), &hash).expect("cache path");
        let installer = Arc::new(test_installer());
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for index in 0..2 {
            let source = root.path().join(format!("source-{index}.zip"));
            fs::write(&source, &archive).expect("source archive");
            let installer = Arc::clone(&installer);
            let barrier = Arc::clone(&barrier);
            let destination = destination.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                installer.persist_cache_artifact(&source, &destination, &expected)
            }));
        }
        for worker in workers {
            worker.join().expect("cache worker").expect("cache commit");
        }
        assert_eq!(fs::read(destination).expect("cached archive"), archive);
    }

    #[test]
    fn installs_tar_xz_with_stripped_root_and_executable_permissions() {
        let archive = tar_xz_bytes(&[
            ("node-v24.0.0-linux-x64/bin/node", b"fake node", 0o755),
            ("node-v24.0.0-linux-x64/bin/npm", b"fake npm", 0o755),
        ]);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), url, sha256_hex(&archive));
        request.target = "linux-x86_64".to_owned();
        request.artifact.format = ArtifactFormat::TarXz;
        request.strip_components = 1;
        request.required_paths = vec![PathBuf::from("bin/node"), PathBuf::from("bin/npm")];

        let outcome = test_installer().install(&request).expect("install");
        server.join().expect("server");

        assert_eq!(
            fs::read(outcome.install_dir.join("bin/node")).expect("runtime"),
            b"fake node"
        );
        assert!(!outcome.install_dir.join("node-v24.0.0-linux-x64").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(outcome.install_dir.join("bin/node"))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        assert_transaction_root_is_empty(root.path());
    }

    #[cfg(unix)]
    #[test]
    fn installs_tar_xz_with_safe_node_style_symlinks() {
        let archive = tar_xz_with_symlink("../lib/node_modules/corepack/dist/corepack.js");
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), url, sha256_hex(&archive));
        request.target = "linux-x86_64".to_owned();
        request.artifact.format = ArtifactFormat::TarXz;
        request.strip_components = 1;
        request.required_paths = vec![PathBuf::from("bin/node"), PathBuf::from("bin/corepack")];

        let outcome = test_installer().install(&request).expect("install");
        server.join().expect("server");

        assert_eq!(
            fs::read_link(outcome.install_dir.join("bin/corepack")).expect("link"),
            PathBuf::from("../lib/node_modules/corepack/dist/corepack.js")
        );
        assert!(outcome.install_dir.join("bin/corepack").is_file());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn rejects_tar_xz_symlinks_that_escape_the_install_root() {
        let archive = tar_xz_with_symlink("../../outside");
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), url, sha256_hex(&archive));
        request.target = "linux-x86_64".to_owned();
        request.artifact.format = ArtifactFormat::TarXz;
        request.strip_components = 1;

        let error = test_installer()
            .install(&request)
            .expect_err("escaping symlink");
        server.join().expect("server");

        assert!(matches!(error, Error::UnsafeArchiveEntry { .. }));
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn falls_back_to_next_source_only_after_network_failure() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (mirror_url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), mirror_url, sha256_hex(&archive));
        request.artifact.sources.insert(
            0,
            ArtifactSource {
                id: "unreachable".to_owned(),
                url: "http://127.0.0.1:1/artifact.zip".to_owned(),
                kind: ArtifactSourceKind::Official,
            },
        );

        let outcome = test_installer()
            .install(&request)
            .expect("fallback install");
        server.join().expect("server");

        assert_eq!(outcome.source_id, "local-mirror");
        assert!(outcome.install_dir.join("bin/node.exe").is_file());
    }

    #[test]
    fn checksum_failure_is_a_hard_stop_without_source_fallback() {
        let expected_archive = zip_bytes(&[("bin/node.exe", b"expected")]);
        let tampered_archive = zip_bytes(&[("bin/node.exe", b"tampered")]);
        let (tampered_url, server) = serve_once(tampered_archive.clone(), tampered_archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), tampered_url, sha256_hex(&expected_archive));
        request.artifact.sources.push(ArtifactSource {
            id: "would-be-fallback".to_owned(),
            url: "http://127.0.0.1:1/should-not-run.zip".to_owned(),
            kind: ArtifactSourceKind::Official,
        });

        let error = test_installer()
            .install(&request)
            .expect_err("checksum hard stop");
        server.join().expect("server");

        assert!(matches!(error, Error::ChecksumMismatch { .. }));
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn checksum_mismatch_never_exposes_installation() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, "00".repeat(32));

        let error = test_installer().install(&request).expect_err("bad hash");
        server.join().expect("server");

        assert!(matches!(error, Error::ChecksumMismatch { .. }));
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn rejects_zip_path_traversal_without_writing_outside_staging() {
        let archive = zip_bytes(&[("../escape.txt", b"escaped")]);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, sha256_hex(&archive));

        let error = test_installer()
            .install(&request)
            .expect_err("unsafe archive");
        server.join().expect("server");

        assert!(matches!(error, Error::UnsafeArchiveEntry { .. }));
        assert!(!root.path().join("escape.txt").exists());
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn interrupted_download_never_exposes_installation() {
        let full_archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let partial = full_archive[..full_archive.len() / 2].to_vec();
        let (url, server) = serve_once(partial, full_archive.len());
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, sha256_hex(&full_archive));

        let error = test_installer()
            .install(&request)
            .expect_err("interrupted download");
        server.join().expect("server");

        assert!(matches!(
            error,
            Error::ArtifactSourcesExhausted { .. } | Error::ChecksumMismatch { .. }
        ));
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn expanded_size_limit_stops_archive_before_commit() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, sha256_hex(&archive));
        let installer = Installer::new(InstallLimits {
            max_download_bytes: 1024 * 1024,
            max_unpacked_bytes: 4,
            max_archive_entries: 100,
            request_timeout: Duration::from_secs(5),
        })
        .expect("installer");

        let error = installer
            .install(&request)
            .expect_err("expanded size limit");
        server.join().expect("server");

        assert!(matches!(error, Error::ArchiveTooLarge { limit: 4 }));
        assert!(!final_dir(root.path()).exists());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn refuses_install_path_traversal_before_network_access() {
        let root = tempdir().expect("temp root");
        let mut request = request(
            root.path(),
            "http://127.0.0.1:1/unused".to_owned(),
            "00".repeat(32),
        );
        request.version = "../outside".to_owned();

        let error = test_installer()
            .install(&request)
            .expect_err("invalid version");

        assert!(matches!(
            error,
            Error::InvalidInstallSegment {
                field: "version",
                ..
            }
        ));
        assert!(!root.path().join("outside").exists());
    }

    fn test_installer() -> Installer {
        Installer::new(InstallLimits {
            max_download_bytes: 1024 * 1024,
            max_unpacked_bytes: 1024 * 1024,
            max_archive_entries: 100,
            request_timeout: Duration::from_secs(5),
        })
        .expect("installer")
    }

    fn request(home: &Path, url: String, sha256: String) -> InstallRequest {
        InstallRequest {
            pinset_home: home.to_path_buf(),
            tool: "node".to_owned(),
            version: "20.0.0".to_owned(),
            target: "windows-x86_64".to_owned(),
            artifact: ArtifactSpec {
                canonical_url: "https://nodejs.org/dist/v20.0.0/node-v20.0.0-win-x64.zip"
                    .to_owned(),
                sources: vec![ArtifactSource {
                    id: "local-mirror".to_owned(),
                    url,
                    kind: ArtifactSourceKind::Mirror,
                }],
                sha256,
                format: ArtifactFormat::Zip,
            },
            strip_components: 0,
            required_paths: vec![PathBuf::from("bin/node.exe")],
        }
    }

    fn final_dir(home: &Path) -> PathBuf {
        home.join("installs")
            .join("node")
            .join("20.0.0")
            .join("windows-x86_64")
    }

    fn assert_transaction_root_is_empty(home: &Path) {
        let temporary_root = home.join("tmp");
        let remaining = fs::read_dir(temporary_root)
            .expect("transaction root")
            .count();
        assert_eq!(remaining, 0, "temporary transaction leaked");
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        for (name, content) in entries {
            writer
                .start_file(name, SimpleFileOptions::default())
                .expect("zip entry");
            writer.write_all(content).expect("zip content");
        }
        writer.finish().expect("zip finish").into_inner()
    }

    fn tar_xz_bytes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let encoder = XzEncoder::new(Vec::new(), 6);
        let mut builder = tar::Builder::new(encoder);
        for (path, content, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("tar path");
            header.set_size(content.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder
                .append(&header, Cursor::new(*content))
                .expect("tar entry");
        }
        let encoder = builder.into_inner().expect("tar finish");
        encoder.finish().expect("xz finish")
    }

    fn tar_xz_with_symlink(link_target: &str) -> Vec<u8> {
        let encoder = XzEncoder::new(Vec::new(), 6);
        let mut builder = tar::Builder::new(encoder);
        for (path, content, mode) in [
            (
                "node-v24.0.0-linux-x64/bin/node",
                b"fake node".as_slice(),
                0o755,
            ),
            (
                "node-v24.0.0-linux-x64/lib/node_modules/corepack/dist/corepack.js",
                b"fake corepack".as_slice(),
                0o755,
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("tar path");
            header.set_size(content.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder
                .append(&header, Cursor::new(content))
                .expect("tar entry");
        }
        let mut link_header = tar::Header::new_gnu();
        link_header
            .set_path("node-v24.0.0-linux-x64/bin/corepack")
            .expect("link path");
        link_header.set_entry_type(tar::EntryType::Symlink);
        link_header.set_link_name(link_target).expect("link target");
        link_header.set_size(0);
        link_header.set_mode(0o777);
        link_header.set_cksum();
        builder
            .append(&link_header, io::empty())
            .expect("tar symlink");

        let encoder = builder.into_inner().expect("tar finish");
        encoder.finish().expect("xz finish")
    }

    fn serve_once(body: Vec<u8>, declared_length: usize) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {declared_length}\r\nConnection: close\r\n\r\n"
            )
            .expect("response headers");
            stream.write_all(&body).expect("response body");
            stream.flush().expect("flush response");
        });
        (format!("http://{address}/artifact.zip"), handle)
    }
}
