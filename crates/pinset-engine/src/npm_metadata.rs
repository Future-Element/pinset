use std::{cmp::Reverse, collections::BTreeMap, io::Read, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use p256::{
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    pkcs8::DecodePublicKey,
};
use reqwest::header::ACCEPT;
use reqwest::{Url, blocking::Client};
use semver::Version;
use serde::Deserialize;

use crate::{
    Error, LockedArtifact, LockedArtifactFormat, LockedArtifactOverlay, LockedTool, Result,
};

const OFFICIAL_NPM_REGISTRY: &str = "https://registry.npmjs.org/";
const MAX_METADATA_BYTES: u64 = 32 * 1024 * 1024;
const SIGNATURE_VERIFICATION: &str = "npm-registry-signature-sha512";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NpmToolRelease {
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpmToolTarget {
    pub target: &'static str,
    pub package: &'static str,
    pub required_path: &'static str,
}

pub const BUN_TARGETS: &[NpmToolTarget] = &[
    NpmToolTarget {
        target: "windows-x86_64-avx2",
        package: "@oven/bun-windows-x64",
        required_path: "bin/bun.exe",
    },
    NpmToolTarget {
        target: "windows-x86_64-baseline",
        package: "@oven/bun-windows-x64-baseline",
        required_path: "bin/bun.exe",
    },
    NpmToolTarget {
        target: "linux-x86_64-avx2",
        package: "@oven/bun-linux-x64",
        required_path: "bin/bun",
    },
    NpmToolTarget {
        target: "linux-x86_64-baseline",
        package: "@oven/bun-linux-x64-baseline",
        required_path: "bin/bun",
    },
    NpmToolTarget {
        target: "linux-aarch64",
        package: "@oven/bun-linux-aarch64",
        required_path: "bin/bun",
    },
    NpmToolTarget {
        target: "macos-aarch64",
        package: "@oven/bun-darwin-aarch64",
        required_path: "bin/bun",
    },
];

const PNPM_NATIVE_TARGETS: &[NpmToolTarget] = &[
    NpmToolTarget {
        target: "windows-x86_64",
        package: "@pnpm/exe.win32-x64",
        required_path: "pnpm.exe",
    },
    NpmToolTarget {
        target: "linux-x86_64",
        package: "@pnpm/exe.linux-x64",
        required_path: "pnpm",
    },
    NpmToolTarget {
        target: "linux-aarch64",
        package: "@pnpm/exe.linux-arm64",
        required_path: "pnpm",
    },
    NpmToolTarget {
        target: "macos-x86_64",
        package: "@pnpm/exe.darwin-x64",
        required_path: "pnpm",
    },
    NpmToolTarget {
        target: "macos-aarch64",
        package: "@pnpm/exe.darwin-arm64",
        required_path: "pnpm",
    },
];

#[derive(Debug)]
pub struct NpmMetadataClient {
    client: Client,
    registry: Url,
}

#[derive(Debug, Deserialize)]
struct PackageDocument {
    #[serde(default)]
    versions: BTreeMap<String, PackageVersion>,
    #[serde(default)]
    time: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PackageVersion {
    name: String,
    version: String,
    #[serde(default)]
    bin: Option<PackageBin>,
    #[serde(rename = "optionalDependencies", default)]
    optional_dependencies: BTreeMap<String, String>,
    dist: PackageDist,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PackageBin {
    Commands(BTreeMap<String, String>),
    Entry(String),
}

#[derive(Debug, Clone, Deserialize)]
struct PackageDist {
    tarball: String,
    integrity: String,
    #[serde(default)]
    signatures: Vec<PackageSignature>,
}

#[derive(Debug, Clone, Deserialize)]
struct PackageSignature {
    keyid: String,
    sig: String,
}

#[derive(Debug, Deserialize)]
struct RegistryKeys {
    keys: Vec<RegistryKey>,
}

#[derive(Debug, Deserialize)]
struct RegistryKey {
    keyid: String,
    key: String,
}

impl NpmMetadataClient {
    pub fn official() -> Result<Self> {
        Self::for_registry(OFFICIAL_NPM_REGISTRY)
    }

    pub fn for_registry(registry: &str) -> Result<Self> {
        let client = crate::http_client_builder()?
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let registry = Url::parse(registry).map_err(|source| Error::InvalidNpmMetadata {
            package: "<registry>".to_owned(),
            reason: source.to_string(),
        })?;
        if !registry.path().ends_with('/') {
            return Err(Error::InvalidNpmMetadata {
                package: "<registry>".to_owned(),
                reason: "registry URL must end with /".to_owned(),
            });
        }
        Ok(Self { client, registry })
    }

    pub fn available_releases(&self, tool: &str) -> Result<Vec<NpmToolRelease>> {
        let package = wrapper_package(tool)?;
        let document: PackageDocument =
            self.download_json_abbreviated(self.package_url(package)?)?;
        if tool == "bun" {
            tool_targets(tool)?;
        }
        let mut releases = Vec::new();
        for (declared_version, manifest) in document.versions {
            let Ok(version) = Version::parse(&declared_version) else {
                continue;
            };
            if manifest.version != declared_version
                || manifest.name != package
                || !version.pre.is_empty()
                || !version.build.is_empty()
            {
                continue;
            }
            releases.push((
                version,
                NpmToolRelease {
                    version: declared_version,
                },
            ));
        }
        releases.sort_by_key(|(version, _)| Reverse(version.clone()));
        if releases.is_empty() {
            return Err(Error::InvalidNpmMetadata {
                package: package.to_owned(),
                reason: "package contains no stable release".to_owned(),
            });
        }
        Ok(releases.into_iter().map(|(_, release)| release).collect())
    }

    pub fn resolve_version_selector(&self, tool: &str, selector: &str) -> Result<String> {
        if let Ok(version) = Version::parse(selector) {
            if version.pre.is_empty() && version.build.is_empty() {
                return Ok(version.to_string());
            }
            return Err(Error::InvalidNpmToolSelector {
                tool: tool.to_owned(),
                selector: selector.to_owned(),
            });
        }
        let normalized = selector.trim().to_ascii_lowercase();
        let parts = normalized.split('.').collect::<Vec<_>>();
        let numeric = matches!(parts.len(), 1 | 2)
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
        if !numeric && !matches!(normalized.as_str(), "latest") {
            return Err(Error::InvalidNpmToolSelector {
                tool: tool.to_owned(),
                selector: selector.to_owned(),
            });
        }
        let requested = numeric
            .then(|| {
                parts
                    .iter()
                    .map(|part| part.parse::<u64>())
                    .collect::<std::result::Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(|_| Error::InvalidNpmToolSelector {
                tool: tool.to_owned(),
                selector: selector.to_owned(),
            })?;
        self.available_releases(tool)?
            .into_iter()
            .find(|release| {
                if matches!(normalized.as_str(), "latest") {
                    return true;
                }
                let version = Version::parse(&release.version)
                    .expect("available npm releases contain valid semver");
                let requested = requested.as_ref().expect("numeric selector parsed");
                version.major == requested[0]
                    && (requested.len() == 1 || version.minor == requested[1])
            })
            .map(|release| release.version)
            .ok_or_else(|| Error::NpmToolSelectorNotFound {
                tool: tool.to_owned(),
                selector: selector.to_owned(),
            })
    }

    pub fn resolve_tool(&self, tool: &str, version: &str) -> Result<LockedTool> {
        let parsed = Version::parse(version).map_err(|_| Error::InvalidNpmToolSelector {
            tool: tool.to_owned(),
            selector: version.to_owned(),
        })?;
        if !parsed.pre.is_empty() || !parsed.build.is_empty() {
            return Err(Error::InvalidNpmToolSelector {
                tool: tool.to_owned(),
                selector: version.to_owned(),
            });
        }
        let wrapper = wrapper_package(tool)?;
        let released_at = self.package_release_time(wrapper, version)?;
        let manifest = self.package_version(wrapper, version)?;
        validate_manifest_identity(wrapper, version, &manifest)?;
        let keys = self.registry_keys()?;
        verify_package_signature(&manifest, &keys)?;
        if tool == "pnpm" {
            let entry = pnpm_manifest_entry(&manifest)?;
            let (canonical_url, artifact_path) =
                official_tarball(wrapper, version, &manifest.dist)?;
            crate::ArtifactIntegrity::parse(&manifest.dist.integrity)?;
            let mut metadata = BTreeMap::new();
            if entry != "bin/pnpm.cjs" {
                metadata.insert("pnpm-entry".into(), entry.into());
            }
            let artifacts = if entry == "pnpm" {
                let mut artifacts = Vec::new();
                for target in PNPM_NATIVE_TARGETS {
                    let Some(dependency) = manifest.optional_dependencies.get(target.package)
                    else {
                        continue;
                    };
                    let package_version =
                        exact_dependency_version(dependency).ok_or_else(|| {
                            Error::InvalidNpmMetadata {
                                package: wrapper.into(),
                                reason: format!(
                                    "{} has non-exact version {dependency:?}",
                                    target.package
                                ),
                            }
                        })?;
                    let platform = self.package_version(target.package, package_version)?;
                    validate_manifest_identity(target.package, package_version, &platform)?;
                    verify_package_signature(&platform, &keys)?;
                    let (platform_url, platform_path) =
                        official_tarball(target.package, package_version, &platform.dist)?;
                    crate::ArtifactIntegrity::parse(&platform.dist.integrity)?;
                    artifacts.push(LockedArtifact {
                        target: target.target.into(),
                        canonical_url: platform_url,
                        artifact_path: platform_path,
                        sha256: String::new(),
                        integrity: Some(platform.dist.integrity),
                        format: LockedArtifactFormat::TarGz,
                        archive_root: "package".into(),
                        verification: SIGNATURE_VERIFICATION.into(),
                        overlays: vec![LockedArtifactOverlay {
                            canonical_url: canonical_url.clone(),
                            artifact_path: artifact_path.clone(),
                            integrity: manifest.dist.integrity.clone(),
                            format: LockedArtifactFormat::TarGz,
                            archive_root: "package".into(),
                            verification: SIGNATURE_VERIFICATION.into(),
                        }],
                    });
                }
                artifacts
            } else {
                crate::SUPPORTED_TARGETS
                    .iter()
                    .map(|target| LockedArtifact {
                        target: (*target).into(),
                        canonical_url: canonical_url.clone(),
                        artifact_path: artifact_path.clone(),
                        sha256: String::new(),
                        integrity: Some(manifest.dist.integrity.clone()),
                        format: LockedArtifactFormat::TarGz,
                        archive_root: "package".into(),
                        verification: SIGNATURE_VERIFICATION.into(),
                        overlays: vec![],
                    })
                    .collect()
            };
            return Ok(LockedTool {
                name: tool.into(),
                requested: version.into(),
                version: version.into(),
                provider: "pnpm-npm".into(),
                released_at,
                metadata,
                options: Default::default(),
                artifacts,
            });
        }
        let mut artifacts = Vec::new();
        for target in tool_targets(tool)? {
            let Some((package, dependency)) = manifest
                .optional_dependencies
                .get_key_value(target.package)
                .map(|(name, version)| (name.as_str(), version.as_str()))
            else {
                continue;
            };
            let package_version =
                exact_dependency_version(dependency).ok_or_else(|| Error::InvalidNpmMetadata {
                    package: wrapper.to_owned(),
                    reason: format!("{package} has non-exact version {dependency:?}"),
                })?;
            let platform = self.package_version(package, package_version)?;
            validate_manifest_identity(package, package_version, &platform)?;
            verify_package_signature(&platform, &keys)?;
            let (canonical_url, artifact_path) =
                official_tarball(package, package_version, &platform.dist)?;
            crate::ArtifactIntegrity::parse(&platform.dist.integrity)?;
            artifacts.push(LockedArtifact {
                target: target.target.to_owned(),
                canonical_url,
                artifact_path,
                sha256: String::new(),
                integrity: Some(platform.dist.integrity),
                format: LockedArtifactFormat::TarGz,
                archive_root: "package".to_owned(),
                verification: SIGNATURE_VERIFICATION.to_owned(),
                overlays: vec![],
            });
        }
        Ok(LockedTool {
            name: tool.to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: format!("{tool}-npm"),
            released_at,
            metadata: std::collections::BTreeMap::new(),
            options: Default::default(),
            artifacts,
        })
    }

    fn package_version(&self, package: &str, version: &str) -> Result<PackageVersion> {
        let url = self
            .package_url(package)?
            .join(version)
            .expect("validated semver is a safe URL segment");
        self.download_json(url)
    }

    fn package_release_time(&self, package: &str, version: &str) -> Result<Option<String>> {
        let document: PackageDocument = self.download_json(self.package_url(package)?)?;
        Ok(document
            .time
            .get(version)
            .filter(|value| crate::valid_release_time(value))
            .cloned())
    }

    fn registry_keys(&self) -> Result<RegistryKeys> {
        let url = self
            .registry
            .join("-/npm/v1/keys")
            .expect("built-in npm keys endpoint is valid");
        self.download_json(url)
    }

    fn package_url(&self, package: &str) -> Result<Url> {
        let encoded = package.replace('/', "%2f");
        Url::parse(&format!("{}{encoded}/", self.registry)).map_err(|source| {
            Error::InvalidNpmMetadata {
                package: package.to_owned(),
                reason: source.to_string(),
            }
        })
    }

    fn download_json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T> {
        self.download_json_with_accept(url, None)
    }

    fn download_json_abbreviated<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T> {
        self.download_json_with_accept(url, Some("application/vnd.npm.install-v1+json"))
    }

    fn download_json_with_accept<T: for<'de> Deserialize<'de>>(
        &self,
        url: Url,
        accept: Option<&str>,
    ) -> Result<T> {
        let display_url = url.to_string();
        let mut request = self.client.get(url);
        if let Some(accept) = accept {
            request = request.header(ACCEPT, accept);
        }
        let mut response = request
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::NpmMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_METADATA_BYTES)
        {
            return Err(Error::NpmMetadataTooLarge {
                limit: MAX_METADATA_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_METADATA_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::NpmMetadataRead {
                url: display_url.clone(),
                source,
            })?;
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err(Error::NpmMetadataTooLarge {
                limit: MAX_METADATA_BYTES,
            });
        }
        serde_json::from_slice(&bytes).map_err(|source| Error::InvalidNpmMetadata {
            package: display_url,
            reason: source.to_string(),
        })
    }
}

fn pnpm_manifest_entry(manifest: &PackageVersion) -> Result<&str> {
    let entry = match manifest.bin.as_ref() {
        Some(PackageBin::Commands(commands)) => commands.get("pnpm").map(String::as_str),
        Some(PackageBin::Entry(entry)) => Some(entry.as_str()),
        None => None,
    };
    match entry {
        Some(entry @ ("bin/pnpm.cjs" | "bin/pnpm.mjs" | "pnpm")) => Ok(entry),
        _ => Err(Error::InvalidNpmMetadata {
            package: manifest.name.clone(),
            reason: "unsupported official pnpm command entry".into(),
        }),
    }
}

fn official_tarball(package: &str, version: &str, dist: &PackageDist) -> Result<(String, String)> {
    let tarball = Url::parse(&dist.tarball).map_err(|source| Error::InvalidNpmMetadata {
        package: package.to_owned(),
        reason: format!("invalid tarball URL: {source}"),
    })?;
    let package_base = package.rsplit('/').next().expect("npm package is nonempty");
    let expected_path = format!("{package}/-/{package_base}-{version}.tgz");
    if tarball.scheme() != "https"
        || tarball.host_str() != Some("registry.npmjs.org")
        || !tarball.username().is_empty()
        || tarball.password().is_some()
        || tarball.path().trim_start_matches('/') != expected_path
    {
        return Err(Error::InvalidNpmMetadata {
            package: package.to_owned(),
            reason: "tarball must use the exact official HTTPS npm registry path".to_owned(),
        });
    }
    Ok((tarball.to_string(), expected_path))
}

pub fn tool_targets(tool: &str) -> Result<&'static [NpmToolTarget]> {
    match tool {
        "bun" => Ok(BUN_TARGETS),
        _ => Err(Error::UnsupportedSourceProvider {
            provider: tool.to_owned(),
        }),
    }
}

pub fn validate_exact_npm_tool_version(tool: &str, version: &str) -> Result<()> {
    let parsed = Version::parse(version).map_err(|_| Error::InvalidNpmToolSelector {
        tool: tool.to_owned(),
        selector: version.to_owned(),
    })?;
    if !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err(Error::InvalidNpmToolSelector {
            tool: tool.to_owned(),
            selector: version.to_owned(),
        });
    }
    Ok(())
}

fn wrapper_package(tool: &str) -> Result<&'static str> {
    match tool {
        "pnpm" => Ok("pnpm"),
        "bun" => Ok("bun"),
        _ => Err(Error::UnsupportedSourceProvider {
            provider: tool.to_owned(),
        }),
    }
}

fn exact_dependency_version(value: &str) -> Option<&str> {
    let value = value.strip_prefix('=').unwrap_or(value);
    Version::parse(value).ok().map(|_| value)
}

fn validate_manifest_identity(
    package: &str,
    version: &str,
    manifest: &PackageVersion,
) -> Result<()> {
    if manifest.name != package || manifest.version != version {
        return Err(Error::InvalidNpmMetadata {
            package: package.to_owned(),
            reason: format!(
                "requested {package}@{version}, received {}@{}",
                manifest.name, manifest.version
            ),
        });
    }
    Ok(())
}

fn verify_package_signature(manifest: &PackageVersion, keys: &RegistryKeys) -> Result<()> {
    let message = format!(
        "{}@{}:{}",
        manifest.name, manifest.version, manifest.dist.integrity
    );
    let mut failures = Vec::new();
    for signature in &manifest.dist.signatures {
        let Some(key) = keys.keys.iter().find(|key| key.keyid == signature.keyid) else {
            failures.push(format!("unknown key {}", signature.keyid));
            continue;
        };
        let public_key = STANDARD
            .decode(&key.key)
            .ok()
            .and_then(|bytes| VerifyingKey::from_public_key_der(&bytes).ok());
        let signature = STANDARD
            .decode(&signature.sig)
            .ok()
            .and_then(|bytes| Signature::from_der(&bytes).ok());
        if let (Some(public_key), Some(signature)) = (public_key, signature)
            && public_key.verify(message.as_bytes(), &signature).is_ok()
        {
            return Ok(());
        }
        failures.push(format!("invalid signature for key {}", key.keyid));
    }
    Err(Error::NpmSignatureVerification {
        package: manifest.name.clone(),
        version: manifest.version.clone(),
        reason: if failures.is_empty() {
            "package metadata contains no signatures".to_owned()
        } else {
            failures.join(", ")
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_platforms_include_cpu_variants_and_arm() {
        assert!(BUN_TARGETS.iter().any(|t| t.target.ends_with("-baseline")));
        assert!(BUN_TARGETS.iter().any(|t| t.target == "linux-aarch64"));
    }

    #[test]
    fn validates_exact_supported_versions_without_registry_metadata() {
        assert!(validate_exact_npm_tool_version("pnpm", "10.34.5").is_ok());
        assert!(validate_exact_npm_tool_version("pnpm", "11.21.0").is_ok());
        assert!(validate_exact_npm_tool_version("bun", "1.3.14").is_ok());
        for (tool, version) in [
            ("pnpm", "10"),
            ("bun", "1.3.14-beta.1"),
            ("bun", "1.3.14+build"),
        ] {
            assert!(validate_exact_npm_tool_version(tool, version).is_err());
        }
    }

    #[test]
    fn pnpm_entries_follow_official_package_metadata() {
        for entry in ["bin/pnpm.cjs", "bin/pnpm.mjs", "pnpm"] {
            let manifest: PackageVersion = serde_json::from_value(serde_json::json!({
                "name":"pnpm", "version":"12.9.1", "bin":{"pnpm":entry},
                "dist":{"tarball":"", "integrity":""}
            }))
            .unwrap();
            assert_eq!(pnpm_manifest_entry(&manifest).unwrap(), entry);
        }
        for entry in ["../pnpm", "/bin/pnpm", "bin/future.js", ""] {
            let manifest: PackageVersion = serde_json::from_value(serde_json::json!({
                "name":"pnpm", "version":"12.9.1", "bin":{"pnpm":entry},
                "dist":{"tarball":"", "integrity":""}
            }))
            .unwrap();
            assert!(pnpm_manifest_entry(&manifest).is_err());
        }
        assert!(crate::SUPPORTED_TARGETS.iter().all(|target| {
            PNPM_NATIVE_TARGETS
                .iter()
                .any(|platform| platform.target == *target)
        }));
    }

    #[test]
    fn npm_metadata_accepts_both_official_bin_formats_without_affecting_bun() {
        for bin in [
            serde_json::json!("bin/bun"),
            serde_json::json!({"bun":"bin/bun"}),
            serde_json::Value::Null,
        ] {
            let manifest: PackageVersion = serde_json::from_value(serde_json::json!({
                "name":"bun", "version":"1.4.2", "bin":bin,
                "dist":{"tarball":"", "integrity":""}
            }))
            .unwrap();
            assert_eq!(manifest.name, "bun");
        }
    }
}
