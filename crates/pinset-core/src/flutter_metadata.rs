use std::{cmp::Reverse, collections::BTreeMap, io::Read, time::Duration};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    Error, FLUTTER_TARGETS, FlutterArchiveFormat, LockedArtifact, LockedArtifactFormat, LockedTool,
    Result, SourceConfig, plan_flutter_artifact, validate_exact_flutter_version,
};

const OFFICIAL_FLUTTER_BASE_URL: &str = "https://storage.googleapis.com/";
const MAX_FLUTTER_INDEX_BYTES: u64 = 8 * 1024 * 1024;
const FLUTTER_VERIFICATION: &str = "flutter-release-json-sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterRelease {
    pub version: String,
    pub dart_version: String,
    pub release_hash: String,
}

#[derive(Debug)]
pub struct FlutterMetadataClient {
    client: Client,
    metadata_base_url: Url,
    verification: String,
}

#[derive(Debug, Deserialize)]
struct FlutterIndex {
    #[serde(default)]
    releases: Vec<FlutterIndexRelease>,
}

#[derive(Debug, Clone, Deserialize)]
struct FlutterIndexRelease {
    hash: String,
    channel: String,
    version: String,
    dart_sdk_version: String,
    #[serde(default)]
    dart_sdk_arch: String,
    archive: String,
    sha256: String,
}

#[derive(Debug)]
struct SupportedFlutterRelease {
    version: String,
    dart_version: String,
    release_hash: String,
    artifacts: Vec<(String, FlutterIndexRelease)>,
}

