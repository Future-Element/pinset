use std::{cmp::Ordering, fmt};

use url::Url;

use crate::{Error, Result};

pub const DOTNET_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotnetArchiveFormat {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DotnetVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl DotnetVersion {
    pub fn parse(value: &str) -> Result<Self> {
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || (part.len() > 1 && part.starts_with('0'))
            })
        {
            return Err(Error::InvalidDotnetVersion {
                version: value.to_owned(),
            });
        }
        let numbers = parts
            .iter()
            .map(|part| part.parse::<u64>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidDotnetVersion {
                version: value.to_owned(),
            })?;
        let version = Self {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
        };
        if version.major == 0 || version.to_string() != value {
            return Err(Error::InvalidDotnetVersion {
                version: value.to_owned(),
            });
        }
        Ok(version)
    }

    pub fn channel(self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

impl fmt::Display for DotnetVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for DotnetVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for DotnetVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotnetArtifactPlan {
    pub version: String,
    pub target: String,
    pub rid: &'static str,
    pub artifact_path: String,
    pub archive_root: String,
    pub format: DotnetArchiveFormat,
    pub canonical_url: String,
}

pub fn plan_dotnet_artifact(
    version: &str,
    target: &str,
    canonical_url: &str,
) -> Result<DotnetArtifactPlan> {
    let version = DotnetVersion::parse(version)?;
    let (rid, format, suffix) = dotnet_platform(target)?;
    let archive_name = format!("dotnet-sdk-{version}-{rid}.{suffix}");
    let artifact_path = format!("dotnet/Sdk/{version}/{archive_name}");
    let expected_url = format!("https://builds.dotnet.microsoft.com/{artifact_path}");
    let url = Url::parse(canonical_url).map_err(|source| Error::InvalidDotnetArtifact {
        reason: format!("invalid archive URL: {source}"),
    })?;
    if url.scheme() != "https"
        || url.host_str() != Some("builds.dotnet.microsoft.com")
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != expected_url
    {
        return Err(Error::InvalidDotnetArtifact {
            reason: format!("archive URL must be {expected_url}"),
        });
    }
    Ok(DotnetArtifactPlan {
        version: version.to_string(),
        target: target.to_owned(),
        rid,
        artifact_path,
        archive_root: String::new(),
        format,
        canonical_url: url.to_string(),
    })
}

pub fn validate_exact_dotnet_version(version: &str) -> Result<()> {
    DotnetVersion::parse(version).map(|_| ())
}

pub fn dotnet_rid(target: &str) -> Result<&'static str> {
    dotnet_platform(target).map(|(rid, _, _)| rid)
}

fn dotnet_platform(target: &str) -> Result<(&'static str, DotnetArchiveFormat, &'static str)> {
    match target {
        "windows-x86_64" => Ok(("win-x64", DotnetArchiveFormat::Zip, "zip")),
        "linux-x86_64" => Ok(("linux-x64", DotnetArchiveFormat::TarGz, "tar.gz")),
        "macos-x86_64" => Ok(("osx-x64", DotnetArchiveFormat::TarGz, "tar.gz")),
        "macos-aarch64" => Ok(("osx-arm64", DotnetArchiveFormat::TarGz, "tar.gz")),
        _ => Err(Error::UnsupportedDotnetTarget {
            target: target.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_exact_sdk_versions() {
        let older = DotnetVersion::parse("10.0.399").expect("older");
        let newer = DotnetVersion::parse("10.0.400").expect("newer");
        assert!(older < newer);
        assert_eq!(newer.channel(), "10.0");
        for invalid in [
            "latest",
            "10",
            "10.0",
            "v10.0.400",
            "10.00.400",
            "10.0.0400",
        ] {
            assert!(DotnetVersion::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn validates_official_sdk_archives() {
        let plan = plan_dotnet_artifact(
            "10.0.400",
            "macos-aarch64",
            "https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.400/dotnet-sdk-10.0.400-osx-arm64.tar.gz",
        )
        .expect("plan");
        assert_eq!(plan.rid, "osx-arm64");
        assert_eq!(plan.archive_root, "");
        assert_eq!(plan.format, DotnetArchiveFormat::TarGz);
    }

    #[test]
    fn rejects_cross_target_or_nonofficial_archives() {
        assert!(
            plan_dotnet_artifact(
                "10.0.400",
                "linux-x86_64",
                "https://example.invalid/dotnet-sdk-10.0.400-linux-x64.tar.gz",
            )
            .is_err()
        );
    }
}
