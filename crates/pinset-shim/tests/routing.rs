use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use pinset_core::{current_target, install_shims};
use tempfile::tempdir;

#[test]
fn executes_fake_node_selected_by_nearest_project_config() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let nested = project.join("packages").join("app").join("src");
    let home = root.path().join("home");
    fs::create_dir_all(&nested).expect("nested directory");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&nested)
        .args(["--", "hello", "pinset"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("20.0.0:hello pinset"), "stdout: {stdout}");
    assert!(stdout.contains("source=project"), "stdout: {stdout}");
}

#[test]
fn executes_fake_node_selected_by_global_config_without_a_project() {
    let root = tempdir().expect("temp directory");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(home.join("state")).expect("global state");
    fs::write(
        home.join("state").join("global.toml"),
        "schema = 1\n[tools]\nnode = \"24.0.0\"\n",
    )
    .expect("global config");
    create_fake_node(&home, "24.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&workspace)
        .args(["--", "hello", "global"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("24.0.0:hello global"), "stdout: {stdout}");
    assert!(stdout.contains("source=global"), "stdout: {stdout}");
}

#[test]
fn rejects_recursive_shim_invocation() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node"])
        .env("PINSET_SHIM_DEPTH", "1")
        .output()
        .expect("run shim");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("recursive shim invocation"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preserves_the_runtime_exit_code() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let status = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "exit42"])
        .env("PINSET_HOME", &home)
        .status()
        .expect("run shim");

    assert_eq!(status.code(), Some(42));
}

#[test]
fn executes_through_an_installed_multicall_shim_name() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let shims = home.join("shims");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"22.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "22.0.0");

    let installed = install_shims(
        Path::new(env!("CARGO_BIN_EXE_pinset-shim")),
        &shims,
        &["node".to_owned()],
    )
    .expect("install shim");
    let node_shim = &installed[0].destination;
    let output = Command::new(node_shim)
        .arg("multicall")
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .output()
        .expect("run installed shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("22.0.0:multicall"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn create_fake_node(home: &Path, version: &str) -> PathBuf {
    let install_dir = home
        .join("installs")
        .join("node")
        .join(version)
        .join(current_target());
    let bin = if cfg!(windows) {
        install_dir
    } else {
        install_dir.join("bin")
    };
    fs::create_dir_all(&bin).expect("runtime bin");

    #[cfg(windows)]
    {
        let executable = bin.join("node.cmd");
        fs::write(
            &executable,
            "@echo off\r\nif \"%1\"==\"exit42\" exit /b 42\r\necho %PINSET_SELECTED_VERSION%:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
        )
        .expect("fake node");
        executable
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = bin.join("node");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"exit42\" ]; then exit 42; fi\nprintf '%s:%s\\nsource=%s\\n' \"$PINSET_SELECTED_VERSION\" \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
        )
        .expect("fake node");
        let mut permissions = fs::metadata(&executable)
            .expect("fake node metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake node permissions");
        executable
    }
}
