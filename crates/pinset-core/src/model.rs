use crate::{ArtifactIntegrity, Result, failure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const PROTOCOL: &str = "pinset/3";
pub const LOCKFILE_SCHEMA: u32 = 3;
pub const LOCKFILE_FILENAME: &str = "lock.toml";
pub const SUPPORTED_TARGETS: [&str; 5] = [
    "windows-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "linux-x86_64",
    "linux-aarch64",
];
pub const TOOLS: [&str; 8] = [
    "node", "pnpm", "bun", "go", "python", "java", "rust", "flutter",
];
pub const JAVA_COMMANDS: &[&str] = &[
    "java",
    "javaw",
    "javac",
    "jar",
    "jarsigner",
    "javadoc",
    "javap",
    "javah",
    "keytool",
    "policytool",
    "jabswitch",
    "jaccessinspector",
    "jaccesswalker",
    "jmod",
    "jdeps",
    "jdeprscan",
    "jimage",
    "jaotc",
    "jpackager",
    "jlink",
    "jpackage",
    "jdb",
    "jcmd",
    "jfr",
    "jconsole",
    "jshell",
    "jps",
    "jstack",
    "jstat",
    "jstatd",
    "jmap",
    "jinfo",
    "jhsdb",
    "jwebserver",
    "serialver",
    "rmiregistry",
    "rmid",
    "native2ascii",
    "pack200",
    "unpack200",
    "appletviewer",
    "extcheck",
    "idlj",
    "orbd",
    "servertool",
    "tnameserv",
    "schemagen",
    "wsgen",
    "wsimport",
    "xjc",
    "kinit",
    "klist",
    "ktab",
    "jrunscript",
];

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn validate_tool(name: &str) -> Result<()> {
    if TOOLS.contains(&name) {
        Ok(())
    } else {
        Err(failure(
            "PINSET_TOOL_UNKNOWN",
            format!("unknown tool {name}"),
        ))
    }
}
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(failure(
            "PINSET_ID_INVALID",
            "identifier must use ASCII letters, digits, '-' or '_'",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub protocol: String,
    pub schema: u32,
    pub project_id: String,
    pub generated_by: String,
    #[serde(rename = "tool", default)]
    pub tools: Vec<LockedTool>,
}
impl Lockfile {
    pub fn empty(project_id: String) -> Self {
        Self {
            protocol: PROTOCOL.into(),
            schema: 3,
            project_id,
            generated_by: crate::pinset_version().into(),
            tools: vec![],
        }
    }
    pub fn tool(&self, name: &str) -> Option<&LockedTool> {
        self.tools.iter().find(|t| t.name == name)
    }
    pub fn upsert_tool(&mut self, tool: LockedTool) -> Result<()> {
        validate_tool(&tool.name)?;
        self.remove_tool(&tool.name);
        self.tools.push(tool);
        self.tools.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }
    pub fn remove_tool(&mut self, name: &str) {
        self.tools.retain(|t| t.name != name);
    }
    pub fn validate(&self) -> Result<()> {
        if self.protocol != PROTOCOL || self.schema != 3 {
            return Err(failure(
                "PINSET_PROTOCOL_UNSUPPORTED",
                "only pinset/3 locks are accepted",
            ));
        }
        validate_id(&self.project_id)?;
        let mut names = std::collections::BTreeSet::new();
        for tool in &self.tools {
            validate_tool(&tool.name)?;
            if !names.insert(&tool.name) || tool.version.is_empty() || tool.artifacts.is_empty() {
                return Err(failure(
                    "PINSET_LOCK_INVALID",
                    "duplicate or incomplete tool record",
                ));
            }
            if tool.version.contains(['/', '\\']) || tool.version == ".." {
                return Err(failure("PINSET_LOCK_INVALID", "unsafe version identity"));
            }
            let mut targets = std::collections::BTreeSet::new();
            for a in &tool.artifacts {
                if !targets.insert(&a.target) || a.target.contains(['/', '\\']) {
                    return Err(failure("PINSET_LOCK_INVALID", "invalid platform"));
                }
                validate_official_url(&tool.name, &a.canonical_url)?;
                a.artifact_integrity()?;
                for overlay in &a.overlays {
                    validate_official_url(&tool.name, &overlay.canonical_url)?;
                    overlay.artifact_integrity()?;
                }
            }
        }
        Ok(())
    }
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
        rename = "released-at",
        skip_serializing_if = "Option::is_none"
    )]
    pub released_at: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(rename = "artifact")]
    pub artifacts: Vec<LockedArtifact>,
}
impl LockedTool {
    pub fn artifact(&self, target: &str) -> Option<&LockedArtifact> {
        self.artifacts.iter().find(|a| a.target == target)
    }
    pub fn installation_version(&self, target: &str) -> String {
        let bytes = serde_json::to_vec(&(
            &self.name,
            &self.version,
            &self.provider,
            &self.metadata,
            &self.options,
            target,
            self.artifact(target),
        ))
        .expect("serializable installation identity");
        format!("{}--{}", self.version, &digest(&bytes)[..24])
    }
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
    #[serde(default, rename = "overlay", skip_serializing_if = "Vec::is_empty")]
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
impl LockedArtifact {
    pub fn artifact_integrity(&self) -> Result<ArtifactIntegrity> {
        ArtifactIntegrity::parse(self.integrity.as_deref().unwrap_or(&self.sha256))
    }
}
impl LockedArtifactOverlay {
    pub fn artifact_integrity(&self) -> Result<ArtifactIntegrity> {
        ArtifactIntegrity::parse(&self.integrity)
    }
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
            Self::TarGz => "tar.gz",
        }
    }
}

