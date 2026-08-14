use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use pinset_core::{
    GlobalConfig, LockedArtifact, LockedArtifactFormat, Lockfile, MVP_NODE_TARGETS,
    NodeArchiveFormat, ProjectConfig, SourceConfig, current_target, global_config_path,
    global_lockfile_path, plan_node_artifact, save_global_config, save_global_state, save_lockfile,
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
    assert_success_contains(&exec, "source=project");

    let child_help = pinset(
        &project,
        &home,
        &["--lang", "zh-CN", "exec", "--", "node", "--help"],
    );
    assert_success_contains(&child_help, "24.0.0:--help");

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
fn which_and_exec_use_the_global_fake_runtime_without_a_project() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir(&workspace).expect("workspace");
    write_global(&home, "24.0.0", "24.0.0");
    let executable = create_fake_node(&home, "24.0.0");

    let which = pinset(&workspace, &home, &["which", "node"]);
    assert_success_contains(&which, &executable.display().to_string());

    let exec = pinset(&workspace, &home, &["exec", "--", "node", "global"]);
    assert_success_contains(&exec, "24.0.0:global");
    assert_success_contains(&exec, "source=global");

    let current = pinset(&workspace, &home, &["current"]);
    assert_success_contains(&current, "source=global");

    let doctor = pinset(&workspace, &home, &["doctor"]);
    assert_success_contains(&doctor, "source=global");
    assert_success_contains(&doctor, "global.lock");
}

#[test]
fn global_command_reports_default_even_when_a_project_overrides_it() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_global(&home, "24.0.0", "24.0.0");
    write_project(&project, "20.0.0", "20.0.0");
    create_fake_node(&home, "24.0.0");
    create_fake_node(&home, "20.0.0");

    let global = pinset(&project, &home, &["global"]);
    assert_success_contains(&global, "node 24.0.0 installed");
    assert_success_contains(&global, "source=global");
    assert_success_contains(&global, "project node@20.0.0 overrides global node@24.0.0");

    let effective = pinset(&project, &home, &["current"]);
    assert_success_contains(&effective, "node 20.0.0 installed");
    assert_success_contains(&effective, "source=project");
}

#[test]
fn global_command_without_state_is_read_only_and_actionable() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir(&workspace).expect("workspace");

    let output = pinset(&workspace, &home, &["global"]);

    assert_success_contains(&output, "pinset global node@<selector>");
    assert!(
        !home.exists(),
        "read-only global inspection must not create state"
    );
}

#[test]
fn shim_path_is_read_only_and_install_keeps_explicit_safe_overrides() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    let fake_shim = root.path().join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    let destination = root.path().join("custom-shims");
    fs::create_dir(&workspace).expect("workspace");
    fs::write(&fake_shim, b"fake shim").expect("fake shim");

    let default_routing = home.join("shims");
    let path = pinset_with_router(
        &workspace,
        &home,
        &fake_shim,
        &default_routing,
        &["shim", "path"],
    );
    assert_success_contains(&path, &home.join("shims").display().to_string());
    assert!(!home.exists(), "shim path must not create PINSET_HOME");

    let install = pinset(
        &workspace,
        &home,
        &[
            "shim",
            "install",
            "--provider",
            "node",
            "--binary",
            fake_shim.to_str().expect("UTF-8 fake shim"),
            "--dir",
            destination.to_str().expect("UTF-8 destination"),
        ],
    );
    assert_success_contains(&install, "shim directory ready");
    for command in ["node", "npm", "npx", "corepack"] {
        let filename = if cfg!(windows) {
            format!("{command}.cmd")
        } else {
            command.to_owned()
        };
        assert!(
            destination.join(filename).is_file(),
            "missing {command} shim"
        );
    }

    let repeated = pinset(
        &workspace,
        &home,
        &[
            "shim",
            "install",
            "--provider",
            "node",
            "--binary",
            fake_shim.to_str().expect("UTF-8 fake shim"),
            "--dir",
            destination.to_str().expect("UTF-8 destination"),
        ],
    );
    assert_success_contains(&repeated, "shim directory ready");
    assert_eq!(
        fs::read(&fake_shim).expect("source shim"),
        b"fake shim",
        "idempotent repair must not modify the source binary"
    );
}

