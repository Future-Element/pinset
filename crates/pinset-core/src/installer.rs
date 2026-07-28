use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use reqwest::blocking::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::Builder;
use zip::ZipArchive;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    Zip,
}

impl ArtifactFormat {
    fn receipt_name(self) -> &'static str {
        match self {
            Self::Zip => "zip",
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
    pub required_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub install_dir: PathBuf,
    pub bytes_downloaded: u64,
    pub sha256: String,
    pub source_id: String,
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
            return Err(Error::InstallAlreadyExists { path: final_dir });
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
                    selected = Some((source, download_path, bytes_downloaded, actual_hash));
                    break;
                }
                Err(error) if is_retryable_source_error(&error) => {
                    last_retryable_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        let (selected_source, download_path, bytes_downloaded, actual_hash) =
            selected.ok_or_else(|| Error::ArtifactSourcesExhausted {
                attempted: attempted.join(", "),
                last_error: last_retryable_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "no source was attempted".to_owned()),
            })?;
        match request.artifact.format {
            ArtifactFormat::Zip => self.extract_zip(&download_path, &staging_dir)?,
        }
        validate_required_paths(&staging_dir, &request.required_paths)?;
        write_receipt(
            &staging_dir,
            request,
            selected_source,
            &actual_hash,
            bytes_downloaded,
        )?;

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
            bytes_downloaded,
            sha256: actual_hash,
            source_id: selected_source.id.clone(),
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

    fn extract_zip(&self, archive_path: &Path, destination: &Path) -> Result<()> {
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
            let relative = entry
                .enclosed_name()
                .ok_or_else(|| Error::UnsafeArchiveEntry {
                    entry: entry_name.clone(),
                })?;
            if !is_safe_relative(&relative) || is_special_zip_entry(entry.unix_mode()) {
                return Err(Error::UnsafeArchiveEntry { entry: entry_name });
            }
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
                    entry: entry_name,
                    path: output_path,
                    source,
                })?;
        }

        Ok(())
    }
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

fn write_receipt(
    staging: &Path,
    request: &InstallRequest,
    selected_source: &ArtifactSource,
    actual_hash: &str,
    bytes_downloaded: u64,
) -> Result<()> {
    let canonical_url = redacted_url(&request.artifact.canonical_url);
    let selected_url = redacted_url(&selected_source.url);
    let receipt = InstallReceipt {
        schema: 1,
        complete: true,
        tool: &request.tool,
        version: &request.version,
        target: &request.target,
        canonical_url: &canonical_url,
        selected_source: &selected_source.id,
        selected_source_kind: selected_source.kind.receipt_name(),
        selected_url: &selected_url,
        artifact_sha256: actual_hash,
        artifact_format: request.artifact.format.receipt_name(),
        bytes_downloaded,
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
        thread,
    };

    use tempfile::tempdir;
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
