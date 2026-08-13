use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactIntegrity, Error, NodeArchiveFormat, Result, SourceConfig, plan_node_artifact,
};

pub const LOCKFILE_FILENAME: &str = "pinset.lock";
pub const LOCKFILE_SCHEMA: u32 = 2;
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    pub format: LockedArtifactFormat,
    pub archive_root: String,
    pub verification: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "overlay")]
    pub overlays: Vec<LockedArtifactOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedArtifactOverlay {
    pub canonical_url: String,
    pub artifact_path: String,
    pub integrity: String,
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
    #[serde(rename = "tar.gz")]
    TarGz,
}

impl LockedArtifactFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
            Self::TarGz => "tar.gz",
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

    pub fn upsert_tool(&mut self, tool: LockedTool) {
        if let Some(existing) = self.tools.iter_mut().find(|item| item.name == tool.name) {
            *existing = tool;
        } else {
            self.tools.push(tool);
        }
        self.schema = LOCKFILE_SCHEMA;
    }

    pub fn remove_tool(&mut self, name: &str) {
        self.tools.retain(|tool| tool.name != name);
        self.schema = LOCKFILE_SCHEMA;
    }
}

impl LockedTool {
    pub fn artifact(&self, target: &str) -> Option<&LockedArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target == target)
    }
}

impl LockedArtifact {
    pub fn artifact_integrity(&self) -> Result<ArtifactIntegrity> {
        let value = self
            .integrity
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.sha256);
        ArtifactIntegrity::parse(value)
    }
}

impl LockedArtifactOverlay {
    pub fn artifact_integrity(&self) -> Result<ArtifactIntegrity> {
        ArtifactIntegrity::parse(&self.integrity)
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
    normalized.schema = LOCKFILE_SCHEMA;
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
    validate_lock_matches_selection(lockfile, project_node_version, Path::new("pinset.toml"))
}

pub fn validate_lock_matches_selection<'a>(
    lockfile: &'a Lockfile,
    selected_node_version: &str,
    selection_path: &Path,
) -> Result<&'a LockedTool> {
    let tool = lockfile.tool("node").ok_or(Error::LockedToolMissing {
        tool: "node".to_owned(),
    })?;
    if tool.requested != selected_node_version || tool.version != selected_node_version {
        return Err(Error::LockfileMismatch {
            selection_path: selection_path.to_path_buf(),
            tool: "node".to_owned(),
            configured: selected_node_version.to_owned(),
            locked: tool.version.clone(),
        });
    }
    Ok(tool)
}

pub fn validate_lock_matches_tool<'a>(
    lockfile: &'a Lockfile,
    tool_name: &str,
    selected_version: &str,
    selection_path: &Path,
) -> Result<&'a LockedTool> {
    let tool = lockfile
        .tool(tool_name)
        .ok_or_else(|| Error::LockedToolMissing {
            tool: tool_name.to_owned(),
        })?;
    if tool.requested != selected_version || tool.version != selected_version {
        return Err(Error::LockfileMismatch {
            selection_path: selection_path.to_path_buf(),
            tool: tool_name.to_owned(),
            configured: selected_version.to_owned(),
            locked: tool.version.clone(),
        });
    }
    Ok(tool)
}

pub fn validate_lock_matches_tools(
    lockfile: &Lockfile,
    configured_tools: &BTreeMap<String, String>,
    selection_path: &Path,
) -> Result<()> {
    for (tool, version) in configured_tools {
        validate_lock_matches_tool(lockfile, tool, version, selection_path)?;
    }
    for locked in &lockfile.tools {
        if !configured_tools.contains_key(&locked.name) {
            return Err(Error::ToolNotConfigured {
                tool: locked.name.clone(),
                config_path: selection_path.to_path_buf(),
            });
        }
    }
    Ok(())
}

