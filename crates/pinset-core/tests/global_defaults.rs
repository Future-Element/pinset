use pinset_core::*;
use std::{collections::BTreeMap, fs, path::Path};

fn tool(name: &str, version: &str) -> LockedTool {
    let (url, verification) = match name {
        "node" => (
            format!("https://nodejs.org/dist/v{version}/node-v{version}-linux-x64.tar.gz"),
            "nodejs-openpgp-sha256",
        ),
        "pnpm" => (
            format!("https://registry.npmjs.org/pnpm/-/pnpm-{version}.tgz"),
            "npm-registry-signature-sha512",
        ),
        "python" => (
            "https://github.com/astral-sh/python-build-standalone/releases/download/20261009/cpython-3.13.16+20261009-x86_64-unknown-linux-gnu-install_only.tar.gz".into(),
            "python-build-standalone-versions-sha256",
        ),
        "go" => (
            format!("https://go.dev/dl/go{version}.linux-amd64.tar.gz"),
            "go-download-json-sha256",
        ),
        "java" => (
            "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.8%2B9/OpenJDK21U-jdk_x64_linux_hotspot_21.0.8_9.tar.gz".into(),
            "adoptium-api-sha256",
        ),
        _ => unreachable!(),
    };
    LockedTool {
        name: name.into(),
        requested: version.into(),
        version: version.into(),
        provider: "official".into(),
        released_at: None,
        metadata: BTreeMap::new(),
        options: BTreeMap::new(),
        artifacts: vec![LockedArtifact {
            target: current_target(),
            canonical_url: url,
            artifact_path: "fixture.tar.gz".into(),
            sha256: "aa".repeat(32),
            integrity: None,
            format: LockedArtifactFormat::TarGz,
            archive_root: name.into(),
            verification: verification.into(),
            overlays: vec![],
        }],
    }
}

fn save_context(root: &Path, global: bool, tools: &[LockedTool]) -> ProjectContext {
    let state = if global {
        root.into()
    } else {
        root.join(".pinset")
    };
    fs::create_dir_all(&state).unwrap();
    let mut config = ProjectConfig::new(if global { "global" } else { "project" }.into());
    let mut lock = Lockfile::empty(config.project_id.clone());
    for tool in tools {
        config
            .tools
            .insert(tool.name.clone(), tool.requested.clone());
        lock.upsert_tool(tool.clone()).unwrap();
    }
    fs::write(state.join("config.toml"), toml::to_string(&config).unwrap()).unwrap();
    fs::write(state.join("lock.toml"), toml::to_string(&lock).unwrap()).unwrap();
    ProjectContext::at(root, global).unwrap()
}

fn install_fixture(home: &Path, tool: &LockedTool) {
    // Receipt/path fixtures only: no tool is downloaded or executed.
    let target = current_target();
    let install = install_directory(home, tool, &target);
    let entry = match tool.name.as_str() {
        "pnpm" => install.join("bin/pnpm.cjs"),
        "python" => interpreter_path(&install, &target),
        name => command_directory(&install, name, &target).join(executable_name(name, &target)),
    };
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, b"entry fixture").unwrap();
    let receipt = InstallReceipt {
        schema: 3,
        complete: true,
        tool: tool.name.clone(),
        version: tool.version.clone(),
        install_identity: tool.installation_version(&target),
        target,
        canonical_url: tool.artifacts[0].canonical_url.clone(),
        selected_source: "official".into(),
        selected_source_kind: "official".into(),
        selected_url: tool.artifacts[0].canonical_url.clone(),
        artifact_integrity: tool.artifacts[0].artifact_integrity().unwrap().canonical(),
        artifact_format: "tar.gz".into(),
        base_artifact_integrities: vec![],
        bytes_downloaded: 0,
        install_root: install.display().to_string(),
        file_count: 1,
        total_size: 13,
        payload_digest: "0".repeat(64),
        pinset_version: pinset_version().into(),
        critical_entries: vec![
            entry
                .strip_prefix(&install)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ],
        commands: BTreeMap::new(),
    };
    fs::write(
        install.join(".pinset-install.toml"),
        toml::to_string(&receipt).unwrap(),
    )
    .unwrap();
}

