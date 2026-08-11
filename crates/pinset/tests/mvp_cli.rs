use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use pinset_core::{
    LockedArtifact, LockedArtifactFormat, Lockfile, MVP_NODE_TARGETS, NodeArchiveFormat,
    ProjectConfig, SourceConfig, current_target, plan_node_artifact, save_lockfile,
    save_project_config,
};
use tempfile::tempdir;

#[test]
fn current_which_exec_and_doctor_share_the_locked_fake_runtime() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, "24.0.0", "24.0.0");
    let executable = create_fake_node(&home, "24.0.0");

    let current = pinset(&project, &home, &["current"]);
    assert_success_contains(&current, "node 24.0.0 installed");

    let which = pinset(&project, &home, &["which", "node"]);
    assert_success_contains(&which, &executable.display().to_string());

    let exec = pinset(&project, &home, &["exec", "--", "node", "hello", "mvp"]);
    assert_success_contains(&exec, "24.0.0:hello mvp");

    #[cfg(not(windows))]
    {
        let npm = pinset(&project, &home, &["exec", "--", "npm", "--version"]);
        assert_success_contains(&npm, "24.0.0:");
        assert_success_contains(&npm, "npm --version");
    }

    let doctor = pinset(&project, &home, &["doctor"]);
    assert_success_contains(&doctor, "lockfile");
    assert_success_contains(&doctor, "matches node@24.0.0");
    assert_success_contains(&doctor, "runtime");
}

#[test]
fn locked_install_rejects_config_mismatch_before_network_or_installation() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, "23.0.0", "24.0.0");

    let output = pinset(&project, &home, &["install", "--locked"]);

    assert!(!output.status.success(), "mismatched lock must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("pinset.toml selects node@23.0.0"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.exists());
}

fn write_project(project: &Path, configured_version: &str, locked_version: &str) {
    let config_path = project.join("pinset.toml");
    let config = ProjectConfig {
        schema: 1,
        tools: BTreeMap::from([("node".to_owned(), configured_version.to_owned())]),
    };
    save_project_config(&config_path, &config).expect("project config");

    let artifacts = MVP_NODE_TARGETS
        .into_iter()
        .map(|target| locked_artifact(locked_version, target))
        .collect();
    let lockfile = Lockfile::new_node(
        "pinset integration test".to_owned(),
        locked_version.to_owned(),
        artifacts,
    );
    save_lockfile(&project.join("pinset.lock"), &lockfile).expect("lockfile");
}

fn locked_artifact(version: &str, target: &str) -> LockedArtifact {
    let plan = plan_node_artifact(&SourceConfig::default(), version, target).expect("plan");
    LockedArtifact {
        target: target.to_owned(),
        canonical_url: plan.canonical_url,
        artifact_path: plan.artifact_path,
        sha256: "ab".repeat(32),
        format: match plan.format {
            NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
            NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
        },
        archive_root: plan.archive_root,
        verification: "nodejs-shasums-https".to_owned(),
    }
}

fn create_fake_node(home: &Path, version: &str) -> PathBuf {
    let install_dir = home
        .join("installs")
        .join("node")
        .join(version)
        .join(current_target());
    let command_dir = if cfg!(windows) {
        install_dir
    } else {
        install_dir.join("bin")
    };
    fs::create_dir_all(&command_dir).expect("command directory");

    #[cfg(windows)]
    {
        let executable = command_dir.join("node.cmd");
        fs::write(
            &executable,
            "@echo off\r\necho %PINSET_SELECTED_VERSION%:%*\r\n",
        )
        .expect("fake node");
        executable
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = command_dir.join("node");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s:%s\\n' \"$PINSET_SELECTED_VERSION\" \"$*\"\n",
        )
        .expect("fake node");
        let mut permissions = fs::metadata(&executable)
            .expect("fake node metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake node permissions");
        let npm = command_dir.join("npm");
        fs::write(&npm, "#!/usr/bin/env node\n").expect("fake npm");
        let mut npm_permissions = fs::metadata(&npm).expect("fake npm metadata").permissions();
        npm_permissions.set_mode(0o755);
        fs::set_permissions(&npm, npm_permissions).expect("fake npm permissions");
        executable
    }
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .output()
        .expect("run pinset")
}

fn assert_success_contains(output: &Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
