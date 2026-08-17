use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    io::Read,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    Error, LockedArtifact, LockedArtifactFormat, LockedTool, PYTHON_TARGETS, PYTHON_VARIANT,
    Result, SourceConfig, is_exact_python_version, plan_python_artifact,
};

const OFFICIAL_PYTHON_INDEX_URL: &str =
    "https://raw.githubusercontent.com/astral-sh/versions/main/v1/python-build-standalone.ndjson";
const MAX_PYTHON_INDEX_BYTES: u64 = 32 * 1024 * 1024;
const PYTHON_VERIFICATION: &str = "python-build-standalone-versions-sha256";

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
    metadata_url: Url,
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
        Self::for_url(OFFICIAL_PYTHON_INDEX_URL)
    }

    pub fn for_url(url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let metadata_url = Url::parse(url).map_err(|source| Error::InvalidSourceBaseUrl {
            url: url.to_owned(),
            reason: source.to_string(),
        })?;
        Ok(Self {
            client,
            metadata_url,
        })
    }

    pub fn available_releases(&self) -> Result<Vec<PythonRelease>> {
        let mut versions = HashSet::new();
        Ok(self
            .supported_releases()?
            .into_iter()
            .filter(|release| versions.insert(release.python_version.clone()))
            .map(|release| PythonRelease {
                version: release.python_version,
                build_id: release.build_id,
                distribution: release.distribution,
                date: release.date,
            })
            .collect())
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(self.resolve_release(selector)?.distribution)
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let release = self.resolve_release(selector)?;
        let artifacts = release
            .artifacts
            .into_iter()
            .map(|(target, artifact)| {
                let plan =
                    plan_python_artifact(&SourceConfig::default(), &release.distribution, &target)?;
                Ok(LockedArtifact {
                    target,
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: artifact.sha256,
                    integrity: None,
                    format: LockedArtifactFormat::TarGz,
                    archive_root: plan.archive_root,
                    verification: PYTHON_VERIFICATION.to_owned(),
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
            metadata,
            artifacts,
        })
    }

    fn resolve_release(&self, selector: &str) -> Result<SupportedPythonRelease> {
        select_release(self.supported_releases()?, selector)
    }

    fn supported_releases(&self) -> Result<Vec<SupportedPythonRelease>> {
        parse_index(&self.download_index()?)
    }

    fn download_index(&self) -> Result<String> {
        let display_url = self.metadata_url.to_string();
        let mut response = self
            .client
            .get(self.metadata_url.clone())
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
        let mut complete = true;
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
            let Some(artifact) = matching else {
                complete = false;
                break;
            };
            artifacts.push((target.to_owned(), artifact.clone()));
        }
        if complete {
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
    fn parses_only_stable_complete_install_only_releases() {
        let complete = fixture_release("3.14.7+20260807", true);
        let prerelease = fixture_release("3.15.0rc1+20260807", true);
        let incomplete = fixture_release("3.13.14+20260807", false);
        let releases =
            parse_index(&format!("{complete}\n{prerelease}\n{incomplete}\n")).expect("index");
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].python_version, "3.14.7");
        assert_eq!(releases[0].build_id, "20260807");
        assert_eq!(releases[0].artifacts.len(), PYTHON_TARGETS.len());
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