fn executable(home: &Path, tool: &LockedTool) -> std::path::PathBuf {
    let target = current_target();
    command_directory(&install_directory(home, tool, &target), &tool.name, &target)
        .join(executable_name(&tool.name, &target))
}

#[test]
fn empty_project_uses_global_defaults_without_mutating_its_lock() {
    let directory = tempfile::tempdir().unwrap();
    let context = save_context(directory.path(), false, &[]);
    let home = directory.path().join("home");
    let node = tool("node", "24.21.0");
    let pnpm = tool("pnpm", "10.34.5");
    save_context(&home.join("global"), true, &[node.clone(), pnpm.clone()]);
    install_fixture(&home, &node);
    install_fixture(&home, &pnpm);
    let original_lock = fs::read(&context.lock_path).unwrap();
    for (command, version) in [("node", "24.21.0"), ("pnpm", "10.34.5")] {
        let plan = plan_command(directory.path(), &home, &context, command).unwrap();
        assert_eq!(plan.source, "global");
        assert_eq!(plan.version.as_deref(), Some(version));
        assert_eq!(plan.project.as_ref(), Some(&context.root));
        assert_eq!(plan.executable, executable(&home, &node));
        if command == "pnpm" {
            assert_eq!(
                plan.prefix.last().unwrap(),
                "--config.use-node-version=24.21.0"
            );
        }
    }
    assert_eq!(fs::read(&context.lock_path).unwrap(), original_lock);
    assert!(context.load().unwrap().tools.is_empty());
}

#[test]
fn explicit_project_selections_override_global_versions() {
    let directory = tempfile::tempdir().unwrap();
    let node = tool("node", "22.20.0");
    let pnpm = tool("pnpm", "10.33.0");
    let context = save_context(directory.path(), false, &[node.clone(), pnpm.clone()]);
    let home = directory.path().join("home");
    save_context(
        &home.join("global"),
        true,
        &[tool("node", "24.21.0"), tool("pnpm", "10.34.5")],
    );
    install_fixture(&home, &node);
    install_fixture(&home, &pnpm);
    for (command, version) in [("node", "22.20.0"), ("pnpm", "10.33.0")] {
        let plan = plan_command(directory.path(), &home, &context, command).unwrap();
        assert_eq!(plan.source, "project");
        assert_eq!(plan.version.as_deref(), Some(version));
        assert_eq!(plan.executable, executable(&home, &node));
    }
}

#[test]
fn inherited_pnpm_keeps_project_node_and_other_project_tool_settings() {
    let directory = tempfile::tempdir().unwrap();
    let node = tool("node", "22.20.0");
    let java = tool("java", "21.0.8+9");
    let context = save_context(directory.path(), false, &[node.clone(), java.clone()]);
    let home = directory.path().join("home");
    let pnpm = tool("pnpm", "10.34.5");
    save_context(
        &home.join("global"),
        true,
        &[
            tool("node", "24.21.0"),
            pnpm.clone(),
            tool("java", "21.0.9+10"),
        ],
    );
    for tool in [&node, &java, &pnpm] {
        install_fixture(&home, tool);
    }
    let plan = plan_command(directory.path(), &home, &context, "pnpm").unwrap();
    assert_eq!(plan.source, "global");
    assert_eq!(plan.executable, executable(&home, &node));
    assert_eq!(
        plan.prefix.last().unwrap(),
        "--config.use-node-version=22.20.0"
    );
    let java_install = install_directory(&home, &java, &current_target());
    assert_eq!(
        plan.environment["JAVA_HOME"],
        sdk_root(&java_install, "java", &current_target())
            .display()
            .to_string()
    );
}

