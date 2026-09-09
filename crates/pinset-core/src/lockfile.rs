use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ArtifactIntegrity, DOTNET_TARGETS, DotnetArchiveFormat, DotnetVersion, Error, FLUTTER_TARGETS,
    FlutterArchiveFormat, GO_TARGETS, GoArchiveFormat, JAVA_TARGETS, JavaArchiveFormat,
    JavaVersion, NodeArchiveFormat, PYTHON_TARGETS, PYTHON_VARIANT, RUST_COMPONENTS, RUST_PROFILE,
    RUST_TARGETS, Result, RustArchiveFormat, SourceConfig, parse_python_distribution,
    plan_dotnet_artifact, plan_flutter_artifact, plan_go_artifact, plan_java_artifact_with_package,
    plan_node_artifact, plan_python_artifact, plan_rust_artifact, plan_rust_nightly_artifact,
};

pub const LOCKFILE_FILENAME: &str = "pinset.lock";
pub const LOCKFILE_SCHEMA: u32 = 5;
pub const MVP_NODE_TARGETS: [&str; 5] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
    "linux-aarch64",
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "released-at"
    )]
    pub released_at: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
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
    #[serde(rename = "binary")]
    Binary,
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
            Self::Binary => "binary",
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
            Self::TarGz => "tar.gz",
        }
    }
}

impl Lockfile {
    pub fn new_node(
        generated_by: String,
        version: String,
        signer_fingerprint: String,
        manifest_source: String,
        artifacts: Vec<LockedArtifact>,
    ) -> Self {
        Self {
            schema: LOCKFILE_SCHEMA,
            generated_by,
            tools: vec![LockedTool {
                name: "node".to_owned(),
                requested: version.clone(),
                version,
                provider: "nodejs-official".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    (
                        "signature_primary_fingerprint".to_owned(),
                        signer_fingerprint,
                    ),
                    (
                        "signed_manifest".to_owned(),
                        "SHASUMS256.txt.asc".to_owned(),
                    ),
                    ("manifest_source".to_owned(), manifest_source),
                ]),
                options: BTreeMap::new(),
                artifacts,
            }],
        }
    }

    pub fn tool(&self, name: &str) -> Option<&LockedTool> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    pub fn upsert_tool(&mut self, tool: LockedTool) -> Result<()> {
        if let Some(existing) = self.tools.iter_mut().find(|item| item.name == tool.name) {
            crate::validate_verification_transition(existing, &tool)?;
            *existing = tool;
        } else {
            self.tools.push(tool);
        }
        self.schema = LOCKFILE_SCHEMA;
        Ok(())
    }

    pub fn remove_tool(&mut self, name: &str) {
        self.tools.retain(|tool| tool.name != name);
        self.schema = LOCKFILE_SCHEMA;
    }
}

impl LockedTool {
    /// Stable on-disk identity. Legacy locks keep the historical version-only layout.
    pub fn installation_version(&self) -> String {
        let mut identity_options = self.options.clone();
        if self.provider == "declarative-github-release" {
            for key in ["provider-id", "provider-revision", "registry-fingerprint"] {
                if let Some(value) = self.metadata.get(key) {
                    identity_options.insert(format!("provider.{key}"), value.clone());
                }
            }
        }
        if self
            .metadata
            .get("channel")
            .is_some_and(|channel| channel == "nightly")
            && let Some(date) = self.metadata.get("manifest_date")
        {
            identity_options
                .entry("nightly-date".to_owned())
                .or_insert_with(|| date.clone());
        }
        if identity_options.is_empty() {
            return self.version.clone();
        }
        let mut canonical = String::new();
        for (key, value) in &identity_options {
            canonical.push_str(key);
            canonical.push('=');
            canonical.push_str(value);
            canonical.push('\n');
        }
        let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
        format!("{}--{}", self.version, &digest[..12])
    }

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
    let lockfile = parse_lockfile(path)?;
    validate_lockfile(&lockfile)?;
    Ok(lockfile)
}

/// Loads a lockfile for a state-changing command that can refresh the historical
/// pre-1.0 target matrices before saving it again.
///
/// The relaxed result must never be used for installation or command resolution.
/// Every artifact that is present is still validated strictly; the only tolerated
/// gaps are the exact provider matrices shipped before Linux ARM64 support was
/// added in Pinset 1.0.
pub fn load_lockfile_for_provider_refresh(path: &Path) -> Result<(Lockfile, Vec<String>)> {
    let lockfile = parse_lockfile(path)?;
    match validate_lockfile(&lockfile) {
        Ok(()) => Ok((lockfile, Vec::new())),
        Err(strict_error) => {
            validate_lockfile_with_target_policy(
                &lockfile,
                TargetMatrixPolicy::AllowPreV1LinuxArm64Matrices,
            )?;
            let mut refresh_tools = lockfile
                .tools
                .iter()
                .filter(|tool| has_pre_v1_target_matrix(tool))
                .map(|tool| tool.name.clone())
                .collect::<Vec<_>>();
            refresh_tools.sort();
            if refresh_tools.is_empty() {
                return Err(strict_error);
            }
            Ok((lockfile, refresh_tools))
        }
    }
}

/// Backwards-compatible target-refresh loader retained for callers that only
/// need to know whether any historical provider matrix requires repair.
pub fn load_lockfile_for_target_refresh(path: &Path) -> Result<(Lockfile, bool)> {
    let (lockfile, refresh_tools) = load_lockfile_for_provider_refresh(path)?;
    Ok((lockfile, !refresh_tools.is_empty()))
}

fn parse_lockfile(path: &Path) -> Result<Lockfile> {
    let content = fs::read_to_string(path).map_err(|source| Error::ReadLockfile {
        path: path.to_path_buf(),
        source,
    })?;
    let lockfile: Lockfile = toml::from_str(&content).map_err(|source| Error::ParseLockfile {
        path: path.to_path_buf(),
        source,
    })?;
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
    selected_node_selector: &str,
    selection_path: &Path,
) -> Result<&'a LockedTool> {
    let tool = lockfile.tool("node").ok_or(Error::LockedToolMissing {
        tool: "node".to_owned(),
    })?;
    if tool.requested != selected_node_selector {
        return Err(Error::LockfileMismatch {
            selection_path: selection_path.to_path_buf(),
            tool: "node".to_owned(),
            configured: selected_node_selector.to_owned(),
            locked: format!("{} -> {}", tool.requested, tool.version),
        });
    }
    Ok(tool)
}

