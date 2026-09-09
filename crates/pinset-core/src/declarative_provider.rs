use std::{collections::BTreeMap, io::Read, time::Duration};

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;

use crate::{
    DeclarativeProviderBackend, DeclarativeProviderManifest, Error, LockedArtifact,
    LockedArtifactFormat, LockedTool, Result,
};

const MAX_RELEASE_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 256 * 1024;
const GITHUB_RELEASE_PAGE_SIZE: usize = 100;
const MAX_GITHUB_RELEASE_PAGES: usize = 20;

#[derive(Debug, Clone)]
pub struct DeclarativeProviderClient {
    http: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeProviderRelease {
    pub version: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

impl DeclarativeProviderClient {
    pub fn official() -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("pinset/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|source| Error::DeclarativeProviderMetadataRequest {
                url: "https://api.github.com".to_owned(),
                source,
            })?;
        Ok(Self { http })
    }

    pub fn resolve_tool(
        &self,
        manifest: &DeclarativeProviderManifest,
        selector: &str,
        registry_fingerprint: &str,
    ) -> Result<LockedTool> {
        if manifest.disabled {
            return invalid(format!(
                "Provider {} revision {} is disabled",
                manifest.id, manifest.revision
            ));
        }
        let DeclarativeProviderBackend::GitHubReleaseBinary {
            repository,
            tag_prefix,
            checksum_asset,
            assets,
        } = manifest
            .backend
            .as_ref()
            .ok_or_else(|| Error::DeclarativeProviderMetadataInvalid {
                reason: format!("Provider {} has no declarative backend", manifest.id),
            })?;
        let releases = self.github_releases(repository)?;
        let (release, version) = select_release(&releases, selector, tag_prefix)?;
        let mut release_assets = BTreeMap::new();
        for asset in &release.assets {
            if release_assets.insert(asset.name.as_str(), asset).is_some() {
                return invalid(format!(
                    "release {} repeats asset {}",
                    release.tag_name, asset.name
                ));
            }
        }
        let checksum = release_assets
            .get(checksum_asset.as_str())
            .copied()
            .ok_or_else(|| Error::DeclarativeProviderMetadataInvalid {
                reason: format!(
                    "release {} has no checksum asset {checksum_asset}",
                    release.tag_name
                ),
            })?;
        validate_release_asset_url(repository, &release.tag_name, checksum)?;
        let checksum_bytes =
            self.get_limited(&checksum.browser_download_url, MAX_CHECKSUM_BYTES)?;
        let checksum_text = std::str::from_utf8(&checksum_bytes).map_err(|source| {
            Error::DeclarativeProviderMetadataInvalid {
                reason: format!("checksum asset is not UTF-8: {source}"),
            }
        })?;
        let checksums = parse_checksums(checksum_text)?;
        let mut locked_artifacts = Vec::with_capacity(assets.len());
        for (target, asset_name) in assets {
            let Some(asset) = release_assets.get(asset_name.as_str()).copied() else {
                continue;
            };
            validate_release_asset_url(repository, &release.tag_name, asset)?;
            let Some(sha256) = checksums.get(asset_name) else {
                continue;
            };
            locked_artifacts.push(LockedArtifact {
                target: target.clone(),
                canonical_url: asset.browser_download_url.clone(),
                artifact_path: asset_name.clone(),
                sha256: sha256.clone(),
                integrity: Some(format!("sha256:{sha256}")),
                format: LockedArtifactFormat::Binary,
                archive_root: String::new(),
                verification: "https-checksum".to_owned(),
                overlays: Vec::new(),
            });
        }
        let released_at = release.published_at.clone().ok_or_else(|| {
            Error::DeclarativeProviderMetadataInvalid {
                reason: format!("release {} has no publication timestamp", release.tag_name),
            }
        })?;
        if !crate::valid_release_time(&released_at) {
            return invalid(format!(
                "release {} has invalid publication timestamp",
                release.tag_name
            ));
        }
        Ok(LockedTool {
            name: manifest.tool.clone(),
            requested: selector.to_owned(),
            version: version.to_string(),
            provider: "declarative-github-release".to_owned(),
            released_at: Some(released_at),
            metadata: BTreeMap::from([
                ("provider-id".to_owned(), manifest.id.clone()),
                (
                    "provider-revision".to_owned(),
                    manifest.revision.to_string(),
                ),
                (
                    "registry-fingerprint".to_owned(),
                    registry_fingerprint.to_owned(),
                ),
                ("repository".to_owned(), repository.clone()),
                ("tag".to_owned(), release.tag_name.clone()),
            ]),
            options: BTreeMap::new(),
            artifacts: locked_artifacts,
        })
    }

    pub fn available_releases(
        &self,
        manifest: &DeclarativeProviderManifest,
    ) -> Result<Vec<DeclarativeProviderRelease>> {
        if manifest.disabled {
            return invalid(format!(
                "Provider {} revision {} is disabled",
                manifest.id, manifest.revision
            ));
        }
        let DeclarativeProviderBackend::GitHubReleaseBinary {
            repository,
            tag_prefix,
            ..
        } = manifest
            .backend
            .as_ref()
            .ok_or_else(|| Error::DeclarativeProviderMetadataInvalid {
                reason: format!("Provider {} has no declarative backend", manifest.id),
            })?;
        let mut releases = self
            .github_releases(repository)?
            .into_iter()
            .filter(|release| !release.draft && !release.prerelease)
            .filter_map(|release| {
                let version = release
                    .tag_name
                    .strip_prefix(tag_prefix)
                    .and_then(parse_stable_version)?;
                Some((
                    version,
                    DeclarativeProviderRelease {
                        version: release.tag_name[tag_prefix.len()..].to_owned(),
                        published_at: release.published_at,
                    },
                ))
            })
            .collect::<Vec<_>>();
        releases.sort_by(|left, right| right.0.cmp(&left.0));
        releases.dedup_by(|left, right| left.0 == right.0);
        Ok(releases.into_iter().map(|(_, release)| release).collect())
    }