fn validate_lockfile(lockfile: &Lockfile) -> Result<()> {
    if !matches!(lockfile.schema, 1 | LOCKFILE_SCHEMA) {
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
    if lockfile.schema == 1 && lockfile.tools.iter().any(|tool| tool.name != "node") {
        return Err(Error::InvalidLockfile {
            reason: "schema 1 lockfiles can contain only Node.js".to_owned(),
        });
    }
    Ok(())
}

fn validate_locked_tool(tool: &LockedTool) -> Result<()> {
    let provider_supported = matches!(
        (tool.name.as_str(), tool.provider.as_str()),
        ("node", "nodejs-official") | ("pnpm", "pnpm-npm") | ("bun", "bun-npm")
    );
    if !provider_supported {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "unsupported tool/provider pair {}/{}",
                tool.name, tool.provider
            ),
        });
    }
    if tool.requested != tool.version {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{} requires requested and version to be the same exact version",
                tool.name
            ),
        });
    }
    let mut targets = HashSet::with_capacity(tool.artifacts.len());
    for artifact in &tool.artifacts {
        if !targets.insert(artifact.target.as_str()) {
            return Err(Error::InvalidLockfile {
                reason: format!("duplicate artifact target {}", artifact.target),
            });
        }
        match tool.name.as_str() {
            "node" => validate_locked_node_artifact(&tool.version, artifact)?,
            "pnpm" | "bun" => validate_locked_npm_artifact(tool, artifact)?,
            _ => unreachable!("provider pair checked above"),
        }
    }
    if tool.name == "node" {
        for target in MVP_NODE_TARGETS {
            if !targets.contains(target) {
                return Err(Error::InvalidLockfile {
                    reason: format!("missing Node MVP artifact for {target}"),
                });
            }
        }
    } else {
        for (target, _) in npm_tool_targets(&tool.name) {
            if !targets.contains(target) {
                return Err(Error::InvalidLockfile {
                    reason: format!("missing {} artifact for {target}", tool.name),
                });
            }
        }
        if targets.len() != npm_tool_targets(&tool.name).len() {
            return Err(Error::InvalidLockfile {
                reason: format!("{} lock contains an unsupported artifact target", tool.name),
            });
        }
    }
    if tool.artifacts.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("{} has no artifacts", tool.name),
        });
    }
    Ok(())
}

fn validate_locked_node_artifact(version: &str, artifact: &LockedArtifact) -> Result<()> {
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
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "nodejs-shasums-https" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("Node artifact {} cannot contain overlays", artifact.target),
        });
    }
    Ok(())
}

fn validate_locked_npm_artifact(tool: &LockedTool, artifact: &LockedArtifact) -> Result<()> {
    if artifact.format != LockedArtifactFormat::TarGz {
        return Err(Error::InvalidLockfile {
            reason: format!("{} artifact {} must be tar.gz", tool.name, artifact.target),
        });
    }
    let package = npm_tool_targets(&tool.name)
        .iter()
        .find_map(|(target, package)| (*target == artifact.target).then_some(*package))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: format!("unsupported {} target {}", tool.name, artifact.target),
        })?;
    let package_base = package.rsplit('/').next().expect("npm package is nonempty");
    let artifact_path = format!("{package}/-/{package_base}-{}.tgz", tool.version);
    let canonical_url = format!("https://registry.npmjs.org/{artifact_path}");
    if artifact.archive_root != "package"
        || artifact.canonical_url != canonical_url
        || artifact.artifact_path != artifact_path
    {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid npm artifact identity for {}", artifact.target),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha512 {
        return Err(Error::InvalidLockfile {
            reason: format!("npm artifact {} must use SHA-512", artifact.target),
        });
    }
    if artifact.verification != "npm-registry-signature-sha512" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported npm verification for {}", artifact.target),
        });
    }
    if tool.name == "pnpm" {
        let uses_wrapper_overlay = pnpm_uses_wrapper_overlay(&tool.version)?;
        let overlays_are_valid = if uses_wrapper_overlay {
            artifact.overlays.len() == 1
        } else {
            artifact.overlays.len() <= 1
        };
        if !overlays_are_valid {
            return Err(Error::InvalidLockfile {
                reason: format!(
                    "pnpm {} artifact {} contains an invalid @pnpm/exe overlay count",
                    tool.version, artifact.target
                ),
            });
        }
        if let Some(overlay) = artifact.overlays.first() {
            validate_pnpm_overlay(&tool.version, overlay)?;
        }
    } else if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{} artifact {} cannot contain overlays",
                tool.name, artifact.target
            ),
        });
    }
    Ok(())
}

fn pnpm_uses_wrapper_overlay(version: &str) -> Result<bool> {
    match version
        .split_once('.')
        .and_then(|(major, _)| major.parse::<u64>().ok())
    {
        Some(10) => Ok(false),
        Some(11) => Ok(true),
        _ => Err(Error::InvalidLockfile {
            reason: format!("unsupported pnpm version {version}"),
        }),
    }
}

