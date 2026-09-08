//! Runtime selection and executable resolution.
//!
//! INVARIANT: the nearest project selection wins, then the global selection, and the system PATH
//! is considered only when neither Pinset scope selects the tool. Pinset shim locations and the
//! currently executing shim are excluded from system fallback to prevent recursive routing.

use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
use crate::current_target;
use crate::{
    Error, Result, RuntimeCommandLayout, RuntimeEnvironmentKind, current_target_for_tool,
    find_project_context, global_config_path, is_managed_command_shim,
    load_effective_project_config, load_optional_global_config, load_project_python_environment,
    load_project_python_environment_for, project_python_command_candidates,
    provider_dependency_order, runtime_provider, runtime_provider_for_command, runtime_providers,
};
#[cfg(feature = "lockfile")]
use crate::{
    global_lockfile_path, load_optional_lockfile, lockfile_path, validate_lock_matches_tool,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSource {
    Project,
    Global,
    System,
}

impl SelectionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSelection {
    pub tool: String,
    pub requested: String,
    pub version: String,
    pub installation_version: String,
    pub source: SelectionSource,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResolution {
    pub command: String,
    pub tool: String,
    pub requested: Option<String>,
    pub version: String,
    pub source: SelectionSource,
    pub selection_path: Option<PathBuf>,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEnvironmentVariable {
    pub name: &'static str,
    pub value: OsString,
}

/// The same runtime PATH and variables are used by explicit execution and direct shims.
#[derive(Debug)]
pub struct ExecutionContext {
    pub path: OsString,
    pub environment: Vec<RuntimeEnvironmentVariable>,
    pub remove_environment: Vec<&'static str>,
}

pub fn execution_context(
    tool: &str,
    executable: &Path,
    cwd: &Path,
    home: &Path,
) -> Result<ExecutionContext> {
    let environment = selected_runtime_environment(tool, cwd, home);
    let remove_environment = if environment
        .iter()
        .any(|variable| variable.name == "VIRTUAL_ENV")
    {
        vec!["PYTHONHOME"]
    } else {
        Vec::new()
    };
    Ok(ExecutionContext {
        path: path_with_selected_tools(tool, executable, cwd, home)?,
        environment,
        remove_environment,
    })
}

/// Explicit execution may use arbitrary commands, while declared runtime failures stay closed.
pub fn resolve_execution_command(
    command: &str,
    cwd: &Path,
    home: &Path,
) -> Result<CommandResolution> {
    if command_tool(command).is_some() {
        return resolve_command(command, cwd, home);
    }
    let configured = effective_configured_tools(cwd, home)?;
    for provider in runtime_providers()
        .iter()
        .filter(|provider| configured.contains_key(provider.tool))
    {
        resolve_command(provider.commands[0], cwd, home)?;
    }
    let command_path = Path::new(command);
    let explicit_path = command_path.is_absolute() || command_path.components().count() > 1;
    if !explicit_path && configured.contains_key("python") {
        match resolve_project_python_command(command, cwd, home) {
            Ok(resolution) => return Ok(resolution),
            Err(
                Error::RuntimeCommandNotFound { .. }
                | Error::PythonEnvironmentSelectionMissing { .. },
            ) => {}
            Err(error) => return Err(error),
        }
    }
    resolve_system_execution_command(command, cwd, home)
}

pub fn resolve_execution_command_for_python_environment(
    command: &str,
    cwd: &Path,
    home: &Path,
    environment_name: &str,
    relative_path: &str,
) -> Result<CommandResolution> {
    if let Some(tool) = command_tool(command) {
        return if tool == "python" {
            resolve_project_python_command_for(command, cwd, home, environment_name, relative_path)
        } else {
            resolve_command(command, cwd, home)
        };
    }
    let configured = effective_configured_tools(cwd, home)?;
    for provider in runtime_providers()
        .iter()
        .filter(|provider| configured.contains_key(provider.tool))
    {
        if provider.tool == "python" {
            resolve_project_python_command_for(
                provider.commands[0],
                cwd,
                home,
                environment_name,
                relative_path,
            )?;
        } else {
            resolve_command(provider.commands[0], cwd, home)?;
        }
    }
    if configured.contains_key("python") {
        match resolve_project_python_command_for(
            command,
            cwd,
            home,
            environment_name,
            relative_path,
        ) {
            Ok(resolution) => return Ok(resolution),
            Err(Error::RuntimeCommandNotFound { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    resolve_system_execution_command(command, cwd, home)
}

fn resolve_system_execution_command(
    command: &str,
    cwd: &Path,
    home: &Path,
) -> Result<CommandResolution> {
    let command_path = Path::new(command);
    let explicit_path = command_path.is_absolute() || command_path.components().count() > 1;
    let excluded = sibling_shim_executable().into_iter().collect::<Vec<_>>();
    let path = env::var_os("PATH");
    let executable = if explicit_path {
        let candidate = if command_path.is_absolute() {
            command_path.to_path_buf()
        } else {
            cwd.join(command_path)
        };
        let candidate = fs::canonicalize(&candidate).unwrap_or(candidate);
        (is_executable_file(&candidate)
            && !excluded
                .iter()
                .any(|shim| same_executable(&candidate, shim, command)))
        .then_some(candidate)
    } else {
        find_system_commands(command, cwd, home, path.as_deref(), &excluded)
            .into_iter()
            .next()
    }
    .ok_or_else(|| Error::CommandSelectionNotFound {
        command: command.to_owned(),
        searched: path
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })?;
    Ok(CommandResolution {
        command: command.to_owned(),
        tool: String::new(),
        requested: None,
        version: "unknown".into(),
        source: SelectionSource::System,
        selection_path: find_project_context(cwd)?.config_path,
        executable,
    })
}

pub fn command_tool(command: &str) -> Option<&'static str> {
    runtime_provider_for_command(command).map(|provider| provider.tool)
}

pub fn pinset_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("PINSET_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }

    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("Pinset"))
            .ok_or(Error::PinsetHomeUnavailable)
    }

    #[cfg(not(windows))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        {
            return Ok(path.join("pinset"));
        }
        env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join(".local").join("share").join("pinset"))
            .ok_or(Error::PinsetHomeUnavailable)
    }
}

pub fn pinset_home_from_env() -> Result<PathBuf> {
    pinset_home()
}

pub fn resolve_from_env(command: &str, cwd: &Path) -> Result<CommandResolution> {
    resolve_command(command, cwd, &pinset_home()?)
}

pub fn resolve_command(command: &str, cwd: &Path, pinset_home: &Path) -> Result<CommandResolution> {
    let path = env::var_os("PATH");
    let excluded = sibling_shim_executable().into_iter().collect::<Vec<_>>();
    resolve_command_with_path(command, cwd, pinset_home, path.as_deref(), &excluded)
}

pub fn resolve_command_with_path(
    command: &str,
    cwd: &Path,
    pinset_home: &Path,
    system_path: Option<&OsStr>,
    excluded_executables: &[PathBuf],
) -> Result<CommandResolution> {
    let tool = command_tool(command).ok_or_else(|| Error::UnsupportedCommand {
        command: command.to_owned(),
    })?;
    // A configured-but-broken project/global runtime is an error, not permission to bypass the
    // lock through PATH. System fallback is reached only for ToolSelectionNotFound.
    let selection = match resolve_tool_selection(tool, cwd, pinset_home) {
        Ok(selection) => selection,
        Err(Error::ToolSelectionNotFound { .. }) => {
            let candidates =
                find_system_commands(command, cwd, pinset_home, system_path, excluded_executables);
            let Some(executable) = candidates.first() else {
                return Err(Error::CommandSelectionNotFound {
                    command: command.to_owned(),
                    searched: system_path
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "<empty>".to_owned()),
                });
            };
            return Ok(CommandResolution {
                command: command.to_owned(),
                tool: tool.to_owned(),
                requested: None,
                version: "unknown".to_owned(),
                source: SelectionSource::System,
                selection_path: None,
                executable: executable.clone(),
            });
        }
        Err(error) => return Err(error),
    };
    let version = selection.version.clone();
    let installation_version = selection.installation_version.clone();

    if tool == "python" && selection.source == SelectionSource::Project {
        let target = current_target_for_tool(tool);
        let environment =
            load_project_python_environment(&selection.config_path, &version, &target)?;
        let candidates = project_python_command_candidates(&environment, command);
        let executable = candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .cloned()
            .ok_or_else(|| Error::RuntimeCommandNotFound {
                tool: tool.to_owned(),
                version: version.clone(),
                command: command.to_owned(),
                searched: candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            })?;
        return Ok(CommandResolution {
            command: command.to_owned(),
            tool: tool.to_owned(),
            requested: Some(selection.requested.clone()),
            version,
            source: selection.source,
            selection_path: Some(selection.config_path),
            executable,
        });
    }

    let install_dir = pinset_home
        .join("installs")
        .join(tool)
        .join(&installation_version)
        .join(current_target_for_tool(tool));
    let candidates = runtime_command_candidates(tool, command, &install_dir);
    let executable = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .ok_or_else(|| Error::RuntimeCommandNotFound {
            tool: tool.to_owned(),
            version: version.clone(),
            command: command.to_owned(),
            searched: candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        })?;

    Ok(CommandResolution {
        command: command.to_owned(),
        tool: tool.to_owned(),
        requested: Some(selection.requested.clone()),
        version,
        source: selection.source,
        selection_path: Some(selection.config_path),
        executable,
    })
}

