use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn workspace_lists_explicit_members_and_runs_inherited_tasks() {
    let temporary = tempdir().expect("workspace");
    let root = temporary.path();
    let home = root.join("home");
    fs::create_dir(root.join(".git")).expect("git boundary");
    for member in ["apps/web", "services/api"] {
        fs::create_dir_all(root.join(member)).expect("member");
    }
    let pinset = env!("CARGO_BIN_EXE_pinset").replace('\\', "\\\\");
    fs::write(
        root.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "4c5652e4-0000-4000-8000-000000000030"

[workspace]
members = ["apps/web", "services/api"]

[tools]

[tasks.smoke]
command = ["{pinset}", "--version"]
"#
        ),
    )
    .expect("root config");
    for (index, member) in ["apps/web", "services/api"].into_iter().enumerate() {
        fs::write(
            root.join(member).join("pinset.toml"),
            format!(
                "schema = 5\nproject-id = \"4c5652e4-0000-4000-8000-00000000003{}\"\n\n[tools]\n",
                index + 1
            ),
        )
        .expect("member config");
    }

    let listed = pinset_command(root.join("apps/web").as_path(), &home)
        .args(["workspace", "members", "--json"])
        .output()
        .expect("workspace members");
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("JSON");
    assert_eq!(output["data"].as_array().expect("members").len(), 2);
    assert_eq!(output["data"][0]["member"], "apps/web");

    let ran = pinset_command(root, &home)
        .args(["workspace", "run", "smoke"])
        .output()
        .expect("workspace run");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(stdout.contains("workspace member apps/web: run smoke"));
    assert!(stdout.contains("workspace member services/api: run smoke"));
    assert_eq!(stdout.matches(pinset_core::pinset_version()).count(), 2);
}

fn pinset_command(cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinset"));
    command
        .current_dir(cwd)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en");
    command
}
