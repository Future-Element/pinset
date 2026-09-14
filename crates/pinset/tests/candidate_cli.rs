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
            r#"schema = 6
project-id = "4c5652e4-0000-4000-8000-000000000041"

[verification]
tasks = ["setup", "smoke"]

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
        .args(["candidate", "apply", "--allow-limited", "--json"])
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
        "stdout={} stderr={}",
        String::from_utf8_lossy(&restored.stdout),
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
            r#"schema = 6
project-id = "4c5652e4-0000-4000-8000-000000000047"

[verification]
tasks = ["fail"]

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

fn fixture(
    task_command: Vec<String>,
    extras: &str,
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temporary = tempdir().expect("candidate fixture");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir_all(&project).unwrap();
    let config = format!(
        "schema = 6\nproject-id = \"4c5652e4-0000-4000-8000-000000000048\"\n[tools]\n[verification]\ntasks = [\"verify\"]\n{extras}\n[tasks.verify]\ncommand = {}\n",
        serde_json::to_string(&task_command).unwrap()
    );
    fs::write(project.join("pinset.toml"), config).unwrap();
    fs::write(
        project.join("pinset.lock"),
        "schema = 4\ngenerated_by = \"fixture\"\ntool = []\n",
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project)
            .status()
            .unwrap()
            .success()
    );
    (temporary, project, home)
}

