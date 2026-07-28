use std::{
    collections::HashSet,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{Error, NodeArchiveFormat, Result, SourceConfig, plan_node_artifact};

pub const LOCKFILE_FILENAME: &str = "pinset.lock";
pub const LOCKFILE_SCHEMA: u32 = 1;
pub const MVP_NODE_TARGETS: [&str; 4] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub schema: u32,
    pub generated_by: String,
    #[serde(rename = "tool")]
    pub tools: Vec<LockedTool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedTool {
    pub name: String,
    pub requested: String,
    pub version: String,
    pub provider: String,
    #[serde(rename = "artifact")]
    pub artifacts: Vec<LockedArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedArtifact {
    pub target: String,
    pub canonical_url: String,
    pub artifact_path: String,
    pub sha256: String,
    pub format: LockedArtifactFormat,
    pub archive_root: String,
    pub verification: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockedArtifactFormat {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "tar.xz")]
    TarXz,
}

impl LockedArtifactFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
        }
    }
}

impl Lockfile {
    pub fn new_node(generated_by: String, version: String, artifacts: Vec<LockedArtifact>) -> Self {
        Self {
            schema: LOCKFILE_SCHEMA,
            generated_by,
            tools: vec![LockedTool {
                name: "node".to_owned(),
                requested: version.clone(),
                version,
                provider: "nodejs-official".to_owned(),
                artifacts,
            }],
        }
    }

    pub fn tool(&self, name: &str) -> Option<&LockedTool> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}

impl LockedTool {
    pub fn artifact(&self, target: &str) -> Option<&LockedArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target == target)
    }
}

pub fn lockfile_path(project_config_path: &Path) -> PathBuf {
    project_config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(LOCKFILE_FILENAME)
}

pub fn load_lockfile(path: &Path) -> Result<Lockfile> {
    let content = fs::read_to_string(path).map_err(|source| Error::ReadLockfile {
        path: path.to_path_buf(),
        source,
    })?;
    let lockfile: Lockfile = toml::from_str(&content).map_err(|source| Error::ParseLockfile {
        path: path.to_path_buf(),
        source,
    })?;
    validate_lockfile(&lockfile)?;
    Ok(lockfile)
}

pub fn save_lockfile(path: &Path, lockfile: &Lockfile) -> Result<()> {
    validate_lockfile(lockfile)?;
    let mut normalized = lockfile.clone();
    normalized
        .tools
        .sort_by(|left, right| left.name.cmp(&right.name));
    for tool in &mut normalized.tools {
        tool.artifacts
            .sort_by(|left, right| left.target.cmp(&right.target));
    }
    let serialized = toml::to_string_pretty(&normalized)
        .map_err(|source| Error::SerializeLockfile { source })?;
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteLockfile {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteLockfile {
            path: path.to_path_buf(),
            source,
        })
}

pub fn validate_lock_matches_project<'a>(
    lockfile: &'a Lockfile,
    project_node_version: &str,
) -> Result<&'a LockedTool> {
    let tool = lockfile.tool("node").ok_or(Error::LockedToolMissing {
        tool: "node".to_owned(),
    })?;
    if tool.requested != project_node_version || tool.version != project_node_version {
        return Err(Error::LockfileMismatch {
            tool: "node".to_owned(),
            configured: project_node_version.to_owned(),
            locked: tool.version.clone(),
        });
    }
    Ok(tool)
}

fn validate_lockfile(lockfile: &Lockfile) -> Result<()> {
    if lockfile.schema != LOCKFILE_SCHEMA {
        return Err(Error::UnsupportedLockfileSchema {
            actual: lockfile.schema,
        });
    }
    if lockfile.generated_by.trim().is_empty() {
        return Err(Error::InvalidLockfile {
            reason: "generated_by cannot be empty".to_owned(),
        });
    }
    let mut tool_names = HashSet::with_capacity(lockfile.tools.len());
    for tool in &lockfile.tools {
        if !tool_names.insert(&tool.name) {
            return Err(Error::InvalidLockfile {
                reason: format!("duplicate tool {}", tool.name),
            });
        }
        validate_locked_tool(tool)?;
    }
    Ok(())
}

