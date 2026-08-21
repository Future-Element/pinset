use std::{collections::BTreeMap, fs, process::Command};

use pinset_core::{
    EnvironmentProfile, ProjectConfig, ProjectEnvironment, decode_environment, save_project_config,
};
use pinset_env::{EnvironmentDocument, generate_identity, trust_project, write_encrypted_profile};
use secrecy::ExposeSecret;
use tempfile::tempdir;

#[test]
fn hidden_broker_requires_bound_trust_and_returns_only_the_selected_profile() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir(&project).expect("project directory");

    let identity = generate_identity();
    let recipient = identity.record.recipient.clone();
    let environment = ProjectEnvironment {
        auto_profile: Some("development".to_owned()),
        profiles: BTreeMap::from([(
            "development".to_owned(),
            EnvironmentProfile {
                file: "pinset.env/development.age".to_owned(),
                recipients: vec![recipient.clone()],
            },
        )]),
        ..ProjectEnvironment::default()
    };
    let project_id = "4c5652e4-0000-4000-8000-000000000000";
    save_project_config(
        &project.join("pinset.toml"),
        &ProjectConfig {
            schema: 4,
            project_id: Some(project_id.to_owned()),
            policy: Default::default(),
            tools: BTreeMap::new(),
            environment: Some(environment.clone()),
        },
    )
    .expect("project config");
    write_encrypted_profile(
        &project,
        "pinset.env/development.age",
        &EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([("DATABASE_URL".to_owned(), "secret-value".to_owned())]),
        },
        &[recipient],
    )
    .expect("encrypted profile");

    let untrusted = broker(&project, &home, identity.secret().expose_secret());
    assert!(!untrusted.status.success());
    assert!(!String::from_utf8_lossy(&untrusted.stderr).contains("secret-value"));

    trust_project(
        &home,
        &project,
        project_id,
        &toml::to_string(&environment).expect("environment TOML"),
    )
    .expect("trust project");
    let trusted = broker(&project, &home, identity.secret().expose_secret());
    assert!(
        trusted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&trusted.stderr)
    );
    let decoded = decode_environment(&trusted.stdout).expect("binary environment protocol");
    assert_eq!(
        decoded.get("DATABASE_URL").map(String::as_str),
        Some("secret-value")
    );
    assert_eq!(decoded.len(), 1);

    let mut changed = environment;
    changed.collision = pinset_core::EnvironmentCollision::ProcessWins;
    let changed_config = ProjectConfig {
        schema: 4,
        project_id: Some(project_id.to_owned()),
        policy: Default::default(),
        tools: BTreeMap::new(),
        environment: Some(changed),
    };
    save_project_config(&project.join("pinset.toml"), &changed_config).expect("changed config");
    let invalidated = broker(&project, &home, identity.secret().expose_secret());
    assert!(!invalidated.status.success());
    assert!(!String::from_utf8_lossy(&invalidated.stderr).contains("secret-value"));
}

fn broker(
    project: &std::path::Path,
    home: &std::path::Path,
    identity: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(["__env-resolve", "--cwd"])
        .arg(project)
        .args(["--shim-version", pinset_core::pinset_version()])
        .env("PINSET_HOME", home)
        .env("PINSET_IDENTITY", identity)
        .output()
        .expect("run hidden environment broker")
}
