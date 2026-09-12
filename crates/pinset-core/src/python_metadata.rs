use std::{
    cmp::Reverse,
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::Builder;

use crate::{
    ArtifactIntegrity, Error, LockedArtifact, LockedArtifactFormat, LockedArtifactOverlay,
    LockedTool, PYTHON_TARGETS, PYTHON_VARIANT, Result, SourceConfig, current_target_for_tool,
    import_download_cache_with_integrity, is_exact_python_version, plan_python_artifact,
};

const PYTHON_BUILD_STANDALONE_INDEX_URL: &str =
    "https://raw.githubusercontent.com/astral-sh/versions/main/v1/python-build-standalone.ndjson";
const OFFICIAL_CPYTHON_RELEASES_URL: &str =
    "https://www.python.org/api/v2/downloads/release/?is_published=true";
const OFFICIAL_CPYTHON_FILES_URL: &str =
    "https://www.python.org/api/v2/downloads/release_file/?release=";
const MAX_PYTHON_INDEX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PYTHON_ARTIFACT_BYTES: u64 = 1_073_741_824;
const PYTHON_BUILD_STANDALONE_VERIFICATION: &str = "python-build-standalone-versions-sha256";
const PYTHON_ORG_API_VERIFICATION: &str = "python-org-api-sha256";
const PYTHON_ORG_HTTPS_VERIFICATION: &str = "python-org-https-sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonRelease {
    pub version: String,
    pub build_id: String,
    pub distribution: String,
    pub date: String,
}

#[derive(Debug)]
pub struct PythonMetadataClient {
    client: Client,
    fallback_metadata_url: Url,
}

