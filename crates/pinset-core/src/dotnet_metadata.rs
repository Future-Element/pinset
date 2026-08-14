use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    io::Read,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    DOTNET_TARGETS, DotnetArchiveFormat, DotnetVersion, Error, LockedArtifact,
    LockedArtifactFormat, LockedTool, Result, dotnet_rid, plan_dotnet_artifact,
};

const OFFICIAL_DOTNET_BASE_URL: &str = "https://builds.dotnet.microsoft.com/dotnet/";
const RELEASES_INDEX_PATH: &str = "release-metadata/releases-index.json";
const MAX_DOTNET_METADATA_BYTES: u64 = 32 * 1024 * 1024;
const DOTNET_VERIFICATION: &str = "dotnet-release-metadata-sha512";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotnetRelease {
    pub version: String,
    pub channel: String,
    pub release_type: String,
    pub support_phase: String,
    pub release_version: String,
    pub date: String,
}

#[derive(Debug)]
pub struct DotnetMetadataClient {
    client: Client,
    base_url: Url,
}

#[derive(Debug, Deserialize)]
struct ReleasesIndex {
    #[serde(rename = "releases-index", default)]
    channels: Vec<ChannelIndex>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChannelIndex {
    #[serde(rename = "channel-version")]
    channel: String,
    #[serde(rename = "release-type")]
    release_type: String,
    #[serde(rename = "support-phase")]
    support_phase: String,
    #[serde(rename = "releases.json")]
    releases_url: String,
}

#[derive(Debug, Deserialize)]
struct ChannelReleases {
    #[serde(rename = "channel-version")]
    channel: String,
    #[serde(default)]
    releases: Vec<ReleaseEntry>,
}

#[derive(Debug, Deserialize)]
struct ReleaseEntry {
    #[serde(rename = "release-date")]
    date: String,
    #[serde(rename = "release-version")]
    release_version: String,
    #[serde(default)]
    sdk: Option<SdkEntry>,
    #[serde(default)]
    sdks: Vec<SdkEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct SdkEntry {
    version: String,
    #[serde(default)]
    files: Vec<SdkFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct SdkFile {
    name: String,
    rid: String,
    url: String,
    hash: String,
}

#[derive(Debug, Clone)]
struct SupportedDotnetArtifact {
    target: String,
    url: String,
    hash: String,
}

#[derive(Debug, Clone)]
struct SupportedDotnetRelease {
    version: DotnetVersion,
    channel: String,
    release_type: String,
    support_phase: String,
    release_version: String,
    date: String,
    artifacts: Vec<SupportedDotnetArtifact>,
}

impl DotnetMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_base_url(OFFICIAL_DOTNET_BASE_URL)
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
                reason: ".NET metadata URL must be a hierarchical base URL".to_owned(),
            });
        }
        Ok(Self { client, base_url })
    }

    pub fn available_releases(&self) -> Result<Vec<DotnetRelease>> {
        Ok(self
            .supported_releases()?
            .into_iter()
            .map(|release| DotnetRelease {
                version: release.version.to_string(),
                channel: release.channel,
                release_type: release.release_type,
                support_phase: release.support_phase,
                release_version: release.release_version,
                date: release.date,
            })
            .collect())
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(select_release(self.supported_releases()?, selector)?
            .version
            .to_string())
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let release = select_release(self.supported_releases()?, selector)?;
        let version = release.version.to_string();
        let artifacts = release
            .artifacts
            .into_iter()
            .map(|artifact| {
                let plan = plan_dotnet_artifact(&version, &artifact.target, &artifact.url)?;
                Ok(LockedArtifact {
                    target: artifact.target,
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: String::new(),
                    integrity: Some(format!("sha512:{}", artifact.hash.to_ascii_lowercase())),
                    format: match plan.format {
                        DotnetArchiveFormat::Zip => LockedArtifactFormat::Zip,
                        DotnetArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
                    },
                    archive_root: plan.archive_root,
                    verification: DOTNET_VERIFICATION.to_owned(),
                    overlays: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(LockedTool {
            name: "dotnet".to_owned(),
            requested: version.clone(),
            version,
            provider: "microsoft-dotnet-sdk".to_owned(),
            metadata: BTreeMap::from([
                ("channel".to_owned(), release.channel),
                ("release_date".to_owned(), release.date),
                ("release_type".to_owned(), release.release_type),
                ("release_version".to_owned(), release.release_version),
                ("support_phase".to_owned(), release.support_phase),
            ]),
            artifacts,
        })
    }

    fn supported_releases(&self) -> Result<Vec<SupportedDotnetRelease>> {
        let index_url = self
            .base_url
            .join(RELEASES_INDEX_PATH)
            .expect("known .NET releases index path");
        let index: ReleasesIndex = serde_json::from_str(&self.download(&index_url)?).map_err(
            |source| Error::InvalidDotnetIndex {
                reason: format!("releases index: {source}"),
            },
        )?;
        let channels = supported_channels(index.channels)?;
        let mut releases = BTreeMap::<DotnetVersion, SupportedDotnetRelease>::new();
        for channel in channels {
            let url = validate_releases_url(&channel)?;
            let document: ChannelReleases =
                serde_json::from_str(&self.download(&url)?).map_err(|source| {
                    Error::InvalidDotnetIndex {
                        reason: format!("channel {} releases: {source}", channel.channel),
                    }
                })?;
            for release in parse_channel_releases(&channel, document)? {
                releases.entry(release.version).or_insert(release);
            }
        }
        if releases.is_empty() {
            return Err(Error::InvalidDotnetIndex {
                reason: "supported .NET channels contain no complete stable SDK release".to_owned(),
            });
        }
        let mut releases = releases.into_values().collect::<Vec<_>>();
        releases.sort_by_key(|release| Reverse(release.version));
        Ok(releases)
    }

    fn download(&self, url: &Url) -> Result<String> {
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url.clone())
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::DotnetMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOTNET_METADATA_BYTES)
        {
            return Err(Error::DotnetMetadataTooLarge {
                limit: MAX_DOTNET_METADATA_BYTES,
            });
        }
        let mut body = String::new();
        (&mut response)
            .take(MAX_DOTNET_METADATA_BYTES + 1)
            .read_to_string(&mut body)
            .map_err(|source| Error::DotnetMetadataRead {
                url: display_url,
                source,
            })?;
        if body.len() as u64 > MAX_DOTNET_METADATA_BYTES {
            return Err(Error::DotnetMetadataTooLarge {
                limit: MAX_DOTNET_METADATA_BYTES,
            });
        }
        Ok(body)
    }
}

fn supported_channels(channels: Vec<ChannelIndex>) -> Result<Vec<ChannelIndex>> {
    let mut supported = channels
        .into_iter()
        .filter(|channel| {
            matches!(channel.support_phase.as_str(), "active" | "maintenance")
                && matches!(channel.release_type.as_str(), "lts" | "sts")
                && valid_channel(&channel.channel)
        })
        .collect::<Vec<_>>();
    supported.sort_by_key(|channel| {
        Reverse(channel_version(&channel.channel).expect("validated channel"))
    });
    if supported.is_empty() {
        return Err(Error::InvalidDotnetIndex {
            reason: "releases index contains no supported GA LTS or STS channel".to_owned(),
        });
    }
    Ok(supported)
}

fn validate_releases_url(channel: &ChannelIndex) -> Result<Url> {
    let expected = format!(
        "https://builds.dotnet.microsoft.com/dotnet/release-metadata/{}/releases.json",
        channel.channel
    );
    let url = Url::parse(&channel.releases_url).map_err(|source| Error::InvalidDotnetIndex {
        reason: format!("invalid releases URL for channel {}: {source}", channel.channel),
    })?;
    if url.as_str() != expected {
        return Err(Error::InvalidDotnetIndex {
            reason: format!("releases URL for channel {} must be {expected}", channel.channel),
        });
    }
    Ok(url)
}

fn parse_channel_releases(
    channel: &ChannelIndex,
    document: ChannelReleases,
) -> Result<Vec<SupportedDotnetRelease>> {
    if document.channel != channel.channel {
        return Err(Error::InvalidDotnetIndex {
            reason: format!(
                "channel {} metadata describes {}",
                channel.channel, document.channel
            ),
        });
    }
    let mut releases = Vec::new();
    for entry in document.releases {
        if !valid_release_date(&entry.date) || DotnetVersion::parse(&entry.release_version).is_err()
        {
            continue;
        }
        let mut sdks = entry.sdk.into_iter().chain(entry.sdks).collect::<Vec<_>>();
        let mut seen = HashSet::new();
        sdks.retain(|sdk| seen.insert(sdk.version.clone()));
        for sdk in sdks {
            let Ok(version) = DotnetVersion::parse(&sdk.version) else {
                continue;
            };
            if version.channel() != channel.channel {
                continue;
            }
            let mut artifacts = Vec::new();
            for target in DOTNET_TARGETS {
                let rid = dotnet_rid(target)?;
                let expected_name = dotnet_archive_name(target)?;
                let Some(file) = sdk
                    .files
                    .iter()
                    .find(|file| file.rid == rid && file.name == expected_name)
                else {
                    artifacts.clear();
                    break;
                };
                if !valid_sha512(&file.hash)
                    || plan_dotnet_artifact(&sdk.version, target, &file.url).is_err()
                {
                    artifacts.clear();
                    break;
                }
                artifacts.push(SupportedDotnetArtifact {
                    target: target.to_owned(),
                    url: file.url.clone(),
                    hash: file.hash.clone(),
                });
            }
            if artifacts.len() == DOTNET_TARGETS.len() {
                releases.push(SupportedDotnetRelease {
                    version,
                    channel: channel.channel.clone(),
                    release_type: channel.release_type.clone(),
                    support_phase: channel.support_phase.clone(),
                    release_version: entry.release_version.clone(),
                    date: entry.date.clone(),
                    artifacts,
                });
            }
        }
    }
    Ok(releases)
}

fn select_release(
    releases: Vec<SupportedDotnetRelease>,
    selector: &str,
) -> Result<SupportedDotnetRelease> {
    let normalized = selector.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "latest" | "current") {
        return releases
            .into_iter()
            .next()
            .ok_or_else(|| Error::DotnetSelectorNotFound {
                selector: selector.to_owned(),
            });
    }
    if normalized == "lts" {
        return releases
            .into_iter()
            .find(|release| release.release_type == "lts")
            .ok_or_else(|| Error::DotnetSelectorNotFound {
                selector: selector.to_owned(),
            });
    }
    let parts = normalized.split('.').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.len() > 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u64>().is_err()
        })
    {
        return Err(Error::InvalidDotnetSelector {
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
            release.version.major == requested[0]
                && (requested.len() < 2 || release.version.minor == requested[1])
                && (requested.len() < 3 || release.version.patch == requested[2])
        })
        .ok_or_else(|| Error::DotnetSelectorNotFound {
            selector: selector.to_owned(),
        })
}

