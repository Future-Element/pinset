//! Pinset-owned project Python environment lifecycle.
//!
//! SAFETY: `.venv` is user-adjacent data. Pinset deletes or reuses it only when a strict marker
//! proves the selected CPython distribution and target; missing, malformed, or symlinked state
//! fails closed rather than guessing ownership.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const PYTHON_ENVIRONMENT_DIR: &str = ".venv";
pub const PYTHON_ENVIRONMENT_MARKER: &str = ".pinset-venv.toml";
const PYTHON_ENVIRONMENT_SCHEMA: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPythonEnvironment {
    pub name: String,
    pub root: PathBuf,
    pub command_directory: PathBuf,
    pub python: PathBuf,
    pub distribution: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PythonEnvironmentMarker {
    schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    distribution: String,
    target: String,
}

pub fn project_python_environment_path(project_config_path: &Path) -> PathBuf {
    project_config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PYTHON_ENVIRONMENT_DIR)
}

pub fn project_python_environment_path_for(
    project_config_path: &Path,
    environment_name: &str,
    relative_path: &str,
) -> Result<PathBuf> {
    if environment_name == "default" {
        if relative_path != PYTHON_ENVIRONMENT_DIR {
            return Err(Error::InvalidProjectConfig {
                reason: "the default Python environment path must remain .venv".to_owned(),
            });
        }
        return Ok(project_python_environment_path(project_config_path));
    }
    if environment_name.is_empty()
        || environment_name.len() > 64
        || !environment_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Error::InvalidProjectConfig {
            reason: format!("invalid Python environment name {environment_name:?}"),
        });
    }
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative_path.contains(['\\', ':'])
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Error::InvalidProjectConfig {
            reason: format!(
                "Python environment {environment_name} path must stay within the project"
            ),
        });
    }
    let project_root = project_config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let root = project_root.join(relative);
    let mut ancestor = project_root.to_path_buf();
    for component in relative.components() {
        ancestor.push(component.as_os_str());
        match fs::symlink_metadata(&ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::PythonEnvironmentNotOwned { path: ancestor });
            }
            Ok(metadata) if !metadata.is_dir() && ancestor != root => {
                return Err(Error::PythonEnvironmentNotOwned { path: ancestor });
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(Error::ReadPythonEnvironmentMarker {
                    path: ancestor,
                    source,
                });
            }
        }
    }
    Ok(root)
}

pub fn load_project_python_environment(
    project_config_path: &Path,
    expected_distribution: &str,
    expected_target: &str,
) -> Result<ProjectPythonEnvironment> {
    load_project_python_environment_for(
        project_config_path,
        "default",
        PYTHON_ENVIRONMENT_DIR,
        expected_distribution,
        expected_target,
    )
}

pub fn load_project_python_environment_for(
    project_config_path: &Path,
    environment_name: &str,
    relative_path: &str,
    expected_distribution: &str,
    expected_target: &str,
) -> Result<ProjectPythonEnvironment> {
    let root =
        project_python_environment_path_for(project_config_path, environment_name, relative_path)?;
    let marker = read_marker(&root)?;
    let marker_environment = marker.environment.as_deref().unwrap_or("default");
    if marker_environment != environment_name {
        return Err(Error::PythonEnvironmentMismatch {
            path: root,
            expected: format!("environment {environment_name}"),
            actual: format!("environment {marker_environment}"),
        });
    }
    if marker.distribution != expected_distribution || marker.target != expected_target {
        return Err(Error::PythonEnvironmentMismatch {
            path: root,
            expected: format!("{expected_distribution} ({expected_target})"),
            actual: format!("{} ({})", marker.distribution, marker.target),
        });
    }
    environment_from_marker(root, marker)
}

pub fn create_project_python_environment(
    project_config_path: &Path,
    base_python: &Path,
    distribution: &str,
    target: &str,
    recreate: bool,
) -> Result<ProjectPythonEnvironment> {
    create_project_python_environment_for(
        project_config_path,
        "default",
        PYTHON_ENVIRONMENT_DIR,
        base_python,
        distribution,
        target,
        recreate,
    )
}

