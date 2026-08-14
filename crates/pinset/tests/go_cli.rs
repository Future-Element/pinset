use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

#[test]
fn resolves_lists_and_executes_a_managed_go_sdk() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n\n[tools]\ngo = \"1.25.1\"\n",
    )
    .expect("config");

    let install_dir = home
        .join("installs")
        .join("go")
        .join("1.25.1")
        .join(pinset_core::current_target_for_tool("go"));
    let bin_dir = install_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("Go bin");
    write_go_command(&bin_dir, &install_dir);
    write_gofmt_command(&bin_dir);
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"go\"\nversion = \"1.25.1\"\ntarget = \"{}\"\n",
            pinset_core::current_target_for_tool("go")
        ),
    )
    .expect("receipt");

    assert_success_contains(&pinset(&project, &home, &["list", "go"]), "go@1.25.1");
    assert_success_contains(
        &pinset(&project, &home, &["current", "go"]),
        "go 1.25.1 installed",
    );
    let which = pinset(&project, &home, &["which", "go"]);
    assert!(
        which.status.success()
            && String::from_utf8_lossy(&which.stdout).contains(&bin_dir.display().to_string()),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&which.stdout),
        String::from_utf8_lossy(&which.stderr)
    );

    let executed = pinset(&project, &home, &["exec", "--", "go", "version"]);
    assert_success_contains(&executed, "go version go1.25.1");
    assert_success_contains(&executed, &format!("GOROOT={}", install_dir.display()));
    assert_success_contains(&executed, "GOTOOLCHAIN=local");

    let explicit_policy = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "en")
        .env("GOTOOLCHAIN", "path")
        .args(["exec", "--", "go", "version"])
        .output()
        .expect("run with explicit Go toolchain policy");
    assert_success_contains(&explicit_policy, "GOTOOLCHAIN=path");

    let gofmt = pinset(&project, &home, &["exec", "--", "gofmt", "fixture.go"]);
    assert_success_contains(&gofmt, "fake-gofmt fixture.go");
}

#[cfg(windows)]
fn write_go_command(directory: &Path, install_dir: &Path) {
    fs::write(
        directory.join("go.cmd"),
        format!(
            "@echo off\r\necho go version go1.25.1\r\necho GOROOT=%GOROOT%\r\necho GOTOOLCHAIN=%GOTOOLCHAIN%\r\nrem {}\r\n",
            install_dir.display()
        ),
    )
    .expect("go command");
}

#[cfg(unix)]
fn write_go_command(directory: &Path, _install_dir: &Path) {
    write_unix_command(
        &directory.join("go"),
        "#!/bin/sh\nprintf 'go version go1.25.1\\nGOROOT=%s\\nGOTOOLCHAIN=%s\\n' \"$GOROOT\" \"$GOTOOLCHAIN\"\n",
    );
}

#[cfg(windows)]
fn write_gofmt_command(directory: &Path) {
    fs::write(
        directory.join("gofmt.cmd"),
        "@echo off\r\necho fake-gofmt %1\r\n",
    )
    .expect("gofmt command");
}

#[cfg(unix)]
fn write_gofmt_command(directory: &Path) {
    write_unix_command(
        &directory.join("gofmt"),
        "#!/bin/sh\nprintf 'fake-gofmt %s\\n' \"$1\"\n",
    );
}

#[cfg(unix)]
fn write_unix_command(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("command");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .env_remove("GOROOT")
        .env_remove("GOTOOLCHAIN")
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
