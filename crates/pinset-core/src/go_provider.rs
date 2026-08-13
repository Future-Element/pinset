use crate::{Error, ResolvedArtifactSource, Result, SourceConfig};

pub const GO_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoArchiveFormat {
    Zip,
    TarGz,
}

impl GoArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoArtifactPlan {
    pub version: String,
    pub upstream_version: String,
    pub target: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub artifact_path: String,
    pub archive_root: String,
    pub format: GoArchiveFormat,
    pub canonical_url: String,
    pub sources: Vec<ResolvedArtifactSource>,
}

pub fn plan_go_artifact(
    config: &SourceConfig,
    version: &str,
    target: &str,
) -> Result<GoArtifactPlan> {
    validate_exact_go_version(version)?;
    let (os, arch, format) = go_platform(target)?;
    let upstream_version = upstream_go_version(version);
    let artifact_path = format!("go{upstream_version}.{os}-{arch}.{}", format.as_str());
    let canonical_url = config.official_artifact_url("go", &artifact_path)?;
    let sources = config.resolve_artifact_sources("go", &artifact_path)?;

    Ok(GoArtifactPlan {
        version: version.to_owned(),
        upstream_version,
        target: target.to_owned(),
        os,
        arch,
        artifact_path,
        archive_root: "go".to_owned(),
        format,
        canonical_url,
        sources,
    })
}

pub fn validate_exact_go_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u64>().is_err()
        })
    {
        return Err(Error::InvalidGoVersion {
            version: version.to_owned(),
        });
    }
    Ok(())
}

fn upstream_go_version(version: &str) -> String {
    let parts = version
        .split('.')
        .map(|part| part.parse::<u64>().expect("validated Go version"))
        .collect::<Vec<_>>();
    if parts[0] == 1 && parts[1] < 21 && parts[2] == 0 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        version.to_owned()
    }
}

fn go_platform(target: &str) -> Result<(&'static str, &'static str, GoArchiveFormat)> {
    match target {
        "windows-x86_64" => Ok(("windows", "amd64", GoArchiveFormat::Zip)),
        "windows-aarch64" => Ok(("windows", "arm64", GoArchiveFormat::Zip)),
        "linux-x86_64" => Ok(("linux", "amd64", GoArchiveFormat::TarGz)),
        "linux-aarch64" => Ok(("linux", "arm64", GoArchiveFormat::TarGz)),
        "macos-x86_64" => Ok(("darwin", "amd64", GoArchiveFormat::TarGz)),
        "macos-aarch64" => Ok(("darwin", "arm64", GoArchiveFormat::TarGz)),
        _ => Err(Error::UnsupportedGoTarget {
            target: target.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_go_archives_for_pinset_targets() {
        let config = SourceConfig::default();
        let windows = plan_go_artifact(&config, "1.25.1", "windows-x86_64").expect("windows plan");
        assert_eq!(windows.format, GoArchiveFormat::Zip);
        assert_eq!(windows.artifact_path, "go1.25.1.windows-amd64.zip");
        assert_eq!(
            windows.canonical_url,
            "https://go.dev/dl/go1.25.1.windows-amd64.zip"
        );

        let macos = plan_go_artifact(&config, "1.25.1", "macos-aarch64").expect("macOS plan");
        assert_eq!(macos.format, GoArchiveFormat::TarGz);
        assert_eq!(macos.artifact_path, "go1.25.1.darwin-arm64.tar.gz");
    }

    #[test]
    fn preserves_historical_go_patch_zero_archive_names() {
        let plan = plan_go_artifact(&SourceConfig::default(), "1.20.0", "linux-x86_64")
            .expect("historical plan");
        assert_eq!(plan.upstream_version, "1.20");
        assert_eq!(plan.artifact_path, "go1.20.linux-amd64.tar.gz");
    }

    #[test]
    fn rejects_floating_versions_and_unknown_targets() {
        for version in ["1", "1.25", "go1.25.0", "v1.25.0", "1.25.0-rc1"] {
            assert!(matches!(
                plan_go_artifact(&SourceConfig::default(), version, "linux-x86_64"),
                Err(Error::InvalidGoVersion { .. })
            ));
        }
        assert!(matches!(
            plan_go_artifact(&SourceConfig::default(), "1.25.0", "linux-riscv64"),
            Err(Error::UnsupportedGoTarget { .. })
        ));
    }
}