pub fn validate_official_url(tool: &str, value: &str) -> Result<()> {
    let url = url::Url::parse(value)
        .map_err(|_| failure("PINSET_SOURCE_INVALID", "invalid official URL"))?;
    let allowed = match tool {
        "node" => url.host_str() == Some("nodejs.org") && url.path().starts_with("/dist/"),
        "go" => matches!(url.host_str(), Some("go.dev" | "dl.google.com")),
        "python" => {
            (url.host_str() == Some("www.python.org") && url.path().starts_with("/ftp/python/"))
                || (url.host_str() == Some("github.com")
                    && url
                        .path()
                        .starts_with("/astral-sh/python-build-standalone/releases/download/"))
        }
        "java" => {
            url.host_str() == Some("github.com")
                && url.path().starts_with("/adoptium/temurin")
                && url.path().contains("-binaries/releases/download/")
                && url.path().contains("-jdk_")
        }
        "rust" => {
            url.host_str() == Some("static.rust-lang.org") && url.path().starts_with("/dist/")
        }
        "flutter" => {
            url.host_str() == Some("storage.googleapis.com")
                && url.path().starts_with("/flutter_infra_release/")
        }
        "pnpm" | "bun" => {
            url.host_str() == Some("registry.npmjs.org")
                || (url.host_str() == Some("github.com")
                    && url.path().starts_with("/oven-sh/bun/releases/"))
        }
        _ => false,
    };
    if !allowed
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(failure(
            "PINSET_SOURCE_INVALID",
            format!("{tool} artifact is outside its official source"),
        ));
    }
    Ok(())
}
pub fn load_lockfile(path: &Path) -> Result<Lockfile> {
    let lock: Lockfile = toml::from_str(&fs::read_to_string(path)?)?;
    lock.validate()?;
    Ok(lock)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallReceipt {
    pub schema: u32,
    pub complete: bool,
    pub tool: String,
    pub version: String,
    pub install_identity: String,
    pub target: String,
    pub canonical_url: String,
    pub selected_source: String,
    pub selected_source_kind: String,
    pub selected_url: String,
    pub artifact_integrity: String,
    pub artifact_format: String,
    #[serde(default)]
    pub base_artifact_integrities: Vec<String>,
    pub bytes_downloaded: u64,
    pub install_root: String,
    pub file_count: u64,
    pub total_size: u64,
    pub payload_digest: String,
    pub pinset_version: String,
    pub critical_entries: Vec<String>,
    #[serde(default)]
    pub commands: BTreeMap<String, String>,
}
pub fn install_directory(home: &Path, tool: &LockedTool, target: &str) -> PathBuf {
    home.join("installs")
        .join(&tool.name)
        .join(tool.installation_version(target))
        .join(target)
}
pub fn read_receipt(home: &Path, tool: &LockedTool, target: &str) -> Result<InstallReceipt> {
    let root = install_directory(home, tool, target);
    let r: InstallReceipt = toml::from_str(
        &fs::read_to_string(root.join(".pinset-install.toml")).map_err(|_| {
            failure(
                "PINSET_INSTALL_MISSING",
                format!(
                    "{} {} is not installed; run pinset install",
                    tool.name, tool.version
                ),
            )
        })?,
    )?;
    let a = tool.artifact(target).ok_or_else(|| {
        failure(
            "PINSET_PLATFORM_UNAVAILABLE",
            format!("{} has no locked artifact for {target}", tool.name),
        )
    })?;
    if r.schema != 3
        || !r.complete
        || r.tool != tool.name
        || r.version != tool.version
        || r.install_identity != tool.installation_version(target)
        || r.target != target
        || ArtifactIntegrity::parse(&r.artifact_integrity)? != a.artifact_integrity()?
        || r.base_artifact_integrities
            != a.overlays
                .iter()
                .map(|o| o.artifact_integrity().map(|i| i.canonical()))
                .collect::<Result<Vec<_>>>()?
    {
        return Err(failure(
            "PINSET_INSTALL_IDENTITY",
            "installation receipt does not match the exact lock",
        ));
    }
    for relative in r.critical_entries.iter().chain(r.commands.values()) {
        let p = Path::new(relative);
        if p.is_absolute()
            || p.components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || !root.join(p).exists()
        {
            return Err(failure(
                "PINSET_INSTALL_DAMAGED",
                format!("missing or unsafe installation entry {relative}"),
            ));
        }
        if !root
            .join(p)
            .canonicalize()?
            .starts_with(root.canonicalize()?)
        {
            return Err(failure(
                "PINSET_INSTALL_DAMAGED",
                "installation entry escapes its root",
            ));
        }
    }
    Ok(r)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidence {
    pub host: String,
    pub command: String,
    pub executable: PathBuf,
    pub version: String,
    pub observed: String,
    pub timestamp: u64,
    pub success: bool,
    pub scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentReport {
    pub protocol: String,
    pub project: Option<PathBuf>,
    pub checks: Vec<EnvironmentCheck>,
    pub evidence: Vec<ExecutionEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentCheck {
    pub tool: String,
    pub configured: bool,
    pub installed: bool,
    pub bound: bool,
    pub actually_verified: bool,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionJournal {
    pub protocol: String,
    pub id: String,
    pub project_id: String,
    pub root: PathBuf,
    pub phase: String,
    pub old_config: String,
    pub old_lock: Option<String>,
    pub new_config: String,
    pub new_lock: String,
    pub profile_before: BTreeMap<String, Option<String>>,
}
