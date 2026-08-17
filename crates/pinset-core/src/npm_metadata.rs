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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmToolRelease {
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpmToolTarget {
    pub target: &'static str,
    pub package: &'static str,
    pub required_path: &'static str,
}

pub const PNPM_TARGETS: &[NpmToolTarget] = &[
    NpmToolTarget {
        target: "windows-x86_64",
        package: "@pnpm/win-x64",
        required_path: "pnpm.exe",
    },
    NpmToolTarget {
        target: "linux-x86_64",
        package: "@pnpm/linux-x64",
        required_path: "pnpm",
    },
    NpmToolTarget {
        target: "linux-aarch64",
        package: "@pnpm/linux-arm64",
        required_path: "pnpm",
    },
    NpmToolTarget {
        target: "macos-aarch64",
        package: "@pnpm/macos-arm64",
        required_path: "pnpm",
    },
];

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

#[derive(Debug)]
pub struct NpmMetadataClient {
    client: Client,
    registry: Url,
}

#[derive(Debug, Deserialize)]
struct PackageDocument {
    #[serde(default)]
    versions: BTreeMap<String, PackageVersion>,
}

#[derive(Debug, Clone, Deserialize)]
struct PackageVersion {
    name: String,
    version: String,
    #[serde(rename = "optionalDependencies", default)]
    optional_dependencies: BTreeMap<String, String>,
    dist: PackageDist,
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
        let client = Client::builder()
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
        let targets = tool_targets(tool)?;
        let mut releases = Vec::new();
        for (declared_version, manifest) in document.versions {
            let Ok(version) = Version::parse(&declared_version) else {
                continue;
            };
            if manifest.version != declared_version
                || manifest.name != package
                || !supported_version(tool, &version)
                || !version.pre.is_empty()
                || !targets
                    .iter()
                    .all(|target| manifest.optional_dependencies.contains_key(target.package))
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
                reason: "package contains no stable release for every supported Pinset target"
                    .to_owned(),
            });
        }
        Ok(releases.into_iter().map(|(_, release)| release).collect())
    }

    pub fn resolve_version_selector(&self, tool: &str, selector: &str) -> Result<String> {
        if let Ok(version) = Version::parse(selector) {
            if version.pre.is_empty()
                && version.build.is_empty()
                && supported_version(tool, &version)
            {
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
        if !numeric && !matches!(normalized.as_str(), "latest" | "current") {
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
                if matches!(normalized.as_str(), "latest" | "current") {
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
        if !parsed.pre.is_empty() || !parsed.build.is_empty() || !supported_version(tool, &parsed) {
            return Err(Error::InvalidNpmToolSelector {
                tool: tool.to_owned(),
                selector: version.to_owned(),
            });
        }
        let wrapper = wrapper_package(tool)?;
        let manifest = self.package_version(wrapper, version)?;
        validate_manifest_identity(wrapper, version, &manifest)?;
        let keys = self.registry_keys()?;
        verify_package_signature(&manifest, &keys)?;
        let wrapper_overlay = if tool == "pnpm" && pnpm_uses_wrapper_overlay(&parsed) {
            let (canonical_url, artifact_path) =
                official_tarball(wrapper, version, &manifest.dist)?;
            crate::ArtifactIntegrity::parse(&manifest.dist.integrity)?;
            Some(LockedArtifactOverlay {
                canonical_url,
                artifact_path,
                integrity: manifest.dist.integrity.clone(),
                format: LockedArtifactFormat::TarGz,
                archive_root: "package".to_owned(),
                verification: SIGNATURE_VERIFICATION.to_owned(),
            })
        } else {
            None
        };
        let mut artifacts = Vec::new();
        for target in tool_targets(tool)? {
            let dependency = manifest
                .optional_dependencies
                .get(target.package)
                .ok_or_else(|| Error::InvalidNpmMetadata {
                    package: wrapper.to_owned(),
                    reason: format!("{version} does not declare {}", target.package),
                })?;
            let package_version =
                exact_dependency_version(dependency).ok_or_else(|| Error::InvalidNpmMetadata {
                    package: wrapper.to_owned(),
                    reason: format!("{} has non-exact version {dependency:?}", target.package),
                })?;
            let platform = self.package_version(target.package, package_version)?;
            validate_manifest_identity(target.package, package_version, &platform)?;
            verify_package_signature(&platform, &keys)?;
            let (canonical_url, artifact_path) =
                official_tarball(target.package, package_version, &platform.dist)?;
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
                overlays: wrapper_overlay.clone().into_iter().collect(),
            });
        }
        Ok(LockedTool {
            name: tool.to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: format!("{tool}-npm"),
            metadata: std::collections::BTreeMap::new(),
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
        "pnpm" => Ok(PNPM_TARGETS),
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
    if !parsed.pre.is_empty() || !parsed.build.is_empty() || !supported_version(tool, &parsed) {
        return Err(Error::InvalidNpmToolSelector {
            tool: tool.to_owned(),
            selector: version.to_owned(),
        });
    }
    Ok(())
}

fn wrapper_package(tool: &str) -> Result<&'static str> {
    match tool {
        "pnpm" => Ok("@pnpm/exe"),
        "bun" => Ok("bun"),
        _ => Err(Error::UnsupportedSourceProvider {
            provider: tool.to_owned(),
        }),
    }
}

fn supported_version(tool: &str, version: &Version) -> bool {
    match tool {
        "pnpm" => matches!(version.major, 10 | 11),
        "bun" => version.major == 1,
        _ => false,
    }
}

pub(crate) fn pnpm_uses_wrapper_overlay(version: &Version) -> bool {
    version.major >= 11
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
    fn target_manifests_cover_current_release_platforms_and_bun_cpu_variants() {
        assert_eq!(PNPM_TARGETS.len(), 4);
        assert!(
            BUN_TARGETS
                .iter()
                .any(|target| target.target.ends_with("-avx2"))
        );
        assert!(
            BUN_TARGETS
                .iter()
                .any(|target| target.target.ends_with("-baseline"))
        );
        assert!(
            PNPM_TARGETS
                .iter()
                .all(|target| !target.required_path.is_empty())
        );
        assert!(PNPM_TARGETS.iter().any(|target| {
            target.target == "linux-aarch64" && target.package == "@pnpm/linux-arm64"
        }));
        assert!(BUN_TARGETS.iter().any(|target| {
            target.target == "linux-aarch64" && target.package == "@oven/bun-linux-aarch64"
        }));
    }

    #[test]
    fn stable_support_windows_are_deliberately_narrow() {
        assert!(supported_version("pnpm", &Version::new(10, 0, 0)));
        assert!(supported_version("pnpm", &Version::new(11, 0, 0)));
        assert!(!supported_version("pnpm", &Version::new(12, 0, 0)));
        assert!(supported_version("bun", &Version::new(1, 3, 0)));
        assert!(!supported_version("bun", &Version::new(2, 0, 0)));
    }

    #[test]
    fn validates_exact_supported_versions_without_registry_metadata() {
        assert!(validate_exact_npm_tool_version("pnpm", "10.34.5").is_ok());
        assert!(validate_exact_npm_tool_version("pnpm", "11.21.0").is_ok());
        assert!(validate_exact_npm_tool_version("bun", "1.3.14").is_ok());
        for (tool, version) in [
            ("pnpm", "10"),
            ("pnpm", "12.0.0"),
            ("bun", "1.3.14-beta.1"),
            ("bun", "1.3.14+build"),
        ] {
            assert!(validate_exact_npm_tool_version(tool, version).is_err());
        }
    }

    #[test]
    fn pnpm_wrapper_overlay_matches_official_package_generations() {
        assert!(!pnpm_uses_wrapper_overlay(&Version::new(10, 34, 5)));
        assert!(pnpm_uses_wrapper_overlay(&Version::new(11, 21, 0)));
    }
}
