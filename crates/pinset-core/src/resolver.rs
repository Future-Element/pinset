use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
use crate::current_target;
use crate::{
    Error, Result, RuntimeCommandLayout, RuntimeEnvironmentKind, current_target_for_tool,
    find_optional_project_config, global_config_path, is_managed_command_shim,
    load_optional_global_config, load_project_config, load_project_python_environment,
    project_python_command_candidates, runtime_provider, runtime_provider_for_command,
    runtime_providers,
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
    pub version: String,
    pub source: SelectionSource,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResolution {
    pub command: String,
    pub tool: String,
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
                version: "unknown".to_owned(),
                source: SelectionSource::System,
                selection_path: None,
                executable: executable.clone(),
            });
        }
        Err(error) => return Err(error),
    };
    let version = selection.version.clone();

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
            version,
            source: selection.source,
            selection_path: Some(selection.config_path),
            executable,
        });
    }

    let install_dir = pinset_home
        .join("installs")
        .join(tool)
        .join(&version)
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
    let selection = resolve_tool_selection("python", cwd, pinset_home)?;
    if selection.source != SelectionSource::Project {
        return Err(Error::PythonEnvironmentSelectionMissing {
            path: cwd.to_path_buf(),
        });
    }
    let target = current_target_for_tool("python");
    let environment =
        load_project_python_environment(&selection.config_path, &selection.version, &target)?;
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
        version: selection.version,
        source: SelectionSource::Project,
        selection_path: Some(selection.config_path),
        executable,
    })
}

pub fn resolve_tool_selection(tool: &str, cwd: &Path, pinset_home: &Path) -> Result<ToolSelection> {
    let project_path = find_optional_project_config(cwd)?;
    if let Some(config_path) = project_path.as_ref() {
        let config = load_project_config(config_path)?;
        if let Some(version) = config.tools.get(tool) {
            return Ok(ToolSelection {
                tool: tool.to_owned(),
                version: version.clone(),
                source: SelectionSource::Project,
                config_path: config_path.clone(),
            });
        }
    }

    let global_path = global_config_path(pinset_home);
    if let Some(config) = load_optional_global_config(&global_path)? {
        if let Some(version) = config.tools.get(tool) {
            return Ok(ToolSelection {
                tool: tool.to_owned(),
                version: version.clone(),
                source: SelectionSource::Global,
                config_path: global_path,
            });
        }
    }

    Err(Error::ToolSelectionNotFound {
        tool: tool.to_owned(),
        start: cwd.to_path_buf(),
        global_config_path: global_path,
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
    let mut entries = vec![selected_dir.clone()];
    for provider in runtime_providers() {
        let Ok(selection) = resolve_tool_selection(provider.tool, cwd, pinset_home) else {
            continue;
        };
        let install_dir = pinset_home
            .join("installs")
            .join(provider.tool)
            .join(&selection.version)
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

pub fn selected_runtime_environment(
    cwd: &Path,
    pinset_home: &Path,
) -> Vec<RuntimeEnvironmentVariable> {
    let mut variables = Vec::new();
    for provider in runtime_providers() {
        if provider.environment == RuntimeEnvironmentKind::None {
            continue;
        }
        let Ok(selection) = resolve_tool_selection(provider.tool, cwd, pinset_home) else {
            continue;
        };
        let install_dir = pinset_home
            .join("installs")
            .join(provider.tool)
            .join(&selection.version)
            .join(current_target_for_tool(provider.tool));
        if provider.environment == RuntimeEnvironmentKind::Python {
            if selection.source == SelectionSource::Project {
                if let Ok(environment) = load_project_python_environment(
                    &selection.config_path,
                    &selection.version,
                    &current_target_for_tool("python"),
                ) {
                    variables.push(RuntimeEnvironmentVariable {
                        name: "VIRTUAL_ENV",
                        value: environment.root.into_os_string(),
                    });
                }
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
    match runtime_provider(tool).map(|provider| provider.environment) {
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
    match runtime_provider(tool).map(|provider| provider.command_layout) {
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
    fn project_without_tool_falls_back_to_global_selection() {
        let root = tempdir().expect("temp directory");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        fs::write(project.join("pinset.toml"), "schema = 1\n[tools]\n").expect("project config");
        let global_path = global_config_path(&home);
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(&global_path, "schema = 1\n[tools]\nnode = \"24.0.0\"\n").expect("global config");

        let selection = resolve_tool_selection("node", &project, &home).expect("selection");
        assert_eq!(selection.version, "24.0.0");
        assert_eq!(selection.source, SelectionSource::Global);
        assert_eq!(selection.config_path, global_path);
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
