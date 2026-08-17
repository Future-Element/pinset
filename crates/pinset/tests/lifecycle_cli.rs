use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn lists_all_installed_versions_and_current_rust_as_json() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n[tools]\nrust = \"1.97.0\"\n",
    )
    .expect("project config");
    create_install(&home, "rust", "1.97.0", "rustc");
    create_install(&home, "pnpm", "11.21.0", "pnpm");

    let listed = pinset(&project, &home, &["list", "--json"]);
    assert_success(&listed);
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("installed JSON");
    assert_eq!(listed["schema"], 1);
    assert_eq!(listed["command"], "list");
    assert_eq!(listed["data"]["versions"].as_array().expect("installed array").len(), 2);

    let current = pinset(&project, &home, &["current", "rust", "--json"]);
    assert_success(&current);
    let current: serde_json::Value = serde_json::from_slice(&current.stdout).expect("current JSON");
    assert_eq!(current["data"]["tool"], "rust");
    assert_eq!(current["data"]["version"], "1.97.0");
    assert_eq!(current["data"]["installed"].as_bool(), Some(true));
}

#[test]
fn uninstall_and_prune_previews_are_safe_before_explicit_pruning() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n[tools]\npnpm = \"11.21.0\"\n",
    )
    .expect("project config");
    create_install(&home, "pnpm", "11.21.0", "pnpm");
    create_install(&home, "bun", "1.3.14", "bun");

    let uninstall = pinset(
        &project,
        &home,
        &["uninstall", "bun@1.3.14", "--dry-run", "--json"],
    );
    assert_success(&uninstall);
    assert!(home.join("installs/bun/1.3.14").is_dir());

    let prune = pinset(&project, &home, &["prune", "--dry-run", "--json"]);
    assert_success(&prune);
    let report: serde_json::Value = serde_json::from_slice(&prune.stdout).expect("prune JSON");
    assert_eq!(report["data"]["candidates"][0]["tool"], "bun");
    assert_eq!(report["data"]["protected"][0]["tool"], "pnpm");
    assert!(home.join("installs/bun/1.3.14").is_dir());
    assert!(home.join("installs/pnpm/11.21.0").is_dir());

    let invalid_project = pinset(
        &project,
        &home,
        &[
            "prune",
            "--project",
            "missing-project",
            "--dry-run",
            "--json",
        ],
    );
    assert!(!invalid_project.status.success());
    let failure: serde_json::Value =
        serde_json::from_slice(&invalid_project.stdout).expect("prune failure JSON");
    assert_eq!(failure["schema"], 1);
    assert_eq!(failure["command"], "prune");
    assert_eq!(failure["ok"], false);
    assert!(home.join("installs/bun/1.3.14").is_dir());

    let pruned = pinset(&project, &home, &["prune", "--json"]);
    assert_success(&pruned);
    let report: serde_json::Value = serde_json::from_slice(&pruned.stdout).expect("prune JSON");
    assert_eq!(report["data"]["removed"], 1);
    assert!(!home.join("installs/bun/1.3.14").exists());
    assert!(home.join("installs/pnpm/11.21.0").is_dir());
}

#[test]
fn verifies_and_repairs_corrupt_cache_archives() {
    use sha2::{Digest, Sha256};

    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    let cache = home.join("downloads/sha256");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&cache).expect("cache");
    let valid = b"valid archive";
    let valid_hash = hex::encode(Sha256::digest(valid));
    let corrupt_hash = "0".repeat(64);
    fs::write(cache.join(format!("{valid_hash}.archive")), valid).expect("valid cache");
    fs::write(cache.join(format!("{corrupt_hash}.archive")), b"corrupt").expect("corrupt cache");

    let verify = pinset(&project, &home, &["cache", "verify", "--json"]);
    assert!(!verify.status.success());
    let report: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("verify JSON");
    assert_eq!(report["command"], "cache.verify");
    assert_eq!(report["error"]["code"], "cache_corrupt");
    assert_eq!(report["error"]["details"]["entries"], 1);

    let clean_preview = pinset(&project, &home, &["cache", "clean", "--dry-run", "--json"]);
    assert_success(&clean_preview);
    let clean_preview: serde_json::Value =
        serde_json::from_slice(&clean_preview.stdout).expect("clean preview JSON");
    assert_eq!(clean_preview["data"]["dry_run"], true);
    assert_eq!(clean_preview["data"]["entries"], 2);
    assert!(cache.join(format!("{valid_hash}.archive")).is_file());
    assert!(cache.join(format!("{corrupt_hash}.archive")).is_file());

    let preview = pinset(&project, &home, &["cache", "repair", "--dry-run", "--json"]);
    assert_success(&preview);
    let preview: serde_json::Value =
        serde_json::from_slice(&preview.stdout).expect("repair preview JSON");
    assert_eq!(preview["data"]["dry_run"], true);
    assert_eq!(preview["data"]["entries"], 1);
    assert!(cache.join(format!("{corrupt_hash}.archive")).is_file());

    let repaired = pinset(&project, &home, &["cache", "repair", "--json"]);
    assert_success(&repaired);
    let repaired: serde_json::Value =
        serde_json::from_slice(&repaired.stdout).expect("repair JSON");
    assert_eq!(repaired["data"]["dry_run"], false);
    assert_eq!(repaired["data"]["entries"], 1);
    assert!(cache.join(format!("{valid_hash}.archive")).is_file());
    assert!(!cache.join(format!("{corrupt_hash}.archive")).exists());
}

