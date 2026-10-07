use pinset_core::*;
use std::{fs, path::Path};
fn write_config(root: &Path, id: &str) {
    fs::create_dir_all(root.join(".pinset")).unwrap();
    fs::write(
        root.join(PROJECT_CONFIG_FILENAME),
        toml::to_string(&ProjectConfig::new(id.into())).unwrap(),
    )
    .unwrap();
}
#[test]
fn nearest_config_within_worktree_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join(".git"), "gitdir: /unused/worktree").unwrap();
    write_config(root, "parent");
    let sub = root.join("app");
    fs::create_dir_all(sub.join("src")).unwrap();
    write_config(&sub, "child");
    let context = find_project_context(&sub.join("src")).unwrap().unwrap();
    assert_eq!(context.root, sub.canonicalize().unwrap());
    assert_eq!(context.load().unwrap().project_id, "child");
}
#[test]
fn nearest_repository_stops_parent_configuration() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), "outside");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    assert!(find_project_context(&repo.join("src")).unwrap().is_none());
}
#[test]
fn rejects_old_and_unknown_config_fields() {
    for text in [
        "schema = 6\n[tools]\nnode='24'",
        "protocol='pinset/3'\nschema=3\nproject_id='x'\nunknown=true",
        "protocol='pinset/2'\nschema=3\nproject_id='x'",
    ] {
        let result = toml::from_str::<ProjectConfig>(text)
            .and_then(|c| c.validate().map_err(serde::de::Error::custom));
        assert!(result.is_err());
    }
}
#[test]
fn no_parent_or_global_config_merge() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    let mut parent = ProjectConfig::new("parent".into());
    parent.tools.insert("java".into(), "21".into());
    write_config(tmp.path(), "parent");
    fs::write(
        tmp.path().join(PROJECT_CONFIG_FILENAME),
        toml::to_string(&parent).unwrap(),
    )
    .unwrap();
    let child = tmp.path().join("child");
    fs::create_dir(&child).unwrap();
    write_config(&child, "child");
    let c = find_project_context(&child).unwrap().unwrap();
    assert!(c.load().unwrap().tools.is_empty());
}
fn locked() -> LockedTool {
    LockedTool{name:"java".into(),requested:"21".into(),version:"21.0.8+9".into(),provider:"adoptium-temurin".into(),released_at:None,metadata:Default::default(),options:Default::default(),artifacts:vec![LockedArtifact{target:"linux-x86_64".into(),canonical_url:"https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.8%2B9/OpenJDK21U-jdk_x64_linux_hotspot_21.0.8_9.tar.gz".into(),artifact_path:"OpenJDK21U-jdk_x64_linux_hotspot_21.0.8_9.tar.gz".into(),sha256:"aa".repeat(32),integrity:None,format:LockedArtifactFormat::TarGz,archive_root:"jdk-21.0.8+9".into(),verification:"adoptium-api-sha256".into(),overlays:vec![]}]}
}
#[test]
fn installation_identity_includes_build_platform_digest_options() {
    let tool = locked();
    let mut changed = tool.clone();
    changed.artifacts[0].sha256 = "bb".repeat(32);
    assert_ne!(
        changed.installation_version("linux-x86_64"),
        tool.installation_version("linux-x86_64")
    );
    let mut build = tool.clone();
    build.version = "21.0.8+10".into();
    assert_ne!(
        build.installation_version("linux-x86_64"),
        tool.installation_version("linux-x86_64")
    );
    let mut platform = tool.clone();
    platform.artifacts[0].target = "linux-aarch64".into();
    assert_ne!(
        platform.installation_version("linux-x86_64"),
        tool.installation_version("linux-x86_64")
    );
    let mut options = tool.clone();
    options.options.insert("profile".into(), "minimal".into());
    assert_ne!(
        options.installation_version("linux-x86_64"),
        tool.installation_version("linux-x86_64")
    );
}
#[test]
fn pnpm_routes_only_supported_locked_entries() {
    let mut tool = locked();
    tool.name = "pnpm".into();
    assert_eq!(
        pnpm_entry_path(&tool, "linux-x86_64").unwrap(),
        Path::new("bin/pnpm.cjs")
    );
    tool.metadata
        .insert("pnpm-entry".into(), "bin/pnpm.mjs".into());
    assert_eq!(
        pnpm_entry_path(&tool, "windows-x86_64").unwrap(),
        Path::new("bin/pnpm.mjs")
    );
    tool.metadata.insert("pnpm-entry".into(), "pnpm".into());
    assert_eq!(
        pnpm_entry_path(&tool, "windows-x86_64").unwrap(),
        Path::new("pnpm.exe")
    );
    assert_eq!(
        pnpm_entry_path(&tool, "linux-aarch64").unwrap(),
        Path::new("pnpm")
    );
    for entry in ["../pnpm", "C:\\external\\pnpm.exe", "bin/unknown.js"] {
        tool.metadata.insert("pnpm-entry".into(), entry.into());
        assert_eq!(
            pnpm_entry_path(&tool, "linux-x86_64").unwrap_err().code(),
            "PINSET_LOCK_INVALID"
        );
    }
}
#[test]
fn another_platform_does_not_duplicate_the_host_installation() {
    let tool = locked();
    let mut multi = tool.clone();
    let mut arm = multi.artifacts[0].clone();
    arm.target = "linux-aarch64".into();
    multi.artifacts.push(arm);
    assert_eq!(
        tool.installation_version("linux-x86_64"),
        multi.installation_version("linux-x86_64")
    );
    assert_ne!(
        multi.installation_version("linux-x86_64"),
        multi.installation_version("linux-aarch64")
    );
}

