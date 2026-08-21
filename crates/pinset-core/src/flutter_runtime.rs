use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, FlutterArchiveFormat,
    InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result,
    SourceConfig, SourceKind, plan_flutter_artifact,
};

pub fn install_locked_flutter(
    installer: &Installer,
    pinset_home: &Path,
    source_config: &SourceConfig,
    locked_flutter: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_flutter.name != "flutter" || locked_flutter.provider != "flutter-official" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in Flutter provider",
                locked_flutter.name, locked_flutter.provider
            ),
        });
    }
    let artifact = locked_flutter
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "flutter".to_owned(),
            target: target.to_owned(),
        })?;
    let plan = plan_flutter_artifact(source_config, &locked_flutter.version, target)?;
    let sources = plan
        .sources
        .into_iter()
        .map(|source| ArtifactSource {
            id: source.alias,
            url: source.url,
            kind: match source.kind {
                SourceKind::Official => ArtifactSourceKind::Official,
                SourceKind::Custom => ArtifactSourceKind::Mirror,
            },
        })
        .collect();
    let required_paths = required_flutter_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "flutter".to_owned(),
        version: locked_flutter.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources,
            integrity: artifact.artifact_integrity()?.canonical(),
            format: match artifact.format {
                LockedArtifactFormat::Zip => ArtifactFormat::Zip,
                LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
                LockedArtifactFormat::TarGz => {
                    return Err(Error::InvalidLockfile {
                        reason: format!("Flutter artifact {target} cannot use tar.gz"),
                    });
                }
            },
        },
        strip_components: 1,
        include_prefixes: Vec::new(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if plan.format == FlutterArchiveFormat::TarXz {
            required_paths
        } else {
            Vec::new()
        },
        aliases: Vec::new(),
    };
    installer.install(&request)
}

fn required_flutter_paths(target: &str) -> Result<Vec<PathBuf>> {
    if target.starts_with("windows-") {
        Ok(vec![
            PathBuf::from("bin/flutter.bat"),
            PathBuf::from("bin/dart.bat"),
            PathBuf::from("bin/cache/dart-sdk/bin/dart.exe"),
        ])
    } else if target.starts_with("linux-") || target.starts_with("macos-") {
        Ok(vec![
            PathBuf::from("bin/flutter"),
            PathBuf::from("bin/dart"),
            PathBuf::from("bin/cache/dart-sdk/bin/dart"),
        ])
    } else {
        Err(Error::UnsupportedFlutterTarget {
            target: target.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_flutter_and_its_bundled_dart_sdk() {
        assert_eq!(
            required_flutter_paths("linux-x86_64").expect("linux paths"),
            [
                PathBuf::from("bin/flutter"),
                PathBuf::from("bin/dart"),
                PathBuf::from("bin/cache/dart-sdk/bin/dart"),
            ]
        );
        assert_eq!(
            required_flutter_paths("windows-x86_64").expect("windows paths"),
            [
                PathBuf::from("bin/flutter.bat"),
                PathBuf::from("bin/dart.bat"),
                PathBuf::from("bin/cache/dart-sdk/bin/dart.exe"),
            ]
        );
    }
}
