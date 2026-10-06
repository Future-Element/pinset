use pinset_engine::*;
use std::fs;

fn project() -> (
    tempfile::TempDir,
    Services,
    ProjectContext,
    ProjectConfig,
    Lockfile,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let service = Services {
        cwd: root.clone(),
        home: dir.path().join("home/v3"),
    };
    service.init().unwrap();
    let context = ProjectContext::at(&root, false).unwrap();
    let config = context.load().unwrap();
    let lock = Lockfile::empty(config.project_id.clone());
    (dir, service, context, config, lock)
}

#[test]
fn interrupted_pair_blocks_reads_and_recovery_restores_both_files() {
    let (_dir, service, context, config, lock) = project();
    let before = fs::read(&context.config_path).unwrap();
    let mut new = config.clone();
    new.verification.timeout = 91;
    let id = service.begin_commit(&context, &new, &lock, None).unwrap();
    assert_eq!(
        context.load().unwrap_err().code(),
        "PINSET_TRANSACTION_PENDING"
    );
    let plan = service.upgrade_recover(true).unwrap();
    assert_eq!(plan["transactions"][0], id);
    assert!(context.load().is_err());
    service.upgrade_recover(false).unwrap();
    assert_eq!(fs::read(&context.config_path).unwrap(), before);
    assert!(!context.lock_path.exists());
    assert_eq!(context.load().unwrap(), config);
}

#[test]
fn recovery_discards_history_from_an_incomplete_apply() {
    let (_dir, service, context, config, lock) = project();
    service
        .begin_commit(&context, &config, &lock, Some("test-history".into()))
        .unwrap();
    let path = service.home.join("state/history/test-history.json");
    write_json(&path, &serde_json::json!({"uncompleted": true})).unwrap();
    service.upgrade_recover(false).unwrap();
    assert!(!path.exists());
}

#[test]
fn recovery_restores_history_before_an_interrupted_restore() {
    let (_dir, service, context, config, lock) = project();
    let path = service.home.join("state/history/test-history.json");
    let original = b"{\"restored\":false}";
    write_atomic(&path, original).unwrap();
    service
        .begin_commit(&context, &config, &lock, Some("test-history".into()))
        .unwrap();
    write_json(&path, &serde_json::json!({"restored": true})).unwrap();
    service.upgrade_recover(false).unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn finalized_state_is_readable_and_not_recovered_again() {
    let (_dir, service, context, config, lock) = project();
    let id = service
        .begin_commit(&context, &config, &lock, None)
        .unwrap();
    service.finish_commit(&context, &id).unwrap();
    assert_eq!(context.load_locked().unwrap().1, lock);
    assert!(
        service.upgrade_recover(false).unwrap()["transactions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn recovery_restores_encrypted_profiles_and_discards_new_profile_files() {
    let (_dir, service, context, mut config, lock) = project();
    let existing = context.root.join(".pinset/env/old.env");
    write_atomic(&existing, b"encrypted-before").unwrap();
    config
        .environment
        .profiles
        .insert("old".into(), ProfileConfig::default());
    write_atomic(
        &context.config_path,
        toml::to_string(&config).unwrap().as_bytes(),
    )
    .unwrap();
    config
        .environment
        .profiles
        .insert("new".into(), ProfileConfig::default());
    service
        .begin_commit(&context, &config, &lock, None)
        .unwrap();
    write_atomic(&existing, b"encrypted-after").unwrap();
    let new = context.root.join(".pinset/env/new.env");
    write_atomic(&new, b"new-ciphertext").unwrap();
    service.upgrade_recover(false).unwrap();
    assert_eq!(fs::read(existing).unwrap(), b"encrypted-before");
    assert!(!new.exists());
}

#[test]
fn external_change_cannot_be_retrusted_by_profile_mutation() {
    let (_dir, service, context, mut config, lock) = project();
    config.environment.profiles.insert(
        "development".into(),
        ProfileConfig {
            recipients: vec![],
            grants: Default::default(),
        },
    );
    write_atomic(
        &context.config_path,
        toml::to_string(&config).unwrap().as_bytes(),
    )
    .unwrap();
    write_atomic(
        &context.root.join(".pinset/env/development.env"),
        b"# pinset-encrypted-env v3\n",
    )
    .unwrap();
    let fingerprint = service.environment_fingerprint(&context).unwrap();
    pinset_env::trust_project(
        &service.home,
        &context.root,
        &config.project_id,
        &fingerprint,
    )
    .unwrap();
    config.verification.timeout = 10;
    service.commit(&context, &config, &lock, None).unwrap();
    assert!(service.profile_use(Some("development"), true).is_err());
    assert_eq!(service.trust("status").unwrap()["trusted"], false);
}
