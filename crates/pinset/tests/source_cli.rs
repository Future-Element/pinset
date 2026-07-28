use std::{fs, process::Command};

use tempfile::tempdir;

#[test]
fn manages_sources_only_inside_explicit_temporary_home() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "node",
            "mirror",
            "--base-url",
            "https://mirror.example/node",
        ],
    )
    .assert_success("added node mirror");
    pinset(&home, &["source", "use", "node", "mirror"]).assert_success("active node mirror");
    pinset(&home, &["source", "fallback", "node", "official"])
        .assert_success("fallback node official");

    let listed = pinset(&home, &["source", "list", "node"]);
    listed.assert_success_contains("node mirror custom active https://mirror.example/node/");
    listed.assert_success_contains("node official official fallback:1 https://nodejs.org/dist/");

    let config = fs::read_to_string(home.join("sources.toml")).expect("source config");
    assert!(config.contains("active = \"mirror\""));
    assert!(config.contains("fallback = [\"official\"]"));
    assert!(!home.join("installs").exists());
    assert!(!home.join("shims").exists());
}

#[test]
fn removes_only_inactive_unreferenced_custom_sources() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "python",
            "corp",
            "--base-url",
            "https://packages.example/python/",
        ],
    )
    .assert_success("added python corp");
    pinset(&home, &["source", "remove", "python", "corp"]).assert_success("removed python corp");

    let listed = pinset(&home, &["source", "list", "python"]);
    listed.assert_success_contains(
        "python official official active https://github.com/astral-sh/python-build-standalone/releases/download/",
    );
    assert!(!listed.stdout.contains("corp"));
    assert!(!home.join("installs").exists());
}

#[test]
fn failed_remove_does_not_rewrite_active_source_config() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "flutter",
            "mirror",
            "--base-url",
            "https://mirror.example/flutter/",
        ],
    )
    .assert_success("added flutter mirror");
    pinset(&home, &["source", "use", "flutter", "mirror"]).assert_success("active flutter mirror");
    let before = fs::read(home.join("sources.toml")).expect("source config before failure");

    let failed = pinset(&home, &["source", "remove", "flutter", "mirror"]);
    assert!(!failed.success, "active source removal must fail");
    assert!(failed.stderr.contains("currently active"));
    let after = fs::read(home.join("sources.toml")).expect("source config after failure");
    assert_eq!(after, before);
    assert!(!home.join("installs").exists());
}

fn pinset(home: &std::path::Path, arguments: &[&str]) -> CommandResult {
    let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .env("PINSET_HOME", home)
        .output()
        .expect("run pinset");
    CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    }
}

struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
}

impl CommandResult {
    fn assert_success(&self, expected: &str) {
        assert!(
            self.success && self.stdout.contains(expected),
            "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
            self.stdout,
            self.stderr
        );
    }

    fn assert_success_contains(&self, expected: &str) {
        self.assert_success(expected);
    }
}
