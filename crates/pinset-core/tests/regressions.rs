use pinset_core::*;
use std::{collections::BTreeMap, fs, path::Path};

fn tool(name: &str, version: &str, verification: &str) -> LockedTool {
    let url = match name {
        "node" => "https://nodejs.org/dist/v24.21.0/node-v24.21.0-linux-x64.tar.xz",
        "go" => "https://go.dev/dl/go1.27.2.linux-amd64.tar.gz",
        _ => {
            "https://github.com/astral-sh/python-build-standalone/releases/download/20261009/cpython-3.13.16+20261009-x86_64-unknown-linux-gnu-install_only.tar.gz"
        }
    };
    LockedTool {
        name: name.into(),
        requested: version.into(),
        version: version.into(),
        provider: "official".into(),
        released_at: None,
        metadata: Default::default(),
        options: Default::default(),
        artifacts: vec![LockedArtifact {
            target: current_target(),
            canonical_url: url.into(),
            artifact_path: "fixture.tar.gz".into(),
            sha256: "aa".repeat(32),
            integrity: None,
            format: LockedArtifactFormat::TarGz,
            archive_root: name.into(),
            verification: verification.into(),
            attestation: None,
            overlays: vec![],
        }],
    }
}
fn save_project(root: &Path, tools: &[LockedTool]) -> (ProjectContext, ProjectConfig) {
    let mut config = ProjectConfig::new("regression".into());
    let mut lock = Lockfile::empty(config.project_id.clone());
    for tool in tools {
        config
            .tools
            .insert(tool.name.clone(), tool.requested.clone());
        lock.upsert_tool(tool.clone()).unwrap();
    }
    fs::create_dir_all(root.join(".pinset")).unwrap();
    fs::write(
        root.join(PROJECT_CONFIG_FILENAME),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".pinset/lock.toml"),
        toml::to_string(&lock).unwrap(),
    )
    .unwrap();
    (ProjectContext::at(root, false).unwrap(), config)
}
fn install_fixture(home: &Path, tool: &LockedTool) {
    let target = current_target();
    let sdk = install_directory(home, tool, &target);
    let executable =
        command_directory(&sdk, &tool.name, &target).join(executable_name(&tool.name, &target));
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"entry fixture").unwrap();
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
        install_root: sdk.display().to_string(),
        file_count: 1,
        total_size: 13,
        payload_digest: "0".repeat(64),
        pinset_version: pinset_version().into(),
        critical_entries: vec![
            executable
                .strip_prefix(&sdk)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ],
        commands: BTreeMap::new(),
    };
    fs::write(
        sdk.join(".pinset-install.toml"),
        toml::to_string(&receipt).unwrap(),
    )
    .unwrap();
}
#[test]
fn unavailable_unrelated_tools_do_not_disable_an_installed_node() {
    let directory = tempfile::tempdir().unwrap();
    let node = tool("node", "24.21.0", "nodejs-openpgp-sha256");
    let python = tool(
        "python",
        "3.13.16+20261009",
        "python-build-standalone-versions-sha256",
    );
    let (context, _) = save_project(directory.path(), &[node.clone(), python]);
    let home = directory.path().join("home");
    install_fixture(&home, &node);
    assert_eq!(
        plan_command(directory.path(), &home, &context, "node")
            .unwrap()
            .version
            .as_deref(),
        Some("24.21.0")
    );
    assert_eq!(
        plan_command(directory.path(), &home, &context, "python")
            .unwrap_err()
            .code(),
        "PINSET_INSTALL_MISSING"
    );
}
#[test]
fn changed_policy_rejects_an_existing_checksum_lock() {
    let directory = tempfile::tempdir().unwrap();
    let go = tool("go", "1.27.2", "go-download-json-sha256");
    let (context, mut config) = save_project(directory.path(), &[go]);
    config.policy.minimum_verification = Some("signed-checksum".into());
    fs::write(&context.config_path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        context.load_locked().unwrap_err().code(),
        "PINSET_POLICY_VIOLATION"
    );
    assert_eq!(
        plan_command(
            directory.path(),
            &directory.path().join("home"),
            &context,
            "go"
        )
        .unwrap_err()
        .code(),
        "PINSET_POLICY_VIOLATION"
    );
    config.policy.minimum_release_age = Some("bogus".into());
    assert_eq!(
        config.validate().unwrap_err().code(),
        "PINSET_POLICY_INVALID"
    );
    config.policy.minimum_release_age = Some("1d".into());
    config.policy.minimum_verification = None;
    fs::write(&context.config_path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        context.load_locked().unwrap_err().code(),
        "PINSET_RELEASE_AGE_UNAVAILABLE"
    );
}
#[test]
fn minimum_strength_includes_overlay_evidence() {
    let mut node = tool("node", "24.21.0", "nodejs-openpgp-sha256");
    let url = node.artifacts[0].canonical_url.clone();
    node.artifacts[0].overlays.push(LockedArtifactOverlay {
        canonical_url: url,
        artifact_path: "overlay.tar.gz".into(),
        integrity: format!("sha256:{}", "aa".repeat(32)),
        format: LockedArtifactFormat::TarGz,
        archive_root: "overlay".into(),
        verification: "go-download-json-sha256".into(),
    });
    let policy = ProjectPolicy {
        minimum_verification: Some("signed-checksum".into()),
        ..Default::default()
    };
    assert_eq!(
        policy
            .validate_tool(&node, std::time::SystemTime::now())
            .unwrap_err()
            .code(),
        "PINSET_POLICY_VIOLATION"
    );
}
#[test]
fn windows_command_normalization_keeps_unix_case_sensitive() {
    for (input, expected) in [
        ("NODE", "node"),
        ("node.EXE", "node"),
        ("PNPM.exe", "pnpm"),
        ("JAVAC", "javac"),
    ] {
        assert_eq!(normalized_command_name(input, "windows-x86_64"), expected);
    }
    assert_eq!(normalized_command_name("NODE", "linux-x86_64"), "NODE");
    assert_eq!(command_tool("python3.13"), Some("python"));
    assert_eq!(command_tool("pip3.13"), Some("python"));
    assert_eq!(command_tool("python3.invalid"), None);
}
