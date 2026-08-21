//! Runtime-independent command router used by every Provider command.
//!
//! INVARIANT: resolution excludes this executable and the managed shim directory, while a depth
//! marker protects the remaining process boundary. A routed runtime must never resolve back to a
//! Pinset shim.

use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};
use zeroize::Zeroize;

type EncryptedEnvironment = Option<(
    EnvironmentCollision,
    std::collections::BTreeMap<String, String>,
)>;

#[cfg(windows)]
use std::ffi::OsStr;

use pinset_core::{
    CommandResolution, EnvironmentCollision, decode_environment, find_optional_project_config,
    load_project_config, managed_runtime_arguments, path_with_selected_tools, pinset_home_from_env,
    resolve_command_with_path, selected_runtime_environment, validate_managed_runtime_invocation,
};

const SHIM_DEPTH_ENV: &str = "PINSET_SHIM_DEPTH";
const SELECTED_TOOL_ENV: &str = "PINSET_SELECTED_TOOL";
const SELECTED_VERSION_ENV: &str = "PINSET_SELECTED_VERSION";
const SELECTION_SOURCE_ENV: &str = "PINSET_SELECTION_SOURCE";
const CONFIG_PATH_ENV: &str = "PINSET_CONFIG_PATH";
const IDENTITY_ENV: &str = "PINSET_IDENTITY";
const ENV_PROFILE_ENV: &str = "PINSET_ENV_PROFILE";
const ENV_DISABLE_ENV: &str = "PINSET_ENV_DISABLE";
const IDENTITY_FILE_ENV: &str = "PINSET_IDENTITY_FILE";

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("pinset shim error: {error}");
            process::exit(8);
        }
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    ensure_not_recursive()?;

    let invocation = Invocation::parse()?;
    let home = pinset_home_from_env()?;
    let current_executable = env::current_exe()?;
    let path = env::var_os("PATH");
    let resolution = resolve_command_with_path(
        &invocation.command,
        &invocation.cwd,
        &home,
        path.as_deref(),
        std::slice::from_ref(&current_executable),
    )?;
    reject_shim_directory_target(&resolution, &home)?;
    if resolution.source != pinset_core::SelectionSource::System {
        validate_managed_runtime_invocation(
            &resolution.tool,
            &invocation.command,
            &invocation.arguments,
        )?;
    }
    let runtime_arguments = if resolution.source == pinset_core::SelectionSource::System {
        invocation.arguments.clone()
    } else {
        managed_runtime_arguments(&resolution.tool, &invocation.command, &invocation.arguments)
    };

    let runtime_path = path_with_selected_tools(
        &resolution.tool,
        &resolution.executable,
        &invocation.cwd,
        &home,
    )?;
    let mut child = command_for_runtime(&resolution.executable, &runtime_arguments);
    child
        .env("PATH", runtime_path)
        .env(SHIM_DEPTH_ENV, "1")
        .env(SELECTED_TOOL_ENV, &resolution.tool)
        .env(SELECTED_VERSION_ENV, &resolution.version)
        .env(SELECTION_SOURCE_ENV, resolution.source.as_str());
    let runtime_environment =
        selected_runtime_environment(&resolution.tool, &invocation.cwd, &home);
    let mut occupied = env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .map(|name| name.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    occupied.extend(
        runtime_environment
            .iter()
            .map(|variable| variable.name.to_ascii_uppercase()),
    );
    for variable in runtime_environment {
        child.env(variable.name, variable.value);
    }
    if let Some((collision, encrypted)) =
        encrypted_environment(&invocation.cwd, &current_executable)?
    {
        for (name, mut value) in encrypted {
            let exists = occupied.contains(&name.to_ascii_uppercase());
            match (collision, exists) {
                (EnvironmentCollision::Error, true) => {
                    value.zeroize();
                    return Err(format!("encrypted environment variable {name} collides with the process environment").into());
                }
                (EnvironmentCollision::ProcessWins, true) => {
                    value.zeroize();
                    continue;
                }
                _ => {
                    child.env(&name, &value);
                    value.zeroize();
                    occupied.insert(name.to_ascii_uppercase());
                }
            }
        }
    }
    child.env_remove(IDENTITY_ENV);
    child.env_remove(ENV_PROFILE_ENV);
    child.env_remove(ENV_DISABLE_ENV);
    child.env_remove(IDENTITY_FILE_ENV);
    if resolution.tool == "python" {
        child.env_remove("PYTHONHOME");
        if resolution.source != pinset_core::SelectionSource::Project {
            child.env_remove("VIRTUAL_ENV");
        }
    }
    if let Some(path) = &resolution.selection_path {
        child.env(CONFIG_PATH_ENV, path);
    } else {
        child.env_remove(CONFIG_PATH_ENV);
    }

    let status = child.status()?;
    Ok(status.code().unwrap_or(1))
}