pub fn validate_lock_matches_tool<'a>(
    lockfile: &'a Lockfile,
    tool_name: &str,
    selected_selector: &str,
    selection_path: &Path,
) -> Result<&'a LockedTool> {
    let tool = lockfile
        .tool(tool_name)
        .ok_or_else(|| Error::LockedToolMissing {
            tool: tool_name.to_owned(),
        })?;
    if tool.requested != selected_selector {
        return Err(Error::LockfileMismatch {
            selection_path: selection_path.to_path_buf(),
            tool: tool_name.to_owned(),
            configured: selected_selector.to_owned(),
            locked: format!("{} -> {}", tool.requested, tool.version),
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

pub fn validate_lock_matches_tool_options(
    lockfile: &Lockfile,
    configured: &BTreeMap<String, crate::ToolOptions>,
    selection_path: &Path,
) -> Result<()> {
    for locked in &lockfile.tools {
        let expected = configured
            .get(&locked.name)
            .map(crate::ToolOptions::lock_options)
            .unwrap_or_default();
        if locked.options != expected {
            return Err(Error::LockfileMismatch {
                selection_path: selection_path.to_path_buf(),
                tool: locked.name.clone(),
                configured: format!("{} with structured options", locked.requested),
                locked: format!("{} with different structured options", locked.version),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetMatrixPolicy {
    Current,
    AllowPreV1LinuxArm64Matrices,
}

fn validate_lockfile(lockfile: &Lockfile) -> Result<()> {
    validate_lockfile_with_target_policy(lockfile, TargetMatrixPolicy::Current)
}

fn validate_lockfile_with_target_policy(
    lockfile: &Lockfile,
    target_policy: TargetMatrixPolicy,
) -> Result<()> {
    if !matches!(lockfile.schema, 1 | 2 | 3 | 4 | LOCKFILE_SCHEMA) {
        return Err(Error::UnsupportedLockfileSchema {
            actual: lockfile.schema,
        });
    }
    if lockfile.generated_by.trim().is_empty() {
        return Err(Error::InvalidLockfile {
            reason: "generated_by cannot be empty".to_owned(),
        });
    }
    if lockfile.schema < 4 && lockfile.tools.iter().any(|tool| !tool.options.is_empty()) {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "lockfile schema {} cannot contain structured tool options",
                lockfile.schema
            ),
        });
    }
    let mut tool_names = HashSet::with_capacity(lockfile.tools.len());
    for tool in &lockfile.tools {
        if !tool_names.insert(&tool.name) {
            return Err(Error::InvalidLockfile {
                reason: format!("duplicate tool {}", tool.name),
            });
        }
        validate_locked_tool_with_target_policy(tool, target_policy)?;
        // Schema 3 introduced independent requested selectors (for example,
        // `bun = "latest"`) while keeping the resolved version exact in the lock.
        // Only schema 1/2 require the two values to be identical.
        if lockfile.schema < LOCKFILE_SCHEMA
            && !has_complete_target_matrix(tool)
            && !(target_policy == TargetMatrixPolicy::AllowPreV1LinuxArm64Matrices
                && has_pre_v1_target_matrix(tool))
        {
            return Err(Error::InvalidLockfile {
                reason: if has_pre_v1_target_matrix(tool) {
                    format!(
                        "schema {} {} lock is missing the historical linux-aarch64 target; refresh the provider lock",
                        lockfile.schema, tool.name
                    )
                } else {
                    format!(
                        "schema {} requires the historical complete target matrix for {}; refresh the provider lock",
                        lockfile.schema, tool.name
                    )
                },
            });
        }
        if lockfile.schema < 3 && tool.requested != tool.version {
            return Err(Error::InvalidLockfile {
                reason: format!(
                    "schema {} requires {} requested and resolved versions to match",
                    lockfile.schema, tool.name
                ),
            });
        }
    }
    if lockfile.schema == 1 && lockfile.tools.iter().any(|tool| tool.name != "node") {
        return Err(Error::InvalidLockfile {
            reason: "schema 1 lockfiles can contain only Node.js".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
fn validate_locked_tool(tool: &LockedTool) -> Result<()> {
    validate_locked_tool_with_target_policy(tool, TargetMatrixPolicy::Current)
}

fn validate_locked_tool_with_target_policy(
    tool: &LockedTool,
    target_policy: TargetMatrixPolicy,
) -> Result<()> {
    let pre_v1_target_matrix = target_policy == TargetMatrixPolicy::AllowPreV1LinuxArm64Matrices
        && has_pre_v1_target_matrix(tool);
    if tool
        .released_at
        .as_deref()
        .is_some_and(|value| !crate::valid_release_time(value))
    {
        return Err(Error::InvalidLockfile {
            reason: format!("{} has an invalid released-at timestamp", tool.name),
        });
    }
    let declarative_provider = tool.provider == "declarative-github-release"
        && crate::runtime_provider(&tool.name).is_some_and(|provider| {
            provider.capabilities.metadata == crate::RuntimeMetadataKind::Declarative
        });
    let provider_supported = declarative_provider
        || matches!(
            (tool.name.as_str(), tool.provider.as_str()),
            ("node", "nodejs-official")
                | ("pnpm", "pnpm-npm")
                | ("bun", "bun-npm")
                | ("go", "go-official")
                | ("flutter", "flutter-official")
                | ("java", "adoptium-temurin")
                | ("rust", "rust-official")
                | ("dotnet", "microsoft-dotnet-sdk")
                | ("python", "python-build-standalone")
        );
    if !provider_supported {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "unsupported tool/provider pair {}/{}",
                tool.name, tool.provider
            ),
        });
    }
    if tool.requested.trim().is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("{} requested selector cannot be empty", tool.name),
        });
    }
    if tool.name != "node"
        && tool.name != "flutter"
        && tool.name != "java"
        && tool.name != "python"
        && tool.name != "rust"
        && tool.name != "dotnet"
        && !declarative_provider
        && !tool.metadata.is_empty()
    {
        return Err(Error::InvalidLockfile {
            reason: format!("{} lock cannot contain provider metadata", tool.name),
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
            "node" => validate_locked_node_artifact(&tool.version, artifact, pre_v1_target_matrix)?,
            "go" => validate_locked_go_artifact(&tool.version, artifact)?,
            "flutter" => validate_locked_flutter_artifact(&tool.version, artifact)?,
            "java" => validate_locked_java_artifact(tool, artifact)?,
            "rust" => validate_locked_rust_artifact(tool, artifact)?,
            "dotnet" => validate_locked_dotnet_artifact(tool, artifact)?,
            "python" => validate_locked_python_artifact(&tool.version, artifact)?,
            "pnpm" | "bun" => validate_locked_npm_artifact(tool, artifact)?,
            _ if declarative_provider => validate_locked_declarative_artifact(tool, artifact)?,
            _ => unreachable!("provider pair checked above"),
        }
    }
    if tool.name == "node" {
        if pre_v1_target_matrix {
            if !tool.metadata.is_empty() {
                return Err(Error::InvalidLockfile {
                    reason: "pre-1.0 Node lock cannot contain provider metadata".to_owned(),
                });
            }
        } else {
            validate_node_metadata(tool)?;
        }
    } else if tool.name == "flutter" {
        validate_flutter_metadata(tool)?;
    } else if tool.name == "python" {
        validate_python_metadata(tool)?;
    } else if tool.name == "java" {
        validate_java_metadata(tool)?;
    } else if tool.name == "rust" {
        validate_rust_metadata(tool)?;
    } else if tool.name == "dotnet" {
        validate_dotnet_metadata(tool)?;
    } else if declarative_provider {
        validate_declarative_metadata(tool)?;
    }
    if tool.artifacts.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("{} has no artifacts", tool.name),
        });
    }
    Ok(())
}

const PRE_V1_NODE_TARGETS: &[&str] = &[
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];
const PRE_V1_PNPM_TARGETS: &[&str] = &["windows-x86_64", "linux-x86_64", "macos-aarch64"];
const PRE_V1_BUN_TARGETS: &[&str] = &[
    "windows-x86_64-avx2",
    "windows-x86_64-baseline",
    "linux-x86_64-avx2",
    "linux-x86_64-baseline",
    "macos-aarch64",
];
const PRE_V1_STANDARD_TARGETS: &[&str] = &[
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
];

fn has_pre_v1_target_matrix(tool: &LockedTool) -> bool {
    let expected = match tool.name.as_str() {
        "node" => PRE_V1_NODE_TARGETS,
        "pnpm" => PRE_V1_PNPM_TARGETS,
        "bun" => PRE_V1_BUN_TARGETS,
        "go" | "python" | "java" | "rust" | "dotnet" => PRE_V1_STANDARD_TARGETS,
        _ => return false,
    };
    tool.artifacts.len() == expected.len()
        && tool
            .artifacts
            .iter()
            .all(|artifact| expected.contains(&artifact.target.as_str()))
}

fn has_complete_target_matrix(tool: &LockedTool) -> bool {
    let expected = match tool.name.as_str() {
        "node" => MVP_NODE_TARGETS.as_slice(),
        "go" => GO_TARGETS.as_slice(),
        "flutter" => FLUTTER_TARGETS.as_slice(),
        "python" => PYTHON_TARGETS.as_slice(),
        "java" => JAVA_TARGETS.as_slice(),
        "rust" => RUST_TARGETS.as_slice(),
        "dotnet" => DOTNET_TARGETS.as_slice(),
        "pnpm" | "bun" => {
            return tool.artifacts.len() == npm_tool_targets(&tool.name).len()
                && npm_tool_targets(&tool.name)
                    .iter()
                    .all(|(target, _)| tool.artifact(target).is_some());
        }
        _ => return true,
    };
    tool.artifacts.len() == expected.len()
        && expected
            .iter()
            .all(|target| tool.artifact(target).is_some())
}

fn validate_node_metadata(tool: &LockedTool) -> Result<()> {
    let expected = [
        "manifest_source",
        "signature_primary_fingerprint",
        "signed_manifest",
    ];
    let fingerprint = tool.metadata.get("signature_primary_fingerprint");
    if tool.metadata.len() != expected.len()
        || expected.iter().any(|key| !tool.metadata.contains_key(*key))
        || tool.metadata.get("signed_manifest").map(String::as_str) != Some("SHASUMS256.txt.asc")
        || tool
            .metadata
            .get("manifest_source")
            .is_none_or(|source| source.trim().is_empty())
        || fingerprint.is_none_or(|value| {
            value.len() != 40
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'A'..=b'F'))
        })
    {
        return Err(Error::InvalidLockfile {
            reason: "Node lock must contain the verified signed manifest and signer fingerprint; regenerate this pre-1.0 lock with `pinset use node@<selector>`"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_flutter_metadata(tool: &LockedTool) -> Result<()> {
    let expected = ["channel", "dart_version", "release_hash"];
    if tool.metadata.len() != expected.len()
        || expected.iter().any(|key| !tool.metadata.contains_key(*key))
        || tool.metadata.get("channel").map(String::as_str) != Some("stable")
        || tool
            .metadata
            .get("dart_version")
            .is_none_or(|value| !is_exact_numeric_triplet(value))
        || tool.metadata.get("release_hash").is_none_or(|value| {
            value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(Error::InvalidLockfile {
            reason: "Flutter lock metadata must contain stable channel, bundled Dart version and release hash"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_python_metadata(tool: &LockedTool) -> Result<()> {
    let (python_version, build_id) = parse_python_distribution(&tool.version)?;
    let expected = BTreeMap::from([
        ("build_id".to_owned(), build_id.to_owned()),
        (
            "distribution".to_owned(),
            "astral-sh/python-build-standalone".to_owned(),
        ),
        ("python_version".to_owned(), python_version.to_owned()),
        ("variant".to_owned(), PYTHON_VARIANT.to_owned()),
    ]);
    if tool.metadata != expected {
        return Err(Error::InvalidLockfile {
            reason: "Python lock metadata must identify the exact CPython version, standalone build and install_only variant"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_java_metadata(tool: &LockedTool) -> Result<()> {
    let version = JavaVersion::parse(&tool.version)?;
    let release_name = tool
        .metadata
        .get("release_name")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Java lock metadata has no release name".to_owned(),
        })?;
    let openjdk_version = tool
        .metadata
        .get("openjdk_version")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Java lock metadata has no OpenJDK version".to_owned(),
        })?;
    let image_type = tool
        .metadata
        .get("image_type")
        .filter(|value| matches!(value.as_str(), "jdk" | "jre"))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Java lock metadata must identify a jdk or jre package".to_owned(),
        })?;
    if tool.options.is_empty() {
        if image_type != "jdk" {
            return Err(Error::InvalidLockfile {
                reason: "legacy Java locks without options may only identify a JDK".to_owned(),
            });
        }
    } else {
        let expected_options = BTreeMap::from([
            ("distribution".to_owned(), "temurin".to_owned()),
            ("package".to_owned(), image_type.clone()),
        ]);
        if tool.options != expected_options {
            return Err(Error::InvalidLockfile {
                reason: "Java lock options must identify the Temurin distribution and jdk or jre package"
                    .to_owned(),
            });
        }
    }
    let mut expected = BTreeMap::from([
        ("distribution".to_owned(), "eclipse-temurin".to_owned()),
        ("vendor".to_owned(), "eclipse".to_owned()),
        ("image_type".to_owned(), image_type.clone()),
        ("jvm_impl".to_owned(), "hotspot".to_owned()),
        ("heap_size".to_owned(), "normal".to_owned()),
        ("release_type".to_owned(), "ga".to_owned()),
        ("feature_version".to_owned(), version.feature().to_string()),
        ("release_name".to_owned(), release_name.clone()),
        ("openjdk_version".to_owned(), openjdk_version.clone()),
    ]);
    for artifact in &tool.artifacts {
        expected.insert(
            format!("signature_link.{}", artifact.target),
            format!("{}.sig", artifact.canonical_url),
        );
    }
    if tool.metadata != expected {
        return Err(Error::InvalidLockfile {
            reason: "Java lock metadata must identify one Eclipse Temurin GA JDK/JRE HotSpot release and each archive signature"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_rust_metadata(tool: &LockedTool) -> Result<()> {
    let date = tool
        .metadata
        .get("manifest_date")
        .filter(|value| valid_release_date(value))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Rust lock metadata has no valid manifest date".to_owned(),
        })?;
    let manifest_sha256 = tool
        .metadata
        .get("manifest_sha256")
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Rust lock metadata has no valid manifest SHA-256".to_owned(),
        })?;
    let channel = tool
        .metadata
        .get("channel")
        .filter(|value| matches!(value.as_str(), "stable" | "nightly"))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Rust lock metadata has no supported channel".to_owned(),
        })?;
    let profile = tool
        .metadata
        .get("profile")
        .filter(|value| matches!(value.as_str(), "minimal" | "default" | "complete"))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Rust lock metadata has no supported profile".to_owned(),
        })?;
    let components = tool
        .metadata
        .get("components")
        .filter(|value| {
            let values = value.split(',').collect::<Vec<_>>();
            !values.is_empty()
                && values.iter().all(|component| !component.is_empty())
                && values.iter().collect::<HashSet<_>>().len() == values.len()
        })
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Rust lock metadata has no valid component set".to_owned(),
        })?;
    let expected_keys = [
        "channel",
        "components",
        "manifest_date",
        "manifest_sha256",
        "profile",
    ];
    if tool.metadata.len() != expected_keys.len()
        || expected_keys
            .iter()
            .any(|key| !tool.metadata.contains_key(*key))
        || manifest_sha256 != &manifest_sha256.to_ascii_lowercase()
    {
        return Err(Error::InvalidLockfile {
            reason: "Rust lock metadata must identify one v2 channel manifest".to_owned(),
        });
    }
    for key in tool.options.keys() {
        if !matches!(key.as_str(), "profile" | "components" | "targets" | "date") {
            return Err(Error::InvalidLockfile {
                reason: format!("Rust lock contains unsupported option {key}"),
            });
        }
    }
    if tool
        .options
        .get("profile")
        .is_some_and(|value| value != profile)
        || tool.options.get("components").is_some_and(|value| {
            let resolved = components.split(',').collect::<HashSet<_>>();
            value
                .split(',')
                .any(|component| !resolved.contains(component))
        })
        || tool.options.get("date").is_some_and(|value| value != date)
        || (channel == "nightly"
            && tool.requested != format!("nightly-{date}")
            && !(tool.requested == "nightly"
                && tool.options.get("date").is_some_and(|value| value == date)))
        || (channel == "stable" && tool.requested.starts_with("nightly"))
        || (tool.options.is_empty()
            && (profile != RUST_PROFILE
                || !matches!(
                    components.as_str(),
                    RUST_COMPONENTS | "rustc,cargo,rust-std,rust-docs"
                )))
    {
        return Err(Error::InvalidLockfile {
            reason: "Rust lock metadata does not match its selector and structured options"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_dotnet_metadata(tool: &LockedTool) -> Result<()> {
    let version = DotnetVersion::parse(&tool.version)?;
    let release_version = tool
        .metadata
        .get("release_version")
        .filter(|value| DotnetVersion::parse(value).is_ok())
        .ok_or_else(|| Error::InvalidLockfile {
            reason: ".NET SDK lock metadata has no exact release version".to_owned(),
        })?;
    let release_date = tool
        .metadata
        .get("release_date")
        .filter(|value| valid_release_date(value))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: ".NET SDK lock metadata has no valid release date".to_owned(),
        })?;
    let release_type = tool
        .metadata
        .get("release_type")
        .filter(|value| matches!(value.as_str(), "lts" | "sts"))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: ".NET SDK lock metadata has no GA release type".to_owned(),
        })?;
    let support_phase = tool
        .metadata
        .get("support_phase")
        .filter(|value| matches!(value.as_str(), "active" | "maintenance" | "eol"))
        .ok_or_else(|| Error::InvalidLockfile {
            reason: ".NET SDK lock metadata has an unknown support phase".to_owned(),
        })?;
    let expected = BTreeMap::from([
        ("channel".to_owned(), version.channel()),
        ("release_date".to_owned(), release_date.clone()),
        ("release_type".to_owned(), release_type.clone()),
        ("release_version".to_owned(), release_version.clone()),
        ("support_phase".to_owned(), support_phase.clone()),
    ]);
    if tool.metadata != expected {
        return Err(Error::InvalidLockfile {
            reason: ".NET SDK lock metadata must identify one Microsoft GA SDK release".to_owned(),
        });
    }
    Ok(())
}

