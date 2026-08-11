use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    Error, Result, current_target, find_optional_project_config, global_config_path,
    load_optional_global_config, load_project_config,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSource {
    Project,
    Global,
}

impl SelectionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
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
    pub config_path: PathBuf,
    pub executable: PathBuf,
}

pub fn command_tool(command: &str) -> Option<&'static str> {
    match command {
        "node" | "npm" | "npx" | "corepack" => Some("node"),
        _ => None,
    }
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
    let tool = command_tool(command).ok_or_else(|| Error::UnsupportedCommand {
        command: command.to_owned(),
    })?;
    let selection = resolve_tool_selection(tool, cwd, pinset_home)?;
    let version = selection.version.clone();

    let install_dir = pinset_home
        .join("installs")
        .join(tool)
        .join(&version)
        .join(current_target());
    let bin_dir = runtime_command_dir(&install_dir);
    let candidates = executable_candidates(&bin_dir, command);
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
        config_path: selection.config_path,
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

    if let Some(config_path) = project_path {
        return Err(Error::ToolNotConfigured {
            tool: tool.to_owned(),
            config_path,
        });
    }

    Err(Error::ToolSelectionNotFound {
        tool: tool.to_owned(),
        start: cwd.to_path_buf(),
        global_config_path: global_path,
    })
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

fn runtime_command_dir(install_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        install_dir.to_path_buf()
    } else {
        install_dir.join("bin")
    }
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
        assert_eq!(resolution.config_path, project.join("pinset.toml"));
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
        assert_eq!(resolution.config_path, global_path);
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
}
