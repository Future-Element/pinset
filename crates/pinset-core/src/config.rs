use crate::{
    Lockfile, PROTOCOL, Result, WorkDirectoryIdentity, failure, validate_id, validate_tool,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
pub const PROJECT_CONFIG_FILENAME: &str = ".pinset/config.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub protocol: String,
    pub schema: u32,
    pub project_id: String,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default, rename = "rust")]
    pub rust_options: ToolOptions,
    #[serde(default)]
    pub policy: ProjectPolicy,
    #[serde(default)]
    pub environment: ProjectEnvironment,
    #[serde(default)]
    pub verification: ProjectVerification,
}
impl ProjectConfig {
    pub fn new(id: String) -> Self {
        Self {
            protocol: PROTOCOL.into(),
            schema: 3,
            project_id: id,
            tools: BTreeMap::new(),
            platforms: vec![],
            rust_options: ToolOptions::default(),
            policy: ProjectPolicy::default(),
            environment: ProjectEnvironment::default(),
            verification: ProjectVerification::default(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.protocol != PROTOCOL || self.schema != 3 {
            return Err(failure(
                "PINSET_PROTOCOL_UNSUPPORTED",
                "only pinset/3 configuration is accepted",
            ));
        }
        validate_id(&self.project_id)?;
        for (name, selector) in &self.tools {
            validate_tool(name)?;
            if selector.is_empty() {
                return Err(failure(
                    "PINSET_SELECTOR_REQUIRED",
                    "tool selector cannot be empty",
                ));
            }
        }
        if self.tools.contains_key("pnpm") && !self.tools.contains_key("node") {
            return Err(failure(
                "PINSET_DEPENDENCY_REQUIRED",
                "pnpm requires an explicit Node selection",
            ));
        }
        if self
            .platforms
            .iter()
            .any(|p| !crate::SUPPORTED_TARGETS.contains(&p.as_str()))
        {
            return Err(failure(
                "PINSET_PLATFORM_INVALID",
                "unsupported project platform",
            ));
        }
        if self.verification.timeout == 0 || self.verification.inputs.len() > 64 {
            return Err(failure(
                "PINSET_VERIFICATION_INVALID",
                "invalid timeout or too many verification inputs",
            ));
        }
        for input in &self.verification.inputs {
            let p = Path::new(input);
            if p.as_os_str().is_empty()
                || p.is_absolute()
                || p.components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(failure(
                    "PINSET_VERIFICATION_INVALID",
                    "verification inputs must be paths inside this project",
                ));
            }
        }
        for name in self.environment.profiles.keys() {
            validate_id(name)?;
        }
        if let Some(name) = &self.environment.default
            && !self.environment.profiles.contains_key(name)
        {
            return Err(failure(
                "PINSET_PROFILE_MISSING",
                "shared default profile does not exist",
            ));
        }
        Ok(())
    }
    pub fn validate_lock(&self, lock: &Lockfile) -> Result<()> {
        self.validate()?;
        lock.validate()?;
        if lock.project_id != self.project_id || lock.tools.len() != self.tools.len() {
            return Err(failure(
                "PINSET_LOCK_MISMATCH",
                "configuration and lock have different project identities or selections",
            ));
        }
        for (name, selector) in &self.tools {
            let tool = lock
                .tool(name)
                .ok_or_else(|| failure("PINSET_LOCK_MISMATCH", format!("{name} is not locked")))?;
            if &tool.requested != selector
                || (name == "rust" && tool.options != self.rust_options.lock_options())
            {
                return Err(failure(
                    "PINSET_LOCK_MISMATCH",
                    format!("{name} selection changed; run pinset use"),
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolOptions {
    pub profile: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    pub date: Option<String>,
}
impl ToolOptions {
    pub fn lock_options(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        for (k, v) in [
            ("profile", self.profile.as_ref()),
            ("date", self.date.as_ref()),
        ] {
            if let Some(v) = v {
                m.insert(k.into(), v.clone());
            }
        }
        for (k, v) in [("components", &self.components), ("targets", &self.targets)] {
            if !v.is_empty() {
                let mut v = v.clone();
                v.sort();
                m.insert(k.into(), v.join(","));
            }
        }
        m
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicy {
    pub minimum_release_age: Option<String>,
    pub minimum_verification: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectEnvironment {
    pub default: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfig {
    pub recipients: Vec<String>,
    #[serde(default)]
    pub grants: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectVerification {
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub external_state: Vec<String>,
}
fn default_timeout() -> u64 {
    300
}
impl Default for ProjectVerification {
    fn default() -> Self {
        Self {
            timeout: 300,
            inputs: vec![],
            external_state: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub lock_path: PathBuf,
    pub local: PathBuf,
    pub directory: WorkDirectoryIdentity,
    pub global: bool,
}
impl ProjectContext {
    pub fn at(root: &Path, global: bool) -> Result<Self> {
        let root = root.canonicalize()?;
        let state = if global {
            root.clone()
        } else {
            root.join(".pinset")
        };
        for path in [
            &state,
            &state.join("config.toml"),
            &state.join("lock.toml"),
            &state.join("local"),
            &state.join("env"),
        ] {
            if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(failure(
                    "PINSET_PROJECT_UNSAFE",
                    "project state cannot be a symbolic link",
                ));
            }
        }
        Ok(Self {
            directory: crate::work_directory_identity(&root)?,
            root,
            config_path: state.join("config.toml"),
            lock_path: state.join("lock.toml"),
            local: state.join("local"),
            global,
        })
    }
    pub fn load(&self) -> Result<ProjectConfig> {
        if self.local.join("transaction.json").exists() {
            return Err(failure(
                "PINSET_TRANSACTION_PENDING",
                "an interrupted transaction requires pinset upgrade recover",
            ));
        }
        let c: ProjectConfig = toml::from_str(&fs::read_to_string(&self.config_path)?)?;
        c.validate()?;
        Ok(c)
    }
    pub fn load_locked(&self) -> Result<(ProjectConfig, Lockfile)> {
        let c = self.load()?;
        let l = if !self.lock_path.exists() && c.tools.is_empty() {
            Lockfile::empty(c.project_id.clone())
        } else {
            crate::load_lockfile(&self.lock_path)?
        };
        c.validate_lock(&l)?;
        Ok((c, l))
    }
}
pub fn user_home() -> Result<PathBuf> {
    env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| failure("PINSET_HOME_MISSING", "user home is unavailable"))
}
pub fn pinset_home() -> Result<PathBuf> {
    let base = env::var_os("PINSET_HOME")
        .map(PathBuf::from)
        .unwrap_or(user_home()?.join(".pinset"));
    if !base.is_absolute() {
        return Err(failure(
            "PINSET_HOME_INVALID",
            "PINSET_HOME must be absolute",
        ));
    }
    Ok(base.join("v3"))
}
pub fn find_project_context(start: &Path) -> Result<Option<ProjectContext>> {
    let start = start.canonicalize()?;
    if !start.is_dir() {
        return Err(failure("PINSET_CWD_INVALID", "cwd must be a directory"));
    }
    let git_boundary = start
        .ancestors()
        .find(|p| p.join(".git").is_dir() || p.join(".git").is_file());
    let home = user_home()?.canonicalize().ok();
    let boundary = git_boundary
        .map(Path::to_path_buf)
        .or_else(|| home.filter(|h| start.starts_with(h)))
        .unwrap_or(start.clone());
    for dir in start.ancestors() {
        if dir.join(PROJECT_CONFIG_FILENAME).exists() {
            return Ok(Some(ProjectContext::at(dir, false)?));
        }
        if dir == boundary {
            break;
        }
    }
    Ok(None)
}
pub fn selected_context(cwd: &Path, global: bool) -> Result<ProjectContext> {
    if !global && let Some(p) = find_project_context(cwd)? {
        return Ok(p);
    }
    let home = pinset_home()?.join("global");
    if !home.join("config.toml").exists() {
        return Err(failure(
            "PINSET_SELECTION_MISSING",
            "no project or explicit global selection",
        ));
    }
    ProjectContext::at(&home, true)
}
