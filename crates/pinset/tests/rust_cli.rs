use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

const RUST_VERSION: &str = "1.97.1";

#[test]
fn routes_a_managed_rust_toolchain_without_taking_over_cargo_or_rustup_state() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!("schema = 2\n\n[tools]\nrust = \"{RUST_VERSION}\"\n"),
    )
    .expect("config");

    let install_dir = home
        .join("installs")
        .join("rust")
        .join(RUST_VERSION)
        .join(pinset_core::current_target_for_tool("rust"));
    let bin = install_dir.join("bin");
    fs::create_dir_all(&bin).expect("Rust bin");
    write_rustc_command(&bin);
    write_cargo_command(&bin);
    write_receipt(&install_dir);

    assert_success_contains(&pinset(&project, &home, &["list", "rust"]), "rust@1.97.1");
    assert_success_contains(
        &pinset(&project, &home, &["current", "rustc"]),
        "rust 1.97.1 installed",
    );
    assert_success_contains(
        &pinset(&project, &home, &["which", "cargo"]),
        &bin.display().to_string(),
    );
    assert_success_contains(
        &pinset(&project, &home, &["exec", "--", "rustc", "--version"]),
        "rustc 1.97.1",
    );

    let cargo = pinset(&project, &home, &["exec", "--", "cargo", "check"]);
    assert_success_contains(&cargo, "fake-cargo check");
    assert_success_contains(&cargo, "CARGO_HOME=fixture-cargo-home");
    assert_success_contains(&cargo, "RUSTUP_HOME=fixture-rustup-home");
    assert_success_contains(&cargo, "RUSTFLAGS=-Cdebuginfo=1");
    assert_success_contains(&cargo, &bin.display().to_string());
}

fn write_receipt(install_dir: &Path) {
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"rust\"\nversion = \"{RUST_VERSION}\"\ntarget = \"{}\"\n",
            pinset_core::current_target_for_tool("rust")
        ),
    )
    .expect("receipt");
}

#[cfg(windows)]
fn write_rustc_command(directory: &Path) {
    fs::write(
        directory.join("rustc.cmd"),
        "@echo off\r\necho rustc 1.97.1\r\n",
    )
    .expect("rustc command");
}

#[cfg(unix)]
fn write_rustc_command(directory: &Path) {
    write_unix_command(&directory.join("rustc"), "#!/bin/sh\necho 'rustc 1.97.1'\n");
}

#[cfg(windows)]
fn write_cargo_command(directory: &Path) {
    fs::write(
        directory.join("cargo.cmd"),
        "@echo off\r\necho fake-cargo %*\r\necho CARGO_HOME=%CARGO_HOME%\r\necho RUSTUP_HOME=%RUSTUP_HOME%\r\necho RUSTFLAGS=%RUSTFLAGS%\r\nwhere rustc\r\n",
    )
    .expect("cargo command");
}

#[cfg(unix)]
fn write_cargo_command(directory: &Path) {
    write_unix_command(
        &directory.join("cargo"),
        "#!/bin/sh\nprintf 'fake-cargo %s\\nCARGO_HOME=%s\\nRUSTUP_HOME=%s\\nRUSTFLAGS=%s\\n' \"$*\" \"$CARGO_HOME\" \"$RUSTUP_HOME\" \"$RUSTFLAGS\"\ncommand -v rustc\n",
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
        .env("CARGO_HOME", "fixture-cargo-home")
        .env("RUSTUP_HOME", "fixture-rustup-home")
        .env("RUSTFLAGS", "-Cdebuginfo=1")
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