pub fn resolve_project_python_command(
    command: &str,
    cwd: &Path,
    pinset_home: &Path,
) -> Result<CommandResolution> {
    resolve_project_python_command_for(command, cwd, pinset_home, "default", ".venv")
}

pub fn resolve_project_python_command_for(
    command: &str,
    cwd: &Path,
    pinset_home: &Path,
    environment_name: &str,
    relative_path: &str,
) -> Result<CommandResolution> {
    let selection = resolve_tool_selection("python", cwd, pinset_home)?;
    if selection.source != SelectionSource::Project {
        return Err(Error::PythonEnvironmentSelectionMissing {
            path: cwd.to_path_buf(),
        });
    }
    let target = current_target_for_tool("python");
    let environment = load_project_python_environment_for(
        &selection.config_path,
        environment_name,
        relative_path,
        &selection.version,
        &target,
    )?;
    let candidates = project_python_command_candidates(&environment, command);
    let executable = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .ok_or_else(|| Error::RuntimeCommandNotFound {
            tool: "python".to_owned(),
            version: selection.version.clone(),
            command: command.to_owned(),
            searched: candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        })?;
    Ok(CommandResolution {
        command: command.to_owned(),
        tool: "python".to_owned(),
        requested: Some(selection.requested),
        version: selection.version,
        source: SelectionSource::Project,
        selection_path: Some(selection.config_path),
        executable,
    })
}