fn encrypted_environment(
    cwd: &Path,
    shim_executable: &Path,
) -> Result<EncryptedEnvironment, Box<dyn std::error::Error>> {
    if env::var_os(ENV_DISABLE_ENV).is_some_and(|value| value == "1") {
        return Ok(None);
    }
    let Some(config_path) = find_optional_project_config(cwd)? else {
        return Ok(None);
    };
    let config = load_project_config(&config_path)?;
    let Some(environment) = config.environment else {
        return Ok(None);
    };
    let explicit_profile = env::var(ENV_PROFILE_ENV).ok();
    if explicit_profile.is_none() && environment.auto_profile.is_none() {
        return Ok(None);
    }
    let directory = shim_executable
        .parent()
        .ok_or("pinset shim executable has no parent directory")?;
    let cli = directory.join(if cfg!(windows) {
        "pinset.exe"
    } else {
        "pinset"
    });
    if !cli.is_file() {
        return Err(format!(
            "matching Pinset CLI is missing next to the shim: {}",
            cli.display()
        )
        .into());
    }
    let mut broker = Command::new(&cli);
    broker
        .arg("__env-resolve")
        .arg("--cwd")
        .arg(cwd)
        .arg("--shim-version")
        .arg(env!("CARGO_PKG_VERSION"))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(profile) = explicit_profile.as_deref() {
        broker.arg("--profile").arg(profile);
    }
    let mut output = broker.output()?;
    if !output.status.success() {
        output.stdout.zeroize();
        return Err(format!(
            "Pinset environment broker failed with status {}",
            output.status
        )
        .into());
    }
    let variables = decode_environment(&output.stdout);
    output.stdout.zeroize();
    let variables = variables?;
    Ok(Some((environment.collision, variables)))
}

#[derive(Debug)]
struct Invocation {
    command: String,
    cwd: PathBuf,
    arguments: Vec<OsString>,
}

impl Invocation {
    fn parse() -> Result<Self, String> {
        let mut arguments = env::args_os();
        let invoked_as = arguments.next().ok_or("missing argv[0]")?;
        let invoked_name = command_name(Path::new(&invoked_as)).ok_or_else(|| {
            format!(
                "cannot derive command name from {}",
                Path::new(&invoked_as).display()
            )
        })?;

        if invoked_name == "pinset-shim" {
            return Self::parse_debug_mode(arguments.collect());
        }

        Ok(Self {
            command: invoked_name,
            cwd: env::current_dir().map_err(|error| error.to_string())?,
            arguments: arguments.collect(),
        })
    }

    fn parse_debug_mode(arguments: Vec<OsString>) -> Result<Self, String> {
        let mut command = None;
        let mut cwd = None;
        let mut runtime_arguments = Vec::new();
        let mut index = 0;

        while index < arguments.len() {
            match arguments[index].to_str() {
                Some("--as") => {
                    index += 1;
                    command = arguments
                        .get(index)
                        .and_then(|value| value.to_str())
                        .map(str::to_owned);
                }
                Some("--cwd") => {
                    index += 1;
                    cwd = arguments.get(index).map(PathBuf::from);
                }
                Some("--") => {
                    runtime_arguments.extend(arguments.into_iter().skip(index + 1));
                    break;
                }
                Some(flag) => return Err(format!("unknown pinset-shim debug option: {flag}")),
                None => return Err("pinset-shim debug options must be valid UTF-8".to_owned()),
            }
            index += 1;
        }

        Ok(Self {
            command: command.ok_or("pinset-shim requires --as <command> in debug mode")?,
            cwd: cwd
                .map(Ok)
                .unwrap_or_else(env::current_dir)
                .map_err(|error| error.to_string())?,
            arguments: runtime_arguments,
        })
    }
}

fn command_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    Some(stem.to_ascii_lowercase())
}

fn ensure_not_recursive() -> Result<(), String> {
    if env::var_os(SHIM_DEPTH_ENV).is_some() {
        return Err(format!(
            "recursive shim invocation detected via {SHIM_DEPTH_ENV}"
        ));
    }
    Ok(())
}

fn reject_shim_directory_target(resolution: &CommandResolution, home: &Path) -> Result<(), String> {
    let shims = home.join("shims");
    if resolution.executable.starts_with(&shims) {
        return Err(format!(
            "resolved runtime points back into the Pinset shim directory: {}",
            resolution.executable.display()
        ));
    }
    Ok(())
}

fn command_for_runtime(executable: &Path, arguments: &[OsString]) -> Command {
    #[cfg(windows)]
    {
        let extension = executable
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command = Command::new("cmd.exe");
            command.arg("/D").arg("/C").arg(executable).args(arguments);
            return command;
        }
    }

    let mut command = Command::new(executable);
    command.args(arguments);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_executable_extension_from_command_name() {
        assert_eq!(
            command_name(Path::new("C:/tools/node.exe")).as_deref(),
            Some("node")
        );
        assert_eq!(
            command_name(Path::new("/tools/npm")).as_deref(),
            Some("npm")
        );
    }
}
