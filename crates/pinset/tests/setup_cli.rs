use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn cli(root: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(root)
        .env("PINSET_HOME", home)
        .env_remove("PINSET_ENV_PROFILE")
        .env_remove("PINSET_ENV_DISABLE")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn setup_preview_is_read_only_and_never_executes_project_tasks() {
    let root = tempdir().unwrap();
    let home = root.path().join("absent-home");
    fs::write(
        root.path().join("pinset.toml"),
        "schema = 5\nproject-id = \"11111111-1111-4111-8111-111111111111\"\n[tools]\nnode = \"24.0.0\"\n[tasks.setup]\ncommand = [\"must-not-execute\"]\n",
    )
    .unwrap();
    let output = cli(root.path(), &home, &["setup", "--plan", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "setup");
    assert_eq!(value["data"]["tasks"][0], "setup");
    assert_eq!(value["data"]["steps"][0]["id"], "resolve");
    assert_eq!(value["data"]["environment"]["runtimes"][0]["requested"], "24.0.0");
    assert!(value["data"]["environment"]["runtimes"][0]["locked_version"].is_null());
    assert!(!home.exists());
    assert!(!root.path().join("pinset.lock").exists());
}

#[test]
fn preview_preserves_effective_profile_source_without_decryption() {
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    fs::write(root.path().join("pinset.toml"), "schema = 5\nproject-id = \"11111111-1111-4111-8111-111111111111\"\n[tools]\nnode = \"24.0.0\"\n[environment]\nauto-profile = \"dev\"\n[environment.profiles.dev]\nfile = \".pinset/dev.age\"\nrecipients = [\"age1example\"]\n").unwrap();
    let output = cli(root.path(), &home, &["setup", "--plan", "--json"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["profile"], "dev");
    assert!(value["data"]["requested_profile"].is_null());
    assert_eq!(value["data"]["environment"]["profile_source"], "project");
    assert!(!home.exists());
    let disabled = cli(root.path(), &home, &["--no-env", "setup", "--plan", "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert!(value["data"]["profile"].is_null());
    assert_eq!(value["data"]["environment"]["profile_source"], "disabled");
}

#[test]
fn offline_failure_is_resumable_but_changed_configuration_is_rejected() {
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("pinset.toml");
    fs::write(&config, "schema = 5\nproject-id = \"11111111-1111-4111-8111-111111111111\"\n[tools]\nnode = \"24.0.0\"\n").unwrap();
    let output = cli(
        root.path(),
        &home,
        &["setup", "--yes", "--offline", "--json"],
    );
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = value["data"]["run"]["id"].as_str().unwrap();
    assert_eq!(value["data"]["run"]["plan"]["steps"][0]["state"], "failed");
    fs::write(&config, "schema = 5\nproject-id = \"11111111-1111-4111-8111-111111111111\"\n[tools]\nnode = \"22.0.0\"\n").unwrap();
    let resumed = cli(
        root.path(),
        &home,
        &["setup", "--resume", id, "--yes", "--offline", "--json"],
    );
    assert_eq!(resumed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&resumed.stdout).contains("inputs changed"));
}

#[test]
fn reports_do_not_claim_execution_or_expose_local_paths() {
    let root = tempdir().unwrap();
    let home = root.path().join("private-home");
    fs::write(
        root.path().join("pinset.toml"),
        "schema = 5\nproject-id = \"11111111-1111-4111-8111-111111111111\"\n[tools]\nnode = \"24.0.0\"\n",
    )
    .unwrap();
    let output = cli(
        root.path(),
        &home,
        &["status", "--report-version", "2", "--json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["report"]["execution_verified"], false);
    assert_eq!(value["data"]["report"]["environment_ready"], false);
    assert!(value["data"]["report"]["project_root"].is_null());
    assert!(!home.exists());
}

#[test]
fn protocol_two_is_opt_in_and_preserves_protocol_one() {
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    for protocol in ["1", "2"] {
        let output = cli(
            root.path(),
            &home,
            &["editor", "context", "--protocol", protocol, "--json"],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["data"]["protocol_schema"].as_u64().unwrap(),
            protocol.parse::<u64>().unwrap()
        );
        assert_eq!(value["data"].get("descriptor").is_some(), protocol == "2");
    }
}
