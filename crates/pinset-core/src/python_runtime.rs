use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, InstallOutcome,
    InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result, SourceConfig, SourceKind,
    plan_python_artifact,
};

pub fn install_locked_python(
    installer: &Installer,
    pinset_home: &Path,
    source_config: &SourceConfig,
    locked_python: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_python.name != "python" || locked_python.provider != "python-build-standalone" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in CPython provider",
                locked_python.name, locked_python.provider
            ),
        });
    }
    let artifact = locked_python
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "python".to_owned(),
            target: target.to_owned(),
        })?;
    let plan = plan_python_artifact(source_config, &locked_python.version, target)?;
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
    if artifact.format != LockedArtifactFormat::TarGz {
        return Err(Error::InvalidLockfile {
            reason: format!("Python artifact {target} must use tar.gz"),
        });
    }
    let required_paths = required_python_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "python".to_owned(),
        version: locked_python.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources,
            integrity: artifact.artifact_integrity()?.canonical(),
            format: ArtifactFormat::TarGz,
        },
        strip_components: 1,
        include_prefixes: Vec::new(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if target.starts_with("windows-") {
            Vec::new()
        } else {
            required_paths
        },
    };
    installer.install(&request)
}

fn required_python_paths(target: &str) -> Result<Vec<PathBuf>> {
    if target.starts_with("windows-") {
        Ok(vec![PathBuf::from("python.exe")])
    } else if target.starts_with("linux-") || target.starts_with("macos-") {
        Ok(vec![PathBuf::from("bin/python3")])
    } else {
        Err(Error::UnsupportedPythonTarget {
            target: target.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_platform_python_entrypoint() {
        assert_eq!(
            required_python_paths("windows-x86_64").expect("Windows paths"),
            [PathBuf::from("python.exe")]
        );
        assert_eq!(
            required_python_paths("linux-x86_64").expect("Linux paths"),
            [PathBuf::from("bin/python3")]
        );
    }
}
