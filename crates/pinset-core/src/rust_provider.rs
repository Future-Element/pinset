use std::{cmp::Ordering, fmt};

use url::Url;

use crate::{Error, Result};

pub const RUST_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

pub const RUST_PROFILE: &str = "default";
pub const RUST_COMPONENTS: &str = "rustc,cargo,rust-std,rust-docs,rustfmt,clippy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustArchiveFormat {
    TarXz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RustVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl RustVersion {
    pub fn parse(value: &str) -> Result<Self> {
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(Error::InvalidRustVersion {
                version: value.to_owned(),
            });
        }
        let numbers = parts
            .iter()
            .map(|part| part.parse::<u64>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidRustVersion {
                version: value.to_owned(),
            })?;
        let version = Self {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
        };
        if version.major == 0 || version.to_string() != value {
            return Err(Error::InvalidRustVersion {
                version: value.to_owned(),
            });
        }
        Ok(version)
    }
}

impl fmt::Display for RustVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for RustVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for RustVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustArtifactPlan {
    pub version: String,
    pub target: String,
    pub triple: &'static str,
    pub date: String,
    pub artifact_path: String,
    pub archive_root: String,
    pub format: RustArchiveFormat,
    pub canonical_url: String,
}

pub fn plan_rust_artifact(
    version: &str,
    date: &str,
    target: &str,
    canonical_url: &str,
) -> Result<RustArtifactPlan> {
    let version = RustVersion::parse(version)?;
    validate_release_date(date)?;
    let triple = rust_target_triple(target)?;
    let archive_name = format!("rust-{version}-{triple}.tar.xz");
    let expected_url = format!("https://static.rust-lang.org/dist/{date}/{archive_name}");
    let url = Url::parse(canonical_url).map_err(|source| Error::InvalidRustArtifact {
        reason: format!("invalid archive URL: {source}"),
    })?;
    if url.scheme() != "https"
        || url.host_str() != Some("static.rust-lang.org")
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != expected_url
    {
        return Err(Error::InvalidRustArtifact {
            reason: format!("archive URL must be {expected_url}"),
        });
    }
    Ok(RustArtifactPlan {
        version: version.to_string(),
        target: target.to_owned(),
        triple,
        date: date.to_owned(),
        artifact_path: format!("dist/{date}/{archive_name}"),
        archive_root: format!("rust-{version}-{triple}"),
        format: RustArchiveFormat::TarXz,
        canonical_url: url.to_string(),
    })
}

pub fn validate_exact_rust_version(version: &str) -> Result<()> {
    RustVersion::parse(version).map(|_| ())
}

pub fn rust_target_triple(target: &str) -> Result<&'static str> {
    match target {
        "windows-x86_64" => Ok("x86_64-pc-windows-msvc"),
        "linux-x86_64" => Ok("x86_64-unknown-linux-gnu"),
        "macos-x86_64" => Ok("x86_64-apple-darwin"),
        "macos-aarch64" => Ok("aarch64-apple-darwin"),
        _ => Err(Error::UnsupportedRustTarget {
            target: target.to_owned(),
        }),
    }
}

fn validate_release_date(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(Error::InvalidRustArtifact {
            reason: format!("invalid release date {value:?}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_exact_stable_versions() {
        let older = RustVersion::parse("1.96.1").expect("older");
        let newer = RustVersion::parse("1.97.0").expect("newer");
        assert!(older < newer);
        assert_eq!(newer.to_string(), "1.97.0");
        for invalid in ["stable", "1.97", "v1.97.0", "1.097.0", "1.97.0-beta.1"] {
            assert!(RustVersion::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn validates_official_combined_toolchain_archives() {
        let plan = plan_rust_artifact(
            "1.97.1",
            "2026-07-16",
            "macos-aarch64",
            "https://static.rust-lang.org/dist/2026-07-16/rust-1.97.1-aarch64-apple-darwin.tar.xz",
        )
        .expect("plan");
        assert_eq!(plan.triple, "aarch64-apple-darwin");
        assert_eq!(plan.archive_root, "rust-1.97.1-aarch64-apple-darwin");
        assert_eq!(plan.format, RustArchiveFormat::TarXz);
    }

    #[test]
    fn rejects_cross_target_or_nonofficial_archives() {
        assert!(
            plan_rust_artifact(
                "1.97.1",
                "2026-07-16",
                "linux-x86_64",
                "https://example.invalid/rust-1.97.1-x86_64-unknown-linux-gnu.tar.xz",
            )
            .is_err()
        );
    }
}
