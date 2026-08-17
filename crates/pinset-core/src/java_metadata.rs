use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Read,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::{
    Error, JAVA_TARGETS, JavaArchiveFormat, JavaVersion, LockedArtifact, LockedArtifactFormat,
    LockedTool, Result, plan_java_artifact,
};

const OFFICIAL_ADOPTIUM_API_BASE_URL: &str = "https://api.adoptium.net/v3/";
const MAX_JAVA_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const JAVA_PAGE_SIZE: usize = 20;
const MAX_JAVA_PAGES: usize = 20;
const JAVA_VERIFICATION: &str = "adoptium-api-sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRelease {
    pub version: String,
    pub feature_version: u64,
    pub lts: bool,
    pub release_name: String,
    pub openjdk_version: String,
    pub date: String,
}

#[derive(Debug)]
pub struct JavaMetadataClient {
    client: Client,
    base_url: Url,
}

#[derive(Debug, Deserialize)]
struct AvailableReleases {
    #[serde(default)]
    available_lts_releases: Vec<u64>,
    #[serde(default)]
    available_releases: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRelease {
    #[serde(default)]
    binaries: Vec<ApiBinary>,
    release_name: String,
    release_type: String,
    #[serde(default)]
    timestamp: String,
    vendor: String,
    version_data: ApiVersionData,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiBinary {
    architecture: String,
    heap_size: String,
    image_type: String,
    jvm_impl: String,
    os: String,
    package: ApiPackage,
    project: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiPackage {
    checksum: String,
    link: String,
    name: String,
    #[serde(default)]
    signature_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiVersionData {
    build: u64,
    major: u64,
    #[serde(default)]
    minor: u64,
    #[serde(default)]
    security: u64,
    #[serde(default)]
    patch: Option<u64>,
    openjdk_version: String,
    #[serde(default)]
    pre: Option<String>,
}

#[derive(Debug, Clone)]
struct SupportedJavaArtifact {
    target: String,
    checksum: String,
    link: String,
    name: String,
    signature_link: String,
}

#[derive(Debug, Clone)]
struct SupportedJavaRelease {
    version: JavaVersion,
    lts: bool,
    release_name: String,
    openjdk_version: String,
    date: String,
    artifacts: Vec<SupportedJavaArtifact>,
}

impl JavaMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_base_url(OFFICIAL_ADOPTIUM_API_BASE_URL)
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
                reason: "Adoptium API URL must be a hierarchical base URL".to_owned(),
            });
        }
        Ok(Self { client, base_url })
    }

    pub fn available_releases(&self) -> Result<Vec<JavaRelease>> {
        Ok(self
            .supported_releases()?
            .into_iter()
            .map(|release| JavaRelease {
                version: release.version.to_string(),
                feature_version: release.version.feature(),
                lts: release.lts,
                release_name: release.release_name,
                openjdk_version: release.openjdk_version,
                date: release.date,
            })
            .collect())
    }

    pub fn resolve_version_selector(&self, selector: &str) -> Result<String> {
        Ok(self.resolve_release(selector)?.version.to_string())
    }

    pub fn resolve_tool(&self, selector: &str) -> Result<LockedTool> {
        let release = self.resolve_release(selector)?;
        let version = release.version.to_string();
        let mut metadata = BTreeMap::from([
            ("distribution".to_owned(), "eclipse-temurin".to_owned()),
            ("vendor".to_owned(), "eclipse".to_owned()),
            ("image_type".to_owned(), "jdk".to_owned()),
            ("jvm_impl".to_owned(), "hotspot".to_owned()),
            ("heap_size".to_owned(), "normal".to_owned()),
            ("release_type".to_owned(), "ga".to_owned()),
            (
                "feature_version".to_owned(),
                release.version.feature().to_string(),
            ),
            ("release_name".to_owned(), release.release_name.clone()),
            (
                "openjdk_version".to_owned(),
                release.openjdk_version.clone(),
            ),
        ]);
        let artifacts = release
            .artifacts
            .into_iter()
            .map(|artifact| {
                let plan = plan_java_artifact(
                    &version,
                    &release.release_name,
                    &artifact.target,
                    &artifact.name,
                    &artifact.link,
                )?;
                metadata.insert(
                    format!("signature_link.{}", artifact.target),
                    artifact.signature_link,
                );
                Ok(LockedArtifact {
                    target: artifact.target,
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: artifact.checksum,
                    integrity: None,
                    format: match plan.format {
                        JavaArchiveFormat::Zip => LockedArtifactFormat::Zip,
                        JavaArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
                    },
                    archive_root: plan.archive_root,
                    verification: JAVA_VERIFICATION.to_owned(),
                    overlays: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(LockedTool {
            name: "java".to_owned(),
            requested: version.clone(),
            version,
            provider: "adoptium-temurin".to_owned(),
            metadata,
            artifacts,
        })
    }

    fn resolve_release(&self, selector: &str) -> Result<SupportedJavaRelease> {
        select_release(self.supported_releases()?, selector)
    }

    fn supported_releases(&self) -> Result<Vec<SupportedJavaRelease>> {
        let available_url = self
            .base_url
            .join("info/available_releases")
            .expect("known Adoptium API path");
        let available: AvailableReleases = serde_json::from_str(&self.download(&available_url)?)
            .map_err(|source| Error::InvalidJavaIndex {
                reason: format!("available releases: {source}"),
            })?;
        let lts = available
            .available_lts_releases
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut features = available.available_releases;
        features.sort_unstable_by(|left, right| right.cmp(left));
        features.dedup();
        let mut releases = Vec::new();
        for feature in features {
            for page in 0..MAX_JAVA_PAGES {
                let mut url = self
                    .base_url
                    .join(&format!("assets/feature_releases/{feature}/ga"))
                    .expect("known Adoptium API path");
                url.query_pairs_mut()
                    .append_pair("heap_size", "normal")
                    .append_pair("image_type", "jdk")
                    .append_pair("jvm_impl", "hotspot")
                    .append_pair("page", &page.to_string())
                    .append_pair("page_size", &JAVA_PAGE_SIZE.to_string())
                    .append_pair("project", "jdk")
                    .append_pair("sort_method", "DEFAULT")
                    .append_pair("sort_order", "DESC")
                    .append_pair("vendor", "eclipse");
                let body = self.download(&url)?;
                let page_releases: Vec<ApiRelease> =
                    serde_json::from_str(&body).map_err(|source| Error::InvalidJavaIndex {
                        reason: format!("feature {feature} page {page}: {source}"),
                    })?;
                let page_len = page_releases.len();
                releases.extend(parse_feature_releases(
                    page_releases,
                    lts.contains(&feature),
                ));
                if page_len < JAVA_PAGE_SIZE {
                    break;
                }
                if page + 1 == MAX_JAVA_PAGES {
                    return Err(Error::InvalidJavaIndex {
                        reason: format!("feature {feature} exceeds {MAX_JAVA_PAGES} pages"),
                    });
                }
            }
        }
        let mut identities = HashSet::new();
        releases.retain(|release| identities.insert(release.version));
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
            .map_err(|source| Error::JavaMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_JAVA_METADATA_BYTES)
        {
            return Err(Error::JavaMetadataTooLarge {
                limit: MAX_JAVA_METADATA_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_JAVA_METADATA_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::JavaMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_JAVA_METADATA_BYTES {
            return Err(Error::JavaMetadataTooLarge {
                limit: MAX_JAVA_METADATA_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidJavaIndex {
            reason: "metadata is not UTF-8".to_owned(),
        })
    }
}

fn parse_feature_releases(releases: Vec<ApiRelease>, lts: bool) -> Vec<SupportedJavaRelease> {
    releases
        .into_iter()
        .filter_map(|release| parse_release(release, lts))
        .collect()
}

fn parse_release(release: ApiRelease, lts: bool) -> Option<SupportedJavaRelease> {
    if release.release_type != "ga"
        || release.vendor != "eclipse"
        || release.version_data.pre.is_some()
        || release.version_data.major == 0
    {
        return None;
    }
    let version = JavaVersion {
        major: release.version_data.major,
        minor: release.version_data.minor,
        security: release.version_data.security,
        patch: release.version_data.patch.unwrap_or(0),
        build: release.version_data.build,
    };
    let version_string = version.to_string();
    let mut artifacts = Vec::with_capacity(JAVA_TARGETS.len());
    for target in JAVA_TARGETS {
        let binary = release
            .binaries
            .iter()
            .find(|binary| binary_matches_target(binary, target))?;
        let signature_link = binary.package.signature_link.as_deref()?;
        if !valid_sha256(&binary.package.checksum)
            || plan_java_artifact(
                &version_string,
                &release.release_name,
                target,
                &binary.package.name,
                &binary.package.link,
            )
            .is_err()
            || !valid_signature_link(&binary.package.link, signature_link)
        {
            return None;
        }
        artifacts.push(SupportedJavaArtifact {
            target: target.to_owned(),
            checksum: binary.package.checksum.clone(),
            link: binary.package.link.clone(),
            name: binary.package.name.clone(),
            signature_link: signature_link.to_owned(),
        });
    }
    Some(SupportedJavaRelease {
        version,
        lts,
        release_name: release.release_name,
        openjdk_version: release.version_data.openjdk_version,
        date: release.timestamp,
        artifacts,
    })
}

fn binary_matches_target(binary: &ApiBinary, target: &str) -> bool {
    let (os, architecture) = match target {
        "windows-x86_64" => ("windows", "x64"),
        "linux-x86_64" => ("linux", "x64"),
        "linux-aarch64" => ("linux", "aarch64"),
        "macos-x86_64" => ("mac", "x64"),
        "macos-aarch64" => ("mac", "aarch64"),
        _ => return false,
    };
    binary.os == os
        && binary.architecture == architecture
        && binary.heap_size == "normal"
        && binary.image_type == "jdk"
        && binary.jvm_impl == "hotspot"
        && binary.project == "jdk"
}

fn select_release(
    releases: Vec<SupportedJavaRelease>,
    selector: &str,
) -> Result<SupportedJavaRelease> {
    let normalized = selector.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "latest" | "current") {
        return releases
            .into_iter()
            .next()
            .ok_or_else(|| Error::JavaSelectorNotFound {
                selector: selector.to_owned(),
            });
    }
    if normalized == "lts" {
        return releases
            .into_iter()
            .find(|release| release.lts)
            .ok_or_else(|| Error::JavaSelectorNotFound {
                selector: selector.to_owned(),
            });
    }
    if normalized.contains('+') {
        let exact = JavaVersion::parse(&normalized).map_err(|_| Error::InvalidJavaSelector {
            selector: selector.to_owned(),
        })?;
        return releases
            .into_iter()
            .find(|release| release.version == exact)
            .ok_or_else(|| Error::JavaSelectorNotFound {
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
        return Err(Error::InvalidJavaSelector {
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
                && (requested.len() < 3 || release.version.security == requested[2])
        })
        .ok_or_else(|| Error::JavaSelectorNotFound {
            selector: selector.to_owned(),
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_signature_link(package_link: &str, signature_link: &str) -> bool {
    signature_link == format!("{package_link}.sig")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_complete_temurin_jdk_releases() {
        let complete: ApiRelease =
            serde_json::from_value(fixture_release("21.0.8+9", true)).expect("complete fixture");
        let incomplete: ApiRelease =
            serde_json::from_value(fixture_release("21.0.7+6", false)).expect("incomplete fixture");
        let releases = parse_feature_releases(vec![complete, incomplete], true);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version.to_string(), "21.0.8+9");
        assert!(releases[0].lts);
        assert_eq!(releases[0].artifacts.len(), JAVA_TARGETS.len());
    }

    #[test]
    fn resolves_lts_feature_update_and_exact_build_selectors() {
        let java_25 = parse_release(
            serde_json::from_value(fixture_release("25.0.2+10", true)).expect("25"),
            true,
        )
        .expect("25 supported");
        let java_21 = parse_release(
            serde_json::from_value(fixture_release("21.0.8+9", true)).expect("21"),
            true,
        )
        .expect("21 supported");
        let releases = vec![java_25, java_21];
        assert_eq!(
            select_release(releases.clone(), "21")
                .expect("feature")
                .version
                .to_string(),
            "21.0.8+9"
        );
        assert_eq!(
            select_release(releases.clone(), "21.0.8")
                .expect("update")
                .version
                .to_string(),
            "21.0.8+9"
        );
        assert_eq!(
            select_release(releases.clone(), "21.0.8+9")
                .expect("exact")
                .version
                .to_string(),
            "21.0.8+9"
        );
        assert_eq!(
            select_release(releases, "lts")
                .expect("latest LTS")
                .version
                .to_string(),
            "25.0.2+10"
        );
    }

    fn fixture_release(version: &str, complete: bool) -> serde_json::Value {
        let parsed = JavaVersion::parse(version).expect("fixture version");
        let release_name = format!("jdk-{version}");
        let mut binaries = Vec::new();
        for (index, target) in JAVA_TARGETS.iter().enumerate() {
            if !complete && index + 1 == JAVA_TARGETS.len() {
                break;
            }
            let (os, arch, extension) = match *target {
                "windows-x86_64" => ("windows", "x64", "zip"),
                "linux-x86_64" => ("linux", "x64", "tar.gz"),
                "linux-aarch64" => ("linux", "aarch64", "tar.gz"),
                "macos-x86_64" => ("mac", "x64", "tar.gz"),
                "macos-aarch64" => ("mac", "aarch64", "tar.gz"),
                _ => unreachable!("known Java target"),
            };
            let package_name = format!(
                "OpenJDK{}U-jdk_{arch}_{os}_hotspot_{}_{}.{}",
                parsed.major,
                version.split('+').next().expect("version part"),
                parsed.build,
                extension,
            );
            let link = format!(
                "https://github.com/adoptium/temurin{}-binaries/releases/download/{}/{}",
                parsed.major,
                release_name.replace('+', "%2B"),
                package_name,
            );
            binaries.push(serde_json::json!({
                "architecture": arch,
                "heap_size": "normal",
                "image_type": "jdk",
                "jvm_impl": "hotspot",
                "os": os,
                "package": {
                    "checksum": "ab".repeat(32),
                    "link": link.clone(),
                    "name": package_name,
                    "signature_link": format!("{link}.sig"),
                },
                "project": "jdk",
            }));
        }
        serde_json::json!({
            "binaries": binaries,
            "release_name": release_name,
            "release_type": "ga",
            "timestamp": "2026-07-15T00:00:00Z",
            "vendor": "eclipse",
            "version_data": {
                "build": parsed.build,
                "major": parsed.major,
                "minor": parsed.minor,
                "security": parsed.security,
                "patch": parsed.patch,
                "openjdk_version": format!("{version}-LTS"),
            },
        })
    }
}