impl FlutterMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_source(OFFICIAL_FLUTTER_BASE_URL, "official")
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
            FLUTTER_VERIFICATION.to_owned()
        } else {
            format!("{FLUTTER_VERIFICATION}-source:{alias}")
        };
        Ok(Self {
            client,
            metadata_base_url,
            verification,
        })
    }

    pub fn available_releases(&self) -> Result<Vec<FlutterRelease>> {
        Ok(self
            .supported_releases()?
            .into_iter()
            .map(|release| FlutterRelease {
                version: release.version,
                dart_version: release.dart_version,
                release_hash: release.release_hash,
            })
            .collect())
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(self.resolve_release(selector)?.version)
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let release = self.resolve_release(selector)?;
        let artifacts = release
            .artifacts
            .into_iter()
            .map(|(target, artifact)| {
                let plan =
                    plan_flutter_artifact(&SourceConfig::default(), &release.version, &target)?;
                Ok(LockedArtifact {
                    target,
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: artifact.sha256,
                    integrity: None,
                    format: match plan.format {
                        FlutterArchiveFormat::Zip => LockedArtifactFormat::Zip,
                        FlutterArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                    },
                    archive_root: plan.archive_root,
                    verification: self.verification.clone(),
                    overlays: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut metadata = BTreeMap::new();
        metadata.insert("channel".to_owned(), "stable".to_owned());
        metadata.insert("dart_version".to_owned(), release.dart_version);
        metadata.insert("release_hash".to_owned(), release.release_hash);
        Ok(LockedTool {
            name: "flutter".to_owned(),
            requested: release.version.clone(),
            version: release.version,
            provider: "flutter-official".to_owned(),
            metadata,
            artifacts,
        })
    }

    fn resolve_release(&self, selector: &str) -> Result<SupportedFlutterRelease> {
        let normalized = selector.trim().to_ascii_lowercase();
        let exact = validate_exact_flutter_version(&normalized).is_ok();
        let numeric_parts = normalized.split('.').collect::<Vec<_>>();
        let numeric_selector = (numeric_parts.len() == 1 || numeric_parts.len() == 2)
            && numeric_parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
        if normalized != "latest" && normalized != "current" && !exact && !numeric_selector {
            return Err(Error::InvalidFlutterSelector {
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
            .map_err(|_| Error::InvalidFlutterSelector {
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
                let tuple = version_tuple(&release.version).expect("validated Flutter release");
                let requested = requested.as_ref().expect("numeric selector");
                tuple.0 == requested[0] && (requested.len() == 1 || tuple.1 == requested[1])
            })
            .ok_or_else(|| Error::FlutterSelectorNotFound {
                selector: selector.to_owned(),
            })
    }

    fn supported_releases(&self) -> Result<Vec<SupportedFlutterRelease>> {
        let linux = self.download_index("linux")?;
        let windows = self.download_index("windows")?;
        let macos = self.download_index("macos")?;
        parse_indexes(&linux, &windows, &macos)
    }

    fn download_index(&self, platform: &str) -> Result<String> {
        let url = self
            .metadata_base_url
            .join(&format!(
                "flutter_infra_release/releases/releases_{platform}.json"
            ))
            .map_err(|source| Error::InvalidSourceBaseUrl {
                url: self.metadata_base_url.to_string(),
                reason: source.to_string(),
            })?;
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::FlutterMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_FLUTTER_INDEX_BYTES)
        {
            return Err(Error::FlutterMetadataTooLarge {
                limit: MAX_FLUTTER_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_FLUTTER_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::FlutterMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_FLUTTER_INDEX_BYTES {
            return Err(Error::FlutterMetadataTooLarge {
                limit: MAX_FLUTTER_INDEX_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidFlutterIndex {
            reason: format!("{platform} index is not UTF-8"),
        })
    }
}

fn parse_indexes(
    linux_body: &str,
    windows_body: &str,
    macos_body: &str,
) -> Result<Vec<SupportedFlutterRelease>> {
    let linux = parse_index(linux_body, "linux", &["linux-x86_64"])?;
    let windows = parse_index(windows_body, "windows", &["windows-x86_64"])?;
    let macos = parse_index(macos_body, "macos", &["macos-x86_64", "macos-aarch64"])?;
    let mut grouped: BTreeMap<String, BTreeMap<String, FlutterIndexRelease>> = BTreeMap::new();
    for (target, releases) in [linux, windows, macos].into_iter().flatten() {
        for release in releases {
            grouped
                .entry(release.version.clone())
                .or_default()
                .insert(target.clone(), release);
        }
    }

    let mut supported = Vec::new();
    for (version, mut artifacts) in grouped {
        if !FLUTTER_TARGETS
            .iter()
            .all(|target| artifacts.contains_key(*target))
        {
            continue;
        }
        let first = artifacts
            .values()
            .next()
            .expect("four required Flutter artifacts");
        if artifacts.values().any(|artifact| {
            artifact.hash != first.hash || artifact.dart_sdk_version != first.dart_sdk_version
        }) {
            continue;
        }
        let release_hash = first.hash.clone();
        let dart_version = first.dart_sdk_version.clone();
        let artifacts = FLUTTER_TARGETS
            .iter()
            .map(|target| {
                (
                    (*target).to_owned(),
                    artifacts.remove(*target).expect("checked target"),
                )
            })
            .collect();
        supported.push(SupportedFlutterRelease {
            version,
            dart_version,
            release_hash,
            artifacts,
        });
    }
    supported.sort_by_key(|release| Reverse(version_tuple(&release.version).unwrap_or_default()));
    Ok(supported)
}

fn parse_index(
    body: &str,
    platform: &str,
    targets: &[&str],
) -> Result<Vec<(String, Vec<FlutterIndexRelease>)>> {
    let index: FlutterIndex =
        serde_json::from_str(body).map_err(|source| Error::InvalidFlutterIndex {
            reason: format!("{platform}: {source}"),
        })?;
    Ok(targets
        .iter()
        .map(|target| {
            let releases = index
                .releases
                .iter()
                .filter(|release| release.channel == "stable")
                .filter(|release| validate_exact_flutter_version(&release.version).is_ok())
                .filter(|release| valid_release_hash(&release.hash))
                .filter(|release| valid_sha256(&release.sha256))
                .filter(|release| validate_exact_flutter_version(&release.dart_sdk_version).is_ok())
                .filter(|release| {
                    plan_flutter_artifact(&SourceConfig::default(), &release.version, target)
                        .is_ok_and(|plan| {
                            release.archive == plan.release_path
                                && (release.dart_sdk_arch.is_empty()
                                    || release.dart_sdk_arch == plan.dart_arch)
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            ((*target).to_owned(), releases)
        })
        .collect())
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

fn valid_release_hash(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    fn combines_only_stable_releases_available_for_all_targets() {
        let (linux, windows, macos) = release_fixtures("3.47.0", true);
        let releases = parse_indexes(&linux, &windows, &macos).expect("indexes");
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version, "3.47.0");
        assert_eq!(releases[0].dart_version, "3.13.0");
        assert_eq!(releases[0].artifacts.len(), FLUTTER_TARGETS.len());

        let (linux, windows, macos) = release_fixtures("3.47.0", false);
        assert!(
            parse_indexes(&linux, &windows, &macos)
                .expect("incomplete indexes")
                .is_empty()
        );
    }

    #[test]
    fn resolves_a_numeric_selector_to_flutter_and_bundled_dart_metadata() {
        let (linux, windows, macos) = release_fixtures("3.47.0", true);
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            for (expected, body) in [
                ("releases_linux.json", linux),
                ("releases_windows.json", windows),
                ("releases_macos.json", macos),
            ] {
                let (mut stream, _) = listener.accept().expect("request");
                let mut request = [0_u8; 2048];
                let count = stream.read(&mut request).expect("read request");
                assert!(String::from_utf8_lossy(&request[..count]).contains(expected));
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .expect("headers");
                stream.write_all(body.as_bytes()).expect("body");
            }
        });
        let client = FlutterMetadataClient::for_base_url(&format!("http://{address}/"))
            .expect("metadata client");
        let locked = client.resolve_tool("3.47").expect("locked Flutter");
        server.join().expect("server");

        assert_eq!(locked.version, "3.47.0");
        assert_eq!(locked.metadata["channel"], "stable");
        assert_eq!(locked.metadata["dart_version"], "3.13.0");
        assert_eq!(locked.artifacts.len(), FLUTTER_TARGETS.len());
        assert!(locked.artifacts.iter().all(|artifact| {
            artifact.sha256 == "ab".repeat(32)
                && artifact.verification == "flutter-release-json-sha256-source:custom"
        }));
    }

    fn release_fixtures(version: &str, complete: bool) -> (String, String, String) {
        let release_hash = "cd".repeat(20);
        let sha256 = "ab".repeat(32);
        let entry = |target: &str| {
            let plan = plan_flutter_artifact(&SourceConfig::default(), version, target)
                .expect("artifact plan");
            serde_json::json!({
                "hash": release_hash,
                "channel": "stable",
                "version": version,
                "dart_sdk_version": "3.13.0",
                "dart_sdk_arch": plan.dart_arch,
                "archive": plan.release_path,
                "sha256": sha256
            })
        };
        let linux = serde_json::json!({"releases": [entry("linux-x86_64")]}).to_string();
        let windows = serde_json::json!({"releases": [entry("windows-x86_64")]}).to_string();
        let macos_entries = if complete {
            vec![entry("macos-x86_64"), entry("macos-aarch64")]
        } else {
            vec![entry("macos-x86_64")]
        };
        let macos = serde_json::json!({"releases": macos_entries}).to_string();
        (linux, windows, macos)
    }
}