fn validate_declarative_metadata(tool: &LockedTool) -> Result<()> {
    let expected = [
        "provider-id",
        "provider-revision",
        "registry-fingerprint",
        "repository",
        "tag",
    ];
    let provider_id = tool.metadata.get("provider-id");
    let revision = tool
        .metadata
        .get("provider-revision")
        .and_then(|value| value.parse::<u32>().ok());
    let fingerprint = tool.metadata.get("registry-fingerprint");
    let repository = tool.metadata.get("repository");
    let tag = tool.metadata.get("tag");
    if !is_exact_numeric_triplet(&tool.version)
        || !tool.options.is_empty()
        || tool.released_at.is_none()
        || tool.metadata.len() != expected.len()
        || expected.iter().any(|key| !tool.metadata.contains_key(*key))
        || provider_id.is_none_or(|value| !valid_scoped_name(value))
        || revision.is_none_or(|value| value == 0)
        || fingerprint.is_none_or(|value| {
            value.len() != 40
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'A'..=b'F'))
        })
        || repository.is_none_or(|value| !valid_scoped_name(value))
        || tag.is_none_or(|value| !valid_lock_component(value))
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{} declarative lock metadata must identify one Provider revision, signer, repository and release tag",
                tool.name
            ),
        });
    }
    Ok(())
}