fn validate_locked_tool(tool: &LockedTool) -> Result<()> {
    if tool.name != "node" || tool.provider != "nodejs-official" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "unsupported tool/provider pair {}/{}",
                tool.name, tool.provider
            ),
        });
    }
    if tool.requested != tool.version {
        return Err(Error::InvalidLockfile {
            reason: "Node MVP requires requested and version to be the same exact version"
                .to_owned(),
        });
    }
    let mut targets = HashSet::with_capacity(tool.artifacts.len());
    for artifact in &tool.artifacts {
        if !targets.insert(artifact.target.as_str()) {
            return Err(Error::InvalidLockfile {
                reason: format!("duplicate artifact target {}", artifact.target),
            });
        }
        validate_locked_artifact(&tool.version, artifact)?;
    }
    for target in MVP_NODE_TARGETS {
        if !targets.contains(target) {
            return Err(Error::InvalidLockfile {
                reason: format!("missing Node MVP artifact for {target}"),
            });
        }
    }
    Ok(())
}

fn validate_locked_artifact(version: &str, artifact: &LockedArtifact) -> Result<()> {
    let plan = plan_node_artifact(&SourceConfig::default(), version, &artifact.target)?;
    let expected_format = match plan.format {
        NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
        NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Node provider",
                artifact.target
            ),
        });
    }
    if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "nodejs-shasums-https" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported verification for {}", artifact.target),
        });
    }
    Ok(())
}

pub fn load_optional_lockfile(path: &Path) -> Result<Option<Lockfile>> {
    match load_lockfile(path) {
        Ok(lockfile) => Ok(Some(lockfile)),
        Err(Error::ReadLockfile { source, .. }) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn saves_deterministically_and_reloads_strictly() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(LOCKFILE_FILENAME);
        let mut artifacts = MVP_NODE_TARGETS
            .into_iter()
            .rev()
            .map(locked_artifact)
            .collect::<Vec<_>>();
        let lockfile = Lockfile::new_node(
            "pinset 0.1.0".to_owned(),
            "24.0.0".to_owned(),
            artifacts.clone(),
        );
        save_lockfile(&path, &lockfile).expect("save lockfile");
        let first = fs::read(&path).expect("first lockfile");

        artifacts.rotate_left(1);
        let reordered =
            Lockfile::new_node("pinset 0.1.0".to_owned(), "24.0.0".to_owned(), artifacts);
        save_lockfile(&path, &reordered).expect("save reordered lockfile");
        let second = fs::read(&path).expect("second lockfile");

        assert_eq!(first, second);
        let loaded = load_lockfile(&path).expect("reload");
        assert_eq!(
            loaded.tools[0]
                .artifacts
                .iter()
                .map(|artifact| artifact.target.as_str())
                .collect::<Vec<_>>(),
            vec![
                "linux-x86_64",
                "macos-aarch64",
                "macos-x86_64",
                "windows-x86_64",
            ]
        );
    }

    #[test]
    fn rejects_unknown_fields_and_mismatched_artifact_identity() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(LOCKFILE_FILENAME);
        fs::write(
            &path,
            "schema = 1\ngenerated_by = \"pinset\"\nunknown = true\n",
        )
        .expect("invalid lock");
        assert!(matches!(
            load_lockfile(&path),
            Err(Error::ParseLockfile { .. })
        ));

        let mut artifacts = MVP_NODE_TARGETS
            .into_iter()
            .map(locked_artifact)
            .collect::<Vec<_>>();
        artifacts[0].canonical_url = "https://mirror.example/node.zip".to_owned();
        let lockfile = Lockfile::new_node("pinset".to_owned(), "24.0.0".to_owned(), artifacts);
        assert!(matches!(
            save_lockfile(&path, &lockfile),
            Err(Error::InvalidLockfile { .. })
        ));
    }

    fn locked_artifact(target: &str) -> LockedArtifact {
        let plan = plan_node_artifact(&SourceConfig::default(), "24.0.0", target).expect("plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            format: match plan.format {
                NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
            },
            archive_root: plan.archive_root,
            verification: "nodejs-shasums-https".to_owned(),
        }
    }
}
