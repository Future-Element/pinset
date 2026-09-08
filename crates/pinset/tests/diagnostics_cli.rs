use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

#[test]
fn status_saves_a_redacted_portable_report_and_check_detects_changes() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home-private-location");
    let report = root.path().join("diagnostic.json");
    fs::create_dir(&project).expect("project");
    write_project(&project, false);
    fs::write(
        project.join("dev.age"),
        "DATABASE_URL=postgres://diagnostic-secret-value\n",
    )
    .expect("secret fixture");

    let output = pinset(
        &project,
        &home,
        &[
            "status",
            "--json",
            "--repair-preview",
            "--save",
            path_text(&report),
        ],
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    let saved = fs::read_to_string(&report).expect("saved report");
    for forbidden in [
        path_text(&project),
        path_text(&home),
        "diagnostic-secret-value",
    ] {
        assert!(!stdout.contains(forbidden), "stdout leaked {forbidden:?}");
        assert!(
            !saved.contains(forbidden),
            "saved report leaked {forbidden:?}"
        );
    }
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("status JSON");
    assert_eq!(json["schema"], 1);
    assert_eq!(json["command"], "status");
    assert_eq!(json["data"]["report"]["schema"], 1);
    assert_eq!(json["data"]["report"]["environment"]["secret_variables"], 1);
    assert!(json["data"]["report"]["repair_preview"].is_array());

    write_project(&project, true);
    let changed = pinset(
        &project,
        &home,
        &["check", "--json", "--compare", path_text(&report)],
    );
    assert_eq!(changed.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&changed.stdout).expect("check JSON");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["comparison"]["changed"], true);
    let encoded = String::from_utf8(changed.stdout).expect("UTF-8 changed report");
    assert!(encoded.contains("/project/tasks"));
    assert!(!encoded.contains("diagnostic-secret-value"));
}

#[test]
fn status_is_non_strict_while_check_uses_exit_one_for_findings() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("empty");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");

    let status = pinset(&project, &home, &["status", "--json"]);
    assert!(status.status.success());
    let check = pinset(&project, &home, &["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).expect("check JSON");
    assert_eq!(json["data"]["report"]["summary"]["passed"], false);
}

#[test]
fn compare_rejects_unknown_or_oversized_report_formats() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    let invalid = root.path().join("invalid.json");
    fs::write(&invalid, r#"{"schema":2}"#).expect("invalid report");
    let output = pinset(
        &project,
        &home,
        &["status", "--compare", path_text(&invalid)],
    );
    assert_eq!(output.status.code(), Some(2));
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(message.contains("missing field") || message.contains("diagnostic report"));

    let oversized = root.path().join("oversized.json");
    fs::write(&oversized, vec![b'x'; 1024 * 1024 + 1]).expect("oversized report");
    let output = pinset(
        &project,
        &home,
        &["status", "--compare", path_text(&oversized)],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("at most 1 MiB"));
}

fn write_project(project: &Path, with_task: bool) {
    let task = if with_task {
        "\n[tasks.test]\ncommand = [\"echo\", \"ok\"]\n"
    } else {
        ""
    };
    fs::write(
        project.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "565652e4-0000-4000-8000-000000000024"

[policy]
inherit-global = false
system-fallback = false
boundary = "git"

[environment]
auto-profile = "dev"

[environment.profiles.dev]
file = "dev.age"
recipients = ["age1diagnostic"]

[environment.variables.DATABASE_URL]
type = "url"
required = true
secret = true
{task}"#
        ),
    )
    .expect("project config");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinset"));
    command
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en");
    for name in [
        "PINSET_ENV_PROFILE",
        "CI",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
    ] {
        command.env_remove(name);
    }
    command.output().expect("run pinset")
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}