#[derive(Debug, Deserialize)]
struct RegistryRelease {
    version: String,
    date: String,
    #[serde(default)]
    artifacts: Vec<RegistryArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryArtifact {
    platform: String,
    variant: String,
    url: String,
    archive_format: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct CpythonReleaseEntry {
    name: String,
    release_date: String,
    #[serde(default)]
    pre_release: bool,
    resource_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CpythonReleaseFile {
    name: String,
    url: String,
    #[serde(default)]
    md5_sum: String,
    #[serde(default)]
    sha256_sum: String,
    #[serde(default)]
    gpg_signature_file: String,
}

#[derive(Debug, Deserialize)]
struct WindowsReleaseManifest {
    versions: Vec<WindowsReleasePackage>,
}

#[derive(Debug, Deserialize)]
struct WindowsReleasePackage {
    id: String,
    #[serde(rename = "sort-version")]
    sort_version: String,
    url: String,
    hash: WindowsReleaseHash,
}

#[derive(Debug, Deserialize)]
struct WindowsReleaseHash {
    sha256: String,
}

#[derive(Debug, Clone)]
struct CpythonRelease {
    version: String,
    date: String,
    release_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OfficialInstallKind {
    FullZip,
    EmbeddableZip,
    Msi,
    MsiBundle,
}

impl OfficialInstallKind {
    const fn metadata_name(self) -> &'static str {
        match self {
            Self::FullZip => "full-zip",
            Self::EmbeddableZip => "embeddable-zip",
            Self::Msi => "msi",
            Self::MsiBundle => "msi-bundle",
        }
    }

    const fn lock_format(self) -> LockedArtifactFormat {
        match self {
            Self::FullZip | Self::EmbeddableZip => LockedArtifactFormat::Zip,
            Self::Msi | Self::MsiBundle => LockedArtifactFormat::Binary,
        }
    }
}

#[derive(Debug, Clone)]
struct SupportedPythonRelease {
    python_version: String,
    build_id: String,
    distribution: String,
    date: String,
    artifacts: Vec<(String, RegistryArtifact)>,
}

impl PythonMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_url(PYTHON_BUILD_STANDALONE_INDEX_URL)
    }

    pub fn for_url(url: &str) -> Result<Self> {
        let client = crate::http_client_builder()?
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let fallback_metadata_url =
            Url::parse(url).map_err(|source| Error::InvalidSourceBaseUrl {
                url: url.to_owned(),
                reason: source.to_string(),
            })?;
        Ok(Self {
            client,
            fallback_metadata_url,
        })
    }

    pub fn available_releases(&self) -> Result<Vec<PythonRelease>> {
        let releases = self
            .cpython_releases()?
            .into_iter()
            .map(|release| PythonRelease {
                version: release.version,
                build_id: String::new(),
                distribution: "python.org/cpython".to_owned(),
                date: release.date,
            })
            .collect::<Vec<_>>();
        Ok(releases)
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        let target = current_target_for_tool("python");
        Ok(self
            .resolve_tool_for_target(selector, &target, None)?
            .version)
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let target = current_target_for_tool("python");
        self.resolve_tool_for_target(selector, &target, None)
    }

    pub fn resolve_tool_for_target(
        &self,
        selector: &str,
        target: &str,
        pinset_home: Option<&Path>,
    ) -> Result<LockedTool> {
        if !PYTHON_TARGETS.contains(&target) {
            return Err(Error::UnsupportedPythonTarget {
                target: target.to_owned(),
            });
        }
        if !selector.contains('+')
            && let Some(release) = select_cpython_release(&self.cpython_releases()?, selector)?
        {
            let files = self.cpython_release_files(&release.release_id)?;
            if let Some(file) = self.windows_full_zip(&release, &files, target)? {
                return self.lock_official_release(
                    &release,
                    file,
                    OfficialInstallKind::FullZip,
                    target,
                    pinset_home,
                );
            }
            if let Some(locked) = self.lock_official_msi_bundle(&release, target, pinset_home)? {
                return Ok(locked);
            }
            if let Some((file, install_kind)) = select_official_artifact(&files, target) {
                return self.lock_official_release(
                    &release,
                    file,
                    install_kind,
                    target,
                    pinset_home,
                );
            }
            if let Ok(fallback) = select_release(self.supported_releases()?, &release.version) {
                return lock_standalone_release(fallback);
            }
            return Err(Error::PythonDistributionUnavailable {
                version: release.version,
                distribution: target.to_owned(),
            });
        }
        lock_standalone_release(select_release(self.supported_releases()?, selector)?)
    }

    fn windows_full_zip(
        &self,
        release: &CpythonRelease,
        files: &[CpythonReleaseFile],
        target: &str,
    ) -> Result<Option<CpythonReleaseFile>> {
        if target != "windows-x86_64" {
            return Ok(None);
        }
        let Some(manifest_file) = files
            .iter()
            .find(|file| file.name == "Windows release manifest")
        else {
            return Ok(None);
        };
        official_artifact_path(&manifest_file.url, &release.version)?;
        let bytes = self.download_metadata_bytes(&manifest_file.url)?;
        if valid_sha256(&manifest_file.sha256_sum)
            && hex::encode(Sha256::digest(&bytes)) != manifest_file.sha256_sum.to_ascii_lowercase()
        {
            return Err(Error::InvalidPythonIndex {
                reason: format!(
                    "python.org Windows release manifest hash mismatch for {}",
                    release.version
                ),
            });
        }
        let manifest: WindowsReleaseManifest =
            serde_json::from_slice(&bytes).map_err(|source| Error::InvalidPythonIndex {
                reason: format!("python.org Windows release manifest: {source}"),
            })?;
        let minor = release
            .version
            .split('.')
            .take(2)
            .collect::<Vec<_>>()
            .join(".");
        let expected_id = format!("pythoncore-{minor}-64");
        let Some(package) = manifest.versions.into_iter().find(|package| {
            package.id == expected_id
                && package.sort_version == release.version
                && valid_sha256(&package.hash.sha256)
        }) else {
            return Ok(None);
        };
        official_artifact_path(&package.url, &release.version)?;
        Ok(Some(CpythonReleaseFile {
            name: "Windows full distribution ZIP (64-bit)".to_owned(),
            url: package.url,
            md5_sum: String::new(),
            sha256_sum: package.hash.sha256,
            gpg_signature_file: String::new(),
        }))
    }

    fn lock_official_msi_bundle(
        &self,
        release: &CpythonRelease,
        target: &str,
        pinset_home: Option<&Path>,
    ) -> Result<Option<LockedTool>> {
        let Some((major, minor, _)) = version_tuple(&release.version) else {
            return Ok(None);
        };
        if target != "windows-x86_64" || major != 3 || !(5..=12).contains(&minor) {
            return Ok(None);
        }
        let base = format!(
            "https://www.python.org/ftp/python/{}/amd64/",
            release.version
        );
        let core_url = format!("{base}core.msi");
        let response =
            self.client
                .head(&core_url)
                .send()
                .map_err(|source| Error::PythonMetadataRequest {
                    url: core_url.clone(),
                    source,
                })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()
            .map_err(|source| Error::PythonMetadataRequest {
                url: core_url,
                source,
            })?;

        let mut locked_components = Vec::new();
        for name in ["core.msi", "exe.msi", "lib.msi"] {
            let file = CpythonReleaseFile {
                name: format!("Windows installer component {name}"),
                url: format!("{base}{name}"),
                md5_sum: String::new(),
                sha256_sum: String::new(),
                gpg_signature_file: format!("{base}{name}.asc"),
            };
            let artifact_path = official_artifact_path(&file.url, &release.version)?;
            let sha256 = self.download_and_pin_official_artifact(&file, pinset_home)?;
            locked_components.push((file.url, artifact_path, sha256));
        }
        let (canonical_url, artifact_path, sha256) =
            locked_components.pop().expect("three MSI components");
        let overlays = locked_components
            .into_iter()
            .map(
                |(canonical_url, artifact_path, sha256)| LockedArtifactOverlay {
                    canonical_url,
                    artifact_path,
                    integrity: format!("sha256:{sha256}"),
                    format: LockedArtifactFormat::Binary,
                    archive_root: String::new(),
                    verification: PYTHON_ORG_HTTPS_VERIFICATION.to_owned(),
                },
            )
            .collect();
        let metadata = BTreeMap::from([
            ("distribution".to_owned(), "python.org/cpython".to_owned()),
            (
                "install_kind".to_owned(),
                OfficialInstallKind::MsiBundle.metadata_name().to_owned(),
            ),
            ("python_version".to_owned(), release.version.clone()),
        ]);
        Ok(Some(LockedTool {
            name: "python".to_owned(),
            requested: release.version.clone(),
            version: release.version.clone(),
            provider: "python.org-cpython".to_owned(),
            released_at: Some(release.date.clone()),
            metadata,
            options: Default::default(),
            artifacts: vec![LockedArtifact {
                target: target.to_owned(),
                canonical_url,
                artifact_path,
                sha256,
                integrity: None,
                format: LockedArtifactFormat::Binary,
                archive_root: String::new(),
                verification: PYTHON_ORG_HTTPS_VERIFICATION.to_owned(),
                overlays,
            }],
        }))
    }

    fn download_metadata_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::PythonMetadataRequest {
                url: url.to_owned(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PYTHON_INDEX_BYTES)
        {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_PYTHON_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::PythonMetadataRead {
                url: url.to_owned(),
                source,
            })?;
        if bytes.len() as u64 > MAX_PYTHON_INDEX_BYTES {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        Ok(bytes)
    }

    fn lock_official_release(
        &self,
        release: &CpythonRelease,
        file: CpythonReleaseFile,
        install_kind: OfficialInstallKind,
        target: &str,
        pinset_home: Option<&Path>,
    ) -> Result<LockedTool> {
        let artifact_path = official_artifact_path(&file.url, &release.version)?;
        if !file.gpg_signature_file.is_empty() {
            let signature_path =
                official_artifact_path(&file.gpg_signature_file, &release.version)?;
            if signature_path != format!("{artifact_path}.asc") {
                return Err(Error::InvalidPythonIndex {
                    reason: format!("python.org signature does not match artifact {}", file.url),
                });
            }
        }
        let (sha256, verification) = if valid_sha256(&file.sha256_sum) {
            (
                file.sha256_sum.to_ascii_lowercase(),
                PYTHON_ORG_API_VERIFICATION,
            )
        } else {
            (
                self.download_and_pin_official_artifact(&file, pinset_home)?,
                PYTHON_ORG_HTTPS_VERIFICATION,
            )
        };
        let mut metadata = BTreeMap::new();
        metadata.insert("distribution".to_owned(), "python.org/cpython".to_owned());
        metadata.insert(
            "install_kind".to_owned(),
            install_kind.metadata_name().to_owned(),
        );
        metadata.insert("python_version".to_owned(), release.version.clone());
        Ok(LockedTool {
            name: "python".to_owned(),
            requested: release.version.clone(),
            version: release.version.clone(),
            provider: "python.org-cpython".to_owned(),
            released_at: Some(release.date.clone()),
            metadata,
            options: Default::default(),
            artifacts: vec![LockedArtifact {
                target: target.to_owned(),
                canonical_url: file.url,
                artifact_path,
                sha256,
                integrity: None,
                format: install_kind.lock_format(),
                archive_root: String::new(),
                verification: verification.to_owned(),
                overlays: Vec::new(),
            }],
        })
    }

    fn download_and_pin_official_artifact(
        &self,
        file: &CpythonReleaseFile,
        pinset_home: Option<&Path>,
    ) -> Result<String> {
        let mut response = self
            .client
            .get(&file.url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::PythonMetadataRequest {
                url: file.url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PYTHON_ARTIFACT_BYTES)
        {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_ARTIFACT_BYTES,
            });
        }
        let temporary_parent = pinset_home.map(|home| home.join("tmp"));
        if let Some(parent) = &temporary_parent {
            fs::create_dir_all(parent).map_err(|source| Error::PythonMetadataRead {
                url: file.url.clone(),
                source,
            })?;
        }
        let mut temporary = if let Some(parent) = &temporary_parent {
            Builder::new()
                .prefix("python-official-")
                .tempfile_in(parent)
        } else {
            Builder::new().prefix("python-official-").tempfile()
        }
        .map_err(|source| Error::PythonMetadataRead {
            url: file.url.clone(),
            source,
        })?;
        let mut sha256 = Sha256::new();
        let mut md5 = md5::Context::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|source| Error::PythonMetadataRead {
                    url: file.url.clone(),
                    source,
                })?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > MAX_PYTHON_ARTIFACT_BYTES {
                return Err(Error::PythonMetadataTooLarge {
                    limit: MAX_PYTHON_ARTIFACT_BYTES,
                });
            }
            sha256.update(&buffer[..read]);
            md5.consume(&buffer[..read]);
            temporary
                .write_all(&buffer[..read])
                .map_err(|source| Error::PythonMetadataRead {
                    url: file.url.clone(),
                    source,
                })?;
        }
        temporary
            .flush()
            .map_err(|source| Error::PythonMetadataRead {
                url: file.url.clone(),
                source,
            })?;
        let actual_md5 = format!("{:x}", md5.finalize());
        if !file.md5_sum.is_empty() && !actual_md5.eq_ignore_ascii_case(&file.md5_sum) {
            return Err(Error::InvalidPythonIndex {
                reason: format!("python.org artifact MD5 mismatch for {}", file.url),
            });
        }
        let sha256 = hex::encode(sha256.finalize());
        if let Some(home) = pinset_home {
            let integrity = ArtifactIntegrity::parse(&sha256)?;
            import_download_cache_with_integrity(home, temporary.path(), &integrity)?;
        }
        Ok(sha256)
    }

    fn supported_releases(&self) -> Result<Vec<SupportedPythonRelease>> {
        parse_index(&self.download_index()?)
    }

    fn download_index(&self) -> Result<String> {
        let display_url = self.fallback_metadata_url.to_string();
        let mut response = self
            .client
            .get(self.fallback_metadata_url.clone())
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::PythonMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PYTHON_INDEX_BYTES)
        {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_PYTHON_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::PythonMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_PYTHON_INDEX_BYTES {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidPythonIndex {
            reason: "index is not UTF-8".to_owned(),
        })
    }

    fn cpython_releases(&self) -> Result<Vec<CpythonRelease>> {
        let url = Url::parse(OFFICIAL_CPYTHON_RELEASES_URL).expect("official CPython API URL");
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::PythonMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PYTHON_INDEX_BYTES)
        {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_PYTHON_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::PythonMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_PYTHON_INDEX_BYTES {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        let entries: Vec<CpythonReleaseEntry> =
            serde_json::from_slice(&bytes).map_err(|source| Error::InvalidPythonIndex {
                reason: format!("python.org releases: {source}"),
            })?;
        let mut releases = entries
            .into_iter()
            .filter(|release| !release.pre_release)
            .filter_map(|release| {
                let version = release.name.strip_prefix("Python ")?;
                let release_id = release
                    .resource_uri
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()?
                    .to_owned();
                (!release_id.is_empty()
                    && release_id.bytes().all(|byte| byte.is_ascii_digit())
                    && is_exact_python_version(version))
                .then(|| CpythonRelease {
                    version: version.to_owned(),
                    date: release.release_date,
                    release_id,
                })
            })
            .collect::<Vec<_>>();
        releases
            .sort_by_key(|release| Reverse(version_tuple(&release.version).unwrap_or_default()));
        releases.dedup_by(|left, right| left.version == right.version);
        Ok(releases)
    }

    fn cpython_release_files(&self, release_id: &str) -> Result<Vec<CpythonReleaseFile>> {
        if release_id.is_empty() || !release_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::InvalidPythonIndex {
                reason: "python.org release ID is invalid".to_owned(),
            });
        }
        let url = format!("{OFFICIAL_CPYTHON_FILES_URL}{release_id}");
        let mut response = self
            .client
            .get(&url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::PythonMetadataRequest {
                url: url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PYTHON_INDEX_BYTES)
        {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_PYTHON_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::PythonMetadataRead {
                url: url.clone(),
                source,
            })?;
        if bytes.len() as u64 > MAX_PYTHON_INDEX_BYTES {
            return Err(Error::PythonMetadataTooLarge {
                limit: MAX_PYTHON_INDEX_BYTES,
            });
        }
        serde_json::from_slice(&bytes).map_err(|source| Error::InvalidPythonIndex {
            reason: format!("python.org release files: {source}"),
        })
    }
}

