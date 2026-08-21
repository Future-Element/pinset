use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, InstallOutcome,
    InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result, RustArchiveFormat,
    plan_rust_artifact,
};

pub fn install_locked_rust(
    installer: &Installer,
    pinset_home: &Path,
    locked_rust: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_rust.name != "rust" || locked_rust.provider != "rust-official" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in Rust provider",
                locked_rust.name, locked_rust.provider
            ),
        });
    }
    let artifact = locked_rust
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "rust".to_owned(),
            target: target.to_owned(),
        })?;
    let manifest_date =
        locked_rust
            .metadata
            .get("manifest_date")
            .ok_or_else(|| Error::InvalidLockfile {
                reason: "Rust lock has no manifest_date metadata".to_owned(),
            })?;
    let plan = plan_rust_artifact(
        &locked_rust.version,
        manifest_date,
        target,
        &artifact.canonical_url,
    )?;
    let required_paths = required_rust_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "rust".to_owned(),
        version: locked_rust.version.clone(),
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
                (LockedArtifactFormat::TarXz, RustArchiveFormat::TarXz) => ArtifactFormat::TarXz,
                _ => {
                    return Err(Error::InvalidLockfile {
                        reason: format!("Rust artifact {target} must use tar.xz"),
                    });
                }
            },
        },
        strip_components: 2,
        include_prefixes: ["bin", "lib", "share", "etc"]
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if target.starts_with("windows-") {
            Vec::new()
        } else {
            required_paths
        },
        aliases: Vec::new(),
    };
    installer.install(&request)
}

fn required_rust_paths(target: &str) -> Result<Vec<PathBuf>> {
    if !matches!(
        target,
        "windows-x86_64" | "linux-x86_64" | "linux-aarch64" | "macos-x86_64" | "macos-aarch64"
    ) {
        return Err(Error::UnsupportedRustTarget {
            target: target.to_owned(),
        });
    }
    let extension = if target.starts_with("windows-") {
        ".exe"
    } else {
        ""
    };
    Ok([
        "rustc",
        "cargo",
        "rustdoc",
        "rustfmt",
        "cargo-fmt",
        "clippy-driver",
        "cargo-clippy",
    ]
    .into_iter()
    .map(|command| PathBuf::from("bin").join(format!("{command}{extension}")))
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_default_profile_commands_under_bin() {
        assert_eq!(
            required_rust_paths("linux-x86_64").expect("Linux paths"),
            [
                "bin/rustc",
                "bin/cargo",
                "bin/rustdoc",
                "bin/rustfmt",
                "bin/cargo-fmt",
                "bin/clippy-driver",
                "bin/cargo-clippy",
            ]
            .map(PathBuf::from)
        );
        assert!(
            required_rust_paths("windows-x86_64")
                .expect("Windows paths")
                .iter()
                .all(|path| path.extension().is_some_and(|extension| extension == "exe"))
        );
    }
}
