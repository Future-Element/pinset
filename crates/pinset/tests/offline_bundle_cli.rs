use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

use pinset_core::{
    LockedArtifact, LockedArtifactFormat, LockedTool, Lockfile, MVP_NODE_TARGETS, ProjectConfig,
    SourceConfig, plan_node_artifact, save_lockfile, save_project_config,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn bundle_round_trip_verifies_identity_and_supports_offline_cache() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    let source_home = temporary.path().join("source-home");
    let target_home = temporary.path().join("target-home");
    fs::create_dir(&project).expect("project");
    let bytes = b"verified-offline-runtime-archive";
    let hash = hex::encode(Sha256::digest(bytes));
    let archive = temporary.path().join("runtime.tar.gz");
    fs::write(&archive, bytes).expect("archive");
    write_project(&project, &hash);

    assert_success(pinset(
        &project,
        &source_home,
        &["cache", "import", path_text(&archive), "--sha256", &hash],
    ));
    let bundle = temporary.path().join("project.pinset-bundle.tar.gz");
    assert_success(pinset(
        &project,
        &source_home,
        &["bundle", "export", "--output", path_text(&bundle), "--json"],
    ));
    assert_success(pinset(
        &project,
        &target_home,
        &["bundle", "import", path_text(&bundle), "--json"],
    ));
    let cached = target_home
        .join("downloads/sha256")
        .join(format!("{hash}.archive"));
    assert_eq!(fs::read(cached).expect("imported cache"), bytes);
    assert_success(pinset(&project, &target_home, &["cache", "verify"]));
}

#[test]
fn offline_install_reports_all_missing_artifacts_without_network() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, &"ab".repeat(32));
    let output = pinset(&project, &home, &["install", "--locked", "--offline"]);
    assert_eq!(output.status.code(), Some(2));
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        message.contains("offline cache is missing 1 artifact"),
        "{message}"
    );
    assert!(!home.join("downloads").exists());
}

fn write_project(project: &Path, hash: &str) {
    let config = ProjectConfig {
        schema: 5,
        project_id: Some("565652e4-0000-4000-8000-000000000025".to_owned()),
        policy: Default::default(),
        tools: BTreeMap::from([("node".to_owned(), "24.0.0".to_owned())]),
        tool_options: Default::default(),
        tasks: BTreeMap::new(),
        python: None,
        workspace: None,
        environment: None,
    };
    save_project_config(&project.join("pinset.toml"), &config).expect("config");
    let lock = Lockfile {
        schema: 3,
        generated_by: "offline bundle test".to_owned(),
        tools: vec![LockedTool {
            name: "node".to_owned(),
            requested: "24.0.0".to_owned(),
            version: "24.0.0".to_owned(),
            provider: "nodejs-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::from([
                (
                    "signature_primary_fingerprint".to_owned(),
                    "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
                ),
                (
                    "signed_manifest".to_owned(),
                    "SHASUMS256.txt.asc".to_owned(),
                ),
                ("manifest_source".to_owned(), "official".to_owned()),
            ]),
            options: Default::default(),
            artifacts: MVP_NODE_TARGETS
                .into_iter()
                .map(|target| {
                    let plan = plan_node_artifact(&SourceConfig::default(), "24.0.0", target)
                        .expect("node artifact plan");
                    LockedArtifact {
                        target: target.to_owned(),
                        canonical_url: plan.canonical_url,
                        artifact_path: plan.artifact_path,
                        sha256: hash.to_owned(),
                        integrity: None,
                        format: match plan.format {
                            pinset_core::NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                            pinset_core::NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                        },
                        archive_root: plan.archive_root,
                        verification: "nodejs-openpgp-sha256".to_owned(),
                        overlays: Vec::new(),
                    }
                })
                .collect(),
        }],
    };
    save_lockfile(&project.join("pinset.lock"), &lock).expect("lock");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .output()
        .expect("run pinset")
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}
