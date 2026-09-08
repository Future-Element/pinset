use std::{collections::BTreeMap, fs, process::Command};

use pinset_core::{
    EnvironmentProfile, EnvironmentVariableContract, EnvironmentVariableType, ProjectConfig,
    ProjectEnvironment, decode_environment, save_project_config,
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
            tool_options: Default::default(),
            tasks: BTreeMap::new(),
            python: None,
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
        tool_options: Default::default(),
        tasks: BTreeMap::new(),
        python: None,
        environment: Some(changed),
    };
    save_project_config(&project.join("pinset.toml"), &changed_config).expect("changed config");
    let invalidated = broker(&project, &home, identity.secret().expose_secret());
    assert!(!invalidated.status.success());
    assert!(!String::from_utf8_lossy(&invalidated.stderr).contains("secret-value"));
}

#[test]
fn environment_contracts_validate_without_revealing_values_and_apply_defaults() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir(&project).expect("project directory");

    let identity = generate_identity();
    let recipient = identity.record.recipient.clone();
    let profiles = ["dev", "test"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                EnvironmentProfile {
                    file: format!("pinset.env/{name}.age"),
                    recipients: vec![recipient.clone()],
                },
            )
        })
        .collect();
    let environment = ProjectEnvironment {
        auto_profile: Some("dev".to_owned()),
        profiles,
        variables: BTreeMap::from([
            (
                "MODE".to_owned(),
                EnvironmentVariableContract {
                    kind: EnvironmentVariableType::Enum,
                    default: Some("development".to_owned()),
                    values: vec!["development".to_owned(), "production".to_owned()],
                    required: true,
                    secret: false,
                    profiles: Vec::new(),
                    description: None,
                },
            ),
            (
                "PORT".to_owned(),
                EnvironmentVariableContract {
                    kind: EnvironmentVariableType::Integer,
                    required: true,
                    secret: false,
                    default: None,
                    profiles: Vec::new(),
                    values: Vec::new(),
                    description: None,
                },
            ),
            (
                "TOKEN".to_owned(),
                EnvironmentVariableContract {
                    kind: EnvironmentVariableType::String,
                    required: true,
                    secret: true,
                    default: None,
                    profiles: Vec::new(),
                    values: Vec::new(),
                    description: None,
                },
            ),
        ]),
        ..ProjectEnvironment::default()
    };
    let project_id = "4c5652e4-0000-4000-8000-000000000003";
    save_project_config(
        &project.join("pinset.toml"),
        &ProjectConfig {
            schema: 5,
            project_id: Some(project_id.to_owned()),
            policy: Default::default(),
            tools: BTreeMap::new(),
            tool_options: Default::default(),
            tasks: BTreeMap::new(),
            python: None,
            environment: Some(environment.clone()),
        },
    )
    .expect("project config");
    write_encrypted_profile(
        &project,
        "pinset.env/dev.age",
        &EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([
                ("PORT".to_owned(), "not-an-integer".to_owned()),
                ("UNDECLARED".to_owned(), "must-not-appear".to_owned()),
            ]),
        },
        std::slice::from_ref(&recipient),
    )
    .expect("development profile");
    write_encrypted_profile(
        &project,
        "pinset.env/test.age",
        &EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([
                ("PORT".to_owned(), "42".to_owned()),
                ("TOKEN".to_owned(), "test-secret".to_owned()),
            ]),
        },
        std::slice::from_ref(&recipient),
    )
    .expect("test profile");

    let check = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "en")
        .env("PINSET_IDENTITY", identity.secret().expose_secret())
        .args(["env", "check", "--profile", "dev", "--json"])
        .output()
        .expect("check environment contract");
    assert_eq!(check.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).expect("check JSON");
    assert_eq!(report["data"]["ok"], false);
    assert_eq!(report["data"]["issues"].as_array().map(Vec::len), Some(2));
    let output = String::from_utf8_lossy(&check.stdout);
    assert!(!output.contains("must-not-appear"));

    let diff = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "en")
        .env("PINSET_IDENTITY", identity.secret().expose_secret())
        .args(["env", "diff", "dev", "test", "--json"])
        .output()
        .expect("diff environment profiles");
    assert!(diff.status.success());
    let report: serde_json::Value = serde_json::from_slice(&diff.stdout).expect("diff JSON");
    assert_eq!(
        report["data"]["only_left"],
        serde_json::json!(["UNDECLARED"])
    );
    assert!(!String::from_utf8_lossy(&diff.stdout).contains("test-secret"));

    write_encrypted_profile(
        &project,
        "pinset.env/dev.age",
        &EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([
                ("PORT".to_owned(), "3000".to_owned()),
                ("TOKEN".to_owned(), "development-secret".to_owned()),
            ]),
        },
        std::slice::from_ref(&recipient),
    )
    .expect("valid development profile");
    trust_project(
        &home,
        &project,
        project_id,
        &toml::to_string(&environment).expect("environment TOML"),
    )
    .expect("trust project");
    let resolved = broker(&project, &home, identity.secret().expose_secret());
    assert!(resolved.status.success());
    let variables = decode_environment(&resolved.stdout).expect("resolved environment");
    assert_eq!(
        variables.get("MODE").map(String::as_str),
        Some("development")
    );
    assert_eq!(variables.get("PORT").map(String::as_str), Some("3000"));
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
