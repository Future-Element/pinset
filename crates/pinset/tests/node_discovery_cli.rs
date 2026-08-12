use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn lists_only_complete_local_node_installations_without_network_access() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).expect("workspace");

    create_install_receipt(&home, "22.12.0", "linux-x86_64");
    create_install_receipt(&home, "24.1.0", "windows-x86_64");
    create_install_receipt(&home, "24.1.0", "linux-x86_64");
    fs::create_dir_all(home.join("installs/node/25.0.0/linux-x86_64")).expect("incomplete install");
    fs::create_dir_all(home.join("installs/node/not-a-version/linux-x86_64"))
        .expect("invalid install");

    let output = pinset(&workspace, &home, &["list", "node"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("24.1.0 installed targets=linux-x86_64,windows-x86_64"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("22.12.0 installed targets=linux-x86_64"));
    assert!(!stdout.contains("25.0.0"));
    assert!(!stdout.contains("not-a-version"));
}

#[test]
fn local_node_listing_and_help_are_localized() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).expect("workspace");

    let empty = pinset(&workspace, &home, &["--lang", "zh-CN", "list", "node"]);
    assert_success_contains(&empty, "尚未安装任何 Node.js 版本");

    let help = pinset(&workspace, &home, &["--lang", "zh-CN", "list", "--help"]);
    assert_success_contains(&help, "列出本机已安装或官方可用的运行时版本");

    let unsupported = pinset(&workspace, &home, &["--lang", "zh-CN", "list", "python"]);
    assert!(!unsupported.status.success());
    assert!(stderr(&unsupported).contains("python"));
}

fn create_install_receipt(home: &Path, version: &str, target: &str) {
    let directory = home
        .join("installs")
        .join("node")
        .join(version)
        .join(target);
    fs::create_dir_all(&directory).expect("install directory");
    fs::write(
        directory.join(".pinset-install.toml"),
        format!(
            "schema = 1\ncomplete = true\ntool = \"node\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
        ),
    )
    .expect("install receipt");
}

fn pinset(cwd: &Path, home: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(cwd)
        .env("PINSET_HOME", home)
        .env_remove("PINSET_LANG")
        .output()
        .expect("run pinset")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success_contains(output: &std::process::Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
