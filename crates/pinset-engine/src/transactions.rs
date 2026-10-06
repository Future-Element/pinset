//! Atomic configuration/lock updates and explicit interrupted transaction recovery.
use crate::*;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionMarker {
    protocol: String,
    id: String,
}

fn profile_path(context: &ProjectContext, name: &str) -> PathBuf {
    let state = if context.global {
        context.root.clone()
    } else {
        context.root.join(".pinset")
    };
    state.join("env").join(format!("{name}.env"))
}

fn regular_file(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(service_error(
            "PINSET_RECOVERY_INVALID",
            "recovery state must be a regular file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    if regular_file(path)? {
        Ok(Some(fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}

impl Services {
    pub(crate) fn recover_transactions(&self, plan: bool) -> Result<Vec<String>> {
        let mut contexts = Vec::new();
        if let Some(context) = find_project_context(&self.cwd)? {
            contexts.push(context);
        }
        let global = self.home.join("global");
        if fs::symlink_metadata(&global).is_ok() {
            if fs::symlink_metadata(&global)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_RECOVERY_INVALID",
                    "global state cannot be a symlink",
                ));
            }
            contexts.push(ProjectContext::at(&global, true)?);
        }
        let mut recovered = Vec::new();
        for context in contexts {
            let marker = context.local.join("transaction.json");
            if !regular_file(&marker)? {
                continue;
            }
            let state: TransactionMarker = read_json(&marker)?;
            validate_id(&state.id)?;
            let directory = self.home.join("state/transactions");
            if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_RECOVERY_INVALID",
                    "transaction directory cannot be a symlink",
                ));
            }
            let path = directory.join(format!("{}.json", state.id));
            if !regular_file(&path)? {
                return Err(service_error(
                    "PINSET_RECOVERY_INVALID",
                    "transaction journal is missing",
                ));
            }
            let initial: TransactionJournal = read_json(&path)?;
            validate_id(&initial.project_id)?;
            let _guard = if plan {
                None
            } else {
                Some(self.guard(&initial.project_id)?)
            };
            // Another writer may have finalized the transaction before the lock was acquired.
            if !regular_file(&marker)? {
                continue;
            }
            let current: TransactionMarker = read_json(&marker)?;
            let mut journal: TransactionJournal = read_json(&path)?;
            if state.protocol != PROTOCOL
                || current.protocol != PROTOCOL
                || current.id != state.id
                || journal.protocol != PROTOCOL
                || journal.id != state.id
                || journal.project_id != initial.project_id
                || journal.root != context.root
                || !matches!(
                    journal.phase.as_str(),
                    "prepared" | "committed" | "recovered"
                )
            {
                return Err(service_error(
                    "PINSET_RECOVERY_INVALID",
                    "journal does not match the selected context",
                ));
            }
            let next: ProjectConfig = toml::from_str(&journal.new_config)?;
            let next_lock: Lockfile = toml::from_str(&journal.new_lock)?;
            next.validate_lock(&next_lock)?;
            if next.project_id != journal.project_id {
                return Err(service_error(
                    "PINSET_RECOVERY_INVALID",
                    "journal project identity changed",
                ));
            }
            if journal.phase == "prepared" {
                // Validate every rollback input before modifying any project state.
                let config: ProjectConfig = if journal.old_config.is_empty()
                    && context.global
                    && journal.old_lock.is_none()
                {
                    // The first global selection has no prior configuration to restore.
                    ProjectConfig::new(journal.project_id.clone())
                } else {
                    toml::from_str(&journal.old_config)?
                };
                let lock = journal
                    .old_lock
                    .as_ref()
                    .map(|text| toml::from_str(text))
                    .transpose()?
                    .unwrap_or_else(|| Lockfile::empty(config.project_id.clone()));
                config.validate_lock(&lock)?;
                if config.project_id != journal.project_id {
                    return Err(service_error(
                        "PINSET_RECOVERY_INVALID",
                        "prior project identity changed",
                    ));
                }
                for name in journal.profile_before.keys() {
                    validate_id(name)?;
                    regular_file(&profile_path(&context, name))?;
                }
                if !plan {
                    if journal.old_config.is_empty() {
                        if context.config_path.exists() {
                            fs::remove_file(&context.config_path)?;
                        }
                    } else {
                        write_atomic(&context.config_path, journal.old_config.as_bytes())?;
                    }
                    for (name, before) in &journal.profile_before {
                        let path = profile_path(&context, name);
                        if let Some(bytes) = before {
                            write_atomic(&path, bytes.as_bytes())?;
                        } else if path.exists() {
                            fs::remove_file(path)?;
                        }
                    }
                    if let Some(bytes) = &journal.old_lock {
                        write_atomic(&context.lock_path, bytes.as_bytes())?;
                    } else if context.lock_path.exists() {
                        fs::remove_file(&context.lock_path)?;
                    }
                    self.bind_local(&context, &config, &lock, false, true)?;
                    journal.phase = "recovered".into();
                    write_json(&path, &journal)?;
                }
            }
            if !plan {
                fs::remove_file(&marker)?;
            }
            recovered.push(journal.id);
        }
        Ok(recovered)
    }
}