#[test]
fn lock_requires_exact_project_identity_and_selection() {
    let mut config = ProjectConfig::new("project".into());
    config.tools.insert("java".into(), "21".into());
    let mut lock = Lockfile::empty("project".into());
    lock.upsert_tool(locked()).unwrap();
    config.validate_lock(&lock).unwrap();
    lock.project_id = "other".into();
    assert_eq!(
        config.validate_lock(&lock).unwrap_err().code(),
        "PINSET_LOCK_MISMATCH"
    );
}
#[test]
fn java_and_managed_tools_never_fall_back() {
    for command in JAVA_COMMANDS {
        assert_eq!(command_tool(command), Some("java"));
    }
    assert_eq!(command_tool("pip3"), Some("python"));
    assert_eq!(command_tool("gradle"), None);
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), "empty");
    let c = ProjectContext::at(tmp.path(), false).unwrap();
    let error = plan_command(tmp.path(), &tmp.path().join("home"), &c, "javac").unwrap_err();
    assert_eq!(error.code(), "PINSET_TOOL_NOT_SELECTED");
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("state")).unwrap();
    fs::create_dir_all(home.join("bin")).unwrap();
    fs::write(home.join("bin/future-jdk-tool"), "external path fixture").unwrap();
    fs::write(home.join("state/shims.json"), serde_json::json!({"protocol":PROTOCOL,"digest":"a".repeat(64),"commands":{"future-jdk-tool":"java"}}).to_string()).unwrap();
    assert_eq!(
        plan_command(tmp.path(), &home, &c, "future-jdk-tool")
            .unwrap_err()
            .code(),
        "PINSET_TOOL_NOT_SELECTED"
    );
    fs::write(home.join("state/shims.json"), "{}").unwrap();
    assert_eq!(
        plan_command(tmp.path(), &home, &c, "future-jdk-tool")
            .unwrap_err()
            .code(),
        "PINSET_SHIM_STATE"
    );
}
#[test]
fn rejects_jre_and_custom_artifact_sources() {
    assert!(validate_official_url("java","https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21/OpenJDK-jre_x64.tar.gz").is_err());
    assert!(validate_official_url("node", "https://mirror.example/node.tar.xz").is_err());
    assert!(validate_official_url("java","https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21/OpenJDK-jdk_x64.tar.gz?token=secret").is_err());
}

