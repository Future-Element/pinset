use std::{cmp::Ordering, fmt};

use url::Url;

use crate::{Error, Result};

pub const JAVA_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaArchiveFormat {
    Zip,
    TarGz,
}

impl JavaArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JavaVersion {
    pub major: u64,
    pub minor: u64,
    pub security: u64,
    pub patch: u64,
    pub build: u64,
}

impl JavaVersion {
    pub fn parse(value: &str) -> Result<Self> {
        let Some((version, build)) = value.split_once('+') else {
            return Err(Error::InvalidJavaVersion {
                version: value.to_owned(),
            });
        };
        if build.is_empty()
            || build.contains('+')
            || !build.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(Error::InvalidJavaVersion {
                version: value.to_owned(),
            });
        }
        let parts = version.split('.').collect::<Vec<_>>();
        if !(parts.len() == 3 || parts.len() == 4)
            || parts.iter().any(|part| {
                part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(Error::InvalidJavaVersion {
                version: value.to_owned(),
            });
        }
        let numbers = parts
            .iter()
            .map(|part| part.parse::<u64>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidJavaVersion {
                version: value.to_owned(),
            })?;
        let parsed = Self {
            major: numbers[0],
            minor: numbers[1],
            security: numbers[2],
            patch: numbers.get(3).copied().unwrap_or(0),
            build: build.parse().map_err(|_| Error::InvalidJavaVersion {
                version: value.to_owned(),
            })?,
        };
        if parsed.major == 0 || parsed.to_string() != value {
            return Err(Error::InvalidJavaVersion {
                version: value.to_owned(),
            });
        }
        Ok(parsed)
    }

    pub const fn feature(self) -> u64 {
        self.major
    }
}

impl fmt::Display for JavaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.patch == 0 {
            write!(
                formatter,
                "{}.{}.{}+{}",
                self.major, self.minor, self.security, self.build
            )
        } else {
            write!(
                formatter,
                "{}.{}.{}.{}+{}",
                self.major, self.minor, self.security, self.patch, self.build
            )
        }
    }
}

impl Ord for JavaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.major,
            self.minor,
            self.security,
            self.patch,
            self.build,
        )
            .cmp(&(
                other.major,
                other.minor,
                other.security,
                other.patch,
                other.build,
            ))
    }
}

impl PartialOrd for JavaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaArtifactPlan {
    pub version: String,
    pub target: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub artifact_path: String,
    pub archive_root: String,
    pub format: JavaArchiveFormat,
    pub canonical_url: String,
}

pub fn plan_java_artifact(
    version: &str,
    release_name: &str,
    target: &str,
    package_name: &str,
    canonical_url: &str,
) -> Result<JavaArtifactPlan> {
    let version = JavaVersion::parse(version)?;
    let (os, arch, format) = java_platform(target)?;
    validate_release_name(release_name)?;
    validate_package_name(&version, os, arch, format, package_name)?;
    let url = Url::parse(canonical_url).map_err(|source| Error::InvalidJavaArtifact {
        reason: format!("invalid package URL: {source}"),
    })?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidJavaArtifact {
            reason: "package URL must be an unqualified HTTPS github.com URL".to_owned(),
        });
    }
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let expected_repository = format!("temurin{}-binaries", version.major);
    if segments.len() != 6
        || segments[0] != "adoptium"
        || segments[1] != expected_repository
        || segments[2] != "releases"
        || segments[3] != "download"
        || !release_segment_matches(segments[4], release_name)
        || segments[5] != package_name
    {
        return Err(Error::InvalidJavaArtifact {
            reason: format!(
                "package URL does not identify adoptium/{expected_repository} release {release_name}"
            ),
        });
    }
    let artifact_path = url.path().trim_start_matches('/').to_owned();
    Ok(JavaArtifactPlan {
        version: version.to_string(),
        target: target.to_owned(),
        os,
        arch,
        artifact_path,
        archive_root: release_name.to_owned(),
        format,
        canonical_url: url.to_string(),
    })
}

pub fn validate_exact_java_version(version: &str) -> Result<()> {
    JavaVersion::parse(version).map(|_| ())
}

fn java_platform(target: &str) -> Result<(&'static str, &'static str, JavaArchiveFormat)> {
    match target {
        "windows-x86_64" => Ok(("windows", "x64", JavaArchiveFormat::Zip)),
        "linux-x86_64" => Ok(("linux", "x64", JavaArchiveFormat::TarGz)),
        "macos-x86_64" => Ok(("mac", "x64", JavaArchiveFormat::TarGz)),
        "macos-aarch64" => Ok(("mac", "aarch64", JavaArchiveFormat::TarGz)),
        _ => Err(Error::UnsupportedJavaTarget {
            target: target.to_owned(),
        }),
    }
}

fn validate_release_name(release_name: &str) -> Result<()> {
    if release_name.is_empty()
        || !release_name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'_' | b'-')
        })
    {
        return Err(Error::InvalidJavaArtifact {
            reason: format!("invalid release name {release_name:?}"),
        });
    }
    Ok(())
}

fn validate_package_name(
    version: &JavaVersion,
    os: &str,
    arch: &str,
    format: JavaArchiveFormat,
    package_name: &str,
) -> Result<()> {
    let prefix = format!("OpenJDK{}U-jdk_{arch}_{os}_hotspot_", version.major);
    let suffix = format!(".{}", format.as_str());
    if !package_name.starts_with(&prefix)
        || !package_name.ends_with(&suffix)
        || package_name.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'+' | b'_' | b'-'))
        })
    {
        return Err(Error::InvalidJavaArtifact {
            reason: format!("package {package_name:?} does not match {os}/{arch} JDK archive"),
        });
    }
    Ok(())
}

fn release_segment_matches(segment: &str, release_name: &str) -> bool {
    segment == release_name
        || segment.eq_ignore_ascii_case(&release_name.replace('+', "%2B"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_version_orders_build_as_part_of_the_release_identity() {
        let older = JavaVersion::parse("21.0.8+8").expect("older");
        let newer = JavaVersion::parse("21.0.8+9").expect("newer");
        let next_update = JavaVersion::parse("21.0.9+1").expect("next update");
        assert!(older < newer);
        assert!(newer < next_update);
        assert_eq!(newer.to_string(), "21.0.8+9");
    }

    #[test]
    fn rejects_floating_and_noncanonical_exact_versions() {
        for value in ["21", "21.0", "21.0.8", "v21.0.8+9", "21.0.8+09"] {
            assert!(matches!(
                JavaVersion::parse(value),
                Err(Error::InvalidJavaVersion { .. })
            ));
        }
    }

    #[test]
    fn validates_official_temurin_archives() {
        let plan = plan_java_artifact(
            "21.0.8+9",
            "jdk-21.0.8+9",
            "macos-aarch64",
            "OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.8_9.tar.gz",
            "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.8%2B9/OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.8_9.tar.gz",
        )
        .expect("plan");
        assert_eq!(plan.format, JavaArchiveFormat::TarGz);
        assert_eq!(plan.archive_root, "jdk-21.0.8+9");
    }

    #[test]
    fn rejects_cross_target_or_nonofficial_archives() {
        assert!(plan_java_artifact(
            "21.0.8+9",
            "jdk-21.0.8+9",
            "linux-x86_64",
            "OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.8_9.tar.gz",
            "https://example.com/archive.tar.gz",
        )
        .is_err());
    }
}