fn validate_locked_declarative_artifact(
    tool: &LockedTool,
    artifact: &LockedArtifact,
) -> Result<()> {
    let repository = &tool.metadata["repository"];
    let tag = &tool.metadata["tag"];
    let expected_url = format!(
        "https://github.com/{repository}/releases/download/{tag}/{}",
        artifact.artifact_path
    );
    let integrity = artifact.artifact_integrity()?;
    if artifact.format != LockedArtifactFormat::Binary
        || !artifact.archive_root.is_empty()
        || artifact.verification != "https-checksum"
        || !artifact.overlays.is_empty()
        || !valid_lock_component(&artifact.artifact_path)
        || artifact.canonical_url != expected_url
        || integrity.algorithm() != crate::IntegrityAlgorithm::Sha256
        || integrity.cache_key() != artifact.sha256
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{} declarative artifact {} does not match its verified binary identity",
                tool.name, artifact.target
            ),
        });
    }
    Ok(())
}

fn valid_scoped_name(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(owner), Some(name), None)
            if valid_lock_component(owner) && valid_lock_component(name)
    )
}

fn valid_lock_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_release_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn is_exact_numeric_triplet(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u64>().is_ok()
        })
}

fn validate_locked_flutter_artifact(version: &str, artifact: &LockedArtifact) -> Result<()> {
    let plan = plan_flutter_artifact(&SourceConfig::default(), version, &artifact.target)?;
    let expected_format = match plan.format {
        FlutterArchiveFormat::Zip => LockedArtifactFormat::Zip,
        FlutterArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Flutter provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid Flutter SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "flutter-release-json-sha256"
        && !artifact
            .verification
            .starts_with("flutter-release-json-sha256-source:")
    {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported Flutter verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "Flutter artifact {} cannot contain overlays",
                artifact.target
            ),
        });
    }
    Ok(())
}

fn validate_locked_go_artifact(version: &str, artifact: &LockedArtifact) -> Result<()> {
    let plan = plan_go_artifact(&SourceConfig::default(), version, &artifact.target)?;
    let expected_format = match plan.format {
        GoArchiveFormat::Zip => LockedArtifactFormat::Zip,
        GoArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Go provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid Go SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "go-download-json-sha256"
        && !artifact
            .verification
            .starts_with("go-download-json-sha256-source:")
    {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported Go verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("Go artifact {} cannot contain overlays", artifact.target),
        });
    }
    Ok(())
}

fn validate_locked_node_artifact(
    version: &str,
    artifact: &LockedArtifact,
    allow_pre_v1_verification: bool,
) -> Result<()> {
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
    let current_verification = artifact.verification == "nodejs-openpgp-sha256"
        || artifact
            .verification
            .starts_with("nodejs-openpgp-sha256-source:");
    let pre_v1_verification = allow_pre_v1_verification
        && (artifact.verification == "nodejs-shasums-https"
            || artifact
                .verification
                .starts_with("nodejs-shasums-https-source:"));
    if !current_verification && !pre_v1_verification {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "unsupported Node verification for {}; regenerate this pre-1.0 lock with `pinset use node@<selector>`",
                artifact.target
            ),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("Node artifact {} cannot contain overlays", artifact.target),
        });
    }
    Ok(())
}

fn validate_locked_python_artifact(version: &str, artifact: &LockedArtifact) -> Result<()> {
    let plan = plan_python_artifact(&SourceConfig::default(), version, &artifact.target)?;
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != LockedArtifactFormat::TarGz
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Python provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid Python SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "python-build-standalone-versions-sha256" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported Python verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "Python artifact {} cannot contain overlays",
                artifact.target
            ),
        });
    }
    Ok(())
}

fn validate_locked_java_artifact(tool: &LockedTool, artifact: &LockedArtifact) -> Result<()> {
    let release_name = tool
        .metadata
        .get("release_name")
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Java lock metadata has no release_name".to_owned(),
        })?;
    let package_name = artifact
        .canonical_url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::InvalidLockfile {
            reason: format!("Java artifact {} has no package name", artifact.target),
        })?;
    let image_type = tool
        .metadata
        .get("image_type")
        .ok_or_else(|| Error::InvalidLockfile {
            reason: "Java lock metadata has no image_type".to_owned(),
        })?;
    let plan = plan_java_artifact_with_package(
        &tool.version,
        release_name,
        &artifact.target,
        image_type,
        package_name,
        &artifact.canonical_url,
    )?;
    let expected_format = match plan.format {
        JavaArchiveFormat::Zip => LockedArtifactFormat::Zip,
        JavaArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Eclipse Temurin provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid Java SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "adoptium-api-sha256" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported Java verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!("Java artifact {} cannot contain overlays", artifact.target),
        });
    }
    Ok(())
}

fn validate_locked_rust_artifact(tool: &LockedTool, artifact: &LockedArtifact) -> Result<()> {
    let manifest_date =
        tool.metadata
            .get("manifest_date")
            .ok_or_else(|| Error::InvalidLockfile {
                reason: "Rust lock metadata has no manifest_date".to_owned(),
            })?;
    let nightly = tool
        .metadata
        .get("channel")
        .is_some_and(|channel| channel == "nightly");
    let plan = if nightly {
        plan_rust_nightly_artifact(
            &tool.version,
            manifest_date,
            &artifact.target,
            &artifact.canonical_url,
        )?
    } else {
        plan_rust_artifact(
            &tool.version,
            manifest_date,
            &artifact.target,
            &artifact.canonical_url,
        )?
    };
    let expected_format = match plan.format {
        RustArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Rust provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid Rust SHA-256 for {}", artifact.target),
        });
    }
    if artifact.verification != "rust-v2-manifest-sha256" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported Rust verification for {}", artifact.target),
        });
    }
    let channel = if nightly { "nightly" } else { &tool.version };
    let expected_targets = tool
        .options
        .get("targets")
        .map(|targets| {
            targets
                .split(',')
                .filter(|target| !target.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if artifact.overlays.len() != expected_targets.len() {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "Rust artifact {} has {} target overlays, expected {}",
                artifact.target,
                artifact.overlays.len(),
                expected_targets.len()
            ),
        });
    }
    for (overlay, target) in artifact.overlays.iter().zip(expected_targets) {
        let archive_name = format!("rust-std-{channel}-{target}.tar.xz");
        let expected_url =
            format!("https://static.rust-lang.org/dist/{manifest_date}/{archive_name}");
        if overlay.canonical_url != expected_url
            || overlay.artifact_path != format!("dist/{manifest_date}/{archive_name}")
            || overlay.archive_root != format!("rust-std-{channel}-{target}")
            || overlay.format != LockedArtifactFormat::TarXz
            || overlay.verification != "rust-v2-manifest-sha256"
            || overlay.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha256
        {
            return Err(Error::InvalidLockfile {
                reason: format!(
                    "Rust target overlay {target} does not match the built-in Rust provider"
                ),
            });
        }
    }
    Ok(())
}

