use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, GoArchiveFormat,
    InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result,
    SourceConfig, SourceKind, plan_go_artifact,
};

pub fn install_locked_go(
    installer: &Installer,
    pinset_home: &Path,
    source_config: &SourceConfig,
    locked_go: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_go.name != "go" || locked_go.provider != "go-official" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in Go provider",
                locked_go.name, locked_go.provider
            ),
        });
    }
    let artifact = locked_go
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "go".to_owned(),
            target: target.to_owned(),
        })?;
    let plan = plan_go_artifact(source_config, &locked_go.version, target)?;
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
    let required_paths = required_go_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "go".to_owned(),
        version: locked_go.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources,
            integrity: artifact.artifact_integrity()?.canonical(),
            format: match artifact.format {
                LockedArtifactFormat::Zip => ArtifactFormat::Zip,
                LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
                LockedArtifactFormat::TarXz => {
                    return Err(Error::InvalidLockfile {
                        reason: format!("Go artifact {target} cannot use tar.xz"),
                    });
                }
            },
        },
        strip_components: 1,
        include_prefixes: Vec::new(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if plan.format == GoArchiveFormat::TarGz {
            required_paths
        } else {
            Vec::new()
        },
        aliases: Vec::new(),
    };
    installer.install(&request)
}

fn required_go_paths(target: &str) -> Result<Vec<PathBuf>> {
    if target.starts_with("windows-") {
        Ok(vec![
            PathBuf::from("bin/go.exe"),
            PathBuf::from("bin/gofmt.exe"),
        ])
    } else if target.starts_with("linux-") || target.starts_with("macos-") {
        Ok(vec![PathBuf::from("bin/go"), PathBuf::from("bin/gofmt")])
    } else {
        Err(Error::UnsupportedGoTarget {
            target: target.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_go_and_gofmt_from_the_sdk_bin_directory() {
        assert_eq!(
            required_go_paths("linux-x86_64").expect("linux paths"),
            [PathBuf::from("bin/go"), PathBuf::from("bin/gofmt")]
        );
        assert_eq!(
            required_go_paths("windows-x86_64").expect("windows paths"),
            [PathBuf::from("bin/go.exe"), PathBuf::from("bin/gofmt.exe")]
        );
    }
}