#[test]
fn every_json_command_uses_the_same_failure_envelope_in_both_languages() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");
    let commands: &[(&str, &[&str])] = &[
        ("which", &["which", "--json", "--invalid"]),
        ("current", &["current", "--json", "--invalid"]),
        ("list", &["list", "--json", "--invalid"]),
        ("outdated", &["outdated", "--json", "--invalid"]),
        ("uninstall", &["uninstall", "--json", "--invalid"]),
        ("prune", &["prune", "--json", "--invalid"]),
        ("doctor", &["doctor", "--json", "--invalid"]),
        ("cache.list", &["cache", "list", "--json", "--invalid"]),
        ("cache.info", &["cache", "info", "--json", "--invalid"]),
        ("cache.verify", &["cache", "verify", "--json", "--invalid"]),
        ("cache.repair", &["cache", "repair", "--json", "--invalid"]),
        ("cache.clean", &["cache", "clean", "--json", "--invalid"]),
    ];

    for language in ["en", "zh-CN"] {
        for (command, arguments) in commands {
            let mut localized = vec!["--lang", language];
            localized.extend_from_slice(arguments);
            let output = pinset(&project, &home, &localized);
            assert_eq!(
                output.status.code(),
                Some(2),
                "{language} {command}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let failure: serde_json::Value =
                serde_json::from_slice(&output.stdout).expect("JSON failure envelope");
            assert_eq!(failure["schema"], 1, "{language} {command}");
            assert_eq!(failure["command"], *command, "{language} {command}");
            assert_eq!(failure["ok"], false, "{language} {command}");
            assert_eq!(
                failure["error"]["code"], "usage_error",
                "{language} {command}"
            );
            assert!(
                failure["error"]["message"]
                    .as_str()
                    .is_some_and(|message| !message.is_empty()),
                "{language} {command}"
            );
            assert!(failure["error"]["details"].is_object());
            assert!(output.stderr.is_empty(), "{language} {command}");
        }
    }
}

#[test]
fn emits_completion_for_each_supported_shell() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = pinset(&project, &home, &["completions", shell]);
        assert_success(&output);
        let script = String::from_utf8_lossy(&output.stdout);
        assert!(script.contains("pinset"));
        assert!(script.contains("node@"));
        assert!(script.contains("dotnet"));
        assert!(script.contains("verify"));
        assert!(script.contains("--json"));
    }
}

fn create_install(home: &Path, tool: &str, version: &str, command: &str) {
    let target = pinset_core::current_target_for_tool(tool);
    let root = home.join("installs").join(tool).join(version).join(&target);
    let command_directory = pinset_core::runtime_command_directory(tool, &root);
    fs::create_dir_all(&command_directory).expect("command directory");
    fs::write(
        root.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"{tool}\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
        ),
    )
    .expect("receipt");
    write_command(&command_directory, command);
}

#[cfg(windows)]
fn write_command(directory: &Path, command: &str) {
    fs::write(directory.join(format!("{command}.cmd")), "@echo off\r\n").expect("command");
}

#[cfg(unix)]
fn write_command(directory: &Path, command: &str) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(command);
    fs::write(&path, "#!/bin/sh\n").expect("command");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