    fn github_releases(&self, repository: &str) -> Result<Vec<GitHubRelease>> {
        let mut releases = Vec::new();
        for page in 1..=MAX_GITHUB_RELEASE_PAGES {
            let releases_url = format!(
                "https://api.github.com/repos/{repository}/releases?per_page={GITHUB_RELEASE_PAGE_SIZE}&page={page}"
            );
            let releases_bytes = self.get_limited(&releases_url, MAX_RELEASE_METADATA_BYTES)?;
            let mut page_releases: Vec<GitHubRelease> = serde_json::from_slice(&releases_bytes)
                .map_err(|source| Error::DeclarativeProviderMetadataInvalid {
                    reason: format!("GitHub release payload is invalid: {source}"),
                })?;
            let page_len = page_releases.len();
            releases.append(&mut page_releases);
            if page_len < GITHUB_RELEASE_PAGE_SIZE {
                return Ok(releases);
            }
        }
        invalid(format!(
            "GitHub repository {repository} exceeds {MAX_GITHUB_RELEASE_PAGES} release pages"
        ))
    }

    fn get_limited(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        let mut response = self
            .http
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::DeclarativeProviderMetadataRequest {
                url: url.to_owned(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return invalid(format!("metadata response exceeds {limit} bytes"));
        }
        let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
        response
            .by_ref()
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::DeclarativeProviderMetadataRead {
                url: url.to_owned(),
                source,
            })?;
        if bytes.len() > limit {
            return invalid(format!("metadata response exceeds {limit} bytes"));
        }
        Ok(bytes)
    }
}

fn select_release<'a>(
    releases: &'a [GitHubRelease],
    selector: &str,
    tag_prefix: &str,
) -> Result<(&'a GitHubRelease, Version)> {
    let mut matching = releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            release
                .tag_name
                .strip_prefix(tag_prefix)
                .and_then(parse_stable_version)
                .filter(|version| selector_matches(selector, version))
                .map(|version| (release, version))
        })
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| left.1.cmp(&right.1));
    matching
        .pop()
        .ok_or_else(|| Error::DeclarativeProviderVersionNotFound {
            selector: selector.to_owned(),
        })
}

fn parse_stable_version(value: &str) -> Option<Version> {
    if let Ok(version) = Version::parse(value) {
        return (version.pre.is_empty() && version.build.is_empty()).then_some(version);
    }
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version::new(major, minor, 0))
}

fn selector_matches(selector: &str, version: &Version) -> bool {
    if matches!(selector, "latest" | "current") {
        return true;
    }
    if let Ok(exact) = Version::parse(selector) {
        return &exact == version;
    }
    let parts = selector.split('.').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.len() > 2
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    parts[0].parse::<u64>().ok() == Some(version.major)
        && (parts.len() == 1 || parts[1].parse::<u64>().ok() == Some(version.minor))
}

fn parse_checksums(input: &str) -> Result<BTreeMap<String, String>> {
    let mut checksums = BTreeMap::new();
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else { continue };
        let Some(filename) = fields.next() else {
            continue;
        };
        let filename = filename.trim_start_matches('*');
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return invalid(format!("invalid SHA-256 checksum for {filename}"));
        }
        if checksums
            .insert(filename.to_owned(), hash.to_ascii_lowercase())
            .is_some()
        {
            return invalid(format!("duplicate checksum for {filename}"));
        }
    }
    Ok(checksums)
}

fn validate_release_asset_url(repository: &str, tag: &str, asset: &GitHubAsset) -> Result<()> {
    let expected = format!(
        "https://github.com/{repository}/releases/download/{tag}/{}",
        asset.name
    );
    if asset.browser_download_url != expected {
        return invalid(format!(
            "release asset {} is outside the declared GitHub repository and tag",
            asset.name
        ));
    }
    Ok(())
}

fn invalid<T>(reason: impl Into<String>) -> Result<T> {
    Err(Error::DeclarativeProviderMetadataInvalid {
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_choose_the_latest_matching_stable_release() {
        let releases = vec![
            release("jq-1.7.1", false),
            release("jq-1.8.1", false),
            release("jq-1.8.2", false),
            release("jq-1.9.0-rc.1", true),
        ];
        assert_eq!(
            select_release(&releases, "1.8", "jq-").expect("1.8").1,
            Version::new(1, 8, 2)
        );
        assert_eq!(
            select_release(&releases, "latest", "jq-")
                .expect("latest")
                .1,
            Version::new(1, 8, 2)
        );
        assert!(select_release(&releases, "2", "jq-").is_err());
    }

    #[test]
    fn checksum_parser_rejects_duplicates_and_invalid_hashes() {
        let hash = "a".repeat(64);
        assert_eq!(
            parse_checksums(&format!("{hash}  jq-linux-amd64\n")).expect("checksums")["jq-linux-amd64"],
            hash
        );
        assert!(parse_checksums("bad  jq\n").is_err());
        assert!(parse_checksums(&format!("{hash} jq\n{hash} jq\n")).is_err());
    }

    fn release(tag: &str, prerelease: bool) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_owned(),
            published_at: Some("2026-06-20T14:11:27Z".to_owned()),
            prerelease,
            draft: false,
            assets: Vec::new(),
        }
    }
}
