use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    io::Read,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    Error, LockedArtifact, LockedArtifactFormat, LockedTool, RUST_COMPONENTS, RUST_PROFILE,
    RUST_TARGETS, Result, RustArchiveFormat, RustVersion, plan_rust_artifact, rust_target_triple,
};

const OFFICIAL_RUST_BASE_URL: &str = "https://static.rust-lang.org/";
const RUST_MANIFESTS_PATH: &str = "manifests.txt";
const MAX_RUST_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RUST_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const RUST_VERIFICATION: &str = "rust-v2-manifest-sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustRelease {
    pub version: String,
    pub date: String,
}

#[derive(Debug)]
pub struct RustMetadataClient {
    client: Client,
    base_url: Url,
}

#[derive(Debug, Deserialize)]
struct ChannelManifest {
    #[serde(rename = "manifest-version")]
    manifest_version: String,
    date: String,
    pkg: BTreeMap<String, ManifestPackage>,
    #[serde(default)]
    profiles: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ManifestPackage {
    version: String,
    #[serde(default)]
    target: BTreeMap<String, ManifestTarget>,
}

#[derive(Debug, Deserialize)]
struct ManifestTarget {
    available: bool,
    #[serde(default)]
    xz_url: Option<String>,
    #[serde(default)]
    xz_hash: Option<String>,
}

impl RustMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_base_url(OFFICIAL_RUST_BASE_URL)
    }

    pub fn for_base_url(base_url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let base_url = Url::parse(base_url).map_err(|source| Error::InvalidSourceBaseUrl {
            url: base_url.to_owned(),
            reason: source.to_string(),
        })?;
        if base_url.cannot_be_a_base() {
            return Err(Error::InvalidSourceBaseUrl {
                url: base_url.to_string(),
                reason: "Rust distribution URL must be a hierarchical base URL".to_owned(),
            });
        }
        Ok(Self { client, base_url })
    }

    pub fn available_releases(&self) -> Result<Vec<RustRelease>> {
        let url = self
            .base_url
            .join(RUST_MANIFESTS_PATH)
            .expect("known Rust manifests path");
        let body = self.download_text(&url, MAX_RUST_INDEX_BYTES)?;
        parse_available_releases(&body)
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(select_release(self.available_releases()?, selector)?.version)
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let selected = select_release(self.available_releases()?, selector)?;
        let manifest_path = format!("dist/channel-rust-{}.toml", selected.version);
        let manifest_url = self
            .base_url
            .join(&manifest_path)
            .expect("validated Rust manifest path");
        let checksum_url = self
            .base_url
            .join(&format!("{manifest_path}.sha256"))
            .expect("validated Rust checksum path");
        let manifest = self.download_bytes(&manifest_url, MAX_RUST_MANIFEST_BYTES)?;
        let checksum = self.download_text(&checksum_url, 1024)?;
        let expected_checksum = parse_sha256_document(&checksum)?;
        let actual_checksum = hex_sha256(&manifest);
        if actual_checksum != expected_checksum {
            return Err(Error::InvalidRustIndex {
                reason: format!(
                    "manifest checksum mismatch: expected {expected_checksum}, got {actual_checksum}"
                ),
            });
        }
        let manifest_text =
            std::str::from_utf8(&manifest).map_err(|_| Error::InvalidRustIndex {
                reason: "release manifest is not UTF-8".to_owned(),
            })?;
        let manifest: ChannelManifest =
            toml::from_str(manifest_text).map_err(|source| Error::InvalidRustIndex {
                reason: format!("release manifest: {source}"),
            })?;
        resolve_manifest_tool(
            &selected.version,
            &selected.date,
            manifest,
            &actual_checksum,
        )
    }

    fn download_text(&self, url: &Url, limit: u64) -> Result<String> {
        let bytes = self.download_bytes(url, limit)?;
        String::from_utf8(bytes).map_err(|_| Error::InvalidRustIndex {
            reason: format!("metadata from {url} is not UTF-8"),
        })
    }

    fn download_bytes(&self, url: &Url, limit: u64) -> Result<Vec<u8>> {
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url.clone())
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::RustMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > limit)
        {
            return Err(Error::RustMetadataTooLarge { limit });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::RustMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > limit {
            return Err(Error::RustMetadataTooLarge { limit });
        }
        Ok(bytes)
    }
}