fn select_cpython_release(
    releases: &[CpythonRelease],
    selector: &str,
) -> Result<Option<CpythonRelease>> {
    let normalized = selector.trim().to_ascii_lowercase();
    if normalized == "latest" || normalized == "current" {
        return Ok(releases.first().cloned());
    }
    let parts = normalized.split('.').collect::<Vec<_>>();
    let numeric = matches!(parts.len(), 1..=3)
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if !numeric {
        return Err(Error::InvalidPythonSelector {
            selector: selector.to_owned(),
        });
    }
    let requested = parts
        .iter()
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Error::InvalidPythonSelector {
            selector: selector.to_owned(),
        })?;
    Ok(releases.iter().find_map(|release| {
        let tuple = version_tuple(&release.version)?;
        (tuple.0 == requested[0]
            && (requested.len() < 2 || tuple.1 == requested[1])
            && (requested.len() < 3 || tuple.2 == requested[2]))
            .then(|| release.clone())
    }))
}

fn select_official_artifact(
    files: &[CpythonReleaseFile],
    target: &str,
) -> Option<(CpythonReleaseFile, OfficialInstallKind)> {
    if target != "windows-x86_64" {
        return None;
    }
    files
        .iter()
        .find(|file| {
            file.name == "Windows embeddable package (64-bit)"
                && file.url.to_ascii_lowercase().ends_with("-embed-amd64.zip")
        })
        .cloned()
        .map(|file| (file, OfficialInstallKind::EmbeddableZip))
        .or_else(|| {
            files
                .iter()
                .find(|file| {
                    file.name == "Windows x86-64 MSI installer"
                        && file.url.to_ascii_lowercase().ends_with(".amd64.msi")
                })
                .cloned()
                .map(|file| (file, OfficialInstallKind::Msi))
        })
}