pub fn resolve_tool_selection(tool: &str, cwd: &Path, pinset_home: &Path) -> Result<ToolSelection> {
    let context = find_project_context(cwd)?;
    if let Some(config_path) = context.config_path.as_ref() {
        let config = load_effective_project_config(config_path)?;
        if let Some(requested) = config.tools.get(tool) {
            return selection_from_config(
                tool,
                requested,
                config.schema,
                config.tool_options.get(tool),
                SelectionSource::Project,
                config_path,
                (pinset_home, lockfile_for_project(config_path)),
            );
        }
        if !config.policy.inherit_global {
            if config.policy.system_fallback {
                return Err(selection_not_found(tool, cwd, pinset_home));
            }
            return Err(Error::ProjectToolSelectionRequired {
                tool: tool.to_owned(),
                config_path: config_path.clone(),
            });
        }

        let global_path = global_config_path(pinset_home);
        if let Some(global) = load_optional_global_config(&global_path)?
            && let Some(requested) = global.tools.get(tool)
        {
            return selection_from_config(
                tool,
                requested,
                global.schema,
                None,
                SelectionSource::Global,
                &global_path,
                (pinset_home, lockfile_for_global(pinset_home)),
            );
        }
        if config.policy.system_fallback {
            return Err(selection_not_found(tool, cwd, pinset_home));
        }
        return Err(Error::ProjectToolSelectionRequired {
            tool: tool.to_owned(),
            config_path: config_path.clone(),
        });
    }

    let global_path = global_config_path(pinset_home);
    if let Some(config) = load_optional_global_config(&global_path)?
        && let Some(requested) = config.tools.get(tool)
    {
        return selection_from_config(
            tool,
            requested,
            config.schema,
            None,
            SelectionSource::Global,
            &global_path,
            (pinset_home, lockfile_for_global(pinset_home)),
        );
    }

    Err(selection_not_found(tool, cwd, pinset_home))
}

fn selection_not_found(tool: &str, cwd: &Path, pinset_home: &Path) -> Error {
    Error::ToolSelectionNotFound {
        tool: tool.to_owned(),
        start: cwd.to_path_buf(),
        global_config_path: global_config_path(pinset_home),
    }
}

#[cfg(feature = "lockfile")]
fn lockfile_for_project(config_path: &Path) -> Result<Option<crate::Lockfile>> {
    load_optional_lockfile(&lockfile_path(config_path))
}

#[cfg(not(feature = "lockfile"))]
fn lockfile_for_project(_config_path: &Path) {}

#[cfg(feature = "lockfile")]
fn lockfile_for_global(pinset_home: &Path) -> Result<Option<crate::Lockfile>> {
    load_optional_lockfile(&global_lockfile_path(pinset_home))
}

#[cfg(not(feature = "lockfile"))]
fn lockfile_for_global(_pinset_home: &Path) {}

#[cfg(feature = "lockfile")]
fn selection_from_config(
    tool: &str,
    requested: &str,
    config_schema: u32,
    configured_options: Option<&crate::ToolOptions>,
    source: SelectionSource,
    config_path: &Path,
    state: (&Path, Result<Option<crate::Lockfile>>),
) -> Result<ToolSelection> {
    let (pinset_home, lockfile) = state;
    let lockfile = lockfile?;
    let (version, installation_version) = if let Some(lockfile) = lockfile {
        let locked = validate_lock_matches_tool(&lockfile, tool, requested, config_path)?;
        let expected_options = configured_options
            .map(crate::ToolOptions::lock_options)
            .unwrap_or_default();
        if locked.options != expected_options {
            return Err(Error::LockfileMismatch {
                selection_path: config_path.to_path_buf(),
                tool: tool.to_owned(),
                configured: format!("{requested} with structured options"),
                locked: format!("{} with different structured options", locked.version),
            });
        }
        #[cfg(feature = "provider-registry")]
        if locked.provider == "declarative-github-release" {
            crate::validate_locked_declarative_provider(pinset_home, locked)?;
        }
        (locked.version.clone(), locked.installation_version())
    } else if config_schema < crate::PROJECT_CONFIG_SCHEMA {
        (requested.to_owned(), requested.to_owned())
    } else {
        return Err(Error::ReadLockfile {
            path: if source == SelectionSource::Project {
                lockfile_path(config_path)
            } else {
                config_path
                    .parent()
                    .unwrap_or(config_path)
                    .join(crate::GLOBAL_LOCKFILE_FILENAME)
            },
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("schema {config_schema} selections require a lockfile"),
            ),
        });
    };
    Ok(ToolSelection {
        tool: tool.to_owned(),
        requested: requested.to_owned(),
        version,
        installation_version,
        source,
        config_path: config_path.to_path_buf(),
    })
}

#[cfg(not(feature = "lockfile"))]
fn selection_from_config(
    tool: &str,
    requested: &str,
    _config_schema: u32,
    _configured_options: Option<&crate::ToolOptions>,
    source: SelectionSource,
    config_path: &Path,
    _state: (&Path, ()),
) -> Result<ToolSelection> {
    Ok(ToolSelection {
        tool: tool.to_owned(),
        requested: requested.to_owned(),
        version: requested.to_owned(),
        installation_version: requested.to_owned(),
        source,
        config_path: config_path.to_path_buf(),
    })
}