#[test]
fn shim_migration_registers_new_routes_and_preserves_legacy_entries() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let destination = root.path().join("routing");
    let fake_shim = root.path().join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    let legacy = home.join("shims");
    fs::create_dir(&project).expect("project");
    fs::create_dir_all(&legacy).expect("legacy shims");
    fs::write(&fake_shim, b"fake provider router").expect("fake shim");
    write_project(&project, "24.0.0", "24.0.0");
    let legacy_node = legacy.join(if cfg!(windows) { "node.exe" } else { "node" });
    fs::write(&legacy_node, b"legacy route remains untouched").expect("legacy node");

    let output = pinset_with_router(
        &project,
        &home,
        &fake_shim,
        &destination,
        &["shim", "migrate", "--provider", "node"],
    );

    assert_success_contains(&output, "registered 4 command routes");
    assert_success_contains(&output, "preserved 1 legacy entries");
    assert_eq!(
        fs::read(&legacy_node).expect("legacy node"),
        b"legacy route remains untouched"
    );
    for command in ["node", "npm", "npx", "corepack"] {
        let filename = if cfg!(windows) {
            format!("{command}.cmd")
        } else {
            command.to_owned()
        };
        assert!(
            destination.join(filename).is_file(),
            "missing {command} route"
        );
    }
}

#[test]
fn doctor_reports_all_provider_commands_and_path_shadowing() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let routing = root.path().join("routing");
    let system_bin = root.path().join("system-bin");
    let fake_shim = root.path().join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    fs::create_dir(&project).expect("project");
    fs::write(&fake_shim, b"fake provider router").expect("fake shim");
    write_project(&project, "24.0.0", "24.0.0");
    create_fake_node(&home, "24.0.0");
    create_fake_system_node(&system_bin);

    let install = pinset(
        &project,
        &home,
        &[
            "shim",
            "install",
            "--provider",
            "node",
            "--binary",
            fake_shim.to_str().expect("UTF-8 fake shim"),
            "--dir",
            routing.to_str().expect("UTF-8 routing"),
        ],
    );
    assert!(install.status.success());

    let doctor = pinset_with_router_and_path(
        &project,
        &home,
        &fake_shim,
        &routing,
        &[&system_bin, &routing],
        &["doctor", "--json"],
    );
    assert!(
        doctor.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("doctor JSON output");
    let commands = report["path_candidates"]
        .as_array()
        .expect("path candidates")
        .iter()
        .filter_map(|candidate| candidate["command"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        commands,
        [
            "bun",
            "bunx",
            "cargo",
            "cargo-clippy",
            "cargo-fmt",
            "clippy-driver",
            "corepack",
            "dart",
            "dotnet",
            "flutter",
            "go",
            "gofmt",
            "jar",
            "java",
            "javac",
            "javadoc",
            "javap",
            "jshell",
            "keytool",
            "node",
            "npm",
            "npx",
            "pip",
            "pip3",
            "pnpm",
            "python",
            "python3",
            "rustc",
            "rustdoc",
            "rustfmt",
        ]
        .into_iter()
        .collect()
    );
    assert!(
        report["routing_issues"]
            .as_array()
            .expect("routing issues")
            .iter()
            .any(|issue| issue["code"] == "provider-route-shadowed" && issue["command"] == "node")
    );
}

#[test]
fn locked_install_registers_provider_commands_without_downloading_a_runtime() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let routing = root.path().join("routing");
    let fake_shim = root.path().join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    fs::create_dir(&project).expect("project");
    fs::create_dir(&routing).expect("routing");
    fs::write(&fake_shim, b"fake provider router").expect("fake shim");
    write_project(&project, "24.0.0", "24.0.0");
    create_fake_node(&home, "24.0.0");
    create_complete_install_receipt(&home, "24.0.0");

    let output = pinset_with_router(
        &project,
        &home,
        &fake_shim,
        &routing,
        &["install", "--locked"],
    );

    assert_success_contains(&output, "node command routing ready");
    assert_success_contains(&output, "managed-existing=-");
    for command in ["node", "npm", "npx", "corepack"] {
        let filename = if cfg!(windows) {
            format!("{command}.cmd")
        } else {
            command.to_owned()
        };
        assert!(routing.join(filename).is_file(), "missing {command} route");
    }
    assert!(!routing.join("python").exists());
    assert!(!routing.join("flutter").exists());

    let repeated = pinset_with_router(
        &project,
        &home,
        &fake_shim,
        &routing,
        &["install", "--locked"],
    );
    assert_success_contains(&repeated, "managed-existing=node,npm,npx,corepack");

    let system_bin = root.path().join("system-bin");
    fs::create_dir(&system_bin).expect("system bin");
    let system_node = system_bin.join(if cfg!(windows) { "node.exe" } else { "node" });
    fs::write(&system_node, b"system node").expect("system node");
    let shadowed = pinset_with_router_and_path(
        &project,
        &home,
        &fake_shim,
        &routing,
        &[&system_bin, &routing],
        &["install", "--locked"],
    );
    assert_success_contains(&shadowed, "earlier PATH entries shadow node=");
    assert_success_contains(&shadowed, "pinset activate");
}

