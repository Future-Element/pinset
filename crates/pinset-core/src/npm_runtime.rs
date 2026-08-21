use std::path::{Path, PathBuf};

use semver::Version;

use crate::{
    ArtifactFormat, ArtifactInstallSpec, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error,
    InstallAlias, InstallOutcome, InstallRequest, Installer, LockedArtifactFormat,
    LockedArtifactOverlay, LockedTool, Result, npm_metadata::pnpm_uses_wrapper_overlay,
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
            target: target.to_owned(),
        })?;
    let target_manifest = tool_targets(&locked_tool.name)?
        .iter()
        .find(|candidate| candidate.target == target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked_tool.name.clone(),
            target: target.to_owned(),
        })?;
    let format = match artifact.format {
        LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
        LockedArtifactFormat::Zip => ArtifactFormat::Zip,
        LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
    };
    let use_overlays = if locked_tool.name == "pnpm" {
        let version = Version::parse(&locked_tool.version).map_err(|_| Error::InvalidLockfile {
            reason: format!("invalid pnpm version {}", locked_tool.version),
        })?;
        pnpm_uses_wrapper_overlay(&version)
    } else {
        true
    };
    let overlays = if use_overlays {
        artifact.overlays.as_slice()
    } else {
        &[]
    };
    let base_artifacts = overlays
        .iter()
        .map(overlay_install_spec)
        .collect::<Result<Vec<_>>>()?;
    let executable_paths = if locked_tool.name == "pnpm" && !target.starts_with("windows-") {
        vec![PathBuf::from(target_manifest.required_path)]
    } else {
        Vec::new()
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
        include_prefixes: if locked_tool.name == "pnpm" {
            vec![PathBuf::from(target_manifest.required_path)]
        } else {
            Vec::new()
        },
        required_paths: vec![PathBuf::from(target_manifest.required_path)],
        base_artifacts,
        executable_paths,
        aliases: npm_install_aliases(&locked_tool.name, target),
    };
    installer.install(&request)
}

fn overlay_install_spec(overlay: &LockedArtifactOverlay) -> Result<ArtifactInstallSpec> {
    let format = match overlay.format {
        LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
        LockedArtifactFormat::Zip => ArtifactFormat::Zip,
        LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
    };
    Ok(ArtifactInstallSpec {
        artifact: ArtifactSpec {
            canonical_url: overlay.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "npm-official-overlay".to_owned(),
                url: overlay.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: overlay.artifact_integrity()?.canonical(),
            format,
        },
        strip_components: 1,
        include_prefixes: vec![PathBuf::from("dist"), PathBuf::from("package.json")],
        required_paths: vec![
            PathBuf::from("dist/pnpm.mjs"),
            PathBuf::from("package.json"),
        ],
    })
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