pub fn find_system_commands(
    command: &str,
    cwd: &Path,
    pinset_home: &Path,
    system_path: Option<&OsStr>,
    excluded_executables: &[PathBuf],
) -> Vec<PathBuf> {
    let Some(system_path) = system_path else {
        return Vec::new();
    };
    let shim_directory = pinset_home.join("shims");
    let mut commands: Vec<PathBuf> = Vec::new();
    for directory in env::split_paths(system_path) {
        let directory = if directory.is_absolute() {
            directory
        } else {
            cwd.join(directory)
        };
        if paths_equal(&directory, &shim_directory) {
            continue;
        }
        for candidate in executable_candidates(&directory, command) {
            if !is_executable_file(&candidate)
                || excluded_executables
                    .iter()
                    .any(|excluded| same_executable(&candidate, excluded, command))
            {
                continue;
            }
            if !commands
                .iter()
                .any(|existing| paths_equal(existing, &candidate))
            {
                commands.push(candidate);
            }
        }
    }
    commands
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn same_executable(left: &Path, right: &Path, command: &str) -> bool {
    paths_equal(left, right)
        || same_file::is_same_file(left, right).unwrap_or(false)
        || is_managed_command_shim(right, left, command).unwrap_or(false)
}

fn sibling_shim_executable() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let directory = executable.parent()?;
    let shim = directory.join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    shim.is_file().then_some(shim)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub fn path_with_selected_runtime(executable: &Path) -> Result<OsString> {
    let command_dir = executable
        .parent()
        .ok_or_else(|| Error::RuntimeCommandDirectoryMissing {
            path: executable.to_path_buf(),
        })?;
    let inherited = env::var_os("PATH");
    let entries = std::iter::once(command_dir.to_path_buf()).chain(
        inherited
            .as_ref()
            .into_iter()
            .flat_map(|value| env::split_paths(value)),
    );
    env::join_paths(entries).map_err(|source| Error::RuntimePathJoin { source })
}

pub fn path_with_selected_tools(
    tool: &str,
    executable: &Path,
    cwd: &Path,
    pinset_home: &Path,
) -> Result<OsString> {
    let selected_dir = executable
        .parent()
        .ok_or_else(|| Error::RuntimeCommandDirectoryMissing {
            path: executable.to_path_buf(),
        })?
        .to_path_buf();
    let shim_dir = pinset_home.join("shims");
    let mut entries = if tool.is_empty() {
        Vec::new()
    } else {
        vec![selected_dir.clone()]
    };
    // Provider dependencies are a managed-selection contract. A command resolved from the
    // system PATH must retain ordinary system toolchain behavior instead of requiring Pinset
    // selections for that Provider's declared dependencies.
    if resolve_tool_selection(tool, cwd, pinset_home).is_ok() {
        for provider in provider_dependency_order(tool)? {
            if provider.tool == tool {
                continue;
            }
            let selection =
                resolve_tool_selection(provider.tool, cwd, pinset_home).map_err(|_| {
                    Error::ProviderDependencyMissing {
                        tool: tool.to_owned(),
                        dependency: provider.tool.to_owned(),
                    }
                })?;
            let install_dir = pinset_home
                .join("installs")
                .join(provider.tool)
                .join(&selection.installation_version)
                .join(current_target_for_tool(provider.tool));
            let command_dir =
                if provider.tool == "python" && selection.source == SelectionSource::Project {
                    load_project_python_environment(
                        &selection.config_path,
                        &selection.version,
                        &current_target_for_tool("python"),
                    )?
                    .command_directory
                } else {
                    runtime_command_directory(provider.tool, &install_dir)
                };
            if command_dir.is_dir()
                && !paths_equal(&command_dir, &selected_dir)
                && !entries.iter().any(|entry| paths_equal(entry, &command_dir))
            {
                entries.push(command_dir);
            }
        }
    }
    let configured_tools = effective_configured_tools(cwd, pinset_home)?;
    for provider in runtime_providers()
        .iter()
        .filter(|provider| configured_tools.contains_key(provider.tool))
    {
        if provider.tool == tool {
            continue;
        }
        let Ok(selection) = resolve_tool_selection(provider.tool, cwd, pinset_home) else {
            continue;
        };
        let install_dir = pinset_home
            .join("installs")
            .join(provider.tool)
            .join(&selection.installation_version)
            .join(current_target_for_tool(provider.tool));
        let command_dir =
            if provider.tool == "python" && selection.source == SelectionSource::Project {
                let Ok(environment) = load_project_python_environment(
                    &selection.config_path,
                    &selection.version,
                    &current_target_for_tool("python"),
                ) else {
                    continue;
                };
                environment.command_directory
            } else {
                runtime_command_directory(provider.tool, &install_dir)
            };
        if command_dir.is_dir() && !entries.iter().any(|entry| paths_equal(entry, &command_dir)) {
            entries.push(command_dir);
        }
    }
    if let Some(inherited) = env::var_os("PATH") {
        for entry in env::split_paths(&inherited) {
            if !paths_equal(&entry, &shim_dir)
                && !entries.iter().any(|existing| paths_equal(existing, &entry))
            {
                entries.push(entry);
            }
        }
    }
    env::join_paths(entries).map_err(|source| Error::RuntimePathJoin { source })
}

fn effective_configured_tools(cwd: &Path, pinset_home: &Path) -> Result<BTreeMap<String, String>> {
    let context = find_project_context(cwd)?;
    if let Some(config_path) = context.config_path {
        let config = load_effective_project_config(&config_path)?;
        if !config.policy.inherit_global {
            return Ok(config.tools);
        }

        let mut tools = load_optional_global_config(&global_config_path(pinset_home))?
            .map(|config| config.tools)
            .unwrap_or_default();
        tools.extend(config.tools);
        return Ok(tools);
    }

    Ok(
        load_optional_global_config(&global_config_path(pinset_home))?
            .map(|config| config.tools)
            .unwrap_or_default(),
    )
}

pub fn selected_runtime_environment(
    tool: &str,
    cwd: &Path,
    pinset_home: &Path,
) -> Vec<RuntimeEnvironmentVariable> {
    let mut variables = Vec::new();
    let mut providers = if tool.is_empty() {
        Vec::new()
    } else {
        let Ok(providers) = provider_dependency_order(tool) else {
            return variables;
        };
        providers
    };
    if let Ok(configured_tools) = effective_configured_tools(cwd, pinset_home) {
        for provider in runtime_providers()
            .iter()
            .filter(|provider| configured_tools.contains_key(provider.tool))
        {
            if !providers
                .iter()
                .any(|existing| existing.tool == provider.tool)
            {
                providers.push(provider);
            }
        }
    }
    for provider in providers {
        if provider.capabilities.environment == RuntimeEnvironmentKind::None {
            continue;
        }
        let Ok(selection) = resolve_tool_selection(provider.tool, cwd, pinset_home) else {
            continue;
        };
        let install_dir = pinset_home
            .join("installs")
            .join(provider.tool)
            .join(&selection.installation_version)
            .join(current_target_for_tool(provider.tool));
        if provider.capabilities.environment == RuntimeEnvironmentKind::Python {
            if selection.source == SelectionSource::Project
                && let Ok(environment) = load_project_python_environment(
                    &selection.config_path,
                    &selection.version,
                    &current_target_for_tool("python"),
                )
            {
                variables.push(RuntimeEnvironmentVariable {
                    name: "VIRTUAL_ENV",
                    value: environment.root.into_os_string(),
                });
            }
        } else if install_dir.is_dir() {
            variables.extend(runtime_environment_for_install(provider.tool, &install_dir));
        }
    }
    variables
}

pub fn runtime_environment_for_install(
    tool: &str,
    install_dir: &Path,
) -> Vec<RuntimeEnvironmentVariable> {
    match runtime_provider(tool).map(|provider| provider.capabilities.environment) {
        Some(RuntimeEnvironmentKind::Go) => {
            let mut variables = vec![RuntimeEnvironmentVariable {
                name: "GOROOT",
                value: install_dir.as_os_str().to_owned(),
            }];
            if env::var_os("GOTOOLCHAIN").is_none() {
                variables.push(RuntimeEnvironmentVariable {
                    name: "GOTOOLCHAIN",
                    value: OsString::from("local"),
                });
            }
            variables
        }
        Some(RuntimeEnvironmentKind::Flutter) => {
            let mut variables = vec![RuntimeEnvironmentVariable {
                name: "FLUTTER_ROOT",
                value: install_dir.as_os_str().to_owned(),
            }];
            if env::var_os("FLUTTER_SUPPRESS_ANALYTICS").is_none() {
                variables.push(RuntimeEnvironmentVariable {
                    name: "FLUTTER_SUPPRESS_ANALYTICS",
                    value: OsString::from("true"),
                });
            }
            variables
        }
        Some(RuntimeEnvironmentKind::Java) => vec![RuntimeEnvironmentVariable {
            name: "JAVA_HOME",
            value: java_home_for_install(install_dir).into_os_string(),
        }],
        Some(RuntimeEnvironmentKind::Dotnet) => vec![RuntimeEnvironmentVariable {
            name: "DOTNET_ROOT",
            value: install_dir.as_os_str().to_owned(),
        }],
        Some(RuntimeEnvironmentKind::Python | RuntimeEnvironmentKind::None) | None => Vec::new(),
    }
}

pub fn validate_managed_runtime_invocation(
    tool: &str,
    command: &str,
    arguments: &[OsString],
) -> Result<()> {
    if tool != "flutter" || command != "flutter" {
        return Ok(());
    }
    let Some(subcommand) = arguments
        .iter()
        .filter_map(|argument| argument.to_str())
        .find(|argument| !argument.starts_with('-'))
    else {
        return Ok(());
    };
    if matches!(subcommand, "upgrade" | "downgrade" | "channel") {
        return Err(Error::ManagedFlutterMutation {
            command: format!("flutter {subcommand}"),
        });
    }
    Ok(())
}

pub fn runtime_command_directory(tool: &str, install_dir: &Path) -> PathBuf {
    match runtime_provider(tool).map(|provider| provider.capabilities.command_layout) {
        Some(RuntimeCommandLayout::NodeNative) if cfg!(windows) => install_dir.to_path_buf(),
        Some(RuntimeCommandLayout::NodeNative | RuntimeCommandLayout::Bin) => {
            install_dir.join("bin")
        }
        Some(RuntimeCommandLayout::Python) if cfg!(windows) => install_dir.to_path_buf(),
        Some(RuntimeCommandLayout::Python) => install_dir.join("bin"),
        Some(RuntimeCommandLayout::Java) => java_home_for_install(install_dir).join("bin"),
        Some(RuntimeCommandLayout::Root) | None => install_dir.to_path_buf(),
    }
}

pub fn java_home_for_install(install_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        install_dir.join("Contents").join("Home")
    } else {
        install_dir.to_path_buf()
    }
}

