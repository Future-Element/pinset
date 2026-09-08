use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn candidate_prepare_test_apply_history_and_restore_form_a_closed_workflow() {
    let temporary = tempdir().expect("candidate project");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir_all(&project).expect("project");
    let pinset = env!("CARGO_BIN_EXE_pinset").replace('\\', "\\\\");
    fs::write(
        project.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000041"

[tools]

[tasks.setup]
command = ["{pinset}", "--version"]

[tasks.smoke]
command = ["{pinset}", "--version"]
depends-on = ["setup"]
"#
        ),
    )
    .expect("config");
    fs::write(
        project.join("pinset.lock"),
        "schema = 4\ngenerated_by = \"pinset test\"\ntool = []\n",
    )
    .expect("lock");

    let prepared = pinset_command(&project, &home)
        .args(["candidate", "prepare", "--json"])
        .output()
        .expect("prepare");
    assert!(
        prepared.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&prepared.stdout),
        String::from_utf8_lossy(&prepared.stderr)
    );
    let prepared: serde_json::Value =
        serde_json::from_slice(&prepared.stdout).expect("prepare JSON");
    assert_eq!(prepared["command"], "candidate.prepare");

    let tested = pinset_command(&project, &home)
        .args(["candidate", "test", "smoke"])
        .output()
        .expect("test");
    assert!(
        tested.status.success(),
        "{}",
        String::from_utf8_lossy(&tested.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&tested.stdout)
            .matches(pinset_core::pinset_version())
            .count(),
        2
    );

    let applied = pinset_command(&project, &home)
        .args(["candidate", "apply", "--json"])
        .output()
        .expect("apply");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).expect("apply JSON");
    let history_id = applied["data"][0]["id"]
        .as_str()
        .expect("history id")
        .to_owned();

    let history = pinset_command(&project, &home)
        .args(["candidate", "history", "--json"])
        .output()
        .expect("history");
    assert!(history.status.success());
    let history: serde_json::Value = serde_json::from_slice(&history.stdout).expect("history JSON");
    assert_eq!(history["data"][0]["id"], history_id);

    let restored = pinset_command(&project, &home)
        .args(["candidate", "restore", &history_id, "--json"])
        .output()
        .expect("restore");
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: serde_json::Value =
        serde_json::from_slice(&restored.stdout).expect("restore JSON");
    assert!(
        restored["data"]["candidate_id"]
            .as_str()
            .expect("restore candidate")
            .starts_with("restore-")
    );
}

#[test]
fn failed_candidate_test_blocks_apply_without_changing_the_lock() {
    let temporary = tempdir().expect("candidate project");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir_all(&project).expect("project");
    let pinset = env!("CARGO_BIN_EXE_pinset").replace('\\', "\\\\");
    fs::write(
        project.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000047"

[tools]

[tasks.fail]
command = ["{pinset}", "definitely-not-a-command"]
"#
        ),
    )
    .expect("config");
    let lock = "schema = 4\ngenerated_by = \"pinset test\"\ntool = []\n";
    fs::write(project.join("pinset.lock"), lock).expect("lock");
    assert!(
        pinset_command(&project, &home)
            .args(["candidate", "prepare", "--no-install"])
            .status()
            .expect("prepare")
            .success()
    );
    let tested = pinset_command(&project, &home)
        .args(["candidate", "test", "fail"])
        .status()
        .expect("test");
    assert!(!tested.success());
    let applied = pinset_command(&project, &home)
        .args(["candidate", "apply", "--json"])
        .output()
        .expect("apply");
    assert!(!applied.status.success());
    let report: serde_json::Value = serde_json::from_slice(&applied.stdout).expect("apply JSON");
    assert_eq!(report["ok"], false);
    assert!(
        report["error"]["message"]
            .as_str()
            .expect("message")
            .contains("no passing test")
    );
    assert_eq!(
        fs::read_to_string(project.join("pinset.lock")).expect("lock"),
        lock
    );
}

fn pinset_command(cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinset"));
    command
        .current_dir(cwd)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en");
    command
}
