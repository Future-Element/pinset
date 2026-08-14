use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, DotnetArchiveFormat, Error,
    InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result,
    plan_dotnet_artifact,
};

pub fn install_locked_dotnet(
    installer: &Installer,
    pinset_home: &Path,
    locked_dotnet: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_dotnet.name != "dotnet" || locked_dotnet.provider != "microsoft-dotnet-sdk" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in Microsoft .NET SDK provider",
                locked_dotnet.name, locked_dotnet.provider
            ),
        });
    }
    let artifact = locked_dotnet
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "dotnet".to_owned(),
            target: target.to_owned(),
        })?;
    let plan = plan_dotnet_artifact(&locked_dotnet.version, target, &artifact.canonical_url)?;
    let required_paths = required_dotnet_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "dotnet".to_owned(),
        version: locked_dotnet.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "official".to_owned(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: artifact.artifact_integrity()?.canonical(),
            format: match (artifact.format, plan.format) {
                (LockedArtifactFormat::Zip, DotnetArchiveFormat::Zip) => ArtifactFormat::Zip,
                (LockedArtifactFormat::TarGz, DotnetArchiveFormat::TarGz) => ArtifactFormat::TarGz,
                _ => {
                    return Err(Error::InvalidLockfile {
                        reason: format!(".NET SDK artifact {target} has an invalid format"),
                    });
                }
            },
        },
        strip_components: 0,
        include_prefixes: Vec::new(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if plan.format == DotnetArchiveFormat::TarGz {
            required_paths
        } else {
            Vec::new()
        },
    };
    installer.install(&request)
}

fn required_dotnet_paths(target: &str) -> Result<Vec<PathBuf>> {
    let command = if target == "windows-x86_64" {
        "dotnet.exe"
    } else if matches!(target, "linux-x86_64" | "macos-x86_64" | "macos-aarch64") {
        "dotnet"
    } else {
        return Err(Error::UnsupportedDotnetTarget {
            target: target.to_owned(),
        });
    };
    Ok(vec![PathBuf::from(command)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_the_root_dotnet_host() {
        assert_eq!(
            required_dotnet_paths("linux-x86_64").expect("Linux paths"),
            [PathBuf::from("dotnet")]
        );
        assert_eq!(
            required_dotnet_paths("windows-x86_64").expect("Windows paths"),
            [PathBuf::from("dotnet.exe")]
        );
    }
}
