use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    ArtifactFormat, ArtifactInstallSpec, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error,
    InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedArtifactOverlay,
    LockedTool, Result, tool_targets,
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
    let base_artifacts = artifact
        .overlays
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
        required_paths: vec![PathBuf::from(target_manifest.required_path)],
        base_artifacts,
        executable_paths,
    };
    let outcome = installer.install(&request)?;
    if locked_tool.name == "bun" {
        ensure_bunx_alias(&outcome.install_dir, target)?;
    }
    Ok(outcome)
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

fn ensure_bunx_alias(install_dir: &Path, target: &str) -> Result<()> {
    let (source, destination) = if target.starts_with("windows-") {
        (
            install_dir.join("bin/bun.exe"),
            install_dir.join("bin/bunx.exe"),
        )
    } else {
        (install_dir.join("bin/bun"), install_dir.join("bin/bunx"))
    };
    if destination.is_file() {
        return Ok(());
    }
    match fs::hard_link(&source, &destination) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied | io::ErrorKind::Other
            ) =>
        {
            fs::copy(&source, &destination)
                .map(|_| ())
                .map_err(|source_error| Error::CreateRuntimeAlias {
                    source_path: source,
                    destination,
                    source: source_error,
                })
        }
        Err(source_error) => Err(Error::CreateRuntimeAlias {
            source_path: source,
            destination,
            source: source_error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bunx_alias_is_idempotent() {
        let root = tempfile::tempdir().expect("root");
        let bin = root.path().join("bin");
        fs::create_dir(&bin).expect("bin");
        let source = if cfg!(windows) {
            bin.join("bun.exe")
        } else {
            bin.join("bun")
        };
        fs::write(&source, b"bun").expect("bun fixture");
        let target = if cfg!(windows) {
            "windows-x86_64-avx2"
        } else {
            "linux-x86_64-avx2"
        };
        ensure_bunx_alias(root.path(), target).expect("first alias");
        ensure_bunx_alias(root.path(), target).expect("second alias");
        let alias = if cfg!(windows) {
            bin.join("bunx.exe")
        } else {
            bin.join("bunx")
        };
        assert_eq!(fs::read(alias).expect("alias"), b"bun");
    }
}
