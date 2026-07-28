use std::{
    env,
    path::{Path, PathBuf},
};

use crate::{Error, Result, current_target, find_project_config, load_project_config};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResolution {
    pub command: String,
    pub tool: String,
    pub version: String,
    pub config_path: PathBuf,
    pub executable: PathBuf,
}

pub fn command_tool(command: &str) -> Option<&'static str> {
    match command {
        "node" | "npm" | "npx" | "corepack" => Some("node"),
        _ => None,
    }
}

pub fn pinset_home_from_env() -> Result<PathBuf> {
    env::var_os("PINSET_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(Error::PinsetHomeNotSet)
}

pub fn resolve_from_env(command: &str, cwd: &Path) -> Result<CommandResolution> {
    resolve_command(command, cwd, &pinset_home_from_env()?)
}

pub fn resolve_command(command: &str, cwd: &Path, pinset_home: &Path) -> Result<CommandResolution> {
    let tool = command_tool(command).ok_or_else(|| Error::UnsupportedCommand {
        command: command.to_owned(),
    })?;
    let config_path = find_project_config(cwd)?;
    let config = load_project_config(&config_path)?;
    let version = config
        .tools
        .get(tool)
        .cloned()
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: tool.to_owned(),
            config_path: config_path.clone(),
        })?;

    let bin_dir = pinset_home
        .join("installs")
        .join(tool)
        .join(&version)
        .join(current_target())
        .join("bin");
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
        config_path,
        executable,
    })
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

        let bin = home
            .join("installs")
            .join("node")
            .join("20.0.0")
            .join(current_target())
            .join("bin");
        fs::create_dir_all(&bin).expect("runtime bin");
        let executable = if cfg!(windows) {
            bin.join("node.exe")
        } else {
            bin.join("node")
        };
        fs::write(&executable, b"fake").expect("fake runtime");

        let resolution = resolve_command("node", &nested, &home).expect("resolution");
        assert_eq!(resolution.version, "20.0.0");
        assert_eq!(resolution.executable, executable);
        assert_eq!(resolution.config_path, project.join("pinset.toml"));
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