fn dotnet_archive_name(target: &str) -> Result<String> {
    let rid = dotnet_rid(target)?;
    let suffix = if target.starts_with("windows-") {
        "zip"
    } else {
        "tar.gz"
    };
    Ok(format!("dotnet-sdk-{rid}.{suffix}"))
}

fn channel_version(value: &str) -> Option<(u64, u64)> {
    let (major, minor) = value.split_once('.')?;
    if major.is_empty()
        || minor.is_empty()
        || major.len() > 1 && major.starts_with('0')
        || minor.len() > 1 && minor.starts_with('0')
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn valid_channel(value: &str) -> bool {
    channel_version(value).is_some()
}

fn valid_sha512(value: &str) -> bool {
    value.len() == 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    fn filters_eol_and_preview_channels() {
        let channels = supported_channels(vec![
            channel("10.0", "lts", "active"),
            channel("9.0", "sts", "maintenance"),
            channel("8.0", "lts", "eol"),
            channel("11.0", "sts", "preview"),
        ])
        .expect("channels");
        assert_eq!(
            channels
                .iter()
                .map(|channel| channel.channel.as_str())
                .collect::<Vec<_>>(),
            ["10.0", "9.0"]
        );
    }

    #[test]
    fn resolves_latest_lts_prefix_and_exact_sdk() {
        let releases = vec![
            supported_release("10.0.400", "10.0", "lts"),
            supported_release("9.0.317", "9.0", "sts"),
            supported_release("8.0.424", "8.0", "lts"),
        ];
        assert_eq!(
            select_release(releases.clone(), "latest")
                .expect("latest")
                .version
                .to_string(),
            "10.0.400"
        );
        assert_eq!(
            select_release(releases.clone(), "lts")
                .expect("lts")
                .version
                .to_string(),
            "10.0.400"
        );
        assert_eq!(
            select_release(releases.clone(), "9")
                .expect("major")
                .version
                .to_string(),
            "9.0.317"
        );
        assert_eq!(
            select_release(releases, "8.0.424")
                .expect("exact")
                .version
                .to_string(),
            "8.0.424"
        );
    }

    #[test]
    fn parses_the_official_sdk_file_name_and_url_shapes() {
        let channel = channel("10.0", "lts", "active");
        let files = DOTNET_TARGETS
            .into_iter()
            .map(|target| {
                let rid = dotnet_rid(target).expect("RID");
                let suffix = if target.starts_with("windows-") {
                    "zip"
                } else {
                    "tar.gz"
                };
                SdkFile {
                    name: format!("dotnet-sdk-{rid}.{suffix}"),
                    rid: rid.to_owned(),
                    url: format!(
                        "https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.400/dotnet-sdk-10.0.400-{rid}.{suffix}"
                    ),
                    hash: "ab".repeat(64),
                }
            })
            .collect();
        let document = ChannelReleases {
            channel: "10.0".to_owned(),
            releases: vec![ReleaseEntry {
                date: "2026-08-11".to_owned(),
                release_version: "10.0.11".to_owned(),
                sdk: Some(SdkEntry {
                    version: "10.0.400".to_owned(),
                    files,
                }),
                sdks: Vec::new(),
            }],
        };
        let releases = parse_channel_releases(&channel, document).expect("releases");
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version.to_string(), "10.0.400");
        assert_eq!(releases[0].artifacts.len(), DOTNET_TARGETS.len());
    }

    fn channel(version: &str, release_type: &str, support_phase: &str) -> ChannelIndex {
        ChannelIndex {
            channel: version.to_owned(),
            release_type: release_type.to_owned(),
            support_phase: support_phase.to_owned(),
            releases_url: format!(
                "https://builds.dotnet.microsoft.com/dotnet/release-metadata/{version}/releases.json"
            ),
        }
    }

    fn supported_release(
        version: &str,
        channel: &str,
        release_type: &str,
    ) -> SupportedDotnetRelease {
        SupportedDotnetRelease {
            version: DotnetVersion::parse(version).expect("version"),
            channel: channel.to_owned(),
            release_type: release_type.to_owned(),
            support_phase: "active".to_owned(),
            release_version: format!("{channel}.0"),
            date: "2026-08-11".to_owned(),
            artifacts: Vec::new(),
        }
    }
}
