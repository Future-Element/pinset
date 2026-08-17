use std::{cmp::Reverse, collections::HashMap, io::Read, time::Duration};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    Error, LockedArtifact, LockedArtifactFormat, Lockfile, MVP_NODE_TARGETS, NodeArchiveFormat,
    Result, SourceConfig, node_trust::verify_node_manifest, plan_node_artifact,
};

const OFFICIAL_NODE_DIST_URL: &str = "https://nodejs.org/dist/";
const MAX_SHASUMS_BYTES: u64 = 1024 * 1024;
const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const REQUIRED_INDEX_FILES: [&str; 5] = [
    "win-x64-zip",
    "linux-x64",
    "linux-arm64",
    "osx-x64-tar",
    "osx-arm64-tar",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRelease {
    pub version: String,
    pub date: String,
    pub lts: Option<String>,
    pub security: bool,
}

#[derive(Debug, Deserialize)]
struct NodeIndexEntry {
    version: String,
    date: String,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    lts: serde_json::Value,
    #[serde(default)]
    security: bool,
}

#[derive(Debug)]
pub struct NodeMetadataClient {
    client: Client,
    metadata_base_url: Url,
    verification: String,
    manifest_source: String,
}

impl NodeMetadataClient {
    pub fn official() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        Ok(Self {
            client,
            metadata_base_url: Url::parse(OFFICIAL_NODE_DIST_URL)
                .expect("built-in Node distribution URL is valid"),
            verification: "nodejs-openpgp-sha256".to_owned(),
            manifest_source: "official".to_owned(),
        })
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
        Ok(Self {
            client,
            metadata_base_url,
            verification: format!("nodejs-openpgp-sha256-source:{alias}"),
            manifest_source: alias.to_owned(),
        })
    }

    pub fn resolve_exact_lock(&self, version: &str, generated_by: &str) -> Result<Lockfile> {
        crate::validate_exact_node_version(version)?;
        let manifest_url = self
            .metadata_base_url
            .join(&format!("v{version}/SHASUMS256.txt.asc"))
            .expect("validated exact version produces a safe relative URL");
        let signed_manifest = self.download_shasums(manifest_url)?;
        // INVARIANT: Never parse attacker-controlled checksums before authenticating the exact
        // clear-signed bytes supplied by the selected trusted metadata source.
        let verified = verify_node_manifest(&signed_manifest)?;
        self.lock_from_verified_manifest(
            version,
            generated_by,
            &verified.text,
            &verified.signer_fingerprint,
        )
    }

    fn lock_from_verified_manifest(
        &self,
        version: &str,
        generated_by: &str,
        manifest: &str,
        signer_fingerprint: &str,
    ) -> Result<Lockfile> {
        let plans = MVP_NODE_TARGETS
            .into_iter()
            .map(|target| plan_node_artifact(&SourceConfig::default(), version, target))
            .collect::<Result<Vec<_>>>()?;
        let checksums = parse_shasums(manifest)?;
        let artifacts =
            plans
                .into_iter()
                .map(|plan| {
                    let filename = plan
                        .artifact_path
                        .rsplit('/')
                        .next()
                        .expect("artifact path contains a filename");
                    let sha256 = checksums.get(filename).cloned().ok_or_else(|| {
                        Error::NodeChecksumMissing {
                            version: version.to_owned(),
                            filename: filename.to_owned(),
                        }
                    })?;
                    Ok(LockedArtifact {
                        target: plan.target,
                        canonical_url: plan.canonical_url,
                        artifact_path: plan.artifact_path,
                        sha256,
                        integrity: None,
                        format: match plan.format {
                            NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                            NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                        },
                        archive_root: plan.archive_root,
                        verification: self.verification.clone(),
                        overlays: Vec::new(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;

        let lockfile = Lockfile::new_node(
            generated_by.to_owned(),
            version.to_owned(),
            signer_fingerprint.to_owned(),
            self.manifest_source.clone(),
            artifacts,
        );
        Ok(lockfile)
    }

    pub fn resolve_lock(&self, selector: &str, generated_by: &str) -> Result<Lockfile> {
        let version = self.resolve_version_selector(selector)?;
        self.resolve_exact_lock(&version, generated_by)
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        if crate::validate_exact_node_version(selector).is_ok() {
            return Ok(selector.to_owned());
        }

        let normalized = selector.trim().to_ascii_lowercase();
        let numeric_parts = normalized.split('.').collect::<Vec<_>>();
        let numeric_selector = (numeric_parts.len() == 1 || numeric_parts.len() == 2)
            && numeric_parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
        if normalized != "current" && normalized != "lts" && !numeric_selector {
            return Err(Error::InvalidNodeSelector {
                selector: selector.to_owned(),
            });
        }

        let requested_numbers = numeric_selector
            .then(|| {
                numeric_parts
                    .iter()
                    .map(|part| part.parse::<u64>())
                    .collect::<std::result::Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(|_| Error::InvalidNodeSelector {
                selector: selector.to_owned(),
            })?;
        self.available_releases()?
            .into_iter()
            .find(|release| {
                if normalized == "current" {
                    return true;
                }
                if normalized == "lts" {
                    return release.lts.is_some();
                }
                let tuple = parse_version_tuple(&release.version)
                    .expect("available releases contain validated versions");
                let requested = requested_numbers
                    .as_ref()
                    .expect("numeric selector has parsed parts");
                tuple.0 == requested[0] && (requested.len() == 1 || tuple.1 == requested[1])
            })
            .map(|release| release.version)
            .ok_or_else(|| Error::NodeSelectorNotFound {
                selector: selector.to_owned(),
            })
    }

    pub fn available_releases(&self) -> Result<Vec<NodeRelease>> {
        let index_url = self
            .metadata_base_url
            .join("index.json")
            .expect("built-in Node index path is valid");
        let body = self.download_index(index_url)?;
        parse_index(&body)
    }

    fn download_shasums(&self, url: Url) -> Result<String> {
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::NodeMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_SHASUMS_BYTES)
        {
            return Err(Error::NodeMetadataTooLarge {
                limit: MAX_SHASUMS_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_SHASUMS_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::NodeMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_SHASUMS_BYTES {
            return Err(Error::NodeMetadataTooLarge {
                limit: MAX_SHASUMS_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidNodeShasums {
            reason: "manifest is not UTF-8".to_owned(),
        })
    }

    fn download_index(&self, url: Url) -> Result<String> {
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::NodeMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_INDEX_BYTES)
        {
            return Err(Error::NodeIndexTooLarge {
                limit: MAX_INDEX_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::NodeMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_INDEX_BYTES {
            return Err(Error::NodeIndexTooLarge {
                limit: MAX_INDEX_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidNodeIndex {
            reason: "index is not UTF-8".to_owned(),
        })
    }
}

fn parse_index(body: &str) -> Result<Vec<NodeRelease>> {
    let entries = serde_json::from_str::<Vec<NodeIndexEntry>>(body).map_err(|source| {
        Error::InvalidNodeIndex {
            reason: source.to_string(),
        }
    })?;
    let mut releases = Vec::new();
    for entry in entries {
        let version = entry
            .version
            .strip_prefix('v')
            .ok_or_else(|| Error::InvalidNodeIndex {
                reason: format!("version {:?} does not start with v", entry.version),
            })?;
        let Some(tuple) = parse_version_tuple(version) else {
            continue;
        };
        if !REQUIRED_INDEX_FILES
            .iter()
            .all(|required| entry.files.iter().any(|file| file == required))
        {
            continue;
        }
        let lts = match entry.lts {
            serde_json::Value::Bool(false) | serde_json::Value::Null => None,
            serde_json::Value::String(name) if !name.trim().is_empty() => Some(name),
            value => {
                return Err(Error::InvalidNodeIndex {
                    reason: format!("invalid LTS value for v{version}: {value}"),
                });
            }
        };
        releases.push((
            tuple,
            NodeRelease {
                version: version.to_owned(),
                date: entry.date,
                lts,
                security: entry.security,
            },
        ));
    }
    releases.sort_by_key(|entry| Reverse(entry.0));
    if releases.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::InvalidNodeIndex {
            reason: "index contains a duplicate stable version".to_owned(),
        });
    }
    if releases.is_empty() {
        return Err(Error::InvalidNodeIndex {
            reason: "index contains no stable releases for every supported Pinset target"
                .to_owned(),
        });
    }
    Ok(releases.into_iter().map(|(_, release)| release).collect())
}

fn parse_version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn parse_shasums(manifest: &str) -> Result<HashMap<String, String>> {
    let mut checksums = HashMap::new();
    for (index, line) in manifest.lines().enumerate() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2
            || parts[0].len() != 64
            || !parts[0].bytes().all(|byte| byte.is_ascii_hexdigit())
            || !is_safe_manifest_path(parts[1])
        {
            return Err(Error::InvalidNodeShasums {
                reason: format!("invalid line {}", index + 1),
            });
        }
        if checksums
            .insert(parts[1].to_owned(), parts[0].to_ascii_lowercase())
            .is_some()
        {
            return Err(Error::InvalidNodeShasums {
                reason: format!("duplicate filename {}", parts[1]),
            });
        }
    }
    if checksums.is_empty() {
        return Err(Error::InvalidNodeShasums {
            reason: "manifest is empty".to_owned(),
        });
    }
    Ok(checksums)
}

fn is_safe_manifest_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        thread,
    };

    use super::*;

    const REQUIRED_FILES_JSON: &str =
        r#"["win-x64-zip","linux-x64","linux-arm64","osx-x64-tar","osx-arm64-tar"]"#;

    #[test]
    fn resolves_floating_selectors_from_supported_stable_releases() {
        let index = format!(
            r#"[
                {{"version":"v24.1.5","date":"2026-01-01","files":{REQUIRED_FILES_JSON},"lts":"Krypton","security":true}},
                {{"version":"v26.1.0","date":"2026-05-01","files":{REQUIRED_FILES_JSON},"lts":false,"security":false}},
                {{"version":"v24.2.0","date":"2026-02-01","files":{REQUIRED_FILES_JSON},"lts":"Krypton","security":false}},
                {{"version":"v26.0.0-rc.1","date":"2026-04-01","files":{REQUIRED_FILES_JSON},"lts":false,"security":false}},
                {{"version":"v25.9.0","date":"2026-03-01","files":["linux-x64"],"lts":false,"security":false}}
            ]"#
        );

        for (selector, expected) in [
            ("current", "26.1.0"),
            ("lts", "24.2.0"),
            ("24", "24.2.0"),
            ("24.1", "24.1.5"),
        ] {
            let (base_url, server) = serve_once(index.clone());
            let client = test_client(&base_url);
            assert_eq!(
                client.resolve_version_selector(selector).expect("selector"),
                expected
            );
            server.join().expect("server");
        }
    }

    #[test]
    fn exact_selectors_remain_offline_and_invalid_or_missing_selectors_fail() {
        let client = test_client("http://127.0.0.1:9/");
        assert_eq!(
            client
                .resolve_version_selector("24.0.0")
                .expect("exact selector"),
            "24.0.0"
        );
        assert!(matches!(
            client.resolve_version_selector("v24"),
            Err(Error::InvalidNodeSelector { .. })
        ));

        let index = format!(
            r#"[{{"version":"v24.2.0","date":"2026-02-01","files":{REQUIRED_FILES_JSON},"lts":"Krypton","security":false}}]"#
        );
        let (base_url, server) = serve_once(index);
        let error = test_client(&base_url)
            .resolve_version_selector("22")
            .expect_err("missing selector");
        server.join().expect("server");
        assert!(matches!(error, Error::NodeSelectorNotFound { .. }));
    }

    #[test]
    fn available_releases_are_sorted_and_expose_lts_and_security_metadata() {
        let index = format!(
            r#"[
                {{"version":"v22.9.0","date":"2025-10-01","files":{REQUIRED_FILES_JSON},"lts":"Jod","security":true,"future_field":"ignored"}},
                {{"version":"v24.0.0","date":"2026-01-01","files":{REQUIRED_FILES_JSON},"lts":false,"security":false}}
            ]"#
        );
        let releases = parse_index(&index).expect("index");
        assert_eq!(releases[0].version, "24.0.0");
        assert_eq!(releases[1].lts.as_deref(), Some("Jod"));
        assert!(releases[1].security);
    }

    #[test]
    fn resolves_all_mvp_targets_from_official_style_shasums() {
        let manifest = MVP_NODE_TARGETS
            .into_iter()
            .enumerate()
            .map(|(index, target)| {
                let plan =
                    plan_node_artifact(&SourceConfig::default(), "24.0.0", target).expect("plan");
                let filename = plan.artifact_path.rsplit('/').next().expect("filename");
                format!("{}  {filename}", format!("{:x}", index + 1).repeat(64))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let client = test_client("http://127.0.0.1:9/");
        let lockfile = client
            .lock_from_verified_manifest(
                "24.0.0",
                "pinset test",
                &manifest,
                "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356",
            )
            .expect("resolve lock");

        let node = lockfile.tool("node").expect("node lock");
        assert_eq!(node.artifacts.len(), MVP_NODE_TARGETS.len());
        assert_eq!(
            node.metadata.get("signed_manifest").map(String::as_str),
            Some("SHASUMS256.txt.asc")
        );
        assert_eq!(
            node.metadata.get("manifest_source").map(String::as_str),
            Some("test")
        );
        assert!(node.artifacts.iter().all(|artifact| {
            artifact
                .canonical_url
                .starts_with("https://nodejs.org/dist/v24.0.0/")
        }));
    }

    #[test]
    fn trusted_metadata_mirror_must_serve_a_valid_signed_manifest() {
        let (base_url, server) = serve_once(
            include_str!("../tests/fixtures/node-v24.19.0-SHASUMS256.txt.asc").to_owned(),
        );
        let client = NodeMetadataClient::for_source(&base_url, "mirror").expect("mirror client");
        let lockfile = client
            .resolve_exact_lock("24.19.0", "pinset test")
            .expect("signed mirror manifest");
        server.join().expect("server");

        let node = lockfile.tool("node").expect("node lock");
        assert_eq!(
            node.metadata.get("manifest_source").map(String::as_str),
            Some("mirror")
        );
        assert!(
            node.artifacts
                .iter()
                .all(|artifact| { artifact.verification == "nodejs-openpgp-sha256-source:mirror" })
        );
    }

    #[test]
    fn rejects_a_signed_manifest_response_over_the_fixed_limit() {
        let (base_url, server) = serve_once("x".repeat(MAX_SHASUMS_BYTES as usize + 1));
        let client = test_client(&base_url);
        let error = client
            .download_shasums(Url::parse(&base_url).expect("manifest URL"))
            .expect_err("oversized manifest");
        server.join().expect("server");
        assert!(matches!(error, Error::NodeMetadataTooLarge { .. }));
    }

    #[test]
    fn rejects_invalid_or_incomplete_shasums() {
        assert!(matches!(
            parse_shasums("not-a-hash  node.zip"),
            Err(Error::InvalidNodeShasums { .. })
        ));
        assert!(matches!(
            parse_shasums(&format!("{}  ../node.zip", "a".repeat(64))),
            Err(Error::InvalidNodeShasums { .. })
        ));

        let client = test_client("http://127.0.0.1:9/");
        let error = client
            .lock_from_verified_manifest(
                "24.0.0",
                "pinset test",
                &format!("{}  unrelated.zip", "a".repeat(64)),
                "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356",
            )
            .expect_err("missing target checksum");
        assert!(matches!(error, Error::NodeChecksumMissing { .. }));
    }

    #[test]
    fn accepts_safe_nested_paths_used_by_official_windows_entries() {
        let hash = "a".repeat(64);
        let checksums = parse_shasums(&format!(
            "{hash}  node-v24.0.0-win-x64.zip\n{hash}  win-x64/node.exe"
        ))
        .expect("official manifest paths");

        assert_eq!(
            checksums.get("win-x64/node.exe").map(String::as_str),
            Some(hash.as_str())
        );
        for unsafe_path in [
            "../node.exe",
            "win-x64/../node.exe",
            "/win-x64/node.exe",
            "win-x64\\node.exe",
            "win-x64//node.exe",
        ] {
            assert!(matches!(
                parse_shasums(&format!("{hash}  {unsafe_path}")),
                Err(Error::InvalidNodeShasums { .. })
            ));
        }
    }

    fn serve_once(body: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("response");
        });
        (format!("http://{address}/"), handle)
    }

    fn test_client(base_url: &str) -> NodeMetadataClient {
        NodeMetadataClient {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            metadata_base_url: Url::parse(base_url).expect("base URL"),
            verification: "nodejs-openpgp-sha256-source:test".to_owned(),
            manifest_source: "test".to_owned(),
        }
    }
}