#[test]
fn flutter_and_dart_route_to_one_fixture_sdk_without_system_fallback() {
    // A tiny receipt/path fixture verifies routing only; no SDK is downloaded or run.
    let root = tempfile::tempdir().unwrap();
    write_config(root.path(), "flutter-project");
    let target = current_target();
    let mut tool = locked();
    tool.name = "flutter".into();
    tool.requested = "3.35.4".into();
    tool.version = "3.35.4".into();
    tool.provider = "flutter-official".into();
    tool.artifacts[0].target = target.clone();
    tool.artifacts[0].canonical_url = "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_3.35.4-stable.tar.xz".into();
    let mut config = ProjectConfig::new("flutter-project".into());
    config
        .tools
        .insert(tool.name.clone(), tool.requested.clone());
    fs::write(
        root.path().join(PROJECT_CONFIG_FILENAME),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let mut lock = Lockfile::empty(config.project_id);
    lock.upsert_tool(tool.clone()).unwrap();
    fs::write(
        root.path().join(".pinset/lock.toml"),
        toml::to_string(&lock).unwrap(),
    )
    .unwrap();
    let home = root.path().join("home");
    let sdk = install_directory(&home, &tool, &target);
    fs::create_dir_all(sdk.join("bin/cache/dart-sdk/bin")).unwrap();
    let flutter = if target.starts_with("windows-") {
        "bin/flutter.bat".to_owned()
    } else {
        "bin/flutter".to_owned()
    };
    let dart = format!(
        "bin/cache/dart-sdk/bin/{}",
        executable_name("dart", &target)
    );
    fs::write(sdk.join(&flutter), "path fixture").unwrap();
    fs::write(sdk.join(&dart), "path fixture").unwrap();
    let receipt = InstallReceipt {
        schema: 3,
        complete: true,
        tool: tool.name.clone(),
        version: tool.version.clone(),
        install_identity: tool.installation_version(&target),
        target: target.clone(),
        canonical_url: tool.artifacts[0].canonical_url.clone(),
        selected_source: "official".into(),
        selected_source_kind: "official".into(),
        selected_url: tool.artifacts[0].canonical_url.clone(),
        artifact_integrity: tool.artifacts[0].artifact_integrity().unwrap().canonical(),
        artifact_format: "tar.xz".into(),
        base_artifact_integrities: vec![],
        bytes_downloaded: 0,
        install_root: sdk.display().to_string(),
        file_count: 2,
        total_size: 24,
        payload_digest: "0".repeat(64),
        pinset_version: pinset_version().into(),
        critical_entries: vec![flutter.clone(), dart.clone()],
        commands: Default::default(),
    };
    fs::write(
        sdk.join(".pinset-install.toml"),
        toml::to_string(&receipt).unwrap(),
    )
    .unwrap();
    let context = ProjectContext::at(root.path(), false).unwrap();
    let flutter_plan = plan_command(root.path(), &home, &context, "flutter").unwrap();
    let dart_plan = plan_command(root.path(), &home, &context, "dart").unwrap();
    assert_eq!(flutter_plan.sdk, dart_plan.sdk);
    assert_eq!(flutter_plan.install_identity, dart_plan.install_identity);
    assert_eq!(flutter_plan.executable, sdk.join(flutter));
    assert_eq!(dart_plan.executable, sdk.join(&dart));
    assert_eq!(
        dart_plan.environment["FLUTTER_ROOT"],
        sdk.display().to_string()
    );
    fs::remove_file(sdk.join(dart)).unwrap();
    assert_eq!(
        plan_command(root.path(), &home, &context, "dart")
            .unwrap_err()
            .code(),
        "PINSET_INSTALL_DAMAGED"
    );
}