fn validate_pnpm_overlay(version: &str, overlay: &LockedArtifactOverlay) -> Result<()> {
    let artifact_path = format!("@pnpm/exe/-/exe-{version}.tgz");
    let canonical_url = format!("https://registry.npmjs.org/{artifact_path}");
    if overlay.canonical_url != canonical_url
        || overlay.artifact_path != artifact_path
        || overlay.archive_root != "package"
        || overlay.format != LockedArtifactFormat::TarGz
        || overlay.verification != "npm-registry-signature-sha512"
        || overlay.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha512
    {
        return Err(Error::InvalidLockfile {
            reason: "invalid @pnpm/exe overlay identity".to_owned(),
        });
    }
    Ok(())
}

fn npm_tool_targets(tool: &str) -> &'static [(&'static str, &'static str)] {
    match tool {
        "pnpm" => &[
            ("windows-x86_64", "@pnpm/win-x64"),
            ("linux-x86_64", "@pnpm/linux-x64"),
            ("macos-aarch64", "@pnpm/macos-arm64"),
        ],
        "bun" => &[
            ("windows-x86_64-avx2", "@oven/bun-windows-x64"),
            ("windows-x86_64-baseline", "@oven/bun-windows-x64-baseline"),
            ("linux-x86_64-avx2", "@oven/bun-linux-x64"),
            ("linux-x86_64-baseline", "@oven/bun-linux-x64-baseline"),
            ("macos-aarch64", "@oven/bun-darwin-aarch64"),
        ],
        _ => &[],
    }
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

    #[test]
    fn validates_pnpm_overlay_by_package_generation() {
        let pnpm_10 = locked_pnpm_artifact("10.34.5", false);
        let pnpm_10_tool = locked_pnpm_tool("10.34.5", pnpm_10.clone());
        validate_locked_npm_artifact(&pnpm_10_tool, &pnpm_10).expect("pnpm 10 artifact");

        let pnpm_10_with_overlay = locked_pnpm_artifact("10.34.5", true);
        let pnpm_10_with_overlay_tool =
            locked_pnpm_tool("10.34.5", pnpm_10_with_overlay.clone());
        validate_locked_npm_artifact(&pnpm_10_with_overlay_tool, &pnpm_10_with_overlay)
            .expect("legacy pnpm 10 artifact");

        let pnpm_11 = locked_pnpm_artifact("11.21.0", true);
        let pnpm_11_tool = locked_pnpm_tool("11.21.0", pnpm_11.clone());
        validate_locked_npm_artifact(&pnpm_11_tool, &pnpm_11).expect("pnpm 11 artifact");

        let pnpm_11_without_overlay = locked_pnpm_artifact("11.21.0", false);
        let pnpm_11_without_overlay_tool =
            locked_pnpm_tool("11.21.0", pnpm_11_without_overlay.clone());
        assert!(matches!(
            validate_locked_npm_artifact(&pnpm_11_without_overlay_tool, &pnpm_11_without_overlay),
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
            integrity: None,
            format: match plan.format {
                NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
            },
            archive_root: plan.archive_root,
            verification: "nodejs-shasums-https".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_pnpm_tool(version: &str, artifact: LockedArtifact) -> LockedTool {
        LockedTool {
            name: "pnpm".to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: "pnpm-npm".to_owned(),
            artifacts: vec![artifact],
        }
    }

    fn locked_pnpm_artifact(version: &str, with_overlay: bool) -> LockedArtifact {
        let artifact_path = format!("@pnpm/linux-x64/-/linux-x64-{version}.tgz");
        LockedArtifact {
            target: "linux-x86_64".to_owned(),
            canonical_url: format!("https://registry.npmjs.org/{artifact_path}"),
            artifact_path,
            sha256: String::new(),
            integrity: Some(format!("sha512:{}", "ab".repeat(64))),
            format: LockedArtifactFormat::TarGz,
            archive_root: "package".to_owned(),
            verification: "npm-registry-signature-sha512".to_owned(),
            overlays: with_overlay.then(|| {
                let artifact_path = format!("@pnpm/exe/-/exe-{version}.tgz");
                LockedArtifactOverlay {
                    canonical_url: format!("https://registry.npmjs.org/{artifact_path}"),
                    artifact_path,
                    integrity: format!("sha512:{}", "cd".repeat(64)),
                    format: LockedArtifactFormat::TarGz,
                    archive_root: "package".to_owned(),
                    verification: "npm-registry-signature-sha512".to_owned(),
                }
            })
            .into_iter()
            .collect(),
        }
    }
}