impl Services {
    pub fn begin_commit(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        lock: &Lockfile,
    ) -> Result<String> {
        config.validate_lock(lock)?;
        if c.local.join("transaction.json").exists() {
            return Err(service_error(
                "PINSET_TRANSACTION_PENDING",
                "recover the existing transaction first",
            ));
        }
        let prior = read_optional(&c.config_path)?;
        let old: Option<ProjectConfig> = prior.as_deref().map(toml::from_str).transpose()?;
        let old_config = prior.unwrap_or_default();
        let mut profile_before = BTreeMap::new();
        for name in config
            .environment
            .profiles
            .keys()
            .chain(old.iter().flat_map(|c| c.environment.profiles.keys()))
        {
            let path = profile_path(c, name);
            if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(service_error(
                    "PINSET_PATH_UNSAFE",
                    "encrypted profile is a symlink",
                ));
            }
            let bytes = if path.exists() {
                Some(fs::read_to_string(path)?)
            } else {
                None
            };
            profile_before.insert(name.clone(), bytes);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let journal = TransactionJournal {
            protocol: PROTOCOL.into(),
            id: id.clone(),
            project_id: config.project_id.clone(),
            root: c.root.clone(),
            phase: "prepared".into(),
            old_config,
            old_lock: read_optional(&c.lock_path)?,
            new_config: toml::to_string_pretty(config)?,
            new_lock: toml::to_string_pretty(lock)?,
            profile_before,
        };
        let path = self
            .home
            .join("state/transactions")
            .join(format!("{id}.json"));
        write_json(&path, &journal)?;
        write_json(
            &c.local.join("transaction.json"),
            &json!({"protocol":PROTOCOL,"id":id}),
        )?;
        write_atomic(&c.config_path, journal.new_config.as_bytes())?;
        write_atomic(&c.lock_path, journal.new_lock.as_bytes())?;
        Ok(id)
    }
    pub fn finish_commit(&self, c: &ProjectContext, id: &str) -> Result<()> {
        validate_id(id)?;
        let path = self
            .home
            .join("state/transactions")
            .join(format!("{id}.json"));
        let mut journal: TransactionJournal = read_json(&path)?;
        if journal.protocol != PROTOCOL
            || journal.id != id
            || journal.root != c.root
            || journal.phase != "prepared"
        {
            return Err(service_error(
                "PINSET_TRANSACTION_INVALID",
                "transaction cannot be finalized",
            ));
        }
        self.register(c)?;
        journal.phase = "committed".into();
        write_json(&path, &journal)?;
        fs::remove_file(c.local.join("transaction.json"))?;
        Ok(())
    }
    pub fn commit(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        lock: &Lockfile,
    ) -> Result<String> {
        let id = self.begin_commit(c, config, lock)?;
        self.finish_commit(c, &id)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        new.policy.minimum_release_age = Some("1d".into());
        let id = service.begin_commit(&context, &new, &lock).unwrap();
        assert_eq!(
            context.load().unwrap_err().code(),
            "PINSET_TRANSACTION_PENDING"
        );
        let plan = service.recover_transactions(true).unwrap();
        assert_eq!(plan[0], id);
        assert!(context.load().is_err());
        service.recover_transactions(false).unwrap();
        assert_eq!(fs::read(&context.config_path).unwrap(), before);
        assert!(!context.lock_path.exists());
        assert_eq!(context.load().unwrap(), config);
    }

    #[test]
    fn finalized_state_is_readable_and_not_recovered_again() {
        let (_dir, service, context, config, lock) = project();
        let id = service.begin_commit(&context, &config, &lock).unwrap();
        service.finish_commit(&context, &id).unwrap();
        assert_eq!(context.load_locked().unwrap().1, lock);
        assert!(service.recover_transactions(false).unwrap().is_empty());
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
        service.begin_commit(&context, &config, &lock).unwrap();
        write_atomic(&existing, b"encrypted-after").unwrap();
        let new = context.root.join(".pinset/env/new.env");
        write_atomic(&new, b"new-ciphertext").unwrap();
        service.recover_transactions(false).unwrap();
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
        config.policy.minimum_release_age = Some("1d".into());
        service.commit(&context, &config, &lock).unwrap();
        assert!(service.profile_use(Some("development"), true).is_err());
        assert_eq!(service.trust("status").unwrap()["trusted"], false);
    }

