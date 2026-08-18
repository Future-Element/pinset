use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::Value;
use tempfile::tempdir;

#[test]
fn detect_emits_a_stable_json_report_without_writes() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(project.join(".git")).expect("project");
    fs::write(project.join(".nvmrc"), "lts/*\n").expect("nvmrc");
    fs::write(
        project.join("package.json"),
        r#"{"packageManager":"pnpm@10.2.0","engines":{"node":">=22"}}"#,
    )
    .expect("package json");

    let output = pinset(&project, &home, &["detect", "--json"]);
    assert_success(&output);
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "detect");
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["can_import"], true);
    assert_eq!(
        value["data"]["target_config"],
        project.join("pinset.toml").display().to_string()
    );
    let findings = value["data"]["findings"].as_array().expect("findings");
    assert!(findings.iter().any(|finding| {
        finding["tool"] == "node"
            && finding["source"] == ".nvmrc"
            && finding["normalized"] == "lts"
            && finding["status"] == "ready"
    }));
    assert!(findings.iter().any(|finding| {
        finding["field"] == "engines.node" && finding["status"] == "informational"
    }));
    assert!(!project.join("pinset.toml").exists());
    assert!(!project.join("pinset.lock").exists());
    assert!(!home.exists());
}

#[test]
fn detect_localizes_human_output() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(project.join(".bun-version"), "1.2.3\n").expect("bun");
    fs::write(
        project.join("package.json"),
        r#"{"engines":{"node":">=22"}}"#,
    )
    .expect("package json");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "zh-CN")
        .args(["detect"])
        .output()
        .expect("run pinset");
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("传统版本配置扫描"), "stdout: {stdout}");
    assert!(stdout.contains("[可导入] bun 1.2.3"), "stdout: {stdout}");
    assert!(
        stdout.contains("可导入: 是") || stdout.contains("可导入：是"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("版本范围仅报告，不参与导入"),
        "stdout: {stdout}"
    );
}

#[test]
fn import_conflict_fails_before_network_or_writes() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(project.join(".git")).expect("project");
    fs::write(project.join(".nvmrc"), "22.0.0\n").expect("nvmrc");
    fs::write(project.join(".node-version"), "24.0.0\n").expect("node version");

    let output = pinset(&project, &home, &["import", "--no-install"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("[conflict]"));
    assert!(!project.join("pinset.toml").exists());
    assert!(!project.join("pinset.lock").exists());
    assert!(!home.exists());
}

#[test]
fn import_requires_a_valid_existing_pinset_state_before_resolution() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(project.join(".nvmrc"), "24.0.0\n").expect("nvmrc");
    let config = "schema = 2\n\n[tools]\nnode = \"22.0.0\"\n";
    fs::write(project.join("pinset.toml"), config).expect("config");

    let output = pinset(&project, &home, &["import", "--force", "--no-install"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no pinset.lock"));
    assert_eq!(
        fs::read_to_string(project.join("pinset.toml")).expect("config"),
        config
    );
    assert!(!project.join("pinset.lock").exists());
    assert!(!home.exists());
}

#[test]
fn import_with_only_constraints_is_a_zero_write_error() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("package.json"),
        r#"{"engines":{"node":">=22"}}"#,
    )
    .expect("package json");

    let output = pinset(&project, &home, &["import", "--no-install"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("[informational]"));
    assert!(!project.join("pinset.toml").exists());
    assert!(!project.join("pinset.lock").exists());
    assert!(!home.exists());
}

#[test]
fn detect_errors_keep_the_schema_one_failure_envelope() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    let missing = project.join("missing");

    let output = pinset(
        &project,
        &home,
        &[
            "detect",
            "--cwd",
            missing.to_str().expect("UTF-8 path"),
            "--json",
        ],
    );
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "detect");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "io_error");
    assert!(output.stderr.is_empty());
    assert!(!home.exists());
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

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