pub fn create_project_python_environment_for(
    project_config_path: &Path,
    environment_name: &str,
    relative_path: &str,
    base_python: &Path,
    distribution: &str,
    target: &str,
    recreate: bool,
) -> Result<ProjectPythonEnvironment> {
    let root =
        project_python_environment_path_for(project_config_path, environment_name, relative_path)?;
    match fs::symlink_metadata(&root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(Error::PythonEnvironmentNotOwned { path: root });
            }
            if !recreate {
                return load_project_python_environment_for(
                    project_config_path,
                    environment_name,
                    relative_path,
                    distribution,
                    target,
                );
            }
            // The marker is deliberately checked before recursive removal. `recreate` is not
            // permission to replace an environment created by the user or another tool.
            load_project_python_environment_for(
                project_config_path,
                environment_name,
                relative_path,
                distribution,
                target,
            )?;
            fs::remove_dir_all(&root).map_err(|source| Error::RemovePythonEnvironment {
                path: root.clone(),
                source,
            })?;
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::ReadPythonEnvironmentMarker {
                path: root.clone(),
                source,
            });
        }
    }

    let mut command = command_for_python(base_python);
    let status = command
        .arg("-m")
        .arg("venv")
        .arg(&root)
        .status()
        .map_err(|source| Error::PythonEnvironmentCreate {
            path: root.clone(),
            source,
        })?;
    if !status.success() {
        clean_failed_environment(&root);
        return Err(Error::PythonEnvironmentCreateFailed {
            path: root,
            code: status.code().unwrap_or(1),
        });
    }

    let marker = PythonEnvironmentMarker {
        schema: if environment_name == "default" {
            1
        } else {
            PYTHON_ENVIRONMENT_SCHEMA
        },
        environment: (environment_name != "default").then(|| environment_name.to_owned()),
        distribution: distribution.to_owned(),
        target: target.to_owned(),
    };
    let serialized = toml::to_string_pretty(&marker).map_err(|source| {
        Error::InvalidPythonEnvironmentMarker {
            path: root.join(PYTHON_ENVIRONMENT_MARKER),
            reason: source.to_string(),
        }
    })?;
    let marker_path = root.join(PYTHON_ENVIRONMENT_MARKER);
    if let Err(source) = fs::write(&marker_path, serialized) {
        clean_failed_environment(&root);
        return Err(Error::WritePythonEnvironmentMarker {
            path: marker_path,
            source,
        });
    }

    match load_project_python_environment_for(
        project_config_path,
        environment_name,
        relative_path,
        distribution,
        target,
    ) {
        Ok(environment) => Ok(environment),
        Err(error) => {
            clean_failed_environment(&root);
            Err(error)
        }
    }
}

pub fn project_python_command_candidates(
    environment: &ProjectPythonEnvironment,
    command: &str,
) -> Vec<PathBuf> {
    if matches!(command, "python" | "python3" | "pip" | "pip3") {
        return vec![environment.python.clone()];
    }
    executable_candidates(&environment.command_directory, command)
}

fn read_marker(root: &Path) -> Result<PythonEnvironmentMarker> {
    let metadata = fs::symlink_metadata(root).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::PythonEnvironmentMissing {
                path: root.to_path_buf(),
            }
        } else {
            Error::ReadPythonEnvironmentMarker {
                path: root.to_path_buf(),
                source,
            }
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::PythonEnvironmentNotOwned {
            path: root.to_path_buf(),
        });
    }
    let marker_path = root.join(PYTHON_ENVIRONMENT_MARKER);
    let content = fs::read_to_string(&marker_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::PythonEnvironmentNotOwned {
                path: root.to_path_buf(),
            }
        } else {
            Error::ReadPythonEnvironmentMarker {
                path: marker_path.clone(),
                source,
            }
        }
    })?;
    let marker: PythonEnvironmentMarker =
        toml::from_str(&content).map_err(|source| Error::InvalidPythonEnvironmentMarker {
            path: marker_path.clone(),
            reason: source.to_string(),
        })?;
    let valid_schema = match marker.schema {
        1 => marker.environment.is_none(),
        PYTHON_ENVIRONMENT_SCHEMA => marker.environment.as_deref().is_some_and(|name| {
            name != "default"
                && !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        }),
        _ => false,
    };
    if !valid_schema || marker.distribution.trim().is_empty() || marker.target.trim().is_empty() {
        return Err(Error::InvalidPythonEnvironmentMarker {
            path: marker_path,
            reason: "expected a supported marker schema with environment identity, distribution and target"
                .to_owned(),
        });
    }
    Ok(marker)
}

fn environment_from_marker(
    root: PathBuf,
    marker: PythonEnvironmentMarker,
) -> Result<ProjectPythonEnvironment> {
    let command_directory = if cfg!(windows) {
        root.join("Scripts")
    } else {
        root.join("bin")
    };
    let python = if cfg!(windows) {
        command_directory.join("python.exe")
    } else {
        command_directory.join("python")
    };
    if !python.is_file() || !root.join("pyvenv.cfg").is_file() {
        return Err(Error::InvalidPythonEnvironmentMarker {
            path: root.join(PYTHON_ENVIRONMENT_MARKER),
            reason: "managed environment is missing pyvenv.cfg or its Python executable".to_owned(),
        });
    }
    Ok(ProjectPythonEnvironment {
        name: marker.environment.unwrap_or_else(|| "default".to_owned()),
        root,
        command_directory,
        python,
        distribution: marker.distribution,
        target: marker.target,
    })
}

fn clean_failed_environment(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

fn command_for_python(executable: &Path) -> Command {
    #[cfg(windows)]
    {
        let extension = executable
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command = Command::new("cmd.exe");
            command.arg("/D").arg("/C").arg(executable);
            return command;
        }
    }
    Command::new(executable)
}

