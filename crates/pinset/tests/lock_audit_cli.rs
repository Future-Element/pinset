use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

use pinset_core::{
    LockedArtifact, LockedArtifactFormat, Lockfile, MVP_NODE_TARGETS, NodeArchiveFormat,
    ProjectConfig, SourceConfig, VerificationStrength, current_target_for_tool, plan_node_artifact,
    save_lockfile, save_project_config,
};
use tempfile::tempdir;

#[test]
fn lock_audit_json_passes_with_a_matching_receipt_and_stable_info_code() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, "24.0.0");
    write_receipt(&home, "24.0.0");

    let output = pinset(
        &project,
        &home,
        &["lock", "audit", "--cwd", path_text(&project), "--json"],
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("audit JSON");
    assert_eq!(json["schema"], 1);
    assert_eq!(json["command"], "lock.audit");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["offline"], true);
    assert_eq!(json["data"]["passed"], true);
    assert_eq!(json["data"]["summary"]["errors"], 0);
    assert_eq!(json["data"]["summary"]["warnings"], 0);
    assert!(
        json["data"]["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["reason_code"] == "cache_entry_missing")
    );
}

#[test]
fn lock_audit_uses_exit_one_for_findings_and_does_not_repair_state() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    write_project(&project, "24.0.0");
    let install = home
        .join("installs")
        .join("node")
        .join("24.0.0")
        .join(current_target_for_tool("node"));
    fs::create_dir_all(&install).expect("unowned install");

    let output = pinset(&project, &home, &["lock", "audit", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("audit JSON");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["passed"], false);
    assert!(
        json["data"]["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["reason_code"] == "receipt_missing")
    );
    assert!(!install.join(".pinset-install.toml").exists());
}

#[test]
fn lock_audit_json_exposes_provenance_policy_reason_codes() {
    let root = tempdir().expect("temporary root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    let mut config = ProjectConfig {
        schema: 3,
        project_id: None,
        policy: Default::default(),
        tools: BTreeMap::from([("node".to_owned(), "24.0.0".to_owned())]),
        environment: None,
    };
    config.policy.verification_strength = Some(VerificationStrength::Provenance);
    save_project_config(&project.join("pinset.toml"), &config).expect("project config");
    save_lockfile(&project.join("pinset.lock"), &node_lockfile("24.0.0")).expect("lockfile");

    let output = pinset(&project, &home, &["lock", "audit", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("audit JSON");
    assert!(
        json["data"]["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| {
                finding["reason_code"] == "verification_below_policy"
                    && finding["category"] == "provenance"
            })
    );
}

fn write_project(project: &Path, version: &str) {
    let config = ProjectConfig {
        schema: 3,
        project_id: None,
        policy: Default::default(),
        tools: BTreeMap::from([("node".to_owned(), version.to_owned())]),
        environment: None,
    };
    save_project_config(&project.join("pinset.toml"), &config).expect("project config");
    save_lockfile(&project.join("pinset.lock"), &node_lockfile(version)).expect("lockfile");
}

fn write_receipt(home: &Path, version: &str) {
    let target = current_target_for_tool("node");
    let plan =
        plan_node_artifact(&SourceConfig::default(), version, &target).expect("Node artifact plan");
    let format = match plan.format {
        NodeArchiveFormat::Zip => "zip",
        NodeArchiveFormat::TarXz => "tar.xz",
    };
    let install = home
        .join("installs")
        .join("node")
        .join(version)
        .join(&target);
    fs::create_dir_all(&install).expect("install");
    fs::write(
        install.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"node\"\nversion = \"{version}\"\ntarget = \"{target}\"\ncanonical_url = \"{url}\"\nselected_source = \"fixture\"\nselected_source_kind = \"official\"\nselected_url = \"{url}\"\nartifact_integrity = \"sha256:{integrity}\"\nartifact_format = \"{format}\"\nbytes_downloaded = 0\n",
            url = plan.canonical_url,
            integrity = "ab".repeat(32),
        ),
    )
    .expect("receipt");
}

fn node_lockfile(version: &str) -> Lockfile {
    let artifacts = MVP_NODE_TARGETS
        .into_iter()
        .map(|target| {
            let plan = plan_node_artifact(&SourceConfig::default(), version, target)
                .expect("Node artifact plan");
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
                verification: "nodejs-openpgp-sha256".to_owned(),
                overlays: Vec::new(),
            }
        })
        .collect();
    Lockfile::new_node(
        "pinset lock audit integration test".to_owned(),
        version.to_owned(),
        "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
        "official".to_owned(),
        artifacts,
    )
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .output()
        .expect("run pinset")
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}
