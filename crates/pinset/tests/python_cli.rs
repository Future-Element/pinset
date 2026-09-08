use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

use pinset_core::{
    LockedArtifact, LockedArtifactFormat, LockedTool, Lockfile, PYTHON_TARGETS, SourceConfig,
    plan_python_artifact, save_lockfile,
};
use tempfile::tempdir;

const DISTRIBUTION: &str = "3.14.7+20260807";

#[test]
fn routes_python_and_project_scripts_through_the_owned_environment_without_activation() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!("schema = 2\n\n[tools]\npython = \"{DISTRIBUTION}\"\n"),
    )
    .expect("config");
    let environment = create_fake_environment(&project);

    let status = pinset(&project, &home, &["venv", "status"]);
    assert_success_contains(&status, &format!("python@{DISTRIBUTION}"));
    assert_success_contains(&status, &environment.display().to_string());

    let which = pinset(&project, &home, &["which", "python"]);
    assert_success_contains(
        &which,
        &python_executable(&environment).display().to_string(),
    );
    let pip = pinset(&project, &home, &["which", "pip"]);
    assert_success_contains(&pip, &python_executable(&environment).display().to_string());

    for arguments in [
        vec!["exec", "--", "pytest", "tests/unit"],
        vec!["--", "pytest", "tests/unit"],
    ] {
        let executed = pinset(&project, &home, &arguments);
        assert_success_contains(&executed, "fake-pytest tests/unit");
        let stdout = String::from_utf8_lossy(&executed.stdout);
        let virtual_env = stdout
            .lines()
            .find_map(|line| line.strip_prefix("VIRTUAL_ENV="))
            .expect("virtual environment");
        assert_eq!(
            fs::canonicalize(virtual_env).unwrap(),
            fs::canonicalize(&environment).unwrap()
        );
        assert_success_contains(&executed, "PYTHONHOME=");
        assert!(!String::from_utf8_lossy(&executed.stdout).contains("PYTHONHOME=must-be-removed"));
    }

    let doctor = pinset(&project, &home, &["doctor", "--json"]);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor JSON");
    assert_eq!(report["data"]["python_environment"]["status"], "ok");
    assert_eq!(
        fs::canonicalize(
            report["data"]["python_environment"]["path"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        fs::canonicalize(&environment).expect("canonical environment")
    );
}

#[test]
fn refuses_to_adopt_an_unmarked_dot_venv() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!("schema = 2\n\n[tools]\npython = \"{DISTRIBUTION}\"\n"),
    )
    .expect("config");
    fs::create_dir(project.join(".venv")).expect("external venv");

    let status = pinset(&project, &home, &["venv", "status"]);
    assert!(!status.status.success());
    assert!(
        String::from_utf8_lossy(&status.stderr).contains("is not owned by Pinset"),
        "stderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let doctor = pinset(&project, &home, &["doctor", "--json"]);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor JSON");
    assert_eq!(report["data"]["python_environment"]["status"], "invalid");
}

#[test]
fn named_environment_isolated_and_bound_to_declared_task() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000027"

[tools]
python = "{DISTRIBUTION}"

[python.environments.test]
path = ".venv-test"

[tasks.test]
command = ["pytest", "suite"]
python-environment = "test"
"#
        ),
    )
    .expect("config");
    write_python_lock(&project);
    let named_environment = create_fake_named_environment(&project, "test", ".venv-test");
    assert!(!project.join(".venv").exists());

    let status = pinset(&project, &home, &["venv", "status", "test"]);
    assert_success_contains(&status, "project environment test");
    assert_success_contains(&status, &named_environment.display().to_string());

    let executed = pinset(&project, &home, &["run", "test"]);
    assert_success_contains(&executed, "fake-pytest suite");
    let stdout = String::from_utf8_lossy(&executed.stdout);
    let virtual_env = stdout
        .lines()
        .find_map(|line| line.strip_prefix("VIRTUAL_ENV="))
        .expect("virtual environment");
    assert_eq!(
        fs::canonicalize(virtual_env).expect("task environment"),
        fs::canonicalize(&named_environment).expect("named environment")
    );
}