fn validate_locked_dotnet_artifact(tool: &LockedTool, artifact: &LockedArtifact) -> Result<()> {
    let plan = plan_dotnet_artifact(&tool.version, &artifact.target, &artifact.canonical_url)?;
    let expected_format = match plan.format {
        DotnetArchiveFormat::Zip => LockedArtifactFormat::Zip,
        DotnetArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
    };
    if artifact.canonical_url != plan.canonical_url
        || artifact.artifact_path != plan.artifact_path
        || artifact.archive_root != plan.archive_root
        || artifact.format != expected_format
    {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "artifact identity for {} does not match the built-in Microsoft .NET SDK provider",
                artifact.target
            ),
        });
    }
    if artifact.artifact_integrity()?.algorithm() != crate::IntegrityAlgorithm::Sha512 {
        return Err(Error::InvalidLockfile {
            reason: format!("invalid .NET SDK SHA-512 for {}", artifact.target),
        });
    }
    if artifact.verification != "dotnet-release-metadata-sha512" {
        return Err(Error::InvalidLockfile {
            reason: format!("unsupported .NET SDK verification for {}", artifact.target),
        });
    }
    if !artifact.overlays.is_empty() {
        return Err(Error::InvalidLockfile {
            reason: format!(
                ".NET SDK artifact {} cannot contain overlays",
                artifact.target
            ),
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
    let packages = if tool.name == "pnpm" {
        pnpm_target_packages(&artifact.target, package)
    } else {
        vec![package]
    };
    let valid_identity = packages.into_iter().any(|package| {
        let package_base = package.rsplit('/').next().expect("npm package is nonempty");
        let artifact_path = format!("{package}/-/{package_base}-{}.tgz", tool.version);
        let canonical_url = format!("https://registry.npmjs.org/{artifact_path}");
        artifact.canonical_url == canonical_url && artifact.artifact_path == artifact_path
    });
    if artifact.archive_root != "package" || !valid_identity {
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
        Some(major) => Ok(major >= 11),
        None => Err(Error::InvalidLockfile {
            reason: format!("invalid pnpm version {version}"),
        }),
    }
}

fn pnpm_target_packages<'a>(target: &str, legacy: &'a str) -> Vec<&'a str> {
    let modern = match target {
        "windows-x86_64" => Some("@pnpm/exe.win32-x64"),
        "linux-x86_64" => Some("@pnpm/exe.linux-x64"),
        "linux-aarch64" => Some("@pnpm/exe.linux-arm64"),
        "macos-aarch64" => Some("@pnpm/exe.darwin-arm64"),
        _ => None,
    };
    std::iter::once(legacy).chain(modern).collect()
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
            ("linux-aarch64", "@pnpm/linux-arm64"),
            ("macos-aarch64", "@pnpm/macos-arm64"),
        ],
        "bun" => &[
            ("windows-x86_64-avx2", "@oven/bun-windows-x64"),
            ("windows-x86_64-baseline", "@oven/bun-windows-x64-baseline"),
            ("linux-x86_64-avx2", "@oven/bun-linux-x64"),
            ("linux-x86_64-baseline", "@oven/bun-linux-x64-baseline"),
            ("linux-aarch64", "@oven/bun-linux-aarch64"),
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
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts.clone(),
        );
        save_lockfile(&path, &lockfile).expect("save lockfile");
        let first = fs::read(&path).expect("first lockfile");

        artifacts.rotate_left(1);
        let reordered = Lockfile::new_node(
            "pinset 0.1.0".to_owned(),
            "24.0.0".to_owned(),
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts,
        );
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
                "linux-aarch64",
                "linux-x86_64",
                "macos-aarch64",
                "macos-x86_64",
                "windows-x86_64",
            ]
        );
    }

    #[test]
    fn target_refresh_loader_accepts_only_known_pre_v1_provider_matrices() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(LOCKFILE_FILENAME);
        let legacy = Lockfile {
            schema: 2,
            generated_by: "pinset 0.9.0".to_owned(),
            tools: vec![
                locked_npm_tool("bun", "1.3.14", false),
                locked_npm_tool("pnpm", "11.21.0", false),
            ],
        };
        fs::write(&path, toml::to_string_pretty(&legacy).expect("legacy TOML"))
            .expect("legacy lockfile");

        let strict = load_lockfile(&path).expect_err("strict loader rejects the old matrix");
        assert!(strict.to_string().contains("linux-aarch64"));
        let (loaded, refresh_tools) =
            load_lockfile_for_provider_refresh(&path).expect("refresh loader accepts old matrix");
        assert_eq!(refresh_tools, ["bun", "pnpm"]);
        assert_eq!(loaded.tools, legacy.tools);

        let (_, requires_refresh) = load_lockfile_for_target_refresh(&path)
            .expect("compatibility loader accepts old matrix");
        assert!(requires_refresh);

        let mut corrupted = legacy;
        corrupted.tools[0].artifacts[0].canonical_url =
            "https://example.invalid/bun.tgz".to_owned();
        fs::write(
            &path,
            toml::to_string_pretty(&corrupted).expect("corrupt TOML"),
        )
        .expect("corrupt lockfile");
        let error = load_lockfile_for_provider_refresh(&path)
            .expect_err("refresh loader still validates every present artifact");
        assert!(error.to_string().contains("invalid npm artifact identity"));
    }

    #[test]
    fn target_refresh_loader_detects_every_provider_expanded_in_v1() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(LOCKFILE_FILENAME);

        for name in [
            "node", "pnpm", "bun", "go", "python", "java", "rust", "dotnet",
        ] {
            let mut tool = locked_current_provider_tool(name);
            tool.artifacts
                .retain(|artifact| artifact.target != "linux-aarch64");
            if name == "node" {
                tool.metadata.clear();
                for artifact in &mut tool.artifacts {
                    artifact.verification = "nodejs-shasums-https".to_owned();
                }
            } else if name == "java" {
                tool.metadata.remove("signature_link.linux-aarch64");
            }
            let legacy = Lockfile {
                schema: 2,
                generated_by: "pinset 0.9.0".to_owned(),
                tools: vec![tool],
            };
            fs::write(&path, toml::to_string_pretty(&legacy).expect("legacy TOML"))
                .expect("legacy lockfile");

            load_lockfile(&path).expect_err("strict loader rejects the old matrix");
            let (loaded, refresh_tools) = load_lockfile_for_provider_refresh(&path)
                .unwrap_or_else(|error| panic!("refresh loader rejected {name}: {error}"));
            assert_eq!(refresh_tools, [name]);
            assert_eq!(loaded.tools, legacy.tools);
        }
    }

    #[test]
    fn schema_three_provider_refresh_accepts_selectors_for_every_legacy_matrix() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(LOCKFILE_FILENAME);

        for (name, requested) in [
            ("node", "24"),
            ("pnpm", "11"),
            ("bun", "latest"),
            ("go", "1.25"),
            ("python", "3.14"),
            ("java", "21"),
            ("rust", "stable"),
            ("dotnet", "10.0"),
        ] {
            let mut tool = locked_current_provider_tool(name);
            tool.requested = requested.to_owned();
            tool.artifacts
                .retain(|artifact| artifact.target != "linux-aarch64");
            if name == "node" {
                tool.metadata.clear();
                for artifact in &mut tool.artifacts {
                    artifact.verification = "nodejs-shasums-https".to_owned();
                }
            } else if name == "java" {
                tool.metadata.remove("signature_link.linux-aarch64");
            }
            let legacy = Lockfile {
                schema: 3,
                generated_by: "pinset 2.1.6".to_owned(),
                tools: vec![tool],
            };
            fs::write(&path, toml::to_string_pretty(&legacy).expect("legacy TOML"))
                .expect("legacy lockfile");

            load_lockfile(&path).expect_err("strict loader rejects the old matrix");
            let (loaded, refresh_tools) = load_lockfile_for_provider_refresh(&path)
                .unwrap_or_else(|error| panic!("refresh loader rejected {name}: {error}"));
            assert_eq!(refresh_tools, [name]);
            assert_eq!(loaded.tools[0].requested, requested);
        }
    }

    #[test]
    fn schema_three_separates_requested_selector_from_resolved_version() {
        for (name, requested) in [
            ("node", "24"),
            ("pnpm", "11"),
            ("bun", "latest"),
            ("go", "1.25"),
            ("flutter", "stable"),
            ("python", "3.14"),
            ("java", "21"),
            ("rust", "stable"),
            ("dotnet", "10.0"),
        ] {
            let mut tool = locked_current_provider_tool(name);
            tool.requested = requested.to_owned();
            let resolved = tool.version.clone();
            let mut lockfile = Lockfile {
                schema: 3,
                generated_by: "pinset 1.5.0".to_owned(),
                tools: vec![tool],
            };

            validate_lockfile(&lockfile)
                .unwrap_or_else(|error| panic!("schema 3 rejected {name} selector: {error}"));
            assert_eq!(
                validate_lock_matches_tool(&lockfile, name, requested, Path::new("pinset.toml"),)
                    .unwrap_or_else(|error| panic!("{name} selector mismatch: {error}"))
                    .version,
                resolved
            );

            lockfile.schema = 2;
            assert!(
                matches!(
                    validate_lockfile(&lockfile),
                    Err(Error::InvalidLockfile { .. })
                ),
                "schema 2 unexpectedly accepted {name} selector"
            );
        }
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
        let lockfile = Lockfile::new_node(
            "pinset".to_owned(),
            "24.0.0".to_owned(),
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts,
        );
        assert!(matches!(
            save_lockfile(&path, &lockfile),
            Err(Error::InvalidLockfile { .. })
        ));
    }

    #[test]
    fn pre_v1_node_verification_requires_an_explicit_relock() {
        let artifacts = MVP_NODE_TARGETS
            .into_iter()
            .map(locked_artifact)
            .collect::<Vec<_>>();
        let mut missing_signature_metadata = Lockfile::new_node(
            "pinset 0.9.0".to_owned(),
            "24.0.0".to_owned(),
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts.clone(),
        );
        missing_signature_metadata.tools[0].metadata.clear();
        let error = validate_lockfile(&missing_signature_metadata).expect_err("legacy metadata");
        assert!(error.to_string().contains("pinset use node@<selector>"));

        let mut https_checksums_only = Lockfile::new_node(
            "pinset 0.9.0".to_owned(),
            "24.0.0".to_owned(),
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts,
        );
        https_checksums_only.tools[0].artifacts[0].verification = "nodejs-shasums-https".to_owned();
        let error = validate_lockfile(&https_checksums_only).expect_err("legacy verification");
        assert!(error.to_string().contains("pinset use node@<selector>"));
    }

    #[test]
    fn validates_pnpm_overlay_by_package_generation() {
        let pnpm_10 = locked_pnpm_artifact("10.34.5", false);
        let pnpm_10_tool = locked_pnpm_tool("10.34.5", pnpm_10.clone());
        validate_locked_npm_artifact(&pnpm_10_tool, &pnpm_10).expect("pnpm 10 artifact");

        let pnpm_10_with_overlay = locked_pnpm_artifact("10.34.5", true);
        let pnpm_10_with_overlay_tool = locked_pnpm_tool("10.34.5", pnpm_10_with_overlay.clone());
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

    #[test]
    fn validates_go_provider_identity_and_partial_targets() {
        let artifacts = GO_TARGETS
            .into_iter()
            .map(|target| locked_go_artifact("1.25.1", target))
            .collect::<Vec<_>>();
        let tool = LockedTool {
            name: "go".to_owned(),
            requested: "1.25.1".to_owned(),
            version: "1.25.1".to_owned(),
            provider: "go-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect("Go lock");

        let mut invalid = tool;
        invalid.artifacts[0].canonical_url = "https://example.invalid/go.zip".to_owned();
        assert!(matches!(
            validate_locked_tool(&invalid),
            Err(Error::InvalidLockfile { .. })
        ));

        let incomplete = LockedTool {
            name: "go".to_owned(),
            requested: "1.25.1".to_owned(),
            version: "1.25.1".to_owned(),
            provider: "go-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            options: Default::default(),
            artifacts: artifacts.into_iter().skip(1).collect(),
        };
        validate_locked_tool(&incomplete).expect("partial Go lock");
    }

    #[test]
    fn validates_flutter_provider_metadata_identity_and_partial_targets() {
        let artifacts = FLUTTER_TARGETS
            .into_iter()
            .map(|target| locked_flutter_artifact("3.47.0", target))
            .collect::<Vec<_>>();
        let mut metadata = BTreeMap::new();
        metadata.insert("channel".to_owned(), "stable".to_owned());
        metadata.insert("dart_version".to_owned(), "3.13.0".to_owned());
        metadata.insert("release_hash".to_owned(), "cd".repeat(20));
        let tool = LockedTool {
            name: "flutter".to_owned(),
            requested: "3.47.0".to_owned(),
            version: "3.47.0".to_owned(),
            provider: "flutter-official".to_owned(),
            released_at: None,
            metadata,
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect("Flutter lock");

        let mut invalid_metadata = tool.clone();
        invalid_metadata
            .metadata
            .insert("channel".to_owned(), "beta".to_owned());
        assert!(matches!(
            validate_locked_tool(&invalid_metadata),
            Err(Error::InvalidLockfile { .. })
        ));

        let mut invalid_dart_version = tool.clone();
        invalid_dart_version
            .metadata
            .insert("dart_version".to_owned(), "3.13".to_owned());
        assert!(matches!(
            validate_locked_tool(&invalid_dart_version),
            Err(Error::InvalidLockfile { .. })
        ));

        let mut incomplete = tool;
        incomplete.artifacts = artifacts.into_iter().skip(1).collect();
        validate_locked_tool(&incomplete).expect("partial Flutter lock");
    }

    #[test]
    fn validates_python_distribution_metadata_and_partial_targets() {
        let distribution = "3.14.7+20260807";
        let artifacts = PYTHON_TARGETS
            .into_iter()
            .map(|target| locked_python_artifact(distribution, target))
            .collect::<Vec<_>>();
        let metadata = BTreeMap::from([
            ("build_id".to_owned(), "20260807".to_owned()),
            (
                "distribution".to_owned(),
                "astral-sh/python-build-standalone".to_owned(),
            ),
            ("python_version".to_owned(), "3.14.7".to_owned()),
            ("variant".to_owned(), "install_only".to_owned()),
        ]);
        let tool = LockedTool {
            name: "python".to_owned(),
            requested: distribution.to_owned(),
            version: distribution.to_owned(),
            provider: "python-build-standalone".to_owned(),
            released_at: None,
            metadata,
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect("Python lock");

        let mut invalid_metadata = tool.clone();
        invalid_metadata
            .metadata
            .insert("build_id".to_owned(), "20260808".to_owned());
        assert!(matches!(
            validate_locked_tool(&invalid_metadata),
            Err(Error::InvalidLockfile { .. })
        ));

        let mut incomplete = tool;
        incomplete.artifacts = artifacts.into_iter().skip(1).collect();
        validate_locked_tool(&incomplete).expect("partial Python lock");
    }

    #[test]
    fn validates_temurin_metadata_signatures_and_partial_targets() {
        let version = "21.0.8+9";
        let release_name = "jdk-21.0.8+9";
        let artifacts = JAVA_TARGETS
            .into_iter()
            .map(|target| locked_java_artifact(version, release_name, target))
            .collect::<Vec<_>>();
        let mut metadata = BTreeMap::from([
            ("distribution".to_owned(), "eclipse-temurin".to_owned()),
            ("vendor".to_owned(), "eclipse".to_owned()),
            ("image_type".to_owned(), "jdk".to_owned()),
            ("jvm_impl".to_owned(), "hotspot".to_owned()),
            ("heap_size".to_owned(), "normal".to_owned()),
            ("release_type".to_owned(), "ga".to_owned()),
            ("feature_version".to_owned(), "21".to_owned()),
            ("release_name".to_owned(), release_name.to_owned()),
            ("openjdk_version".to_owned(), "21.0.8+9-LTS".to_owned()),
        ]);
        for artifact in &artifacts {
            metadata.insert(
                format!("signature_link.{}", artifact.target),
                format!("{}.sig", artifact.canonical_url),
            );
        }
        let tool = LockedTool {
            name: "java".to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: "adoptium-temurin".to_owned(),
            released_at: None,
            metadata,
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect("Java lock");

        let jre_artifacts = JAVA_TARGETS
            .into_iter()
            .map(|target| locked_java_artifact_for(version, release_name, target, "jre"))
            .collect::<Vec<_>>();
        let mut jre = tool.clone();
        jre.metadata
            .insert("image_type".to_owned(), "jre".to_owned());
        jre.metadata
            .retain(|key, _| !key.starts_with("signature_link."));
        for artifact in &jre_artifacts {
            jre.metadata.insert(
                format!("signature_link.{}", artifact.target),
                format!("{}.sig", artifact.canonical_url),
            );
        }
        jre.options = BTreeMap::from([
            ("distribution".to_owned(), "temurin".to_owned()),
            ("package".to_owned(), "jre".to_owned()),
        ]);
        jre.artifacts = jre_artifacts;
        validate_locked_tool(&jre).expect("Temurin JRE lock");
        assert_ne!(tool.installation_version(), jre.installation_version());

        let mut invalid_signature = tool.clone();
        invalid_signature.metadata.insert(
            "signature_link.linux-x86_64".to_owned(),
            "https://example.invalid/jdk.sig".to_owned(),
        );
        assert!(matches!(
            validate_locked_tool(&invalid_signature),
            Err(Error::InvalidLockfile { .. })
        ));

        let mut incomplete = tool;
        incomplete.artifacts = artifacts.into_iter().skip(1).collect();
        incomplete
            .metadata
            .retain(|key, _| !key.starts_with("signature_link.windows-x86_64"));
        validate_locked_tool(&incomplete).expect("partial Java lock");
    }

    #[test]
    fn validates_official_rust_manifest_identity_and_partial_targets() {
        let version = "1.97.1";
        let date = "2026-07-16";
        let artifacts = RUST_TARGETS
            .into_iter()
            .map(|target| locked_rust_artifact(version, date, target))
            .collect::<Vec<_>>();
        let tool = LockedTool {
            name: "rust".to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: "rust-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::from([
                ("channel".to_owned(), "stable".to_owned()),
                ("components".to_owned(), RUST_COMPONENTS.to_owned()),
                ("manifest_date".to_owned(), date.to_owned()),
                ("manifest_sha256".to_owned(), "ab".repeat(32)),
                ("profile".to_owned(), RUST_PROFILE.to_owned()),
            ]),
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect("Rust lock");
        assert_eq!(tool.installation_version(), version);

        let mut minimal = tool.clone();
        minimal
            .options
            .insert("profile".to_owned(), "minimal".to_owned());
        let mut complete = tool.clone();
        complete
            .options
            .insert("profile".to_owned(), "complete".to_owned());
        assert_ne!(minimal.installation_version(), version);
        assert_ne!(
            minimal.installation_version(),
            complete.installation_version()
        );

        let mut invalid = tool.clone();
        invalid.artifacts[0].canonical_url = "https://example.invalid/rust.tar.xz".to_owned();
        assert!(matches!(
            validate_locked_tool(&invalid),
            Err(Error::InvalidRustArtifact { .. }) | Err(Error::InvalidLockfile { .. })
        ));

        let mut incomplete = tool;
        incomplete.artifacts = artifacts.into_iter().skip(1).collect();
        validate_locked_tool(&incomplete).expect("partial Rust lock");
    }

    #[test]
    fn validates_fixed_date_nightly_with_extra_compilation_targets() {
        let version = "1.98.0";
        let date = "2026-07-16";
        let target = "wasm32-unknown-unknown";
        let artifacts = RUST_TARGETS
            .into_iter()
            .map(|platform| locked_rust_nightly_artifact(version, date, platform, target))
            .collect::<Vec<_>>();
        let tool = LockedTool {
            name: "rust".to_owned(),
            requested: format!("nightly-{date}"),
            version: version.to_owned(),
            provider: "rust-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::from([
                ("channel".to_owned(), "nightly".to_owned()),
                ("components".to_owned(), RUST_COMPONENTS.to_owned()),
                ("manifest_date".to_owned(), date.to_owned()),
                ("manifest_sha256".to_owned(), "ab".repeat(32)),
                ("profile".to_owned(), RUST_PROFILE.to_owned()),
            ]),
            options: BTreeMap::from([
                ("date".to_owned(), date.to_owned()),
                ("targets".to_owned(), target.to_owned()),
            ]),
            artifacts,
        };

        validate_locked_tool(&tool).expect("nightly Rust lock");
    }

    #[test]
    fn validates_official_dotnet_sdk_metadata_and_partial_targets() {
        let version = "10.0.400";
        let artifacts = DOTNET_TARGETS
            .into_iter()
            .map(|target| locked_dotnet_artifact(version, target))
            .collect::<Vec<_>>();
        let tool = LockedTool {
            name: "dotnet".to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: "microsoft-dotnet-sdk".to_owned(),
            released_at: None,
            metadata: BTreeMap::from([
                ("channel".to_owned(), "10.0".to_owned()),
                ("release_date".to_owned(), "2026-08-11".to_owned()),
                ("release_type".to_owned(), "lts".to_owned()),
                ("release_version".to_owned(), "10.0.14".to_owned()),
                ("support_phase".to_owned(), "active".to_owned()),
            ]),
            options: Default::default(),
            artifacts: artifacts.clone(),
        };
        validate_locked_tool(&tool).expect(".NET SDK lock");

        let mut invalid = tool.clone();
        invalid
            .metadata
            .insert("support_phase".to_owned(), "eol".to_owned());
        validate_locked_tool(&invalid).expect("EOL .NET lock");

        let mut incomplete = tool;
        incomplete.artifacts = artifacts.into_iter().skip(1).collect();
        validate_locked_tool(&incomplete).expect("partial .NET lock");
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
            verification: "nodejs-openpgp-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_pnpm_tool(version: &str, artifact: LockedArtifact) -> LockedTool {
        LockedTool {
            name: "pnpm".to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: "pnpm-npm".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            options: Default::default(),
            artifacts: vec![artifact],
        }
    }

    fn locked_current_provider_tool(name: &str) -> LockedTool {
        match name {
            "node" => Lockfile::new_node(
                "pinset test".to_owned(),
                "24.0.0".to_owned(),
                "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
                "official".to_owned(),
                MVP_NODE_TARGETS.into_iter().map(locked_artifact).collect(),
            )
            .tools
            .remove(0),
            "pnpm" => locked_npm_tool("pnpm", "11.21.0", true),
            "bun" => locked_npm_tool("bun", "1.3.14", true),
            "go" => LockedTool {
                name: "go".to_owned(),
                requested: "1.25.1".to_owned(),
                version: "1.25.1".to_owned(),
                provider: "go-official".to_owned(),
                released_at: None,
                metadata: BTreeMap::new(),
                options: Default::default(),
                artifacts: GO_TARGETS
                    .into_iter()
                    .map(|target| locked_go_artifact("1.25.1", target))
                    .collect(),
            },
            "flutter" => LockedTool {
                name: "flutter".to_owned(),
                requested: "3.47.0".to_owned(),
                version: "3.47.0".to_owned(),
                provider: "flutter-official".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    ("channel".to_owned(), "stable".to_owned()),
                    ("dart_version".to_owned(), "3.13.0".to_owned()),
                    ("release_hash".to_owned(), "cd".repeat(20)),
                ]),
                options: Default::default(),
                artifacts: FLUTTER_TARGETS
                    .into_iter()
                    .map(|target| locked_flutter_artifact("3.47.0", target))
                    .collect(),
            },
            "python" => LockedTool {
                name: "python".to_owned(),
                requested: "3.14.7+20260807".to_owned(),
                version: "3.14.7+20260807".to_owned(),
                provider: "python-build-standalone".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    ("build_id".to_owned(), "20260807".to_owned()),
                    (
                        "distribution".to_owned(),
                        "astral-sh/python-build-standalone".to_owned(),
                    ),
                    ("python_version".to_owned(), "3.14.7".to_owned()),
                    ("variant".to_owned(), PYTHON_VARIANT.to_owned()),
                ]),
                options: Default::default(),
                artifacts: PYTHON_TARGETS
                    .into_iter()
                    .map(|target| locked_python_artifact("3.14.7+20260807", target))
                    .collect(),
            },
            "java" => {
                let version = "21.0.8+9";
                let release_name = "jdk-21.0.8+9";
                let artifacts = JAVA_TARGETS
                    .into_iter()
                    .map(|target| locked_java_artifact(version, release_name, target))
                    .collect::<Vec<_>>();
                let mut metadata = BTreeMap::from([
                    ("distribution".to_owned(), "eclipse-temurin".to_owned()),
                    ("vendor".to_owned(), "eclipse".to_owned()),
                    ("image_type".to_owned(), "jdk".to_owned()),
                    ("jvm_impl".to_owned(), "hotspot".to_owned()),
                    ("heap_size".to_owned(), "normal".to_owned()),
                    ("release_type".to_owned(), "ga".to_owned()),
                    ("feature_version".to_owned(), "21".to_owned()),
                    ("release_name".to_owned(), release_name.to_owned()),
                    ("openjdk_version".to_owned(), "21.0.8+9-LTS".to_owned()),
                ]);
                for artifact in &artifacts {
                    metadata.insert(
                        format!("signature_link.{}", artifact.target),
                        format!("{}.sig", artifact.canonical_url),
                    );
                }
                LockedTool {
                    name: "java".to_owned(),
                    requested: version.to_owned(),
                    version: version.to_owned(),
                    provider: "adoptium-temurin".to_owned(),
                    released_at: None,
                    metadata,
                    options: Default::default(),
                    artifacts,
                }
            }
            "rust" => LockedTool {
                name: "rust".to_owned(),
                requested: "1.97.1".to_owned(),
                version: "1.97.1".to_owned(),
                provider: "rust-official".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    ("channel".to_owned(), "stable".to_owned()),
                    ("components".to_owned(), RUST_COMPONENTS.to_owned()),
                    ("manifest_date".to_owned(), "2026-07-16".to_owned()),
                    ("manifest_sha256".to_owned(), "ab".repeat(32)),
                    ("profile".to_owned(), RUST_PROFILE.to_owned()),
                ]),
                options: Default::default(),
                artifacts: RUST_TARGETS
                    .into_iter()
                    .map(|target| locked_rust_artifact("1.97.1", "2026-07-16", target))
                    .collect(),
            },
            "dotnet" => LockedTool {
                name: "dotnet".to_owned(),
                requested: "10.0.400".to_owned(),
                version: "10.0.400".to_owned(),
                provider: "microsoft-dotnet-sdk".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    ("channel".to_owned(), "10.0".to_owned()),
                    ("release_date".to_owned(), "2026-08-11".to_owned()),
                    ("release_type".to_owned(), "lts".to_owned()),
                    ("release_version".to_owned(), "10.0.14".to_owned()),
                    ("support_phase".to_owned(), "active".to_owned()),
                ]),
                options: Default::default(),
                artifacts: DOTNET_TARGETS
                    .into_iter()
                    .map(|target| locked_dotnet_artifact("10.0.400", target))
                    .collect(),
            },
            other => panic!("unsupported provider fixture {other}"),
        }
    }

    fn locked_go_artifact(version: &str, target: &str) -> LockedArtifact {
        let plan = plan_go_artifact(&SourceConfig::default(), version, target).expect("Go plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: match plan.format {
                GoArchiveFormat::Zip => LockedArtifactFormat::Zip,
                GoArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
            },
            archive_root: plan.archive_root,
            verification: "go-download-json-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_flutter_artifact(version: &str, target: &str) -> LockedArtifact {
        let plan =
            plan_flutter_artifact(&SourceConfig::default(), version, target).expect("Flutter plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: match plan.format {
                FlutterArchiveFormat::Zip => LockedArtifactFormat::Zip,
                FlutterArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
            },
            archive_root: plan.archive_root,
            verification: "flutter-release-json-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_python_artifact(distribution: &str, target: &str) -> LockedArtifact {
        let plan = plan_python_artifact(&SourceConfig::default(), distribution, target)
            .expect("Python plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: LockedArtifactFormat::TarGz,
            archive_root: plan.archive_root,
            verification: "python-build-standalone-versions-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_java_artifact(version: &str, release_name: &str, target: &str) -> LockedArtifact {
        locked_java_artifact_for(version, release_name, target, "jdk")
    }

    fn locked_java_artifact_for(
        version: &str,
        release_name: &str,
        target: &str,
        image_type: &str,
    ) -> LockedArtifact {
        let (os, arch, extension) = match target {
            "windows-x86_64" => ("windows", "x64", "zip"),
            "linux-x86_64" => ("linux", "x64", "tar.gz"),
            "linux-aarch64" => ("linux", "aarch64", "tar.gz"),
            "macos-x86_64" => ("mac", "x64", "tar.gz"),
            "macos-aarch64" => ("mac", "aarch64", "tar.gz"),
            _ => unreachable!("known Java target"),
        };
        let package = format!("OpenJDK21U-{image_type}_{arch}_{os}_hotspot_21.0.8_9.{extension}");
        let canonical_url = format!(
            "https://github.com/adoptium/temurin21-binaries/releases/download/{}/{}",
            release_name.replace('+', "%2B"),
            package,
        );
        let plan = crate::plan_java_artifact_with_package(
            version,
            release_name,
            target,
            image_type,
            &package,
            &canonical_url,
        )
        .expect("Java plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: match plan.format {
                JavaArchiveFormat::Zip => LockedArtifactFormat::Zip,
                JavaArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
            },
            archive_root: plan.archive_root,
            verification: "adoptium-api-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_rust_artifact(version: &str, date: &str, target: &str) -> LockedArtifact {
        let triple = crate::rust_target_triple(target).expect("Rust triple");
        let canonical_url =
            format!("https://static.rust-lang.org/dist/{date}/rust-{version}-{triple}.tar.xz");
        let plan = plan_rust_artifact(version, date, target, &canonical_url).expect("Rust plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: LockedArtifactFormat::TarXz,
            archive_root: plan.archive_root,
            verification: "rust-v2-manifest-sha256".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_rust_nightly_artifact(
        version: &str,
        date: &str,
        platform: &str,
        compilation_target: &str,
    ) -> LockedArtifact {
        let triple = crate::rust_target_triple(platform).expect("Rust target triple");
        let canonical_url =
            format!("https://static.rust-lang.org/dist/{date}/rust-nightly-{triple}.tar.xz");
        let plan = plan_rust_nightly_artifact(version, date, platform, &canonical_url)
            .expect("Rust nightly plan");
        let overlay_name = format!("rust-std-nightly-{compilation_target}.tar.xz");
        LockedArtifact {
            target: platform.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: "ab".repeat(32),
            integrity: None,
            format: LockedArtifactFormat::TarXz,
            archive_root: plan.archive_root,
            verification: "rust-v2-manifest-sha256".to_owned(),
            overlays: vec![LockedArtifactOverlay {
                canonical_url: format!("https://static.rust-lang.org/dist/{date}/{overlay_name}"),
                artifact_path: format!("dist/{date}/{overlay_name}"),
                integrity: format!("sha256:{}", "cd".repeat(32)),
                format: LockedArtifactFormat::TarXz,
                archive_root: format!("rust-std-nightly-{compilation_target}"),
                verification: "rust-v2-manifest-sha256".to_owned(),
            }],
        }
    }

    fn locked_dotnet_artifact(version: &str, target: &str) -> LockedArtifact {
        let (rid, extension) = match target {
            "windows-x86_64" => ("win-x64", "zip"),
            "linux-x86_64" => ("linux-x64", "tar.gz"),
            "linux-aarch64" => ("linux-arm64", "tar.gz"),
            "macos-x86_64" => ("osx-x64", "tar.gz"),
            "macos-aarch64" => ("osx-arm64", "tar.gz"),
            _ => unreachable!("known .NET SDK target"),
        };
        let canonical_url = format!(
            "https://builds.dotnet.microsoft.com/dotnet/Sdk/{version}/dotnet-sdk-{version}-{rid}.{extension}"
        );
        let plan = plan_dotnet_artifact(version, target, &canonical_url).expect(".NET SDK plan");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: plan.canonical_url,
            artifact_path: plan.artifact_path,
            sha256: String::new(),
            integrity: Some(format!("sha512:{}", "ab".repeat(64))),
            format: match plan.format {
                DotnetArchiveFormat::Zip => LockedArtifactFormat::Zip,
                DotnetArchiveFormat::TarGz => LockedArtifactFormat::TarGz,
            },
            archive_root: plan.archive_root,
            verification: "dotnet-release-metadata-sha512".to_owned(),
            overlays: Vec::new(),
        }
    }

    fn locked_pnpm_artifact(version: &str, with_overlay: bool) -> LockedArtifact {
        locked_npm_artifact("pnpm", version, "linux-x86_64", with_overlay)
    }

    fn locked_npm_tool(tool: &str, version: &str, include_linux_arm64: bool) -> LockedTool {
        LockedTool {
            name: tool.to_owned(),
            requested: version.to_owned(),
            version: version.to_owned(),
            provider: format!("{tool}-npm"),
            released_at: None,
            metadata: BTreeMap::new(),
            options: Default::default(),
            artifacts: npm_tool_targets(tool)
                .iter()
                .filter(|(target, _)| include_linux_arm64 || *target != "linux-aarch64")
                .map(|(target, _)| locked_npm_artifact(tool, version, target, tool == "pnpm"))
                .collect(),
        }
    }

    fn locked_npm_artifact(
        tool: &str,
        version: &str,
        target: &str,
        with_overlay: bool,
    ) -> LockedArtifact {
        let package = npm_tool_targets(tool)
            .iter()
            .find_map(|(candidate, package)| (*candidate == target).then_some(*package))
            .expect("known npm target");
        let package_base = package.rsplit('/').next().expect("npm package");
        let artifact_path = format!("{package}/-/{package_base}-{version}.tgz");
        LockedArtifact {
            target: target.to_owned(),
            canonical_url: format!("https://registry.npmjs.org/{artifact_path}"),
            artifact_path,
            sha256: String::new(),
            integrity: Some(format!("sha512:{}", "ab".repeat(64))),
            format: LockedArtifactFormat::TarGz,
            archive_root: "package".to_owned(),
            verification: "npm-registry-signature-sha512".to_owned(),
            overlays: (tool == "pnpm" && with_overlay)
                .then(|| {
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
