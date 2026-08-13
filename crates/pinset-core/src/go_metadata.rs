use std::{cmp::Reverse, collections::BTreeMap, io::Read, time::Duration};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    Error, GO_TARGETS, GoArchiveFormat, LockedArtifact, LockedArtifactFormat, LockedTool, Result,
    SourceConfig, plan_go_artifact, validate_exact_go_version,
};

const OFFICIAL_GO_DOWNLOAD_URL: &str = "https://go.dev/dl/";
const MAX_GO_INDEX_BYTES: u64 = 32 * 1024 * 1024;
const GO_VERIFICATION: &str = "go-download-json-sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoRelease {
    pub version: String,
}

#[derive(Debug)]
pub struct GoMetadataClient {
    client: Client,
    metadata_base_url: Url,
    verification: String,
}

#[derive(Debug, Deserialize)]
struct GoIndexRelease {
    version: String,
    stable: bool,
    #[serde(default)]
    files: Vec<GoIndexFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoIndexFile {
    filename: String,
    os: String,
    arch: String,
    version: String,
    sha256: String,
    size: u64,
    kind: String,
}

#[derive(Debug)]
struct SupportedGoRelease {
    version: String,
    files: Vec<(String, GoIndexFile)>,
}

impl GoMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_source(OFFICIAL_GO_DOWNLOAD_URL, "official")
    }

    pub fn for_base_url(base_url: &str) -> Result<Self> {
        Self::for_source(base_url, "custom")
    }

    pub fn for_source(base_url: &str, alias: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let metadata_base_url =
            Url::parse(base_url).map_err(|source| Error::InvalidSourceBaseUrl {
                url: base_url.to_owned(),
                reason: source.to_string(),
            })?;
        let verification = if alias == "official" {
            GO_VERIFICATION.to_owned()
        } else {
            format!("{GO_VERIFICATION}-source:{alias}")
        };
        Ok(Self {
            client,
            metadata_base_url,
            verification,
        })
    }

    pub fn available_releases(&self) -> Result<Vec<GoRelease>> {
        Ok(self
            .supported_releases()?
            .into_iter()
            .map(|release| GoRelease {
                version: release.version,
            })
            .collect())
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(self.resolve_release(selector)?.version)
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let release = self.resolve_release(selector)?;
        let artifacts = release
            .files
            .into_iter()
            .map(|(target, file)| {
                let plan = plan_go_artifact(&SourceConfig::default(), &release.version, &target)?;
                Ok(LockedArtifact {
                    target,
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: file.sha256,
                    integrity: None,
                    format: match plan.format {
                        GoArchiveFormat::Zip => LockedArtifactFormat::Zip,
                        GoArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
                    },
                    archive_root: plan.archive_root,
                    verification: self.verification.clone(),
                    overlays: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(LockedTool {
            name: "go".to_owned(),
            requested: release.version.clone(),
            version: release.version,
            provider: "go-official".to_owned(),
            metadata: BTreeMap::new(),
            artifacts,
        })
    }

    fn resolve_release(&self, selector: &str) -> Result<SupportedGoRelease> {
        let normalized = selector.trim().to_ascii_lowercase();
        let exact = validate_exact_go_version(&normalized).is_ok();
        let numeric_parts = normalized.split('.').collect::<Vec<_>>();
        let numeric_selector = (numeric_parts.len() == 1 || numeric_parts.len() == 2)
            && numeric_parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
        if normalized != "latest" && normalized != "current" && !exact && !numeric_selector {
            return Err(Error::InvalidGoSelector {
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
            .map_err(|_| Error::InvalidGoSelector {
                selector: selector.to_owned(),
            })?;

        self.supported_releases()?
            .into_iter()
            .find(|release| {
                if normalized == "latest" || normalized == "current" {
                    return true;
                }
                if exact {
                    return release.version == normalized;
                }
                let tuple = version_tuple(&release.version).expect("validated Go release");
                let requested = requested.as_ref().expect("numeric selector");
                tuple.0 == requested[0] && (requested.len() == 1 || tuple.1 == requested[1])
            })
            .ok_or_else(|| Error::GoSelectorNotFound {
                selector: selector.to_owned(),
            })
    }

    fn supported_releases(&self) -> Result<Vec<SupportedGoRelease>> {
        let body = self.download_index()?;
        parse_index(&body)
    }

    fn download_index(&self) -> Result<String> {
        let mut url = self.metadata_base_url.clone();
        url.set_query(Some("mode=json&include=all"));
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::GoMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_GO_INDEX_BYTES)
        {
            return Err(Error::GoMetadataTooLarge {
                limit: MAX_GO_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_GO_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::GoMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_GO_INDEX_BYTES {
            return Err(Error::GoMetadataTooLarge {
                limit: MAX_GO_INDEX_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidGoIndex {
            reason: "index is not UTF-8".to_owned(),
        })
    }
}

fn parse_index(body: &str) -> Result<Vec<SupportedGoRelease>> {
    let entries: Vec<GoIndexRelease> =
        serde_json::from_str(body).map_err(|source| Error::InvalidGoIndex {
            reason: source.to_string(),
        })?;
    let mut releases = Vec::new();
    for entry in entries.into_iter().filter(|entry| entry.stable) {
        let Some(version) = normalized_release_version(&entry.version) else {
            continue;
        };
        let mut files = Vec::with_capacity(GO_TARGETS.len());
        let mut supported = true;
        for target in GO_TARGETS {
            let plan = plan_go_artifact(&SourceConfig::default(), &version, target)?;
            let matching = entry.files.iter().find(|file| {
                file.kind == "archive"
                    && file.os == plan.os
                    && file.arch == plan.arch
                    && file.version == entry.version
                    && file.filename == plan.artifact_path
                    && file.size > 0
                    && valid_sha256(&file.sha256)
            });
            let Some(file) = matching else {
                supported = false;
                break;
            };
            files.push((target.to_owned(), file.clone()));
        }
        if supported {
            releases.push(SupportedGoRelease { version, files });
        }
    }
    releases.sort_by_key(|release| Reverse(version_tuple(&release.version).unwrap_or_default()));
    releases.dedup_by(|left, right| left.version == right.version);
    Ok(releases)
}

fn normalized_release_version(value: &str) -> Option<String> {
    let value = value.strip_prefix("go")?;
    let parts = value.split('.').collect::<Vec<_>>();
    if !(parts.len() == 2 || parts.len() == 3)
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let normalized = if parts.len() == 2 {
        format!("{value}.0")
    } else {
        value.to_owned()
    };
    validate_exact_go_version(&normalized).ok()?;
    Some(normalized)
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

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[test]
    fn parses_only_stable_releases_with_every_pinset_target() {
        let body = go_index_fixture("go1.25.1", true, true);
        let releases = parse_index(&body).expect("index");
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version, "1.25.1");
        assert_eq!(releases[0].files.len(), GO_TARGETS.len());

        let incomplete = go_index_fixture("go1.25.1", true, false);
        assert!(
            parse_index(&incomplete)
                .expect("incomplete index")
                .is_empty()
        );
        let unstable = go_index_fixture("go1.26rc1", false, true);
        assert!(parse_index(&unstable).expect("unstable index").is_empty());
    }

    #[test]
    fn normalizes_historical_patch_zero_versions() {
        assert_eq!(
            normalized_release_version("go1.20").as_deref(),
            Some("1.20.0")
        );
        assert_eq!(
            normalized_release_version("go1.21.0").as_deref(),
            Some("1.21.0")
        );
        assert!(normalized_release_version("go1.26rc1").is_none());
    }

    #[test]
    fn resolves_a_numeric_selector_to_a_sha256_locked_tool() {
        let body = go_index_fixture("go1.25.1", true, true).into_bytes();
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0_u8; 2048];
            let count = stream.read(&mut request).expect("read request");
            assert!(
                String::from_utf8_lossy(&request[..count]).contains("GET /?mode=json&include=all")
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("headers");
            stream.write_all(&body).expect("body");
        });
        let client =
            GoMetadataClient::for_base_url(&format!("http://{address}/")).expect("metadata client");
        let locked = client.resolve_tool("1.25").expect("locked Go");
        server.join().expect("server");

        assert_eq!(locked.name, "go");
        assert_eq!(locked.version, "1.25.1");
        assert_eq!(locked.provider, "go-official");
        assert_eq!(locked.artifacts.len(), GO_TARGETS.len());
        assert!(locked.artifacts.iter().all(|artifact| {
            artifact.sha256 == "ab".repeat(32)
                && artifact.verification == "go-download-json-sha256-source:custom"
        }));
    }

    fn go_index_fixture(version: &str, stable: bool, complete: bool) -> String {
        let normalized = normalized_release_version(version).unwrap_or_else(|| "1.25.1".to_owned());
        let targets: &[&str] = if complete {
            &GO_TARGETS
        } else {
            &GO_TARGETS[..GO_TARGETS.len() - 1]
        };
        let files = targets
            .iter()
            .map(|target| {
                let plan = plan_go_artifact(&SourceConfig::default(), &normalized, target)
                    .expect("artifact plan");
                serde_json::json!({
                    "filename": plan.artifact_path,
                    "os": plan.os,
                    "arch": plan.arch,
                    "version": version,
                    "sha256": "ab".repeat(32),
                    "size": 123,
                    "kind": "archive"
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!([{"version": version, "stable": stable, "files": files}]).to_string()
    }
}