#[test]
fn strict_project_policy_rejects_unselected_global_tools() {
    let directory = tempfile::tempdir().unwrap();
    let context = save_context(directory.path(), false, &[]);
    let home = directory.path().join("home");
    let node = tool("node", "24.21.0");
    save_context(&home.join("global"), true, &[node.clone()]);
    install_fixture(&home, &node);
    let mut config = context.load().unwrap();
    config.policy.require_project_tools = true;
    fs::write(&context.config_path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        plan_command(directory.path(), &home, &context, "node")
            .unwrap_err()
            .code(),
        "PINSET_TOOL_NOT_SELECTED"
    );
}

#[test]
fn missing_or_damaged_project_selections_and_invalid_locks_never_fall_back() {
    let directory = tempfile::tempdir().unwrap();
    let node = tool("node", "22.20.0");
    let context = save_context(directory.path(), false, &[node.clone()]);
    let home = directory.path().join("home");
    let global_node = tool("node", "24.21.0");
    save_context(&home.join("global"), true, &[global_node.clone()]);
    install_fixture(&home, &global_node);
    assert_eq!(
        plan_command(directory.path(), &home, &context, "node")
            .unwrap_err()
            .code(),
        "PINSET_INSTALL_MISSING"
    );
    install_fixture(&home, &node);
    fs::remove_file(executable(&home, &node)).unwrap();
    assert_eq!(
        plan_command(directory.path(), &home, &context, "node")
            .unwrap_err()
            .code(),
        "PINSET_INSTALL_DAMAGED"
    );
    let (_, mut lock) = context.load_locked().unwrap();
    lock.project_id = "different-project".into();
    fs::write(&context.lock_path, toml::to_string(&lock).unwrap()).unwrap();
    assert_eq!(
        plan_command(directory.path(), &home, &context, "node")
            .unwrap_err()
            .code(),
        "PINSET_LOCK_MISMATCH"
    );
}

#[test]
fn inherited_global_python_uses_its_interpreter_without_creating_a_project_venv() {
    let directory = tempfile::tempdir().unwrap();
    let context = save_context(directory.path(), false, &[]);
    let home = directory.path().join("home");
    let python = tool("python", "3.13.16+20261009");
    save_context(&home.join("global"), true, &[python.clone()]);
    install_fixture(&home, &python);
    let plan = plan_command(directory.path(), &home, &context, "python").unwrap();
    assert_eq!(plan.source, "global");
    assert_eq!(
        plan.executable,
        interpreter_path(
            &install_directory(&home, &python, &current_target()),
            &current_target()
        )
    );
    assert!(!plan.environment.contains_key("VIRTUAL_ENV"));
    assert!(!directory.path().join(".venv").exists());
}

#[test]
fn project_supply_chain_policy_applies_to_inherited_tools_and_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let context = save_context(directory.path(), false, &[]);
    let home = directory.path().join("home");
    let go = tool("go", "1.27.2");
    let node = tool("node", "24.21.0");
    let mut pnpm = tool("pnpm", "10.34.5");
    pnpm.released_at = Some("2020-01-01T00:00:00Z".into());
    save_context(
        &home.join("global"),
        true,
        &[go.clone(), node.clone(), pnpm.clone()],
    );
    for tool in [&go, &node, &pnpm] {
        install_fixture(&home, tool);
    }
    let mut config = context.load().unwrap();
    config.policy.minimum_verification = Some("signed-checksum".into());
    fs::write(&context.config_path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        plan_command(directory.path(), &home, &context, "go")
            .unwrap_err()
            .code(),
        "PINSET_POLICY_VIOLATION"
    );
    config.policy.minimum_verification = None;
    config.policy.minimum_release_age = Some("1d".into());
    fs::write(&context.config_path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        plan_command(directory.path(), &home, &context, "pnpm")
            .unwrap_err()
            .code(),
        "PINSET_RELEASE_AGE_UNAVAILABLE"
    );
}
