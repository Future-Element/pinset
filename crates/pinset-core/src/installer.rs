use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use flate2::read::GzDecoder;
use reqwest::{
    StatusCode,
    blocking::Client,
    header::{CONTENT_RANGE, RANGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::Builder;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::download_cache::{
    download_cache_path_for_integrity, download_partial_path_for_integrity,
};
use crate::{ArtifactIntegrity, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    Zip,
    TarXz,
    TarGz,
}

impl ArtifactFormat {
    fn receipt_name(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
            Self::TarGz => "tar.gz",
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
    pub integrity: String,
    pub format: ArtifactFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactInstallSpec {
    pub artifact: ArtifactSpec,
    pub strip_components: usize,
    pub include_prefixes: Vec<PathBuf>,
    pub required_paths: Vec<PathBuf>,
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
    pub base_artifacts: Vec<ArtifactInstallSpec>,
    pub executable_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub install_dir: PathBuf,
    pub bytes_downloaded: u64,
    pub integrity: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadProgressEvent {
    Started {
        url: String,
        total_bytes: Option<u64>,
    },
    Advanced {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Finished {
        downloaded_bytes: u64,
    },
    Failed,
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

pub struct Installer {
    client: Client,
    limits: InstallLimits,
    progress_reporter: Option<Arc<dyn Fn(DownloadProgressEvent) + Send + Sync>>,
}

#[derive(Debug)]
struct SelectedArtifact {
    source_id: String,
    source_kind: String,
    source_url: String,
    path: PathBuf,
    bytes_downloaded: u64,
    actual_integrity: String,
}

struct InstallLock {
    file: File,
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

impl Installer {
    pub fn new(limits: InstallLimits) -> Result<Self> {
        let client = Client::builder()
            .timeout(limits.request_timeout)
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        Ok(Self {
            client,
            limits,
            progress_reporter: None,
        })
    }

    pub fn with_progress_reporter(
        mut self,
        reporter: impl Fn(DownloadProgressEvent) + Send + Sync + 'static,
    ) -> Self {
        self.progress_reporter = Some(Arc::new(reporter));
        self
    }

    pub fn install(&self, request: &InstallRequest) -> Result<InstallOutcome> {
        validate_request(request)?;
        let _install_lock = acquire_install_lock(request)?;
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

        let mut selected_bases = Vec::with_capacity(request.base_artifacts.len());
        for base in &request.base_artifacts {
            let selected = self.select_artifact(&request.pinset_home, &base.artifact)?;
            self.extract_selected(
                &selected,
                &staging_dir,
                base.artifact.format,
                base.strip_components,
                &base.include_prefixes,
            )?;
            validate_required_paths(&staging_dir, &base.required_paths)?;
            selected_bases.push(selected);
        }

        let selected = self.select_artifact(&request.pinset_home, &request.artifact)?;
        self.extract_selected(
            &selected,
            &staging_dir,
            request.artifact.format,
            request.strip_components,
            &[],
        )?;
        validate_required_paths(&staging_dir, &request.required_paths)?;
        ensure_executable_paths(&staging_dir, &request.executable_paths)?;
        write_receipt(&staging_dir, request, &selected, &selected_bases)?;

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
            bytes_downloaded: selected.bytes_downloaded
                + selected_bases
                    .iter()
                    .map(|artifact| artifact.bytes_downloaded)
                    .sum::<u64>(),
            integrity: selected.actual_integrity,
            source_id: selected.source_id,
            reused_existing: false,
        })
    }

    fn select_artifact(
        &self,
        pinset_home: &Path,
        artifact: &ArtifactSpec,
    ) -> Result<SelectedArtifact> {
        let expected_integrity = ArtifactIntegrity::parse(&artifact.integrity)?;
        let cache_path = download_cache_path_for_integrity(pinset_home, &expected_integrity)?;
        if self.cached_artifact_is_valid(&cache_path, &expected_integrity)? {
            return Ok(SelectedArtifact {
                source_id: "cache".to_owned(),
                source_kind: "cache".to_owned(),
                source_url: format!(
                    "cache:{}:{}",
                    expected_integrity.algorithm().as_str(),
                    expected_integrity.cache_key()
                ),
                path: cache_path,
                bytes_downloaded: 0,
                actual_integrity: expected_integrity.canonical(),
            });
        }

        let mut attempted = Vec::with_capacity(artifact.sources.len());
        let mut last_retryable_error = None;
        let download_path = download_partial_path_for_integrity(pinset_home, &expected_integrity)?;
        for source in &artifact.sources {
            attempted.push(source.id.clone());
            match self.download_verified(&source.url, &expected_integrity, &download_path) {
                Ok((bytes_downloaded, actual_integrity)) => {
                    self.persist_cache_artifact(&download_path, &cache_path, &expected_integrity)?;
                    fs::remove_file(&download_path).map_err(|source| Error::WriteDownload {
                        path: download_path.clone(),
                        source,
                    })?;
                    return Ok(SelectedArtifact {
                        source_id: source.id.clone(),
                        source_kind: source.kind.receipt_name().to_owned(),
                        source_url: redacted_url(&source.url),
                        path: cache_path,
                        bytes_downloaded,
                        actual_integrity,
                    });
                }
                Err(error) if is_retryable_source_error(&error) => {
                    last_retryable_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::ArtifactSourcesExhausted {
            attempted: attempted.join(", "),
            last_error: last_retryable_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "no source was attempted".to_owned()),
        })
    }

    fn extract_selected(
        &self,
        selected: &SelectedArtifact,
        staging_dir: &Path,
        format: ArtifactFormat,
        strip_components: usize,
        include_prefixes: &[PathBuf],
    ) -> Result<()> {
        match format {
            ArtifactFormat::Zip => {
                debug_assert!(include_prefixes.is_empty());
                self.extract_zip(&selected.path, staging_dir, strip_components)
            }
            ArtifactFormat::TarXz | ArtifactFormat::TarGz => self.extract_tar(
                &selected.path,
                staging_dir,
                strip_components,
                format,
                include_prefixes,
            ),
        }
    }

    fn cached_artifact_is_valid(
        &self,
        path: &Path,
        expected_integrity: &ArtifactIntegrity,
    ) -> Result<bool> {
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
        let mut hasher = expected_integrity.hasher();
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
        let actual = hasher.finalize();
        if actual == expected_integrity.expected_bytes() {
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
        expected_integrity: &ArtifactIntegrity,
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
                    if self.cached_artifact_is_valid(destination, expected_integrity)? {
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
        expected_integrity: &ArtifactIntegrity,
        destination: &Path,
    ) -> Result<(u64, String)> {
        let result = self.download_verified_inner(url, expected_integrity, destination);
        if result.is_err() {
            self.report_progress(DownloadProgressEvent::Failed);
        }
        result
    }

    fn download_verified_inner(
        &self,
        url: &str,
        expected_integrity: &ArtifactIntegrity,
        destination: &Path,
    ) -> Result<(u64, String)> {
        let display_url = redacted_url(url);
        let parent = destination
            .parent()
            .expect("partial download path always has a parent");
        fs::create_dir_all(parent).map_err(|source| Error::WriteDownload {
            path: parent.to_path_buf(),
            source,
        })?;
        let mut resume_from = match fs::symlink_metadata(destination) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata.file_type().is_symlink() =>
            {
                if metadata.len() > self.limits.max_download_bytes {
                    fs::remove_file(destination).map_err(|source| Error::WriteDownload {
                        path: destination.to_path_buf(),
                        source,
                    })?;
                    0
                } else {
                    metadata.len()
                }
            }
            Ok(_) => {
                return Err(Error::UnsafeDownloadCacheEntry {
                    path: destination.to_path_buf(),
                });
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => 0,
            Err(source) => {
                return Err(Error::ReadDownloadCache {
                    path: destination.to_path_buf(),
                    source,
                });
            }
        };

        let mut hasher = expected_integrity.hasher();
        if resume_from > 0 {
            let mut existing =
                File::open(destination).map_err(|source| Error::ReadDownloadCache {
                    path: destination.to_path_buf(),
                    source,
                })?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count =
                    existing
                        .read(&mut buffer)
                        .map_err(|source| Error::ReadDownloadCache {
                            path: destination.to_path_buf(),
                            source,
                        })?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            let existing_hash = hasher.clone().finalize();
            if existing_hash == expected_integrity.expected_bytes() {
                self.report_progress(DownloadProgressEvent::Started {
                    url: display_url,
                    total_bytes: Some(resume_from),
                });
                self.report_progress(DownloadProgressEvent::Finished {
                    downloaded_bytes: resume_from,
                });
                return Ok((resume_from, expected_integrity.canonical()));
            }
        }

        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header(RANGE, format!("bytes={resume_from}-"));
        }
        let mut response = request.send().map_err(|source| Error::DownloadRequest {
            url: display_url.clone(),
            source,
        })?;
        if resume_from > 0 && response.status() == StatusCode::OK {
            resume_from = 0;
            hasher = expected_integrity.hasher();
        } else if resume_from > 0 && response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            fs::remove_file(destination).map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
            resume_from = 0;
            hasher = expected_integrity.hasher();
            response = self
                .client
                .get(url)
                .send()
                .map_err(|source| Error::DownloadRequest {
                    url: display_url.clone(),
                    source,
                })?;
        } else if resume_from > 0 && response.status() == StatusCode::PARTIAL_CONTENT {
            let expected_prefix = format!("bytes {resume_from}-");
            let valid_content_range = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with(&expected_prefix));
            if !valid_content_range {
                fs::remove_file(destination).map_err(|source| Error::WriteDownload {
                    path: destination.to_path_buf(),
                    source,
                })?;
                resume_from = 0;
                hasher = expected_integrity.hasher();
                response =
                    self.client
                        .get(url)
                        .send()
                        .map_err(|source| Error::DownloadRequest {
                            url: display_url.clone(),
                            source,
                        })?;
            }
        }
        let resumed = resume_from > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
        let mut response =
            response
                .error_for_status()
                .map_err(|source| Error::DownloadRequest {
                    url: display_url.clone(),
                    source,
                })?;
        let total_bytes = response
            .content_length()
            .and_then(|length| length.checked_add(resume_from));
        if total_bytes.is_some_and(|length| length > self.limits.max_download_bytes) {
            return Err(Error::DownloadTooLarge {
                url: display_url.clone(),
                limit: self.limits.max_download_bytes,
            });
        }
        self.report_progress(DownloadProgressEvent::Started {
            url: display_url.clone(),
            total_bytes,
        });

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(resumed)
            .truncate(!resumed)
            .open(destination)
            .map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
        let mut total = resume_from;
        if resumed {
            self.report_progress(DownloadProgressEvent::Advanced {
                downloaded_bytes: total,
                total_bytes,
            });
        }
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
            self.report_progress(DownloadProgressEvent::Advanced {
                downloaded_bytes: total,
                total_bytes,
            });
        }
        file.sync_all().map_err(|source| Error::WriteDownload {
            path: destination.to_path_buf(),
            source,
        })?;

        let actual = hasher.finalize();
        let actual_integrity = format!(
            "{}:{}",
            expected_integrity.algorithm().as_str(),
            hex::encode(&actual)
        );
        if actual != expected_integrity.expected_bytes() {
            fs::remove_file(destination).map_err(|source| Error::WriteDownload {
                path: destination.to_path_buf(),
                source,
            })?;
            return Err(Error::ChecksumMismatch {
                expected: expected_integrity.canonical(),
                actual: actual_integrity,
            });
        }

        self.report_progress(DownloadProgressEvent::Finished {
            downloaded_bytes: total,
        });

        Ok((total, expected_integrity.canonical()))
    }

    fn report_progress(&self, event: DownloadProgressEvent) {
        if let Some(reporter) = &self.progress_reporter {
            reporter(event);
        }
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

    fn extract_tar(
        &self,
        archive_path: &Path,
        destination: &Path,
        strip_components: usize,
        format: ArtifactFormat,
        include_prefixes: &[PathBuf],
    ) -> Result<()> {
        let file = File::open(archive_path).map_err(|source| Error::ExtractArchiveEntry {
            entry: "<archive>".to_owned(),
            path: archive_path.to_path_buf(),
            source,
        })?;
        let reader: Box<dyn Read> = match format {
            ArtifactFormat::TarXz => Box::new(XzDecoder::new(file)),
            ArtifactFormat::TarGz => Box::new(GzDecoder::new(file)),
            ArtifactFormat::Zip => unreachable!("ZIP archives use extract_zip"),
        };
        let mut archive = tar::Archive::new(reader);
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
            if !include_prefixes.is_empty()
                && !include_prefixes
                    .iter()
                    .any(|prefix| relative.starts_with(prefix))
            {
                continue;
            }
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

fn acquire_install_lock(request: &InstallRequest) -> Result<InstallLock> {
    let identity = format!("{}\0{}\0{}", request.tool, request.version, request.target);
    let name = hex::encode(Sha256::digest(identity.as_bytes()));
    let directory = request.pinset_home.join("locks").join("installs");
    fs::create_dir_all(&directory).map_err(|source| Error::OpenInstallLock {
        path: directory.clone(),
        source,
    })?;
    let path = directory.join(format!("{name}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| Error::OpenInstallLock {
            path: path.clone(),
            source,
        })?;
    fs4::FileExt::lock(&file).map_err(|source| Error::AcquireInstallLock { path, source })?;
    Ok(InstallLock { file })
}

fn existing_install_outcome(final_dir: &Path, request: &InstallRequest) -> Option<InstallOutcome> {
    let content = fs::read_to_string(final_dir.join(".pinset-install.toml")).ok()?;
    let receipt: ExistingInstallReceipt = toml::from_str(&content).ok()?;
    let receipt_integrity = receipt
        .artifact_integrity
        .or(receipt.artifact_sha256)
        .and_then(|value| ArtifactIntegrity::parse(&value).ok())?
        .canonical();
    let expected_integrity = ArtifactIntegrity::parse(&request.artifact.integrity)
        .ok()?
        .canonical();
    let expected_base_integrities = request
        .base_artifacts
        .iter()
        .map(|artifact| {
            ArtifactIntegrity::parse(&artifact.artifact.integrity)
                .ok()
                .map(|integrity| integrity.canonical())
        })
        .collect::<Option<Vec<_>>>()?;
    if !receipt.complete
        || receipt.tool != request.tool
        || receipt.version != request.version
        || receipt.target != request.target
        || receipt_integrity != expected_integrity
        || receipt.base_artifact_integrities != expected_base_integrities
    {
        return None;
    }
    if validate_required_paths(final_dir, &request.required_paths).is_err() {
        return None;
    }
    if ensure_executable_paths(final_dir, &request.executable_paths).is_err() {
        return None;
    }
    Some(InstallOutcome {
        install_dir: final_dir.to_path_buf(),
        bytes_downloaded: 0,
        integrity: receipt_integrity,
        source_id: receipt.selected_source,
        reused_existing: true,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn validate_request(request: &InstallRequest) -> Result<()> {
    validate_segment("tool", &request.tool)?;
    validate_segment("version", &request.version)?;
    validate_segment("target", &request.target)?;
    validate_artifact_request(&request.artifact, request.strip_components)?;
    for base in &request.base_artifacts {
        validate_artifact_request(&base.artifact, base.strip_components)?;
        debug_assert!(
            base.artifact.format != ArtifactFormat::Zip || base.include_prefixes.is_empty()
        );
        for path in &base.include_prefixes {
            if !is_safe_relative(path) {
                return Err(Error::InvalidRequiredPath { path: path.clone() });
            }
        }
        for path in &base.required_paths {
            if !is_safe_relative(path) {
                return Err(Error::InvalidRequiredPath { path: path.clone() });
            }
        }
    }
    if request.required_paths.is_empty() {
        return Err(Error::RequiredPathsEmpty);
    }
    for path in &request.required_paths {
        if !is_safe_relative(path) {
            return Err(Error::InvalidRequiredPath { path: path.clone() });
        }
    }
    for path in &request.executable_paths {
        if !is_safe_relative(path) {
            return Err(Error::InvalidRequiredPath { path: path.clone() });
        }
    }
    Ok(())
}

fn validate_artifact_request(artifact: &ArtifactSpec, strip_components: usize) -> Result<()> {
    if strip_components > 8 {
        return Err(Error::InvalidStripComponents {
            value: strip_components,
        });
    }
    if artifact.sources.is_empty() {
        return Err(Error::ArtifactSourcesEmpty);
    }
    let mut source_ids = HashSet::with_capacity(artifact.sources.len());
    for source in &artifact.sources {
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

fn ensure_executable_paths(staging: &Path, executable_paths: &[PathBuf]) -> Result<()> {
    for relative in executable_paths {
        let path = staging.join(relative);
        if !path.is_file() {
            return Err(Error::RequiredPathMissing { path });
        }
        set_executable_permissions(&path, Some(0o755)).map_err(|source| {
            Error::ExtractArchiveEntry {
                entry: relative.display().to_string(),
                path,
                source,
            }
        })?;
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
    artifact_integrity: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact_sha256: Option<&'a str>,
    artifact_format: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    base_artifact_integrities: Vec<&'a str>,
    bytes_downloaded: u64,
}

#[derive(Deserialize)]
struct ExistingInstallReceipt {
    complete: bool,
    tool: String,
    version: String,
    target: String,
    selected_source: String,
    #[serde(default)]
    artifact_integrity: Option<String>,
    #[serde(default)]
    artifact_sha256: Option<String>,
    #[serde(default)]
    base_artifact_integrities: Vec<String>,
}

fn write_receipt(
    staging: &Path,
    request: &InstallRequest,
    selected: &SelectedArtifact,
    selected_bases: &[SelectedArtifact],
) -> Result<()> {
    let canonical_url = redacted_url(&request.artifact.canonical_url);
    let legacy_sha256 = selected.actual_integrity.strip_prefix("sha256:");
    let receipt = InstallReceipt {
        schema: 2,
        complete: true,
        tool: &request.tool,
        version: &request.version,
        target: &request.target,
        canonical_url: &canonical_url,
        selected_source: &selected.source_id,
        selected_source_kind: &selected.source_kind,
        selected_url: &selected.source_url,
        artifact_integrity: &selected.actual_integrity,
        artifact_sha256: legacy_sha256,
        artifact_format: request.artifact.format.receipt_name(),
        base_artifact_integrities: selected_bases
            .iter()
            .map(|artifact| artifact.actual_integrity.as_str())
            .collect(),
        bytes_downloaded: selected.bytes_downloaded
            + selected_bases
                .iter()
                .map(|artifact| artifact.bytes_downloaded)
                .sum::<u64>(),
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
        sync::{Arc, Barrier, Mutex},
        thread,
    };

    use base64::Engine as _;
    use flate2::{Compression, write::GzEncoder};
    use sha2::Sha512;
    use tempfile::tempdir;
    use xz2::write::XzEncoder;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;
    use crate::{download_cache::download_partial_path, download_cache_path};

    #[test]
    fn installs_verified_zip_atomically_from_local_http() {
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (base_url, server) = serve_once(archive.clone(), archive.len());
        let url = format!("{base_url}?token=must-not-be-recorded");
        let root = tempdir().expect("temp root");
        let request = request(root.path(), url, sha256_hex(&archive));
        let progress = Arc::new(Mutex::new(Vec::new()));
        let reported = Arc::clone(&progress);

        let outcome = test_installer()
            .with_progress_reporter(move |event| {
                reported.lock().expect("progress lock").push(event);
            })
            .install(&request)
            .expect("install");
        server.join().expect("server");

        let progress = progress.lock().expect("progress events");
        assert!(matches!(
            progress.first(),
            Some(DownloadProgressEvent::Started {
                total_bytes: Some(total),
                ..
            }) if *total == archive.len() as u64
        ));
        assert!(progress.iter().any(|event| matches!(
            event,
            DownloadProgressEvent::Advanced {
                downloaded_bytes,
                total_bytes: Some(total),
            } if *downloaded_bytes == archive.len() as u64 && *total == archive.len() as u64
        )));
        assert_eq!(
            progress.last(),
            Some(&DownloadProgressEvent::Finished {
                downloaded_bytes: archive.len() as u64,
            })
        );
        drop(progress);

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
        let expected = ArtifactIntegrity::parse(&hash).expect("sha256");
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
            let expected = expected.clone();
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
    fn concurrent_installs_of_the_same_runtime_download_only_once() {
        let root = tempdir().expect("temp root");
        let archive = zip_bytes(&[("bin/node.exe", b"fake node")]);
        let (url, server) = serve_once(archive.clone(), archive.len());
        let request = request(root.path(), url, sha256_hex(&archive));
        let installer = Arc::new(test_installer());
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let installer = Arc::clone(&installer);
            let barrier = Arc::clone(&barrier);
            let request = request.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                installer.install(&request)
            }));
        }

        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("install worker").expect("install"))
            .collect::<Vec<_>>();
        server.join().expect("server");

        assert_eq!(outcomes[0].install_dir, outcomes[1].install_dir);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome.reused_existing)
                .count(),
            1
        );
        assert!(final_dir(root.path()).join("bin/node.exe").is_file());
        assert_transaction_root_is_empty(root.path());
    }

    #[test]
    fn resumes_a_verified_partial_download_with_an_http_range_request() {
        let root = tempdir().expect("temp root");
        let archive = zip_bytes(&[("bin/node.exe", b"fake node with enough bytes")]);
        let hash = sha256_hex(&archive);
        let split = archive.len() / 2;
        let partial_path = download_partial_path(root.path(), &hash).expect("partial path");
        fs::create_dir_all(partial_path.parent().expect("partial parent")).expect("partial root");
        fs::write(&partial_path, &archive[..split]).expect("partial content");
        let (url, server) = serve_range_once(archive[split..].to_vec(), split, archive.len());
        let request = request(root.path(), url, hash);

        let outcome = test_installer().install(&request).expect("resumed install");
        server.join().expect("range server");

        assert_eq!(outcome.bytes_downloaded, archive.len() as u64);
        assert!(outcome.install_dir.join("bin/node.exe").is_file());
        assert!(!partial_path.exists());
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

    #[test]
    fn installs_tar_gz_with_npm_sha512_integrity() {
        let archive = tar_gz_bytes(&[("package/pnpm.exe", b"fake pnpm", 0o755)]);
        let integrity = format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&archive))
        );
        let (url, server) = serve_once(archive.clone(), archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), url, integrity.clone());
        request.tool = "pnpm".to_owned();
        request.version = "11.21.0".to_owned();
        request.target = "windows-x86_64".to_owned();
        request.artifact.format = ArtifactFormat::TarGz;
        request.strip_components = 1;
        request.required_paths = vec![PathBuf::from("pnpm.exe")];

        let outcome = test_installer().install(&request).expect("install tar.gz");
        server.join().expect("server");

        assert_eq!(outcome.integrity, integrity);
        assert_eq!(
            fs::read(outcome.install_dir.join("pnpm.exe")).expect("pnpm"),
            b"fake pnpm"
        );
        assert!(root.path().join("downloads/sha512").is_dir());
        let receipt =
            fs::read_to_string(outcome.install_dir.join(".pinset-install.toml")).expect("receipt");
        assert!(receipt.contains("schema = 2"));
        assert!(receipt.contains("artifact_integrity = \"sha512-"));
    }

    #[test]
    fn merges_a_verified_npm_base_archive_before_the_platform_binary() {
        let base_archive = tar_gz_bytes(&[
            ("package/dist/pnpm.mjs", b"shared pnpm runtime", 0o644),
            (
                "package/package.json",
                br#"{"name":"@pnpm/exe","type":"module"}"#,
                0o644,
            ),
            ("package/setup.js", b"must not be installed", 0o644),
        ]);
        let platform_archive = tar_gz_bytes(&[("package/pnpm", b"native pnpm", 0o644)]);
        let base_integrity = format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&base_archive))
        );
        let platform_integrity = format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&platform_archive))
        );
        let (base_url, base_server) = serve_once(base_archive.clone(), base_archive.len());
        let (platform_url, platform_server) =
            serve_once(platform_archive.clone(), platform_archive.len());
        let root = tempdir().expect("temp root");
        let mut request = request(root.path(), platform_url, platform_integrity);
        request.tool = "pnpm".to_owned();
        request.version = "11.21.0".to_owned();
        request.target = "linux-x86_64".to_owned();
        request.artifact.format = ArtifactFormat::TarGz;
        request.strip_components = 1;
        request.required_paths = vec![PathBuf::from("pnpm")];
        request.executable_paths = vec![PathBuf::from("pnpm")];
        request.base_artifacts = vec![ArtifactInstallSpec {
            artifact: ArtifactSpec {
                canonical_url: "https://registry.npmjs.org/@pnpm/exe/-/exe-11.21.0.tgz".to_owned(),
                sources: vec![ArtifactSource {
                    id: "local-base".to_owned(),
                    url: base_url,
                    kind: ArtifactSourceKind::Mirror,
                }],
                integrity: base_integrity.clone(),
                format: ArtifactFormat::TarGz,
            },
            strip_components: 1,
            include_prefixes: vec![PathBuf::from("dist"), PathBuf::from("package.json")],
            required_paths: vec![
                PathBuf::from("dist/pnpm.mjs"),
                PathBuf::from("package.json"),
            ],
        }];

        let outcome = test_installer()
            .install(&request)
            .expect("install merged pnpm");
        base_server.join().expect("base server");
        platform_server.join().expect("platform server");

        assert_eq!(
            fs::read(outcome.install_dir.join("pnpm")).expect("platform binary"),
            b"native pnpm"
        );
        assert_eq!(
            fs::read(outcome.install_dir.join("dist/pnpm.mjs")).expect("shared runtime"),
            b"shared pnpm runtime"
        );
        assert_eq!(
            fs::read(outcome.install_dir.join("package.json")).expect("package metadata"),
            br#"{"name":"@pnpm/exe","type":"module"}"#
        );
        assert!(!outcome.install_dir.join("setup.js").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(outcome.install_dir.join("pnpm"))
                    .expect("pnpm metadata")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        let receipt =
            fs::read_to_string(outcome.install_dir.join(".pinset-install.toml")).expect("receipt");
        assert!(receipt.contains(&base_integrity));
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
        let progress = Arc::new(Mutex::new(Vec::new()));
        let reported = Arc::clone(&progress);

        let error = test_installer()
            .with_progress_reporter(move |event| {
                reported.lock().expect("progress lock").push(event);
            })
            .install(&request)
            .expect_err("bad hash");
        server.join().expect("server");

        assert!(matches!(error, Error::ChecksumMismatch { .. }));
        assert_eq!(
            progress.lock().expect("progress events").last(),
            Some(&DownloadProgressEvent::Failed)
        );
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
                integrity: sha256,
                format: ArtifactFormat::Zip,
            },
            strip_components: 0,
            required_paths: vec![PathBuf::from("bin/node.exe")],
            base_artifacts: Vec::new(),
            executable_paths: Vec::new(),
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

    fn tar_gz_bytes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
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
        encoder.finish().expect("gzip finish")
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

    fn serve_range_once(
        body: Vec<u8>,
        range_start: usize,
        total_length: usize,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind range server");
        let address = listener.local_addr().expect("range server address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept range request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("range read timeout");
            let mut request = [0_u8; 4096];
            let length = stream.read(&mut request).expect("read range request");
            let request = String::from_utf8_lossy(&request[..length]).to_ascii_lowercase();
            assert!(
                request.contains(&format!("range: bytes={range_start}-")),
                "missing expected Range header in {request:?}"
            );
            let range_end = total_length - 1;
            write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {range_start}-{range_end}/{total_length}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("range response headers");
            stream.write_all(&body).expect("range response body");
            stream.flush().expect("flush range response");
        });
        (format!("http://{address}/artifact.zip"), handle)
    }
}