fn success(project: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    let output = pinset_command(project, home).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "args={args:?} stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn content_changes_in_an_already_dirty_worktree_invalidate_the_passing_result() {
    let (_temporary, project, home) = fixture(
        vec![env!("CARGO_BIN_EXE_pinset").into(), "--version".into()],
        "inputs = [\"ignored.txt\"]",
    );
    fs::write(project.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(project.join("ignored.txt"), "ignored-first").unwrap();
    fs::write(project.join("source.txt"), "dirty-first").unwrap();
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    success(
        &project,
        &home,
        &["candidate", "test", "verify", "--compare"],
    );
    let status = success(&project, &home, &["candidate", "status", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        report["data"][0]["candidate"]["tests"][0]["evidence"]["current_exit_code"],
        0
    );
    let before = fs::read(project.join("pinset.lock")).unwrap();
    success(&project, &home, &["candidate", "apply", "--plan", "--json"]);
    assert_eq!(fs::read(project.join("pinset.lock")).unwrap(), before);
    for (file, value) in [
        ("source.txt", "dirty-other"),
        ("ignored.txt", "ignored-other"),
    ] {
        fs::write(project.join(file), value).unwrap();
        let rejected = pinset_command(&project, &home)
            .args(["candidate", "apply", "--allow-limited"])
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("input content"));
        assert_eq!(fs::read(project.join("pinset.lock")).unwrap(), before);
        success(&project, &home, &["candidate", "test", "verify"]);
    }
    let limited = pinset_command(&project, &home)
        .args(["candidate", "apply"])
        .output()
        .unwrap();
    assert!(!limited.status.success());
    assert!(String::from_utf8_lossy(&limited.stderr).contains("limited verification"));
    success(&project, &home, &["candidate", "apply", "--allow-limited"]);
}

#[test]
fn candidate_snapshot_mutations_do_not_touch_original_inputs_or_the_other_comparison() {
    let command: Vec<String> = if cfg!(windows) {
        vec!["cmd.exe", "/D", "/C", "if exist generated.txt (exit /b 7) else (echo changed>generated.txt & echo changed>source.txt & echo isolated)"]
    } else {
        vec!["sh", "-c", "test ! -e generated.txt || exit 7; printf changed > generated.txt; printf changed > source.txt; printf isolated"]
    }.into_iter().map(str::to_owned).collect();
    let (_temporary, project, home) = fixture(command, "");
    fs::write(project.join("source.txt"), "original").unwrap();
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    let tested = success(
        &project,
        &home,
        &["candidate", "test", "verify", "--compare"],
    );
    assert_eq!(
        String::from_utf8_lossy(&tested.stdout)
            .matches("isolated")
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(project.join("source.txt")).unwrap(),
        "original"
    );
    assert!(!project.join("generated.txt").exists());
}

#[test]
fn additional_argument_values_are_never_persisted_in_candidate_evidence() {
    let (_temporary, project, home) = fixture(
        vec![env!("CARGO_BIN_EXE_pinset").into(), "--version".into()],
        "",
    );
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    let _ = pinset_command(&project, &home)
        .args([
            "candidate",
            "test",
            "verify",
            "--",
            "candidate-secret-sentinel-39843",
        ])
        .output()
        .unwrap();
    let status = success(&project, &home, &["candidate", "status", "--json"]);
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(!text.contains("candidate-secret-sentinel-39843"));
    assert!(text.contains("<redacted>"));
}

#[cfg(unix)]
#[test]
fn timed_out_tasks_cannot_leave_a_descendant_writing_after_snapshot_cleanup() {
    let (_temporary, project, home) = fixture(
        vec![
            "sh".into(),
            "-c".into(),
            "(sleep 3; printf leaked > \"$1\") & wait".into(),
            "worker".into(),
        ],
        "timeout-seconds = 1\nexternal-state = true",
    );
    let sentinel = home.with_file_name("descendant-output");
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    let started = std::time::Instant::now();
    let tested = pinset_command(&project, &home)
        .args(["candidate", "test", "verify", "--"])
        .arg(&sentinel)
        .output()
        .unwrap();
    assert_eq!(tested.status.code(), Some(124));
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(!sentinel.exists());
    let rejected = pinset_command(&project, &home)
        .args(["candidate", "apply", "--allow-limited"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
}

#[cfg(unix)]
#[test]
fn canceled_validation_records_failure_and_stops_the_entire_task_group() {
    let (_temporary, project, home) = fixture(
        vec![
            "sh".into(),
            "-c".into(),
            "printf started > \"$1\"; (sleep 3; printf leaked > \"$2\") & wait".into(),
            "worker".into(),
        ],
        "external-state = true",
    );
    let started = home.with_file_name("started");
    let sentinel = home.with_file_name("cancel-descendant-output");
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    let mut process = pinset_command(&project, &home)
        .args(["candidate", "test", "verify", "--"])
        .arg(&started)
        .arg(&sentinel)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !started.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(started.exists());
    assert!(
        Command::new("kill")
            .args(["-TERM", &process.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(process.wait().unwrap().code(), Some(130));
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(!sentinel.exists());
    let status = success(&project, &home, &["candidate", "status", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(report["data"][0]["candidate"]["tests"][0]["exit_code"], 130);
}

#[cfg(windows)]
#[test]
fn windows_job_terminates_the_task_before_it_can_write_after_timeout() {
    let (_temporary, project, home) = fixture(
        vec![
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            "exit 0".into(),
        ],
        "timeout-seconds = 1\nexternal-state = true",
    );
    let sentinel = home.with_file_name("windows-descendant-output");
    let path = project.join("pinset.toml");
    let mut config = pinset_core::load_project_config(&path).unwrap();
    *config
        .tasks
        .get_mut("verify")
        .unwrap()
        .command
        .last_mut()
        .unwrap() = format!(
        "Start-Sleep -Seconds 4; Set-Content -LiteralPath '{}' -Value leaked",
        sentinel.to_string_lossy().replace('\'', "''")
    );
    pinset_core::save_project_config(&path, &config).unwrap();
    success(&project, &home, &["candidate", "prepare", "--no-install"]);
    let tested = pinset_command(&project, &home)
        .args(["candidate", "test", "verify"])
        .output()
        .unwrap();
    assert_eq!(
        tested.status.code(),
        Some(124),
        "{}",
        String::from_utf8_lossy(&tested.stderr)
    );
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(!sentinel.exists());
}