fn executable_candidates(directory: &Path, command: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ["exe", "cmd", "bat"]
            .into_iter()
            .map(|extension| directory.join(command).with_extension(extension))
            .chain(std::iter::once(directory.join(command)))
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![directory.join(command)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_unowned_environment() {
        let project = tempfile::tempdir().expect("project");
        let config = project.path().join("pinset.toml");
        fs::write(&config, "schema = 2\n[tools]\n").expect("config");
        fs::create_dir(project.path().join(PYTHON_ENVIRONMENT_DIR)).expect("venv");
        assert!(matches!(
            load_project_python_environment(&config, "3.14.7+20260807", "windows-x86_64"),
            Err(Error::PythonEnvironmentNotOwned { .. })
        ));
        assert!(matches!(
            create_project_python_environment(
                &config,
                Path::new("unused-python"),
                "3.14.7+20260807",
                "windows-x86_64",
                true,
            ),
            Err(Error::PythonEnvironmentNotOwned { .. })
        ));
    }

    #[test]
    fn validates_owned_environment_identity() {
        let project = tempfile::tempdir().expect("project");
        let config = project.path().join("pinset.toml");
        let root = project.path().join(PYTHON_ENVIRONMENT_DIR);
        let command_directory = if cfg!(windows) {
            root.join("Scripts")
        } else {
            root.join("bin")
        };
        fs::create_dir_all(&command_directory).expect("commands");
        fs::write(root.join("pyvenv.cfg"), "home = test\n").expect("pyvenv");
        fs::write(
            if cfg!(windows) {
                command_directory.join("python.exe")
            } else {
                command_directory.join("python")
            },
            "test",
        )
        .expect("python");
        fs::write(
            root.join(PYTHON_ENVIRONMENT_MARKER),
            "schema = 1\ndistribution = \"3.14.7+20260807\"\ntarget = \"windows-x86_64\"\n",
        )
        .expect("marker");
        let environment =
            load_project_python_environment(&config, "3.14.7+20260807", "windows-x86_64")
                .expect("environment");
        assert_eq!(environment.root, root);
        assert!(matches!(
            load_project_python_environment(&config, "3.14.7+20260808", "windows-x86_64"),
            Err(Error::PythonEnvironmentMismatch { .. })
        ));
    }

    #[test]
    fn creates_reuses_and_explicitly_recreates_an_owned_environment() {
        let project = tempfile::tempdir().expect("project");
        let config = project.path().join("pinset.toml");
        fs::write(&config, "schema = 2\n[tools]\n").expect("config");
        let base = fake_base_python(project.path());
        let target = if cfg!(windows) {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        let environment =
            create_project_python_environment(&config, &base, "3.14.7+20260807", target, false)
                .expect("create");
        let sentinel = environment.root.join("sentinel");
        fs::write(&sentinel, "preserved").expect("sentinel");

        create_project_python_environment(&config, &base, "3.14.7+20260807", target, false)
            .expect("reuse");
        assert!(sentinel.is_file());

        create_project_python_environment(&config, &base, "3.14.7+20260807", target, true)
            .expect("recreate");
        assert!(!sentinel.exists());
    }

    #[test]
    fn creates_named_environment_with_distinct_marker_and_path() {
        let project = tempfile::tempdir().expect("project");
        let config = project.path().join("pinset.toml");
        let base = fake_base_python(project.path());
        let target = if cfg!(windows) {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        let environment = create_project_python_environment_for(
            &config,
            "docs",
            ".venv-docs",
            &base,
            "3.14.7+20260807",
            target,
            false,
        )
        .expect("named environment");
        assert_eq!(environment.name, "docs");
        assert_eq!(environment.root, project.path().join(".venv-docs"));
        assert!(!project.path().join(PYTHON_ENVIRONMENT_DIR).exists());
        let marker =
            fs::read_to_string(environment.root.join(PYTHON_ENVIRONMENT_MARKER)).expect("marker");
        assert!(marker.contains("schema = 2"));
        assert!(marker.contains("environment = \"docs\""));
        assert!(
            load_project_python_environment_for(
                &config,
                "other",
                ".venv-docs",
                "3.14.7+20260807",
                target,
            )
            .is_err()
        );
    }

    #[cfg(windows)]
    fn fake_base_python(directory: &Path) -> PathBuf {
        let path = directory.join("python.cmd");
        fs::write(
            &path,
            "@echo off\r\nmkdir \"%~3\\Scripts\"\r\necho home = pinset-test>\"%~3\\pyvenv.cfg\"\r\ncopy /Y \"%ComSpec%\" \"%~3\\Scripts\\python.exe\" >nul\r\n",
        )
        .expect("fake Python");
        path
    }

    #[cfg(unix)]
    fn fake_base_python(directory: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("python");
        fs::write(
            &path,
            "#!/bin/sh\nroot=\"$3\"\nmkdir -p \"$root/bin\"\nprintf 'home = pinset-test\\n' > \"$root/pyvenv.cfg\"\nprintf '#!/bin/sh\\nexit 0\\n' > \"$root/bin/python\"\nchmod +x \"$root/bin/python\"\n",
        )
        .expect("fake Python");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
        path
    }
}