#[test]
fn provider_registration_stops_before_overwriting_a_foreign_command() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let routing = root.path().join("routing");
    let fake_shim = root.path().join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    });
    fs::create_dir(&project).expect("project");
    fs::create_dir(&routing).expect("routing");
    fs::write(&fake_shim, b"fake provider router").expect("fake shim");
    write_project(&project, "24.0.0", "24.0.0");
    create_fake_node(&home, "24.0.0");
    create_complete_install_receipt(&home, "24.0.0");
    let existing_npm = routing.join(if cfg!(windows) { "npm.exe" } else { "npm" });
    fs::write(&existing_npm, b"foreign npm entry").expect("existing npm");

    let output = pinset_with_router(
        &project,
        &home,
        &fake_shim,
        &routing,
        &["install", "--locked"],
    );

    assert!(
        output.status.success(),
        "runtime install remains successful"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&existing_npm).expect("existing npm"),
        b"foreign npm entry"
    );
    assert!(
        !routing
            .join(if cfg!(windows) { "node.cmd" } else { "node" })
            .exists()
    );
}

#[test]
fn project_selection_overrides_the_global_fake_runtime() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_global(&home, "24.0.0", "24.0.0");
    write_project(&project, "20.0.0", "20.0.0");
    create_fake_node(&home, "24.0.0");
    create_fake_node(&home, "20.0.0");

    let exec = pinset(&project, &home, &["exec", "--", "node", "priority"]);
    assert_success_contains(&exec, "20.0.0:priority");
    assert_success_contains(&exec, "source=project");
}

#[test]
fn unset_clears_project_then_global_selection_without_uninstalling_runtimes() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::create_dir(&workspace).expect("workspace");
    write_global(&home, "24.0.0", "24.0.0");
    write_project(&project, "20.0.0", "20.0.0");
    let global_runtime = create_fake_node(&home, "24.0.0");
    let project_runtime = create_fake_node(&home, "20.0.0");

    let project_unset = pinset(&project, &home, &["unset", "node"]);
    assert_success_contains(&project_unset, "cleared project Node.js selection");
    assert!(!project.join("pinset.lock").exists());
    assert!(
        fs::read_to_string(project.join("pinset.toml"))
            .expect("project config")
            .contains("[tools]")
    );
    let current = pinset(&project, &home, &["current"]);
    assert_success_contains(&current, "node 24.0.0 installed");
    assert_success_contains(&current, "source=global");
    assert!(
        project_runtime.is_file(),
        "project runtime must be preserved"
    );

    let global_unset = pinset(&workspace, &home, &["unset", "node", "--global"]);
    assert_success_contains(&global_unset, "cleared global Node.js selection");
    assert!(!home.join("state/global.lock").exists());
    assert!(global_runtime.is_file(), "global runtime must be preserved");
    let global = pinset(&workspace, &home, &["global"]);
    assert_success_contains(&global, "no global Node.js version selected");
}

