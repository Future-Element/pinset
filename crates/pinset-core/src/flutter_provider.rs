use crate::{Error, ResolvedArtifactSource, Result, SourceConfig};

pub const FLUTTER_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlutterArchiveFormat {
    Zip,
    TarXz,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterArtifactPlan {
    pub version: String,
    pub target: String,
    pub dart_arch: &'static str,
    pub release_path: String,
    pub artifact_path: String,
    pub archive_root: String,
    pub format: FlutterArchiveFormat,
    pub canonical_url: String,
    pub sources: Vec<ResolvedArtifactSource>,
}

pub fn plan_flutter_artifact(
    config: &SourceConfig,
    version: &str,
    target: &str,
) -> Result<FlutterArtifactPlan> {
    validate_exact_flutter_version(version)?;
    let (release_path, dart_arch, format) = match target {
        "windows-x86_64" => (
            format!("stable/windows/flutter_windows_{version}-stable.zip"),
            "x64",
            FlutterArchiveFormat::Zip,
        ),
        "linux-x86_64" => (
            format!("stable/linux/flutter_linux_{version}-stable.tar.xz"),
            "x64",
            FlutterArchiveFormat::TarXz,
        ),
        "macos-x86_64" => (
            format!("stable/macos/flutter_macos_{version}-stable.zip"),
            "x64",
            FlutterArchiveFormat::Zip,
        ),
        "macos-aarch64" => (
            format!("stable/macos/flutter_macos_arm64_{version}-stable.zip"),
            "arm64",
            FlutterArchiveFormat::Zip,
        ),
        _ => {
            return Err(Error::UnsupportedFlutterTarget {
                target: target.to_owned(),
            });
        }
    };
    let artifact_path = format!("flutter_infra_release/releases/{release_path}");
    let canonical_url = config.official_artifact_url("flutter", &artifact_path)?;
    let sources = config.resolve_artifact_sources("flutter", &artifact_path)?;

    Ok(FlutterArtifactPlan {
        version: version.to_owned(),
        target: target.to_owned(),
        dart_arch,
        release_path,
        artifact_path,
        archive_root: "flutter".to_owned(),
        format,
        canonical_url,
        sources,
    })
}

pub fn validate_exact_flutter_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u64>().is_err()
        })
    {
        return Err(Error::InvalidFlutterVersion {
            version: version.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_official_stable_archives_for_every_pinset_target() {
        let config = SourceConfig::default();
        let linux = plan_flutter_artifact(&config, "3.47.0", "linux-x86_64").expect("linux plan");
        assert_eq!(linux.format, FlutterArchiveFormat::TarXz);
        assert_eq!(
            linux.canonical_url,
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_3.47.0-stable.tar.xz"
        );

        let macos = plan_flutter_artifact(&config, "3.47.0", "macos-aarch64").expect("macOS plan");
        assert_eq!(macos.format, FlutterArchiveFormat::Zip);
        assert_eq!(macos.dart_arch, "arm64");
        assert_eq!(
            macos.release_path,
            "stable/macos/flutter_macos_arm64_3.47.0-stable.zip"
        );

        let windows =
            plan_flutter_artifact(&config, "3.47.0", "windows-x86_64").expect("windows plan");
        assert_eq!(
            windows.release_path,
            "stable/windows/flutter_windows_3.47.0-stable.zip"
        );
    }

    #[test]
    fn rejects_non_exact_versions_and_unsupported_targets() {
        for version in ["3", "3.47", "v3.47.0", "3.47.0-beta"] {
            assert!(matches!(
                plan_flutter_artifact(&SourceConfig::default(), version, "linux-x86_64"),
                Err(Error::InvalidFlutterVersion { .. })
            ));
        }
        assert!(matches!(
            plan_flutter_artifact(&SourceConfig::default(), "3.47.0", "linux-aarch64"),
            Err(Error::UnsupportedFlutterTarget { .. })
        ));
    }
}
