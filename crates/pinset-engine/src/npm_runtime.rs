use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, InstallAlias,
    InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result,
    tool_targets,
};

pub fn install_locked_npm_tool(
    installer: &Installer,
    pinset_home: &Path,
    locked_tool: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if !matches!(locked_tool.name.as_str(), "pnpm" | "bun") {
        return Err(Error::InvalidLockfile {
            reason: format!("{} is not an npm-distributed tool", locked_tool.name),
        });
    }
    let artifact = locked_tool
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked_tool.name.clone(),
            version: locked_tool.version.clone(),
            target: target.to_owned(),
        })?;
    if locked_tool.name == "pnpm" {
        return installer.install(&InstallRequest {
            pinset_home: pinset_home.to_path_buf(),
            tool: "pnpm".into(),
            version: locked_tool.version.clone(),
            target: target.into(),
            artifact: ArtifactSpec {
                canonical_url: artifact.canonical_url.clone(),
                sources: vec![ArtifactSource {
                    id: "npm-official".into(),
                    url: artifact.canonical_url.clone(),
                    kind: ArtifactSourceKind::Official,
                }],
                integrity: artifact.artifact_integrity()?.canonical(),
                format: ArtifactFormat::TarGz,
            },
            strip_components: 1,
            include_prefixes: vec![],
            required_paths: vec![PathBuf::from("bin/pnpm.cjs"), PathBuf::from("package.json")],
            base_artifacts: vec![],
            executable_paths: vec![],
            aliases: vec![],
        });
    }
    let target_manifest = tool_targets(&locked_tool.name)?
        .iter()
        .find(|candidate| candidate.target == target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked_tool.name.clone(),
            version: locked_tool.version.clone(),
            target: target.to_owned(),
        })?;
    let format = match artifact.format {
        LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
        LockedArtifactFormat::Zip => ArtifactFormat::Zip,
        LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
        LockedArtifactFormat::Binary => {
            return Err(Error::InvalidLockfile {
                reason: format!("{} artifact cannot use binary format", locked_tool.name),
            });
        }
    };
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: locked_tool.name.clone(),
        version: locked_tool.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "npm-official".to_owned(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: artifact.artifact_integrity()?.canonical(),
            format,
        },
        strip_components: 1,
        include_prefixes: vec![],
        required_paths: vec![PathBuf::from(target_manifest.required_path)],
        base_artifacts: vec![],
        executable_paths: vec![PathBuf::from(target_manifest.required_path)],
        aliases: npm_install_aliases(&locked_tool.name, target),
    };
    installer.install(&request)
}

fn npm_install_aliases(tool: &str, target: &str) -> Vec<InstallAlias> {
    if tool != "bun" {
        return Vec::new();
    }
    let (source, destination) = if target.starts_with("windows-") {
        (PathBuf::from("bin/bun.exe"), PathBuf::from("bin/bunx.exe"))
    } else {
        (PathBuf::from("bin/bun"), PathBuf::from("bin/bunx"))
    };
    vec![InstallAlias {
        source,
        destination,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_declares_bunx_as_an_atomic_install_alias() {
        let aliases = npm_install_aliases("bun", "linux-x86_64-avx2");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].source, Path::new("bin/bun"));
        assert_eq!(aliases[0].destination, Path::new("bin/bunx"));
        assert!(npm_install_aliases("pnpm", "linux-x86_64").is_empty());
    }
}