#[test]
fn which_and_exec_safely_pass_through_the_first_system_path_runtime() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    let system_bin = root.path().join("system-bin");
    fs::create_dir(&workspace).expect("workspace");
    let executable = create_fake_system_node(&system_bin);

    let which = pinset_with_path(&workspace, &home, &system_bin, &["which", "node"]);
    assert_success_contains(&which, &executable.display().to_string());

    let exec = pinset_with_path(
        &workspace,
        &home,
        &system_bin,
        &["exec", "--", "node", "passthrough"],
    );
    assert_success_contains(&exec, "system:passthrough");
    assert_success_contains(&exec, "source=system");
    assert!(
        !home.exists(),
        "system passthrough must not create Pinset state"
    );

    let current = pinset_with_path(&workspace, &home, &system_bin, &["current", "node"]);
    assert_success_contains(&current, "source=system");

    let doctor = pinset_with_path(&workspace, &home, &system_bin, &["doctor"]);
    assert_success_contains(&doctor, "source=system");

    let exit = pinset_with_path(
        &workspace,
        &home,
        &system_bin,
        &["exec", "--", "node", "exit23"],
    );
    assert_eq!(exit.status.code(), Some(23));
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

#[test]
fn global_locked_install_rejects_mismatch_before_network_or_installation() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir(&workspace).expect("workspace");
    write_global_mismatch(&home, "23.0.0", "24.0.0");

    let output = pinset(&workspace, &home, &["install", "--global", "--locked"]);

    assert!(!output.status.success(), "mismatched global lock must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("global.toml selects node@23.0.0"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.join("installs").exists());
}

#[test]
fn exec_can_select_an_installed_exact_version_without_changing_project_state() {
    let root = tempdir().expect("temporary root");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir(&workspace).expect("workspace");
    create_fake_node(&home, "24.0.0");

    let output = pinset(
        &workspace,
        &home,
        &["exec", "node@24.0.0", "--", "node", "ephemeral"],
    );
    assert_success_contains(&output, "24.0.0:ephemeral");
    assert_success_contains(&output, "source=ephemeral");
    assert!(!workspace.join("pinset.toml").exists());
    assert!(!home.join("state").exists());
}