pub fn runtime_command_candidates(tool: &str, command: &str, install_dir: &Path) -> Vec<PathBuf> {
    let directory = runtime_command_directory(tool, install_dir);
    if tool != "python" {
        return executable_candidates(&directory, command);
    }
    let command = match command {
        "pip" | "pip3" => "python",
        command => command,
    };
    if cfg!(windows) {
        return executable_candidates(&directory, "python");
    }
    let names = if command == "python3" {
        ["python3", "python"]
    } else {
        ["python", "python3"]
    };
    names.into_iter().map(|name| directory.join(name)).collect()
}

pub fn managed_runtime_arguments(
    tool: &str,
    command: &str,
    arguments: &[OsString],
) -> Vec<OsString> {
    let mut resolved = Vec::with_capacity(arguments.len() + 2);
    if tool == "python" && matches!(command, "pip" | "pip3") {
        resolved.push(OsString::from("-m"));
        resolved.push(OsString::from("pip"));
    }
    resolved.extend_from_slice(arguments);
    resolved
}

pub fn validate_windows_batch_arguments(executable: &Path, arguments: &[OsString]) -> Result<()> {
    if !cfg!(windows)
        || !executable.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        })
    {
        return Ok(());
    }

    for (index, argument) in arguments.iter().enumerate() {
        let Some(argument) = argument.to_str() else {
            return Err(Error::UnsafeWindowsBatchArgument {
                path: executable.to_path_buf(),
                index: index + 1,
            });
        };
        if argument
            .chars()
            .any(|character| "&|<>()@^%!\"\r\n".contains(character))
        {
            return Err(Error::UnsafeWindowsBatchArgument {
                path: executable.to_path_buf(),
                index: index + 1,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
fn runtime_command_dir(install_dir: &Path) -> PathBuf {
    runtime_command_directory("node", install_dir)
}

fn executable_candidates(bin_dir: &Path, command: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ["exe", "cmd", "bat"]
            .into_iter()
            .map(|extension| bin_dir.join(command).with_extension(extension))
            .chain(std::iter::once(bin_dir.join(command)))
            .collect()
    }

    #[cfg(not(windows))]
    {
        vec![bin_dir.join(command)]
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn blocks_in_place_flutter_sdk_mutations_but_allows_project_commands() {
        for subcommand in ["upgrade", "downgrade", "channel"] {
            assert!(matches!(
                validate_managed_runtime_invocation(
                    "flutter",
                    "flutter",
                    &[OsString::from(subcommand)]
                ),
                Err(Error::ManagedFlutterMutation { .. })
            ));
            assert!(matches!(
                validate_managed_runtime_invocation(
                    "flutter",
                    "flutter",
                    &[OsString::from("--verbose"), OsString::from(subcommand)]
                ),
                Err(Error::ManagedFlutterMutation { .. })
            ));
        }
        validate_managed_runtime_invocation(
            "flutter",
            "flutter",
            &[OsString::from("pub"), OsString::from("get")],
        )
        .expect("flutter pub remains available");
        validate_managed_runtime_invocation("flutter", "dart", &[OsString::from("--version")])
            .expect("bundled Dart remains available");
    }

    #[test]
    fn resolves_node_from_nearest_project_config() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let nested = project.join("src").join("feature");
        let home = root.path().join("home");
        fs::create_dir_all(&nested).expect("nested directory");
        fs::create_dir(project.join(".git")).expect("git marker");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
        )
        .expect("project config");

        let install_dir = home
            .join("installs")
            .join("node")
            .join("20.0.0")
            .join(current_target());
        let bin = runtime_command_dir(&install_dir);
        fs::create_dir_all(&bin).expect("runtime bin");
        let executable = if cfg!(windows) {
            bin.join("node.exe")
        } else {
            bin.join("node")
        };
        fs::write(&executable, b"fake").expect("fake runtime");

        let resolution = resolve_command("node", &nested, &home).expect("resolution");
        assert_eq!(resolution.version, "20.0.0");
        assert_eq!(resolution.source, SelectionSource::Project);
        assert_eq!(resolution.executable, executable);
        assert_eq!(resolution.selection_path, Some(project.join("pinset.toml")));
    }

    #[test]
    fn selected_tool_path_includes_other_project_providers() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[tools]\nbun = \"1.3.14\"\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\npython = \"3.13.0\"\n",
        )
        .expect("project config");

        let pnpm_install_dir = home
            .join("installs/pnpm/11.22.0")
            .join(current_target_for_tool("pnpm"));
        let node_install_dir = home
            .join("installs/node/24.0.0")
            .join(current_target_for_tool("node"));
        let bun_install_dir = home
            .join("installs/bun/1.3.14")
            .join(current_target_for_tool("bun"));
        let pnpm_dir = runtime_command_directory("pnpm", &pnpm_install_dir);
        let node_dir = runtime_command_directory("node", &node_install_dir);
        let bun_dir = runtime_command_directory("bun", &bun_install_dir);
        for directory in [&pnpm_dir, &node_dir, &bun_dir] {
            fs::create_dir_all(directory).expect("runtime command directory");
        }
        let pnpm = pnpm_dir.join(if cfg!(windows) { "pnpm.exe" } else { "pnpm" });
        fs::write(&pnpm, b"fake pnpm").expect("pnpm executable");

        let path = path_with_selected_tools("pnpm", &pnpm, &project, &home)
            .expect("selected provider PATH");
        let entries = env::split_paths(&path).collect::<Vec<_>>();
        assert_eq!(entries[0], pnpm_dir);
        assert_eq!(entries[1], node_dir);
        assert_eq!(entries[2], bun_dir);
    }

    #[test]
    fn selected_tool_path_includes_inherited_global_providers() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[policy]\ninherit-global = true\n[tools]\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\n",
        )
        .expect("project config");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("global state directory"))
            .expect("global state directory");
        fs::write(&global_path, "schema = 1\n[tools]\nbun = \"1.3.14\"\n").expect("global config");

        let pnpm_install_dir = home
            .join("installs/pnpm/11.22.0")
            .join(current_target_for_tool("pnpm"));
        let node_install_dir = home
            .join("installs/node/24.0.0")
            .join(current_target_for_tool("node"));
        let bun_install_dir = home
            .join("installs/bun/1.3.14")
            .join(current_target_for_tool("bun"));
        let pnpm_dir = runtime_command_directory("pnpm", &pnpm_install_dir);
        let node_dir = runtime_command_directory("node", &node_install_dir);
        let bun_dir = runtime_command_directory("bun", &bun_install_dir);
        for directory in [&pnpm_dir, &node_dir, &bun_dir] {
            fs::create_dir_all(directory).expect("runtime command directory");
        }
        let pnpm = pnpm_dir.join(if cfg!(windows) { "pnpm.exe" } else { "pnpm" });
        fs::write(&pnpm, b"fake pnpm").expect("pnpm executable");

        let path = path_with_selected_tools("pnpm", &pnpm, &project, &home)
            .expect("selected provider PATH");
        let entries = env::split_paths(&path).collect::<Vec<_>>();
        assert_eq!(entries[0], pnpm_dir);
        assert_eq!(entries[1], node_dir);
        assert_eq!(entries[2], bun_dir);
    }

    #[test]
    fn selected_tool_path_and_environment_cover_every_configured_provider_capability() {
        let root = tempdir().expect("temp directory");
        let workspace = root.path().join("workspace");
        let home = root.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("global state directory"))
            .expect("global state directory");
        let mut config = String::from("schema = 1\n[tools]\n");
        for provider in runtime_providers() {
            config.push_str(&format!("{} = \"1.0.0\"\n", provider.tool));
        }
        fs::write(&global_path, config).expect("global config");

        let mut provider_directories = Vec::new();
        for provider in runtime_providers() {
            let install_dir = home
                .join("installs")
                .join(provider.tool)
                .join("1.0.0")
                .join(current_target_for_tool(provider.tool));
            let command_dir = runtime_command_directory(provider.tool, &install_dir);
            fs::create_dir_all(&command_dir).expect("provider command directory");
            provider_directories.push((provider.tool, install_dir, command_dir));
        }
        let node_dir = provider_directories
            .iter()
            .find(|(tool, _, _)| *tool == "node")
            .map(|(_, _, directory)| directory.clone())
            .expect("Node command directory");
        let node = node_dir.join(if cfg!(windows) { "node.exe" } else { "node" });
        fs::write(&node, b"fake node").expect("Node executable");

        let path =
            path_with_selected_tools("node", &node, &workspace, &home).expect("all-provider PATH");
        let entries = env::split_paths(&path).collect::<Vec<_>>();
        for (tool, _, expected) in &provider_directories {
            let matches = entries
                .iter()
                .filter(|entry| paths_equal(entry, expected))
                .count();
            assert_eq!(
                matches, 1,
                "Provider {tool} command directory must appear exactly once in PATH: {entries:?}"
            );
        }
        assert!(
            entries
                .iter()
                .all(|entry| !paths_equal(entry, &home.join("shims"))),
            "managed child PATH must not point back to Pinset shims"
        );

        let variables = selected_runtime_environment("node", &workspace, &home);
        for (tool, install_dir, _) in &provider_directories {
            let expected = match *tool {
                "go" => Some(RuntimeEnvironmentVariable {
                    name: "GOROOT",
                    value: install_dir.as_os_str().to_owned(),
                }),
                "flutter" => Some(RuntimeEnvironmentVariable {
                    name: "FLUTTER_ROOT",
                    value: install_dir.as_os_str().to_owned(),
                }),
                "java" => Some(RuntimeEnvironmentVariable {
                    name: "JAVA_HOME",
                    value: java_home_for_install(install_dir).into_os_string(),
                }),
                "dotnet" => Some(RuntimeEnvironmentVariable {
                    name: "DOTNET_ROOT",
                    value: install_dir.as_os_str().to_owned(),
                }),
                _ => None,
            };
            if let Some(expected) = expected {
                assert!(
                    variables.contains(&expected),
                    "Provider {tool} environment missing {expected:?}: {variables:?}"
                );
            }
        }
    }

    #[test]
    fn selected_runtime_environment_includes_peer_provider_roots() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[tools]\njava = \"21.0.0\"\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\n",
        )
        .expect("project config");
        let java_install_dir = home
            .join("installs")
            .join("java")
            .join("21.0.0")
            .join(current_target_for_tool("java"));
        fs::create_dir_all(&java_install_dir).expect("Java install directory");

        let variables = selected_runtime_environment("pnpm", &project, &home);
        let expected = RuntimeEnvironmentVariable {
            name: "JAVA_HOME",
            value: java_home_for_install(&java_install_dir).into_os_string(),
        };
        assert!(
            variables.contains(&expected),
            "expected {expected:?}; resolved environment variables: {variables:?}"
        );
    }

    #[test]
    fn derives_java_home_and_command_directory_from_the_platform_layout() {
        let install = PathBuf::from("pinset-java");
        let expected_home = if cfg!(target_os = "macos") {
            install.join("Contents/Home")
        } else {
            install.clone()
        };
        assert_eq!(java_home_for_install(&install), expected_home);
        assert_eq!(
            runtime_command_directory("java", &install),
            expected_home.join("bin")
        );
        assert_eq!(
            runtime_environment_for_install("java", &install),
            [RuntimeEnvironmentVariable {
                name: "JAVA_HOME",
                value: expected_home.into_os_string(),
            }]
        );
    }

    #[test]
    fn exposes_the_selected_dotnet_sdk_root() {
        let install = PathBuf::from("pinset-dotnet");
        assert_eq!(runtime_command_directory("dotnet", &install), install);
        assert_eq!(
            runtime_environment_for_install("dotnet", &install),
            [RuntimeEnvironmentVariable {
                name: "DOTNET_ROOT",
                value: install.into_os_string(),
            }]
        );
    }

    #[test]
    fn routes_pip_through_the_selected_python_module() {
        let install = PathBuf::from("pinset-python");
        assert_eq!(
            runtime_command_candidates("python", "pip", &install),
            runtime_command_candidates("python", "python", &install)
        );
        assert_eq!(
            managed_runtime_arguments(
                "python",
                "pip3",
                &[OsString::from("install"), OsString::from("ruff")]
            ),
            ["-m", "pip", "install", "ruff"].map(OsString::from)
        );
        assert_eq!(
            managed_runtime_arguments("python", "python", &[OsString::from("--version")]),
            [OsString::from("--version")]
        );
    }

    #[test]
    fn resolves_global_selection_without_a_project() {
        let root = tempdir().expect("temp directory");
        let cwd = root.path().join("workspace");
        let home = root.path().join("home");
        fs::create_dir_all(&cwd).expect("workspace");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(&global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");

        let install_dir = home
            .join("installs")
            .join("node")
            .join("24.0.0")
            .join(current_target());
        let bin = runtime_command_dir(&install_dir);
        fs::create_dir_all(&bin).expect("runtime bin");
        let executable = if cfg!(windows) {
            bin.join("node.exe")
        } else {
            bin.join("node")
        };
        fs::write(&executable, b"fake").expect("fake runtime");

        let resolution = resolve_command("node", &cwd, &home).expect("resolution");
        assert_eq!(resolution.version, "24.0.0");
        assert_eq!(resolution.source, SelectionSource::Global);
        assert_eq!(resolution.selection_path, Some(global_path));
    }

    #[test]
    fn project_selection_overrides_global_and_does_not_fallback_when_missing() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
        )
        .expect("project config");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");

        let global_install = home.join("installs/node/24.0.0").join(current_target());
        let global_bin = runtime_command_dir(&global_install);
        fs::create_dir_all(&global_bin).expect("global runtime bin");
        let global_executable = if cfg!(windows) {
            global_bin.join("node.exe")
        } else {
            global_bin.join("node")
        };
        fs::write(global_executable, b"fake").expect("fake global runtime");

        let error = resolve_command("node", &project, &home).expect_err("project is authoritative");
        let message = error.to_string();
        assert!(message.contains("20.0.0"));
        assert!(!message.contains("24.0.0"));
    }

    #[test]
    fn project_can_explicitly_inherit_global_selection() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 2\n[policy]\ninherit-global = true\n[tools]\n",
        )
        .expect("project config");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(&global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");

        let selection = resolve_tool_selection("node", &project, &home).expect("selection");
        assert_eq!(selection.version, "24.0.0");
        assert_eq!(selection.source, SelectionSource::Global);
        assert_eq!(selection.config_path, global_path);
    }

    #[test]
    fn project_without_tool_is_strict_by_default() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(project.join("pinset.toml"), "schema = 2\n[tools]\n").expect("project config");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");

        assert!(matches!(
            resolve_tool_selection("node", &project, &home),
            Err(Error::ProjectToolSelectionRequired { .. })
        ));
    }

    #[test]
    fn project_can_explicitly_fall_back_to_system_path() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        let system_bin = root.path().join("system-bin");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 2\n[policy]\nsystem-fallback = true\n[tools]\n",
        )
        .expect("project config");
        let executable = create_fake_command(&system_bin, "node");
        let path = env::join_paths([&system_bin]).expect("system PATH");

        let resolution =
            resolve_command_with_path("node", &project, &home, Some(path.as_os_str()), &[])
                .expect("system resolution");
        assert_eq!(resolution.source, SelectionSource::System);
        assert_eq!(resolution.executable, executable);
    }

    #[test]
    fn resolves_system_path_only_when_no_project_or_global_selection_exists() {
        let root = tempdir().expect("temp directory");
        let cwd = root.path().join("workspace");
        let home = root.path().join("home");
        let system_bin = root.path().join("system-bin");
        fs::create_dir_all(&cwd).expect("workspace");
        let executable = create_fake_command(&system_bin, "node");
        let path = env::join_paths([&system_bin]).expect("system PATH");

        let resolution =
            resolve_command_with_path("node", &cwd, &home, Some(path.as_os_str()), &[])
                .expect("system resolution");

        assert_eq!(resolution.source, SelectionSource::System);
        assert_eq!(resolution.version, "unknown");
        assert_eq!(resolution.selection_path, None);
        assert_eq!(resolution.executable, executable);
    }

    #[test]
    fn system_search_excludes_pinset_shim_directory_and_current_executable() {
        let root = tempdir().expect("temp directory");
        let cwd = root.path().join("workspace");
        let home = root.path().join("home");
        let shim_bin = home.join("shims");
        let system_bin = root.path().join("system-bin");
        fs::create_dir_all(&cwd).expect("workspace");
        create_fake_command(&shim_bin, "node");
        let system_node = create_fake_command(&system_bin, "node");
        let path = env::join_paths([&shim_bin, &system_bin]).expect("system PATH");

        let resolution =
            resolve_command_with_path("node", &cwd, &home, Some(path.as_os_str()), &[])
                .expect("system resolution");
        assert_eq!(resolution.executable, system_node);

        let original_dir = root.path().join("pinset-bin");
        let original = create_fake_command(&original_dir, "node");
        let alias_dir = root.path().join("alias-bin");
        fs::create_dir_all(&alias_dir).expect("alias directory");
        let alias = command_path(&alias_dir, "node");
        fs::hard_link(&original, &alias).expect("hard-link shim alias");
        let alias_path = env::join_paths([&alias_dir]).expect("alias PATH");
        assert!(matches!(
            resolve_command_with_path(
                "node",
                &cwd,
                &home,
                Some(alias_path.as_os_str()),
                std::slice::from_ref(&original),
            ),
            Err(Error::CommandSelectionNotFound { .. })
        ));

        let copy_dir = root.path().join("copy-bin");
        fs::create_dir_all(&copy_dir).expect("copy directory");
        let copied = command_path(&copy_dir, "node");
        fs::copy(&original, &copied).expect("copied shim alias");
        let copy_path = env::join_paths([&copy_dir]).expect("copy PATH");
        assert!(matches!(
            resolve_command_with_path(
                "node",
                &cwd,
                &home,
                Some(copy_path.as_os_str()),
                std::slice::from_ref(&original),
            ),
            Err(Error::CommandSelectionNotFound { .. })
        ));
    }

    #[test]
    fn missing_global_runtime_does_not_fall_back_to_system_path() {
        let root = tempdir().expect("temp directory");
        let cwd = root.path().join("workspace");
        let home = root.path().join("home");
        let system_bin = root.path().join("system-bin");
        fs::create_dir_all(&cwd).expect("workspace");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");
        create_fake_command(&system_bin, "node");
        let path = env::join_paths([&system_bin]).expect("system PATH");

        let error = resolve_command_with_path("node", &cwd, &home, Some(path.as_os_str()), &[])
            .expect_err("global selection must fail closed");

        assert!(matches!(
            error,
            Error::RuntimeCommandNotFound { version, .. } if version == "24.0.0"
        ));
    }

    #[test]
    fn reports_all_searched_runtime_candidates() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
        )
        .expect("project config");

        let error =
            resolve_command("node", &project, &root.path().join("home")).expect_err("missing");
        let message = error.to_string();
        assert!(message.contains("node"));
        assert!(message.contains("20.0.0"));
        assert!(message.contains("searched"));
    }

    fn command_path(directory: &Path, command: &str) -> PathBuf {
        if cfg!(windows) {
            directory.join(command).with_extension("exe")
        } else {
            directory.join(command)
        }
    }

    fn create_fake_command(directory: &Path, command: &str) -> PathBuf {
        fs::create_dir_all(directory).expect("command directory");
        let executable = command_path(directory, command);
        fs::write(&executable, b"fake").expect("fake command");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&executable)
                .expect("fake command metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).expect("fake command permissions");
        }
        executable
    }
}