fn parse_available_releases(body: &str) -> Result<Vec<RustRelease>> {
    let mut releases = BTreeMap::<RustVersion, String>::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some(path) = line.strip_prefix("static.rust-lang.org/dist/") else {
            continue;
        };
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() != 2 || !valid_release_date(parts[0]) {
            continue;
        }
        let Some(version) = parts[1]
            .strip_prefix("channel-rust-")
            .and_then(|name| name.strip_suffix(".toml"))
        else {
            continue;
        };
        let Ok(version) = RustVersion::parse(version) else {
            continue;
        };
        releases
            .entry(version)
            .and_modify(|date| {
                if parts[0] > date.as_str() {
                    *date = parts[0].to_owned();
                }
            })
            .or_insert_with(|| parts[0].to_owned());
    }
    if releases.is_empty() {
        return Err(Error::InvalidRustIndex {
            reason: "manifests.txt contains no exact stable releases".to_owned(),
        });
    }
    let mut releases = releases
        .into_iter()
        .map(|(version, date)| RustRelease {
            version: version.to_string(),
            date,
        })
        .collect::<Vec<_>>();
    releases.sort_by_key(|release| Reverse(RustVersion::parse(&release.version).expect("parsed")));
    Ok(releases)
}

fn select_release(releases: Vec<RustRelease>, selector: &str) -> Result<RustRelease> {
    let normalized = selector.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "latest" | "current" | "stable") {
        return releases
            .into_iter()
            .next()
            .ok_or_else(|| Error::RustSelectorNotFound {
                selector: selector.to_owned(),
            });
    }
    let parts = normalized.split('.').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.len() > 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u64>().is_err()
        })
    {
        return Err(Error::InvalidRustSelector {
            selector: selector.to_owned(),
        });
    }
    let requested = parts
        .iter()
        .map(|part| part.parse::<u64>().expect("validated numeric selector"))
        .collect::<Vec<_>>();
    releases
        .into_iter()
        .find(|release| {
            let version = RustVersion::parse(&release.version).expect("available version");
            version.major == requested[0]
                && (requested.len() < 2 || version.minor == requested[1])
                && (requested.len() < 3 || version.patch == requested[2])
        })
        .ok_or_else(|| Error::RustSelectorNotFound {
            selector: selector.to_owned(),
        })
}