#[test]
fn doctor_json_is_machine_readable_and_has_no_manager_migration_report() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, "24.0.0", "24.0.0");
    create_fake_node(&home, "24.0.0");
    let doctor = pinset(&project, &home, &["doctor", "--json"]);
    assert!(
        doctor.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("doctor JSON output");
    assert_eq!(report["schema"], 2);
    assert_eq!(report["selection"]["version"], "24.0.0");
    assert_eq!(report["runtime"]["status"], "ok");
    assert!(report.get("legacy_node_configs").is_none());
    assert_eq!(
        fs::read_to_string(project.join("pinset.toml")).expect("project config"),
        "schema = 2\n\n[tools]\nnode = \"24.0.0\"\n"
    );
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

fn write_global(home: &Path, configured_version: &str, locked_version: &str) {
    let config = GlobalConfig {
        schema: 1,
        tools: BTreeMap::from([("node".to_owned(), configured_version.to_owned())]),
    };
    let lockfile = test_lockfile(locked_version);
    save_global_state(home, &config, &lockfile).expect("global state");
}

fn write_global_mismatch(home: &Path, configured_version: &str, locked_version: &str) {
    let config = GlobalConfig {
        schema: 1,
        tools: BTreeMap::from([("node".to_owned(), configured_version.to_owned())]),
    };
    save_global_config(&global_config_path(home), &config).expect("global config");
    save_lockfile(&global_lockfile_path(home), &test_lockfile(locked_version))
        .expect("global lockfile");
}

fn test_lockfile(version: &str) -> Lockfile {
    let artifacts = MVP_NODE_TARGETS
        .into_iter()
        .map(|target| locked_artifact(version, target))
        .collect();
    Lockfile::new_node(
        "pinset integration test".to_owned(),
        version.to_owned(),
        artifacts,
    )
}

fn locked_artifact(version: &str, target: &str) -> LockedArtifact {
    let plan = plan_node_artifact(&SourceConfig::default(), version, target).expect("plan");
    LockedArtifact {
        target: target.to_owned(),
        canonical_url: plan.canonical_url,
        artifact_path: plan.artifact_path,
        sha256: "ab".repeat(32),
        integrity: None,
        format: match plan.format {
            NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
            NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
        },
        archive_root: plan.archive_root,
        verification: "nodejs-shasums-https".to_owned(),
        overlays: Vec::new(),
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
            "@echo off\r\necho %PINSET_SELECTED_VERSION%:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
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
            "#!/bin/sh\nprintf '%s:%s\\nsource=%s\\n' \"$PINSET_SELECTED_VERSION\" \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
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

fn create_complete_install_receipt(home: &Path, version: &str) {
    let install_dir = home
        .join("installs")
        .join("node")
        .join(version)
        .join(current_target());
    if cfg!(windows) {
        fs::write(install_dir.join("node.exe"), b"fake node executable")
            .expect("required node executable");
    }
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "complete = true\ntool = \"node\"\nversion = \"{version}\"\ntarget = \"{}\"\nselected_source = \"fixture\"\nartifact_sha256 = \"{}\"\n",
            current_target(),
            "ab".repeat(32)
        ),
    )
    .expect("install receipt");
}

fn create_fake_system_node(directory: &Path) -> PathBuf {
    fs::create_dir_all(directory).expect("system command directory");

    #[cfg(windows)]
    {
        let executable = directory.join("node.cmd");
        fs::write(
            &executable,
            "@echo off\r\nif \"%1\"==\"exit23\" exit /b 23\r\necho system:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
        )
        .expect("fake system node");
        for command in [
            "npm", "npx", "corepack", "pnpm", "bun", "bunx", "go", "gofmt", "flutter", "dart",
        ] {
            fs::write(
                directory.join(format!("{command}.cmd")),
                format!("@echo off\r\necho fake {command}\r\n"),
            )
            .expect("fake system command");
        }
        executable
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = directory.join("node");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"exit23\" ]; then exit 23; fi\nprintf 'system:%s\\nsource=%s\\n' \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
        )
        .expect("fake system node");
        let mut permissions = fs::metadata(&executable)
            .expect("fake system node metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake system node permissions");
        for command in [
            "npm", "npx", "corepack", "pnpm", "bun", "bunx", "go", "gofmt", "flutter", "dart",
        ] {
            let command_path = directory.join(command);
            fs::write(
                &command_path,
                format!("#!/bin/sh\nprintf '%s\\n' 'fake {command}'\n"),
            )
            .expect("fake system command");
            let mut command_permissions = fs::metadata(&command_path)
                .expect("fake system command metadata")
                .permissions();
            command_permissions.set_mode(0o755);
            fs::set_permissions(&command_path, command_permissions)
                .expect("fake system command permissions");
        }
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

fn pinset_with_path(project: &Path, home: &Path, first: &Path, arguments: &[&str]) -> Output {
    let inherited_path = env::var_os("PATH");
    let path = std::iter::once(first.to_path_buf()).chain(
        inherited_path
            .as_ref()
            .into_iter()
            .flat_map(|value| env::split_paths(value)),
    );
    let path = env::join_paths(path).expect("test PATH");
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PATH", path)
        .output()
        .expect("run pinset")
}

fn pinset_with_router(
    project: &Path,
    home: &Path,
    shim_binary: &Path,
    shim_directory: &Path,
    arguments: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_SHIM_BINARY", shim_binary)
        .env("PINSET_SHIM_DIR", shim_directory)
        .output()
        .expect("run pinset")
}

fn pinset_with_router_and_path(
    project: &Path,
    home: &Path,
    shim_binary: &Path,
    shim_directory: &Path,
    first: &[&Path],
    arguments: &[&str],
) -> Output {
    let inherited_path = env::var_os("PATH");
    let path = first.iter().map(|path| (*path).to_path_buf()).chain(
        inherited_path
            .as_ref()
            .into_iter()
            .flat_map(|value| env::split_paths(value)),
    );
    let path = env::join_paths(path).expect("test PATH");
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_SHIM_BINARY", shim_binary)
        .env("PINSET_SHIM_DIR", shim_directory)
        .env("PATH", path)
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
