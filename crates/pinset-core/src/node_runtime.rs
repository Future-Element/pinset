use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, InstallOutcome,
    InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result, SourceConfig, SourceKind,
};

pub fn install_locked_node(
    installer: &Installer,
    pinset_home: &Path,
    source_config: &SourceConfig,
    locked_node: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    let artifact = locked_node
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "node".to_owned(),
            target: target.to_owned(),
        })?;
    let sources = source_config
        .resolve_artifact_sources("node", &artifact.artifact_path)?
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
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "node".to_owned(),
        version: locked_node.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources,
            sha256: artifact.sha256.clone(),
            format: match artifact.format {
                LockedArtifactFormat::Zip => ArtifactFormat::Zip,
                LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
            },
        },
        strip_components: 1,
        required_paths: required_node_paths(target)?,
    };
    installer.install(&request)
}

pub fn node_command_directory(install_dir: &Path, target: &str) -> Result<PathBuf> {
    if target.starts_with("windows-") {
        Ok(install_dir.to_path_buf())
    } else if target.starts_with("linux-") || target.starts_with("macos-") {
        Ok(install_dir.join("bin"))
    } else {
        Err(Error::UnsupportedNodeTarget {
            target: target.to_owned(),
        })
    }
}

fn required_node_paths(target: &str) -> Result<Vec<PathBuf>> {
    if target.starts_with("windows-") {
        Ok(vec![PathBuf::from("node.exe")])
    } else if target.starts_with("linux-") || target.starts_with("macos-") {
        Ok(vec![PathBuf::from("bin/node")])
    } else {
        Err(Error::UnsupportedNodeTarget {
            target: target.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_node_commands_to_native_archive_layout() {
        let root = Path::new("/pinset/node");
        assert_eq!(
            node_command_directory(root, "windows-x86_64").expect("windows"),
            root
        );
        assert_eq!(
            node_command_directory(root, "linux-x86_64").expect("linux"),
            root.join("bin")
        );
        assert_eq!(
            node_command_directory(root, "macos-aarch64").expect("macOS"),
            root.join("bin")
        );
    }
}
