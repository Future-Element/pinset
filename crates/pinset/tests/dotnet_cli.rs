use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

const DOTNET_VERSION: &str = "10.0.400";

#[test]
fn routes_the_managed_dotnet_sdk_and_preserves_user_cli_state() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!("schema = 2\n\n[tools]\ndotnet = \"{DOTNET_VERSION}\"\n"),
    )
    .expect("config");

    let install_dir = home
        .join("installs")
        .join("dotnet")
        .join(DOTNET_VERSION)
        .join(pinset_core::current_target_for_tool("dotnet"));
    fs::create_dir_all(&install_dir).expect(".NET SDK root");
    write_dotnet_command(&install_dir);
    write_receipt(&install_dir);

    assert_success_contains(
        &pinset(&project, &home, &["list", "dotnet"]),
        "dotnet@10.0.400",
    );
    assert_success_contains(
        &pinset(&project, &home, &["current", "dotnet"]),
        "dotnet 10.0.400 installed",
    );
    assert_success_contains(
        &pinset(&project, &home, &["which", "dotnet"]),
        &install_dir.display().to_string(),
    );
    let executed = pinset(&project, &home, &["exec", "--", "dotnet", "--info"]);
    assert_success_contains(&executed, "fake-dotnet --info");
    assert_success_contains(&executed, &format!("DOTNET_ROOT={}", install_dir.display()));
    assert_success_contains(&executed, "DOTNET_CLI_HOME=fixture-dotnet-home");
    assert_success_contains(&executed, "NUGET_PACKAGES=fixture-nuget-packages");
}

fn write_receipt(install_dir: &Path) {
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"dotnet\"\nversion = \"{DOTNET_VERSION}\"\ntarget = \"{}\"\n",
            pinset_core::current_target_for_tool("dotnet")
        ),
    )
    .expect("receipt");
}

#[cfg(windows)]
fn write_dotnet_command(directory: &Path) {
    fs::write(
        directory.join("dotnet.cmd"),
        "@echo off\r\necho fake-dotnet %*\r\necho DOTNET_ROOT=%DOTNET_ROOT%\r\necho DOTNET_CLI_HOME=%DOTNET_CLI_HOME%\r\necho NUGET_PACKAGES=%NUGET_PACKAGES%\r\n",
    )
    .expect("dotnet command");
}

#[cfg(unix)]
fn write_dotnet_command(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("dotnet");
    fs::write(
        &path,
        "#!/bin/sh\nprintf 'fake-dotnet %s\nDOTNET_ROOT=%s\nDOTNET_CLI_HOME=%s\nNUGET_PACKAGES=%s\n' \"$*\" \"$DOTNET_ROOT\" \"$DOTNET_CLI_HOME\" \"$NUGET_PACKAGES\"\n",
    )
    .expect("dotnet command");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .env("DOTNET_ROOT", "system-dotnet-root")
        .env("DOTNET_CLI_HOME", "fixture-dotnet-home")
        .env("NUGET_PACKAGES", "fixture-nuget-packages")
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
