use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tempfile::tempdir;

#[test]
fn resolves_lists_and_executes_pnpm_and_bun_with_a_composite_path() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n\n[tools]\nbun = \"1.3.14\"\npnpm = \"11.21.0\"\n",
    )
    .expect("config");

    let pnpm_dir = install_dir(&home, "pnpm", "11.21.0");
    fs::create_dir_all(&pnpm_dir).expect("pnpm dir");
    write_command(&pnpm_dir, "pnpm", nested_bun_command());
    write_receipt(&pnpm_dir, "pnpm", "11.21.0");

    let bun_dir = install_dir(&home, "bun", "1.3.14").join("bin");
    fs::create_dir_all(&bun_dir).expect("bun dir");
    write_command(&bun_dir, "bun", &echo_command("fake-bun-1.3.14"));
    write_command(&bun_dir, "bunx", &echo_command("fake-bunx-1.3.14"));
    write_receipt(bun_dir.parent().expect("bun install root"), "bun", "1.3.14");

    let which_pnpm = pinset(&project, &home, &["which", "pnpm"]);
    assert_success_contains(&which_pnpm, "pnpm");
    let which_bunx = pinset(&project, &home, &["which", "bunx"]);
    assert_success_contains(&which_bunx, "bunx");

    let current_pnpm = pinset(&project, &home, &["current", "pnpm"]);
    assert_success_contains(&current_pnpm, "pnpm 11.21.0");
    let current_bun = pinset(&project, &home, &["current", "bun"]);
    assert_success_contains(&current_bun, "bun 1.3.14");

    let executed = pinset(&project, &home, &["exec", "--", "pnpm", "--version"]);
    assert_success_contains(&executed, "fake-bun-1.3.14");

    let pnpm_list = pinset(&project, &home, &["list", "pnpm"]);
    assert_success_contains(&pnpm_list, "pnpm@11.21.0");
    let bun_list = pinset(&project, &home, &["list", "bun"]);
    assert_success_contains(&bun_list, "bun@1.3.14");
}

fn install_dir(home: &Path, tool: &str, version: &str) -> PathBuf {
    home.join("installs")
        .join(tool)
        .join(version)
        .join(pinset_core::current_target_for_tool(tool))
}

fn write_receipt(directory: &Path, tool: &str, version: &str) {
    let target = pinset_core::current_target_for_tool(tool);
    fs::write(
        directory.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"{tool}\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
        ),
    )
    .expect("receipt");
}

#[cfg(windows)]
fn write_command(directory: &Path, command: &str, body: &str) {
    fs::write(directory.join(format!("{command}.cmd")), body).expect("command");
}

#[cfg(unix)]
fn write_command(directory: &Path, command: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(command);
    fs::write(&path, body).expect("command");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
}

#[cfg(windows)]
fn nested_bun_command() -> &'static str {
    "@echo off\r\nbun --version\r\n"
}

#[cfg(unix)]
fn nested_bun_command() -> &'static str {
    "#!/bin/sh\nexec bun --version\n"
}

#[cfg(windows)]
fn echo_command(value: &str) -> String {
    format!("@echo off\r\necho {value}\r\n")
}

#[cfg(unix)]
fn echo_command(value: &str) -> String {
    format!("#!/bin/sh\nprintf '%s\\n' '{value}'\n")
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn assert_success_contains(output: &Output, expected: &str) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected {expected:?}, stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
