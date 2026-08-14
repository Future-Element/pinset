use crate::{Error, ResolvedArtifactSource, Result, SourceConfig};

pub const PYTHON_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

pub const PYTHON_VARIANT: &str = "install_only";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonArtifactPlan {
    pub distribution: String,
    pub python_version: String,
    pub build_id: String,
    pub target: String,
    pub platform: &'static str,
    pub variant: &'static str,
    pub artifact_path: String,
    pub archive_root: String,
    pub canonical_url: String,
    pub sources: Vec<ResolvedArtifactSource>,
}

pub fn plan_python_artifact(
    config: &SourceConfig,
    distribution: &str,
    target: &str,
) -> Result<PythonArtifactPlan> {
    let (python_version, build_id) = parse_python_distribution(distribution)?;
    let platform = python_platform(target)?;
    let filename =
        format!("cpython-{python_version}+{build_id}-{platform}-{PYTHON_VARIANT}.tar.gz");
    let artifact_path = format!("{build_id}/{filename}");
    let canonical_url = config.official_artifact_url("python", &artifact_path)?;
    let sources = config.resolve_artifact_sources("python", &artifact_path)?;

    Ok(PythonArtifactPlan {
        distribution: distribution.to_owned(),
        python_version,
        build_id,
        target: target.to_owned(),
        platform,
        variant: PYTHON_VARIANT,
        artifact_path,
        archive_root: "python".to_owned(),
        canonical_url,
        sources,
    })
}

pub fn validate_exact_python_version(version: &str) -> Result<()> {
    parse_python_distribution(version).map(|_| ())
}

pub fn parse_python_distribution(distribution: &str) -> Result<(String, String)> {
    let Some((version, build_id)) = distribution.split_once('+') else {
        return Err(Error::InvalidPythonVersion {
            version: distribution.to_owned(),
        });
    };
    if distribution.matches('+').count() != 1
        || !is_exact_python_version(version)
        || build_id.len() != 8
        || !build_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Error::InvalidPythonVersion {
            version: distribution.to_owned(),
        });
    }
    Ok((version.to_owned(), build_id.to_owned()))
}

pub fn is_exact_python_version(version: &str) -> bool {
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u64>().is_ok()
        })
}

fn python_platform(target: &str) -> Result<&'static str> {
    match target {
        "windows-x86_64" => Ok("x86_64-pc-windows-msvc"),
        "linux-x86_64" => Ok("x86_64-unknown-linux-gnu"),
        "macos-x86_64" => Ok("x86_64-apple-darwin"),
        "macos-aarch64" => Ok("aarch64-apple-darwin"),
        _ => Err(Error::UnsupportedPythonTarget {
            target: target.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_install_only_archives_for_all_pinset_targets() {
        let config = SourceConfig::default();
        let windows = plan_python_artifact(&config, "3.14.7+20260807", "windows-x86_64")
            .expect("Windows plan");
        assert_eq!(windows.platform, "x86_64-pc-windows-msvc");
        assert_eq!(windows.variant, "install_only");
        assert_eq!(windows.archive_root, "python");
        assert_eq!(
            windows.artifact_path,
            "20260807/cpython-3.14.7+20260807-x86_64-pc-windows-msvc-install_only.tar.gz"
        );

        let macos =
            plan_python_artifact(&config, "3.14.7+20260807", "macos-aarch64").expect("macOS plan");
        assert_eq!(macos.platform, "aarch64-apple-darwin");
    }

    #[test]
    fn rejects_unlocked_versions_and_unknown_targets() {
        for version in [
            "3",
            "3.14",
            "3.14.7",
            "v3.14.7+20260807",
            "3.15.0rc1+20260807",
        ] {
            assert!(matches!(
                validate_exact_python_version(version),
                Err(Error::InvalidPythonVersion { .. })
            ));
        }
        assert!(matches!(
            plan_python_artifact(&SourceConfig::default(), "3.14.7+20260807", "linux-aarch64"),
            Err(Error::UnsupportedPythonTarget { .. })
        ));
    }
}