fn official_artifact_path(url: &str, version: &str) -> Result<String> {
    let parsed = Url::parse(url).map_err(|source| Error::InvalidPythonIndex {
        reason: format!("invalid python.org artifact URL: {source}"),
    })?;
    let prefix = format!("/ftp/python/{version}/");
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("www.python.org")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with(&prefix)
    {
        return Err(Error::InvalidPythonIndex {
            reason: format!("artifact is outside the official python.org archive: {url}"),
        });
    }
    let relative = parsed
        .path()
        .strip_prefix("/ftp/python/")
        .expect("validated python.org prefix");
    if relative.is_empty()
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::InvalidPythonIndex {
            reason: format!("invalid python.org artifact path: {url}"),
        });
    }
    Ok(relative.to_owned())
}

fn lock_standalone_release(release: SupportedPythonRelease) -> Result<LockedTool> {
    let artifacts = release
        .artifacts
        .iter()
        .map(|(target, artifact)| {
            let plan =
                plan_python_artifact(&SourceConfig::default(), &release.distribution, target)?;
            Ok(LockedArtifact {
                target: target.clone(),
                // Standalone is a fallback provider and must retain its own canonical origin;
                // Python's configurable official source now points at python.org.
                canonical_url: artifact.url.clone(),
                artifact_path: plan.artifact_path,
                sha256: artifact.sha256.clone(),
                integrity: None,
                format: LockedArtifactFormat::TarGz,
                archive_root: plan.archive_root,
                verification: PYTHON_BUILD_STANDALONE_VERIFICATION.to_owned(),
                overlays: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut metadata = BTreeMap::new();
    metadata.insert("python_version".to_owned(), release.python_version);
    metadata.insert("build_id".to_owned(), release.build_id);
    metadata.insert("variant".to_owned(), PYTHON_VARIANT.to_owned());
    metadata.insert(
        "distribution".to_owned(),
        "astral-sh/python-build-standalone".to_owned(),
    );
    Ok(LockedTool {
        name: "python".to_owned(),
        requested: release.distribution.clone(),
        version: release.distribution,
        provider: "python-build-standalone".to_owned(),
        released_at: Some(release.date),
        metadata,
        options: Default::default(),
        artifacts,
    })
}

fn parse_index(body: &str) -> Result<Vec<SupportedPythonRelease>> {
    let mut releases = Vec::new();
    for (line_index, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: RegistryRelease =
            serde_json::from_str(line).map_err(|source| Error::InvalidPythonIndex {
                reason: format!("line {}: {source}", line_index + 1),
            })?;
        let Some((python_version, build_id)) = entry.version.split_once('+') else {
            continue;
        };
        if !is_exact_python_version(python_version)
            || build_id.len() != 8
            || !build_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let mut artifacts = Vec::with_capacity(PYTHON_TARGETS.len());
        for target in PYTHON_TARGETS {
            let distribution = format!("{python_version}+{build_id}");
            let plan = plan_python_artifact(&SourceConfig::default(), &distribution, target)?;
            let matching = entry.artifacts.iter().find(|artifact| {
                artifact.platform == plan.platform
                    && artifact.variant == plan.variant
                    && artifact.archive_format == "tar.gz"
                    && valid_sha256(&artifact.sha256)
                    && valid_official_artifact_url(&artifact.url, &plan.artifact_path)
            });
            if let Some(artifact) = matching {
                artifacts.push((target.to_owned(), artifact.clone()));
            }
        }
        if !artifacts.is_empty() {
            releases.push(SupportedPythonRelease {
                python_version: python_version.to_owned(),
                build_id: build_id.to_owned(),
                distribution: entry.version,
                date: entry.date,
                artifacts,
            });
        }
    }
    releases.sort_by_key(|release| {
        Reverse((
            version_tuple(&release.python_version).unwrap_or_default(),
            release.build_id.clone(),
        ))
    });
    Ok(releases)
}

fn select_release(
    releases: Vec<SupportedPythonRelease>,
    selector: &str,
) -> Result<SupportedPythonRelease> {
    let normalized = selector.trim().to_ascii_lowercase();
    let exact_distribution = normalized.split_once('+').is_some_and(|(version, build)| {
        is_exact_python_version(version)
            && build.len() == 8
            && build.bytes().all(|byte| byte.is_ascii_digit())
    });
    let exact_python = is_exact_python_version(&normalized);
    let numeric_parts = normalized.split('.').collect::<Vec<_>>();
    let numeric_selector = (numeric_parts.len() == 1 || numeric_parts.len() == 2)
        && numeric_parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if normalized != "latest"
        && normalized != "current"
        && !exact_distribution
        && !exact_python
        && !numeric_selector
    {
        return Err(Error::InvalidPythonSelector {
            selector: selector.to_owned(),
        });
    }
    let requested = numeric_selector
        .then(|| {
            numeric_parts
                .iter()
                .map(|part| part.parse::<u64>())
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|_| Error::InvalidPythonSelector {
            selector: selector.to_owned(),
        })?;

    releases
        .into_iter()
        .find(|release| {
            if normalized == "latest" || normalized == "current" {
                return true;
            }
            if exact_distribution {
                return release.distribution == normalized;
            }
            if exact_python {
                return release.python_version == normalized;
            }
            let tuple = version_tuple(&release.python_version).expect("validated Python release");
            let requested = requested.as_ref().expect("numeric selector");
            tuple.0 == requested[0] && (requested.len() == 1 || tuple.1 == requested[1])
        })
        .ok_or_else(|| Error::PythonSelectorNotFound {
            selector: selector.to_owned(),
        })
}

fn version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_official_artifact_url(value: &str, artifact_path: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return false;
    }
    let expected = format!("/astral-sh/python-build-standalone/releases/download/{artifact_path}");
    url.path() == expected || url.path() == expected.replace('+', "%2B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_install_only_releases_with_available_targets() {
        let complete = fixture_release("3.14.7+20260807", true);
        let prerelease = fixture_release("3.15.0rc1+20260807", true);
        let incomplete = fixture_release("3.13.14+20260807", false);
        let releases =
            parse_index(&format!("{complete}\n{prerelease}\n{incomplete}\n")).expect("index");
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].python_version, "3.14.7");
        assert_eq!(releases[0].build_id, "20260807");
        assert_eq!(releases[0].artifacts.len(), PYTHON_TARGETS.len());
        assert_eq!(releases[1].artifacts.len(), PYTHON_TARGETS.len() - 1);
    }

    #[test]
    fn resolves_python_and_build_selectors() {
        let body = format!(
            "{}\n{}\n{}\n",
            fixture_release("3.14.7+20260807", true),
            fixture_release("3.14.7+20260701", true),
            fixture_release("3.13.14+20260807", true)
        );
        let releases = parse_index(&body).expect("index");
        assert_eq!(
            select_release(releases.clone(), "3.14")
                .expect("minor")
                .distribution,
            "3.14.7+20260807"
        );
        assert_eq!(
            select_release(releases.clone(), "3.13.14")
                .expect("exact Python")
                .distribution,
            "3.13.14+20260807"
        );
        assert_eq!(
            select_release(releases, "3.14.7+20260807")
                .expect("exact distribution")
                .python_version,
            "3.14.7"
        );

        let releases = parse_index(&body).expect("index");
        assert_eq!(
            select_release(releases, "3.14.7+20260701")
                .expect("older exact distribution")
                .build_id,
            "20260701"
        );
    }

    #[test]
    fn identifies_archived_cpython_history_without_claiming_an_installable_distribution() {
        let releases = vec![
            CpythonRelease {
                version: "3.4.10".to_owned(),
                date: "2019-03-18T16:10:00Z".to_owned(),
                release_id: "296".to_owned(),
            },
            CpythonRelease {
                version: "2.7.18".to_owned(),
                date: "2020-04-20T14:18:29Z".to_owned(),
                release_id: "432".to_owned(),
            },
        ];
        assert_eq!(
            select_cpython_release(&releases, "2.7")
                .expect("selector")
                .as_ref()
                .map(|release| release.version.as_str()),
            Some("2.7.18")
        );
        assert_eq!(
            select_cpython_release(&releases, "3.4.10")
                .expect("selector")
                .as_ref()
                .map(|release| release.version.as_str()),
            Some("3.4.10")
        );
        assert_eq!(
            select_cpython_release(&releases, "latest")
                .expect("latest")
                .expect("release")
                .version,
            "3.4.10"
        );
    }

    #[test]
    fn official_windows_artifacts_prefer_embeddable_zip_then_msi() {
        let files = vec![
            CpythonReleaseFile {
                name: "Windows x86-64 MSI installer".to_owned(),
                url: "https://www.python.org/ftp/python/2.7.18/python-2.7.18.amd64.msi".to_owned(),
                md5_sum: "a".repeat(32),
                sha256_sum: String::new(),
                gpg_signature_file: "https://www.python.org/example.asc".to_owned(),
            },
            CpythonReleaseFile {
                name: "Windows embeddable package (64-bit)".to_owned(),
                url: "https://www.python.org/ftp/python/3.13.14/python-3.13.14-embed-amd64.zip"
                    .to_owned(),
                md5_sum: String::new(),
                sha256_sum: "b".repeat(64),
                gpg_signature_file: String::new(),
            },
        ];
        let selected = select_official_artifact(&files, "windows-x86_64").expect("artifact");
        assert_eq!(selected.1, OfficialInstallKind::EmbeddableZip);
        assert!(select_official_artifact(&files, "linux-x86_64").is_none());
    }

    #[test]
    fn official_artifact_paths_cannot_escape_python_org() {
        assert_eq!(
            official_artifact_path(
                "https://www.python.org/ftp/python/2.7.18/python-2.7.18.amd64.msi",
                "2.7.18"
            )
            .expect("official path"),
            "2.7.18/python-2.7.18.amd64.msi"
        );
        assert!(
            official_artifact_path(
                "https://example.test/ftp/python/2.7.18/python-2.7.18.amd64.msi",
                "2.7.18"
            )
            .is_err()
        );
    }

    fn fixture_release(distribution: &str, complete: bool) -> String {
        let (_, build_id) = distribution.split_once('+').expect("distribution");
        let mut artifacts = Vec::new();
        for (index, target) in PYTHON_TARGETS.iter().enumerate() {
            if !complete && index + 1 == PYTHON_TARGETS.len() {
                break;
            }
            let platform = match *target {
                "windows-x86_64" => "x86_64-pc-windows-msvc",
                "linux-x86_64" => "x86_64-unknown-linux-gnu",
                "linux-aarch64" => "aarch64-unknown-linux-gnu",
                "macos-x86_64" => "x86_64-apple-darwin",
                "macos-aarch64" => "aarch64-apple-darwin",
                _ => unreachable!("known Python target"),
            };
            let artifact_path =
                format!("{build_id}/cpython-{distribution}-{platform}-{PYTHON_VARIANT}.tar.gz");
            artifacts.push(serde_json::json!({
                "platform": platform,
                "variant": PYTHON_VARIANT,
                "url": format!(
                    "https://github.com/astral-sh/python-build-standalone/releases/download/{artifact_path}"
                ).replace('+', "%2B"),
                "archive_format": "tar.gz",
                "sha256": "ab".repeat(32),
            }));
        }
        serde_json::json!({
            "version": distribution,
            "date": "2026-08-07T00:00:00Z",
            "artifacts": artifacts,
        })
        .to_string()
    }
}