fn resolve_manifest_tool(
    requested_version: &str,
    expected_date: &str,
    manifest: ChannelManifest,
    manifest_sha256: &str,
) -> Result<LockedTool> {
    if manifest.manifest_version != "2" || !valid_release_date(&manifest.date) {
        return Err(Error::InvalidRustIndex {
            reason: "release manifest must use schema 2 and an exact date".to_owned(),
        });
    }
    let package = manifest
        .pkg
        .get("rust")
        .ok_or_else(|| Error::InvalidRustIndex {
            reason: "release manifest has no rust package".to_owned(),
        })?;
    let manifest_version =
        package
            .version
            .split_whitespace()
            .next()
            .ok_or_else(|| Error::InvalidRustIndex {
                reason: "rust package has no version".to_owned(),
            })?;
    if manifest_version != requested_version {
        return Err(Error::InvalidRustIndex {
            reason: format!(
                "requested Rust {requested_version}, but manifest describes {manifest_version}"
            ),
        });
    }
    if manifest.date != expected_date {
        return Err(Error::InvalidRustIndex {
            reason: format!(
                "Rust {requested_version} was indexed for {expected_date}, but manifest describes {}",
                manifest.date
            ),
        });
    }
    validate_default_profile(&manifest.profiles)?;
    let artifacts = RUST_TARGETS
        .into_iter()
        .map(|target| {
            let triple = rust_target_triple(target)?;
            let artifact = package
                .target
                .get(triple)
                .filter(|artifact| artifact.available)
                .ok_or_else(|| Error::InvalidRustIndex {
                    reason: format!("Rust {requested_version} has no {triple} toolchain"),
                })?;
            let url = artifact
                .xz_url
                .as_deref()
                .ok_or_else(|| Error::InvalidRustIndex {
                    reason: format!("Rust {requested_version} {triple} has no tar.xz URL"),
                })?;
            let hash = artifact
                .xz_hash
                .as_deref()
                .filter(|hash| valid_sha256(hash))
                .ok_or_else(|| Error::InvalidRustIndex {
                    reason: format!("Rust {requested_version} {triple} has no valid SHA-256"),
                })?;
            let plan = plan_rust_artifact(requested_version, &manifest.date, target, url)?;
            Ok(LockedArtifact {
                target: target.to_owned(),
                canonical_url: plan.canonical_url,
                artifact_path: plan.artifact_path,
                sha256: hash.to_ascii_lowercase(),
                integrity: None,
                format: match plan.format {
                    RustArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                },
                archive_root: plan.archive_root,
                verification: RUST_VERIFICATION.to_owned(),
                overlays: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LockedTool {
        name: "rust".to_owned(),
        requested: requested_version.to_owned(),
        version: requested_version.to_owned(),
        provider: "rust-official".to_owned(),
        metadata: BTreeMap::from([
            ("channel".to_owned(), "stable".to_owned()),
            ("components".to_owned(), RUST_COMPONENTS.to_owned()),
            ("manifest_date".to_owned(), manifest.date),
            ("manifest_sha256".to_owned(), manifest_sha256.to_owned()),
            ("profile".to_owned(), RUST_PROFILE.to_owned()),
        ]),
        artifacts,
    })
}

fn validate_default_profile(profiles: &BTreeMap<String, Vec<String>>) -> Result<()> {
    let profile = profiles
        .get(RUST_PROFILE)
        .ok_or_else(|| Error::InvalidRustIndex {
            reason: "release manifest has no default profile".to_owned(),
        })?;
    let components = profile.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let required = ["rustc", "cargo", "rust-std", "rust-docs"];
    if required
        .iter()
        .any(|component| !components.contains(component))
        || !(components.contains("rustfmt") || components.contains("rustfmt-preview"))
        || !(components.contains("clippy") || components.contains("clippy-preview"))
    {
        return Err(Error::InvalidRustIndex {
            reason: "Rust default profile does not contain rustc, cargo, rust-std, rust-docs, rustfmt and clippy"
                .to_owned(),
        });
    }
    Ok(())
}

fn parse_sha256_document(value: &str) -> Result<String> {
    let checksum = value
        .split_whitespace()
        .next()
        .filter(|checksum| valid_sha256(checksum))
        .ok_or_else(|| Error::InvalidRustIndex {
            reason: "manifest checksum document has no valid SHA-256".to_owned(),
        })?;
    Ok(checksum.to_ascii_lowercase())
}

fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_release_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_exact_stable_release_manifests() {
        let releases = parse_available_releases(
            "static.rust-lang.org/dist/2026-07-09/channel-rust-1.97.0.toml\n\
             static.rust-lang.org/dist/2026-07-16/channel-rust-1.97.1.toml\n\
             static.rust-lang.org/dist/2026-07-17/channel-rust-1.98.0-beta.4.toml\n\
             static.rust-lang.org/dist/2026-07-18/channel-rust-nightly.toml\n",
        )
        .expect("releases");
        assert_eq!(
            releases
                .iter()
                .map(|release| release.version.as_str())
                .collect::<Vec<_>>(),
            ["1.97.1", "1.97.0"]
        );
    }

    #[test]
    fn resolves_stable_prefixes_and_exact_versions() {
        let releases = vec![
            RustRelease {
                version: "1.97.1".to_owned(),
                date: "2026-07-16".to_owned(),
            },
            RustRelease {
                version: "1.96.1".to_owned(),
                date: "2026-06-30".to_owned(),
            },
        ];
        assert_eq!(
            select_release(releases.clone(), "latest")
                .expect("latest")
                .version,
            "1.97.1"
        );
        assert_eq!(
            select_release(releases.clone(), "stable")
                .expect("stable")
                .version,
            "1.97.1"
        );
        assert_eq!(
            select_release(releases.clone(), "1.96")
                .expect("minor")
                .version,
            "1.96.1"
        );
        assert_eq!(
            select_release(releases, "1.97.1").expect("exact").version,
            "1.97.1"
        );
    }

    #[test]
    fn resolves_a_complete_default_profile_manifest() {
        let manifest: ChannelManifest = toml::from_str(&fixture_manifest()).expect("manifest");
        let tool = resolve_manifest_tool("1.97.1", "2026-07-16", manifest, &"ab".repeat(32))
            .expect("tool");
        assert_eq!(tool.provider, "rust-official");
        assert_eq!(tool.artifacts.len(), RUST_TARGETS.len());
        assert_eq!(
            tool.metadata.get("components").map(String::as_str),
            Some(RUST_COMPONENTS)
        );
    }

    #[test]
    fn rejects_a_manifest_that_does_not_match_the_indexed_release_date() {
        let manifest: ChannelManifest = toml::from_str(&fixture_manifest()).expect("manifest");
        assert!(matches!(
            resolve_manifest_tool("1.97.1", "2026-07-15", manifest, &"ab".repeat(32),),
            Err(Error::InvalidRustIndex { .. })
        ));
    }

    fn fixture_manifest() -> String {
        let mut value = String::from(
            "manifest-version = \"2\"\ndate = \"2026-07-16\"\n\
             [profiles]\ndefault = [\"rustc\", \"cargo\", \"rust-std\", \"rust-docs\", \"rustfmt\", \"clippy\"]\n\
             [pkg.rust]\nversion = \"1.97.1 (fixture 2026-07-16)\"\n",
        );
        for target in RUST_TARGETS {
            let triple = rust_target_triple(target).expect("triple");
            value.push_str(&format!(
                "[pkg.rust.target.{triple}]\navailable = true\n\
                 xz_url = \"https://static.rust-lang.org/dist/2026-07-16/rust-1.97.1-{triple}.tar.xz\"\n\
                 xz_hash = \"{}\"\n",
                "cd".repeat(32)
            ));
        }
        value
    }
}