fn write_python_lock(project: &Path) {
    let artifacts = PYTHON_TARGETS
        .into_iter()
        .map(|target| {
            let plan = plan_python_artifact(&SourceConfig::default(), DISTRIBUTION, target)
                .expect("Python artifact plan");
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
        })
        .collect();
    save_lockfile(
        &project.join("pinset.lock"),
        &Lockfile {
            schema: pinset_core::LOCKFILE_SCHEMA,
            generated_by: format!("pinset {}", pinset_core::pinset_version()),
            tools: vec![LockedTool {
                name: "python".to_owned(),
                requested: DISTRIBUTION.to_owned(),
                version: DISTRIBUTION.to_owned(),
                provider: "python-build-standalone".to_owned(),
                released_at: None,
                metadata: BTreeMap::from([
                    ("build_id".to_owned(), "20260807".to_owned()),
                    (
                        "distribution".to_owned(),
                        "astral-sh/python-build-standalone".to_owned(),
                    ),
                    ("python_version".to_owned(), "3.14.7".to_owned()),
                    ("variant".to_owned(), "install_only".to_owned()),
                ]),
                options: BTreeMap::new(),
                artifacts,
            }],
        },
    )
    .expect("Python lock");
}

fn create_fake_environment(project: &Path) -> std::path::PathBuf {
    create_fake_environment_at(project, "default", ".venv", 1)
}

fn create_fake_named_environment(
    project: &Path,
    name: &str,
    relative_path: &str,
) -> std::path::PathBuf {
    create_fake_environment_at(project, name, relative_path, 2)
}

fn create_fake_environment_at(
    project: &Path,
    name: &str,
    relative_path: &str,
    schema: u32,
) -> std::path::PathBuf {
    let root = project.join(relative_path);
    let commands = if cfg!(windows) {
        root.join("Scripts")
    } else {
        root.join("bin")
    };
    fs::create_dir_all(&commands).expect("commands");
    fs::write(root.join("pyvenv.cfg"), "home = pinset-test\n").expect("pyvenv");
    fs::write(
        root.join(".pinset-venv.toml"),
        if schema == 1 {
            format!(
                "schema = 1\ndistribution = \"{DISTRIBUTION}\"\ntarget = \"{}\"\n",
                pinset_core::current_target_for_tool("python")
            )
        } else {
            format!(
                "schema = 2\nenvironment = \"{name}\"\ndistribution = \"{DISTRIBUTION}\"\ntarget = \"{}\"\n",
                pinset_core::current_target_for_tool("python")
            )
        },
    )
    .expect("marker");
    write_python(&commands);
    write_pytest(&commands);
    root
}

#[cfg(windows)]
fn write_python(commands: &Path) {
    fs::copy(
        std::env::var_os("ComSpec").expect("ComSpec"),
        commands.join("python.exe"),
    )
    .expect("python executable");
}

#[cfg(unix)]
fn write_python(commands: &Path) {
    write_unix_command(&commands.join("python"), "#!/bin/sh\nexit 0\n");
}

#[cfg(windows)]
fn write_pytest(commands: &Path) {
    fs::write(
        commands.join("pytest.cmd"),
        "@echo off\r\necho fake-pytest %*\r\necho VIRTUAL_ENV=%VIRTUAL_ENV%\r\necho PYTHONHOME=%PYTHONHOME%\r\n",
    )
    .expect("pytest");
}

#[cfg(unix)]
fn write_pytest(commands: &Path) {
    write_unix_command(
        &commands.join("pytest"),
        "#!/bin/sh\nprintf 'fake-pytest %s\\nVIRTUAL_ENV=%s\\nPYTHONHOME=%s\\n' \"$*\" \"$VIRTUAL_ENV\" \"$PYTHONHOME\"\n",
    );
}

#[cfg(unix)]
fn write_unix_command(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body).expect("command");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn python_executable(environment: &Path) -> std::path::PathBuf {
    if cfg!(windows) {
        environment.join("Scripts").join("python.exe")
    } else {
        environment.join("bin").join("python")
    }
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .env("PYTHONHOME", "must-be-removed")
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn assert_success_contains(output: &Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