    #[test]
    fn recovery_preview_preserves_all_transaction_bytes() {
        let (_dir, service, context, mut config, lock) = project();
        config.policy.minimum_release_age = Some("2d".into());
        let id = service.begin_commit(&context, &config, &lock).unwrap();
        let paths = [
            context.config_path.clone(),
            context.lock_path.clone(),
            context.local.join("transaction.json"),
            service.home.join(format!("state/transactions/{id}.json")),
        ];
        let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
        assert_eq!(service.recover_transactions(true).unwrap(), vec![id]);
        assert_eq!(
            paths
                .iter()
                .map(|p| fs::read(p).unwrap())
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn committed_and_recovered_markers_are_safe_to_finish_repeatedly() {
        for phase in ["committed", "recovered"] {
            let (_dir, service, context, mut config, lock) = project();
            config.policy.minimum_release_age = Some("2d".into());
            let id = service.begin_commit(&context, &config, &lock).unwrap();
            let path = service.home.join(format!("state/transactions/{id}.json"));
            let mut journal: TransactionJournal = read_json(&path).unwrap();
            journal.phase = phase.into();
            write_json(&path, &journal).unwrap();
            let before = fs::read(&context.config_path).unwrap();
            assert_eq!(service.recover_transactions(false).unwrap(), vec![id]);
            assert_eq!(fs::read(&context.config_path).unwrap(), before);
            assert!(!context.local.join("transaction.json").exists());
            assert!(service.recover_transactions(false).unwrap().is_empty());
        }
    }

    #[test]
    fn global_recovery_works_outside_a_project_without_touching_another_project() {
        let (dir, mut service, context, config, lock) = project();
        let project_id = service.begin_commit(&context, &config, &lock).unwrap();
        let global_root = service.home.join("global");
        fs::create_dir_all(&global_root).unwrap();
        let global = ProjectContext::at(&global_root, true).unwrap();
        let original = ProjectConfig::new("global-test".into());
        write_atomic(
            &global.config_path,
            toml::to_string(&original).unwrap().as_bytes(),
        )
        .unwrap();
        let mut changed = original.clone();
        changed.policy.minimum_release_age = Some("1d".into());
        let id = service
            .begin_commit(
                &global,
                &changed,
                &Lockfile::empty(original.project_id.clone()),
            )
            .unwrap();
        service.cwd = dir.path().join("outside");
        fs::create_dir(&service.cwd).unwrap();
        assert_eq!(service.recover_transactions(false).unwrap(), vec![id]);
        assert_eq!(global.load().unwrap(), original);
        let marker: TransactionMarker = read_json(&context.local.join("transaction.json")).unwrap();
        assert_eq!(marker.id, project_id);
    }

    #[test]
    fn invalid_rollback_is_rejected_before_any_write() {
        let (_dir, service, context, config, lock) = project();
        let id = service.begin_commit(&context, &config, &lock).unwrap();
        let path = service.home.join(format!("state/transactions/{id}.json"));
        let mut journal: TransactionJournal = read_json(&path).unwrap();
        journal.old_config = "invalid configuration".into();
        write_json(&path, &journal).unwrap();
        let before = fs::read(&context.config_path).unwrap();
        assert!(service.recover_transactions(false).is_err());
        assert_eq!(fs::read(&context.config_path).unwrap(), before);
        assert!(context.local.join("transaction.json").exists());
    }

    #[test]
    fn first_global_selection_recovers_to_no_global_default() {
        let (_dir, service, _context, _config, _lock) = project();
        let root = service.home.join("global");
        fs::create_dir_all(&root).unwrap();
        let global = ProjectContext::at(&root, true).unwrap();
        let config = ProjectConfig::new("global".into());
        let id = service
            .begin_commit(&global, &config, &Lockfile::empty("global".into()))
            .unwrap();
        assert_eq!(
            service.recover_transactions(true).unwrap(),
            vec![id.clone()]
        );
        assert!(global.config_path.exists());
        assert_eq!(service.recover_transactions(false).unwrap(), vec![id]);
        assert!(!global.config_path.exists());
        assert!(!global.lock_path.exists());
        assert!(service.recover_transactions(false).unwrap().is_empty());
    }

    #[test]
    fn repair_preview_does_not_initialize_a_home() {
        let dir = tempfile::tempdir().unwrap();
        let service = Services {
            cwd: dir.path().to_path_buf(),
            home: dir.path().join("home/v3"),
        };
        assert_eq!(service.self_repair(true).unwrap()["plan"], true);
        assert!(!service.home.exists());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_refuses_a_symlink_journal() {
        let (_dir, service, context, config, lock) = project();
        let id = service.begin_commit(&context, &config, &lock).unwrap();
        let path = service.home.join(format!("state/transactions/{id}.json"));
        let foreign = service.home.join("foreign.json");
        fs::rename(&path, &foreign).unwrap();
        std::os::unix::fs::symlink(&foreign, &path).unwrap();
        assert_eq!(
            service.recover_transactions(false).unwrap_err().code(),
            "PINSET_RECOVERY_INVALID"
        );
        assert!(context.local.join("transaction.json").exists());
    }
}
