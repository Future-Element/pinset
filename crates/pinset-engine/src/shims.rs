//! One recoverable publication of the CLI/router pair, aliases and registry.
//! Targets are never unlinked before replacement, including active Windows EXEs.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    protocol: String,
    digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli_digest: Option<String>,
    commands: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    before: Option<String>,
    after: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refresh {
    protocol: String,
    id: String,
    target: String,
    phase: String,
    cli_digest: String,
    before: Option<Registry>,
    after: Registry,
    entries: Vec<Entry>,
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn validate_registry(registry: &Registry) -> Result<()> {
    if registry.protocol != PROTOCOL
        || !valid_digest(&registry.digest)
        || registry
            .cli_digest
            .as_ref()
            .is_some_and(|digest| !valid_digest(digest))
        || registry.commands.iter().any(|(name, tool)| {
            validate_registered_command(&normalized_command_name(name, &current_target()), tool)
                .is_err()
        })
    {
        return Err(service_error(
            "PINSET_SHIM_STATE",
            "invalid managed command registry",
        ));
    }
    Ok(())
}
fn fingerprint(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(Some(digest(&fs::read(path)?)))
        }
        Ok(_) => Err(service_error(
            "PINSET_SHIM_EXTERNAL",
            format!("managed entry must be a regular file: {}", path.display()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn registry_at(path: &Path) -> Result<Option<Registry>> {
    if fingerprint(path)?.is_none() {
        return Ok(None);
    }
    let registry: Registry = read_json(path)?;
    validate_registry(&registry)?;
    Ok(Some(registry))
}
fn transaction_path(home: &Path) -> PathBuf {
    home.join("state/shim-refresh/transaction.json")
}
fn staging(bin: &Path, journal: &Refresh) -> PathBuf {
    bin.join(format!(".pinset-shims-{}", journal.id))
}
fn cli_name(target: &str) -> String {
    executable_name("pinset", target)
}
fn shim_name(target: &str) -> String {
    executable_name("pinset-shim", target)
}
fn expected_files(journal: &Refresh) -> BTreeMap<String, String> {
    let mut files = journal
        .after
        .commands
        .keys()
        .map(|name| {
            (
                executable_name(name, &journal.target),
                journal.after.digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    files.insert(cli_name(&journal.target), journal.cli_digest.clone());
    files.insert(shim_name(&journal.target), journal.after.digest.clone());
    files
}
fn validate_record(journal: &Refresh) -> Result<()> {
    let invalid = || {
        service_error(
            "PINSET_SHIM_RECOVERY",
            "invalid managed entry refresh journal",
        )
    };
    if journal.protocol != PROTOCOL
        || journal.target != current_target()
        || uuid::Uuid::parse_str(&journal.id)
            .ok()
            .is_none_or(|id| id.to_string() != journal.id)
        || !matches!(
            journal.phase.as_str(),
            "prepared" | "rolling-back" | "committed"
        )
        || !valid_digest(&journal.cli_digest)
    {
        return Err(invalid());
    }
    validate_registry(&journal.after)?;
    if journal.after.cli_digest.as_ref() != Some(&journal.cli_digest) {
        return Err(invalid());
    }
    if let Some(before) = &journal.before {
        validate_registry(before)?;
    }
    let expected = expected_files(journal);
    let mut seen = BTreeSet::new();
    for entry in &journal.entries {
        if !seen.insert(&entry.name)
            || expected.get(&entry.name) != Some(&entry.after)
            || entry
                .before
                .as_ref()
                .is_some_and(|value| !valid_digest(value))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn validate_journal(bin: &Path, journal: &Refresh) -> Result<()> {
    validate_record(journal)?;
    let invalid = || {
        service_error(
            "PINSET_SHIM_RECOVERY",
            "invalid managed entry refresh payload",
        )
    };
    let directory = staging(bin, journal);
    if !fs::symlink_metadata(&directory)?.is_dir()
        || fs::symlink_metadata(&directory)?.file_type().is_symlink()
    {
        return Err(invalid());
    }
    for entry in &journal.entries {
        if let Some(before) = &entry.before
            && fingerprint(&directory.join(format!("old-{}", entry.name)))?.as_ref() != Some(before)
        {
            return Err(invalid());
        }
    }
    if fingerprint(&directory.join("source-cli"))?.as_ref() != Some(&journal.cli_digest)
        || fingerprint(&directory.join("source-shim"))?.as_ref() != Some(&journal.after.digest)
    {
        return Err(invalid());
    }
    Ok(())
}
fn check_live(home: &Path, journal: &Refresh, incoming: Option<(&str, &str)>) -> Result<()> {
    let current = registry_at(&home.join("state/shims.json"))?;
    if current != journal.before && current.as_ref() != Some(&journal.after) {
        return Err(service_error(
            "PINSET_SHIM_EXTERNAL",
            "registry changed during managed entry refresh",
        ));
    }
    let bin = home.join("bin");
    for (name, after) in expected_files(journal) {
        let found = fingerprint(&bin.join(&name))?;
        let entry = journal.entries.iter().find(|entry| entry.name == name);
        let owned = found.as_ref() == Some(&after)
            || entry.is_some_and(|entry| found == entry.before || found.is_none())
            || (name == cli_name(&journal.target)
                && journal
                    .before
                    .as_ref()
                    .and_then(|registry| registry.cli_digest.as_ref())
                    .is_some_and(|before| found.as_ref() == Some(before)))
            || (name == shim_name(&journal.target)
                && journal.before.as_ref().map(|registry| &registry.digest) == found.as_ref())
            || incoming.is_some_and(|(cli, shim)| {
                (name == cli_name(&journal.target) && found.as_deref() == Some(cli))
                    || (name == shim_name(&journal.target) && found.as_deref() == Some(shim))
            });
        if !owned {
            return Err(service_error(
                "PINSET_SHIM_EXTERNAL",
                format!("entry changed during managed refresh: {name}"),
            ));
        }
    }
    Ok(())
}
fn publish(source: &Path, target: &Path) -> Result<()> {
    // Rust 1.97 uses FileRenameInfoEx POSIX replacement on modern Windows when
    // MoveFileEx denies an in-use target. If unsupported, retain the old name.
    fs::rename(source, target).map_err(|error| {
        if cfg!(windows) { service_error("PINSET_SHIM_BUSY", format!("cannot atomically replace {}; close the running command and rerun self repair: {error}", target.display())) }
        else { error.into() }
    })
}
fn copy_staged_binary(source: &Path, target: &Path) -> Result<()> {
    fs::copy(source, target).map_err(|error| {
        service_error(
            "PINSET_SHIM_IO",
            format!(
                "cannot stage {} as {}: {error}",
                source.display(),
                target.display()
            ),
        )
    })?;
    // Windows FlushFileBuffers requires a writable handle, even when the
    // completed copy only needs to be flushed before publication.
    let file = fs::OpenOptions::new()
        .write(true)
        .open(target)
        .map_err(|error| {
            service_error(
                "PINSET_SHIM_IO",
                format!(
                    "cannot open staged binary {} for flushing: {error}",
                    target.display()
                ),
            )
        })?;
    file.sync_all().map_err(|error| {
        service_error(
            "PINSET_SHIM_IO",
            format!("cannot flush staged binary {}: {error}", target.display()),
        )
    })
}
fn finish(
    home: &Path,
    journal: &mut Refresh,
    rollback: bool,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<()> {
    let bin = home.join("bin");
    let directory = staging(&bin, journal);
    validate_journal(&bin, journal)?;
    if rollback && journal.phase != "rolling-back" {
        journal.phase = "rolling-back".into();
        write_json(&transaction_path(home), journal)?;
        checkpoint()?;
    }
    for entry in &journal.entries {
        let desired = if rollback {
            entry.before.as_ref()
        } else {
            Some(&entry.after)
        };
        let target = bin.join(&entry.name);
        if fingerprint(&target)?.as_ref() == desired {
            continue;
        }
        if let Some(desired) = desired {
            let stage = directory.join(format!("stage-{}", entry.name));
            if let Some(found) = fingerprint(&stage)?
                && &found != desired
            {
                if found != entry.after && entry.before.as_ref() != Some(&found) {
                    return Err(service_error(
                        "PINSET_SHIM_EXTERNAL",
                        "staged entry changed outside the refresh",
                    ));
                }
                fs::remove_file(&stage)?;
            }
            if fingerprint(&stage)?.is_none() {
                let source = if rollback {
                    directory.join(format!("old-{}", entry.name))
                } else {
                    directory.join(if entry.name == cli_name(&journal.target) {
                        "source-cli"
                    } else {
                        "source-shim"
                    })
                };
                fs::hard_link(source, &stage)?;
            }
            publish(&stage, &target)?;
        } else if fingerprint(&target)?.is_some() {
            // This entry did not exist before the aborted transaction.
            fs::remove_file(&target)?;
        }
        checkpoint()?;
    }
    let state = home.join("state/shims.json");
    if rollback {
        if let Some(before) = &journal.before {
            write_json(&state, before)?;
        } else if state.exists() {
            fs::remove_file(&state)?;
        }
    } else {
        write_json(&state, &journal.after)?;
    }
    checkpoint()?;
    journal.phase = "committed".into();
    write_json(&transaction_path(home), journal)?;
    checkpoint()?;
    // Keep a validated manifest for deferred Windows cleanup of active backups.
    write_json(&directory.join("manifest.json"), journal)?;
    fs::remove_file(transaction_path(home))?;
    cleanup(&bin, journal);
    Ok(())
}
fn cleanup(bin: &Path, journal: &Refresh) {
    let directory = staging(bin, journal);
    let mut owned = BTreeMap::from([
        ("source-cli".into(), journal.cli_digest.clone()),
        ("source-shim".into(), journal.after.digest.clone()),
    ]);
    for entry in &journal.entries {
        if let Some(before) = &entry.before {
            owned.insert(format!("old-{}", entry.name), before.clone());
        }
        // A leftover stage may represent either direction of a resumed refresh.
        let stage = format!("stage-{}", entry.name);
        if let Ok(Some(found)) = fingerprint(&directory.join(&stage))
            && (found == entry.after || entry.before.as_ref() == Some(&found))
        {
            owned.insert(stage, found);
        }
    }
    for (name, hash) in owned {
        let path = directory.join(name);
        if fingerprint(&path).ok().flatten().as_ref() == Some(&hash) {
            let _ = fs::remove_file(path);
        }
    }
    if fs::read_dir(&directory).is_ok_and(|entries| {
        entries
            .filter_map(|entry| entry.ok())
            .all(|entry| entry.file_name() == "manifest.json")
    }) {
        let _ = fs::remove_file(directory.join("manifest.json"));
        let _ = fs::remove_dir(directory);
    }
}
fn cleanup_completed(bin: &Path) -> Result<()> {
    for entry in fs::read_dir(bin)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(".pinset-shims-") {
            continue;
        }
        let manifest = entry.path().join("manifest.json");
        if fingerprint(&manifest).ok().flatten().is_none() {
            continue;
        }
        if let Ok(journal) = read_json::<Refresh>(&manifest)
            && journal.phase == "committed"
            && validate_record(&journal).is_ok()
            && staging(bin, &journal) == entry.path()
        {
            cleanup(bin, &journal);
        }
    }
    Ok(())
}
impl Services {
    pub fn install_shims(&self) -> Result<()> {
        let cli = crate::self_update::resolve_cli_executable(&std::env::current_exe()?)?;
        let shim = cli
            .parent()
            .ok_or_else(|| service_error("PINSET_SHIM_MISSING", "missing adjacent router"))?
            .join(shim_name(&current_target()));
        self.install_shims_from(&cli, &shim)
    }
    pub(crate) fn install_shims_from(&self, source_cli: &Path, source_shim: &Path) -> Result<()> {
        let _guard = self.resource_guard("shims")?;
        let refresh = self.home.join("state/shim-refresh");
        match fs::symlink_metadata(&refresh) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(service_error(
                    "PINSET_SHIM_RECOVERY",
                    "shim refresh state must be a real directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let cli_digest = fingerprint(source_cli)?
            .ok_or_else(|| service_error("PINSET_SHIM_MISSING", "missing source CLI"))?;
        let shim_digest = fingerprint(source_shim)?.ok_or_else(|| {
            service_error(
                "PINSET_SHIM_MISSING",
                "pinset-shim must be distributed beside pinset",
            )
        })?;
        let bin = self.home.join("bin");
        fs::create_dir_all(&bin)?;
        let pending = transaction_path(&self.home);
        if fingerprint(&pending)?.is_some() {
            let mut journal: Refresh = read_json(&pending)?;
            validate_journal(&bin, &journal)?;
            check_live(&self.home, &journal, Some((&cli_digest, &shim_digest)))?;
            let rollback = journal.phase == "rolling-back"
                || journal.cli_digest != cli_digest
                || journal.after.digest != shim_digest;
            finish(&self.home, &mut journal, rollback, || Ok(()))?;
        }
        cleanup_completed(&bin)?;
        let state = self.home.join("state/shims.json");
        let before = registry_at(&state)?;
        let mut commands = public_commands();
        let mut generic_commands = BTreeMap::new();
        if let Some(prior) = &before {
            commands.extend(prior.commands.keys().cloned());
            generic_commands.extend(
                prior
                    .commands
                    .iter()
                    .filter(|(_, tool)| !TOOLS.contains(&tool.as_str()))
                    .map(|(command, tool)| (command.clone(), tool.clone())),
            );
        }
        let mut inventory_tools = vec!["java".to_owned(), "python".to_owned()];
        if self.home.join("installs").is_dir() {
            for entry in fs::read_dir(self.home.join("installs"))? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_dir() && validate_generic_name(&name).is_ok() {
                    inventory_tools.push(name);
                }
            }
        }
        for tool in inventory_tools {
            let installs = self.home.join("installs").join(&tool);
            if !installs.is_dir() {
                continue;
            }
            for identity in fs::read_dir(installs)? {
                let identity = identity?;
                if !identity.file_type()?.is_dir() {
                    continue;
                }
                for target in fs::read_dir(identity.path())? {
                    let target = target?;
                    if !target.file_type()?.is_dir() {
                        continue;
                    }
                    if let Ok(receipt) = fs::read_to_string(
                        target.path().join(".pinset-install.toml"),
                    )
                    .and_then(|text| {
                        toml::from_str::<InstallReceipt>(&text).map_err(std::io::Error::other)
                    }) && receipt.schema == 3
                        && receipt.complete
                        && receipt.tool == tool
                    {
                        if tool == "java" {
                            commands.extend(
                                receipt
                                    .commands
                                    .keys()
                                    .filter(|name| validate_managed_command_name(name).is_ok())
                                    .cloned(),
                            );
                        } else if tool == "python"
                            && semver::Version::parse(&receipt.version)
                                .is_ok_and(|version| version.major == 3)
                        {
                            commands.extend(python_version_commands(&receipt.version));
                        } else if validate_generic_name(&tool).is_ok()
                            && receipt.commands.len() == 1
                            && receipt
                                .commands
                                .get(&tool)
                                .is_some_and(|path| validate_generic_path(path).is_ok())
                        {
                            commands.push(tool.clone());
                            generic_commands.insert(tool.clone(), tool.clone());
                        }
                    }
                }
            }
        }
        commands = commands
            .into_iter()
            .map(|command| normalized_command_name(&command, &current_target()))
            .collect();
        commands.sort();
        commands.dedup();
        let after = Registry {
            protocol: PROTOCOL.into(),
            digest: shim_digest.clone(),
            cli_digest: Some(cli_digest.clone()),
            commands: commands
                .into_iter()
                .map(|command| {
                    let tool = generic_commands
                        .get(&command)
                        .cloned()
                        .unwrap_or_else(|| command_tool(&command).unwrap_or("java").to_owned());
                    (command, tool)
                })
                .collect(),
        };
        validate_registry(&after)?;
        let mut journal = Refresh {
            protocol: PROTOCOL.into(),
            id: uuid::Uuid::new_v4().to_string(),
            target: current_target(),
            phase: "prepared".into(),
            cli_digest: cli_digest.clone(),
            before,
            after,
            entries: vec![],
        };
        // Inspect every target before any pair, alias or registry is changed.
        for (name, after) in expected_files(&journal) {
            let target = bin.join(&name);
            let found = fingerprint(&target)?;
            if found.as_ref() == Some(&after) {
                continue;
            }
            if let Some(found) = &found {
                let owned = if name == cli_name(&journal.target) {
                    journal
                        .before
                        .as_ref()
                        .and_then(|registry| registry.cli_digest.as_ref())
                        == Some(found)
                } else {
                    journal
                        .before
                        .as_ref()
                        .is_some_and(|registry| &registry.digest == found)
                };
                if !owned {
                    return Err(service_error(
                        "PINSET_SHIM_EXTERNAL",
                        format!(
                            "refusing to replace external command {name}; confirm or move the unowned entry before self repair"
                        ),
                    ));
                }
            }
            journal.entries.push(Entry {
                name,
                before: found,
                after,
            });
        }
        if journal.entries.is_empty() && journal.before.as_ref() == Some(&journal.after) {
            return Ok(());
        }
        let directory = staging(&bin, &journal);
        fs::create_dir(&directory)?;
        for (source, name) in [(source_cli, "source-cli"), (source_shim, "source-shim")] {
            copy_staged_binary(source, &directory.join(name))?;
        }
        for entry in &journal.entries {
            if entry.before.is_some() {
                fs::hard_link(
                    bin.join(&entry.name),
                    directory.join(format!("old-{}", entry.name)),
                )?;
            }
        }
        #[cfg(unix)]
        fs::File::open(&directory)?.sync_all()?;
        write_json(&pending, &journal)?;
        check_live(&self.home, &journal, None)?;
        finish(&self.home, &mut journal, false, || Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, Refresh) {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("home");
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let old_cli = digest(b"old-cli");
        let old_shim = digest(b"old-shim");
        let commands = BTreeMap::from([
            ("node".into(), "node".into()),
            ("python3.12".into(), "python".into()),
        ]);
        let before = Registry {
            protocol: PROTOCOL.into(),
            digest: old_shim.clone(),
            cli_digest: Some(old_cli.clone()),
            commands: commands.clone(),
        };
        let mut journal = Refresh {
            protocol: PROTOCOL.into(),
            id: uuid::Uuid::new_v4().to_string(),
            target: current_target(),
            phase: "prepared".into(),
            cli_digest: digest(b"new-cli"),
            before: Some(before.clone()),
            after: Registry {
                protocol: PROTOCOL.into(),
                digest: digest(b"new-shim"),
                cli_digest: Some(digest(b"new-cli")),
                commands,
            },
            entries: vec![],
        };
        let directory = staging(&bin, &journal);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("source-cli"), b"new-cli").unwrap();
        fs::write(directory.join("source-shim"), b"new-shim").unwrap();
        for (name, after) in expected_files(&journal) {
            let cli = name == cli_name(&journal.target);
            fs::write(
                bin.join(&name),
                if cli {
                    &b"old-cli"[..]
                } else {
                    &b"old-shim"[..]
                },
            )
            .unwrap();
            fs::hard_link(bin.join(&name), directory.join(format!("old-{name}"))).unwrap();
            journal.entries.push(Entry {
                name,
                before: Some(if cli {
                    old_cli.clone()
                } else {
                    old_shim.clone()
                }),
                after,
            });
        }
        write_json(&home.join("state/shims.json"), &before).unwrap();
        write_json(&transaction_path(&home), &journal).unwrap();
        (temporary, home, journal)
    }
    fn assert_direction(home: &Path, journal: &Refresh, rollback: bool) {
        for entry in &journal.entries {
            assert_eq!(
                fingerprint(&home.join("bin").join(&entry.name))
                    .unwrap()
                    .as_ref(),
                if rollback {
                    entry.before.as_ref()
                } else {
                    Some(&entry.after)
                }
            );
        }
        assert_eq!(
            registry_at(&home.join("state/shims.json")).unwrap(),
            if rollback {
                journal.before.clone()
            } else {
                Some(journal.after.clone())
            }
        );
        assert!(!transaction_path(home).exists());
    }
    #[test]
    fn every_publication_boundary_can_resume_without_consuming_backups() {
        for fail in 0..6 {
            let (_temporary, home, mut journal) = fixture();
            let mut step = 0;
            let error = finish(&home, &mut journal, false, || {
                let current = step;
                step += 1;
                if current == fail {
                    Err(service_error(
                        "PINSET_TEST_INTERRUPTED",
                        "injected publication interruption",
                    ))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert_eq!(error.code(), "PINSET_TEST_INTERRUPTED");
            let mut pending: Refresh = read_json(&transaction_path(&home)).unwrap();
            check_live(&home, &pending, None).unwrap();
            finish(&home, &mut pending, false, || Ok(())).unwrap();
            assert_direction(&home, &journal, false);
        }
    }
    #[test]
    fn every_rollback_boundary_can_resume_and_restore_the_complete_registry() {
        for fail in 0..6 {
            let (_temporary, home, mut journal) = fixture();
            let mut published = 0;
            finish(&home, &mut journal, false, || {
                published += 1;
                if published == 3 {
                    Err(service_error(
                        "PINSET_TEST_INTERRUPTED",
                        "published three entries",
                    ))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            let mut pending: Refresh = read_json(&transaction_path(&home)).unwrap();
            let mut step = 0;
            finish(&home, &mut pending, true, || {
                let current = step;
                step += 1;
                if current == fail {
                    Err(service_error(
                        "PINSET_TEST_INTERRUPTED",
                        "injected rollback interruption",
                    ))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            let mut pending: Refresh = read_json(&transaction_path(&home)).unwrap();
            check_live(&home, &pending, None).unwrap();
            finish(&home, &mut pending, true, || Ok(())).unwrap();
            assert_direction(&home, &journal, true);
        }
    }
    #[test]
    fn recovery_never_overwrites_an_entry_changed_outside_the_transaction() {
        let (_temporary, home, journal) = fixture();
        let external = home
            .join("bin")
            .join(executable_name("node", &current_target()));
        fs::write(&external, b"external").unwrap();
        assert!(
            validate_journal(&home.join("bin"), &journal).is_err(),
            "hard-linked backup changes must also fail validation"
        );
        assert_eq!(fs::read(external).unwrap(), b"external");
        assert!(transaction_path(&home).exists());
    }
    fn service(home: &Path, cwd: &Path) -> Services {
        write_json(
            &home.join(".pinset-home.json"),
            &serde_json::json!({"protocol": PROTOCOL, "owner": "pinset"}),
        )
        .unwrap();
        Services {
            cwd: cwd.into(),
            home: home.into(),
            progress: ProgressReporter::default(),
        }
    }
    #[test]
    fn external_conflicts_are_prevalidated_before_any_managed_entry_changes() {
        let (temporary, home, journal) = fixture();
        fs::remove_file(transaction_path(&home)).unwrap();
        let directory = staging(&home.join("bin"), &journal);
        let cli = directory.join("source-cli");
        fs::copy(&cli, home.join("bin").join(cli_name(&current_target()))).unwrap();
        let node = home
            .join("bin")
            .join(executable_name("node", &current_target()));
        let rustfmt = home
            .join("bin")
            .join(executable_name("rustfmt", &current_target()));
        fs::write(&rustfmt, b"external command").unwrap();
        let old_node = fs::read(&node).unwrap();
        let old_registry = fs::read(home.join("state/shims.json")).unwrap();
        let error = service(&home, temporary.path())
            .install_shims_from(&cli, &directory.join("source-shim"))
            .unwrap_err();
        assert_eq!(error.code(), "PINSET_SHIM_EXTERNAL");
        assert_eq!(fs::read(node).unwrap(), old_node);
        assert_eq!(
            fs::read(home.join("state/shims.json")).unwrap(),
            old_registry
        );
        assert_eq!(fs::read(rustfmt).unwrap(), b"external command");
        assert!(!transaction_path(&home).exists());
    }
    #[test]
    #[cfg(unix)]
    fn matching_alias_digest_preserves_the_existing_file_identity() {
        use std::os::unix::fs::MetadataExt;
        let (temporary, home, journal) = fixture();
        fs::remove_file(transaction_path(&home)).unwrap();
        let directory = staging(&home.join("bin"), &journal);
        let cli = directory.join("source-cli");
        let shim = directory.join("source-shim");
        fs::copy(&cli, home.join("bin").join(cli_name(&current_target()))).unwrap();
        fs::copy(&shim, home.join("bin").join(shim_name(&current_target()))).unwrap();
        for command in ["node", "python3.12"] {
            fs::copy(&shim, home.join("bin").join(command)).unwrap();
        }
        let original = fs::metadata(home.join("bin/node")).unwrap().ino();
        let source = fs::metadata(&shim).unwrap().ino();
        assert_ne!(original, source);
        let service = service(&home, temporary.path());
        service.install_shims_from(&cli, &shim).unwrap();
        assert_eq!(fs::metadata(home.join("bin/node")).unwrap().ino(), original);
        service.install_shims_from(&cli, &shim).unwrap();
        assert_eq!(fs::metadata(home.join("bin/node")).unwrap().ino(), original);
    }
    #[test]
    fn a_recorded_cli_digest_allows_one_complete_pair_update() {
        let (temporary, home, journal) = fixture();
        fs::remove_file(transaction_path(&home)).unwrap();
        let directory = staging(&home.join("bin"), &journal);
        let cli = directory.join("source-cli");
        let shim = directory.join("source-shim");
        service(&home, temporary.path())
            .install_shims_from(&cli, &shim)
            .unwrap();
        let registry = registry_at(&home.join("state/shims.json"))
            .unwrap()
            .unwrap();
        assert_eq!(registry.cli_digest, Some(digest(b"new-cli")));
        assert_eq!(registry.digest, digest(b"new-shim"));
        assert_eq!(
            fingerprint(&home.join("bin").join(cli_name(&current_target()))).unwrap(),
            registry.cli_digest
        );
        for command in registry.commands.keys() {
            assert_eq!(
                fingerprint(
                    &home
                        .join("bin")
                        .join(executable_name(command, &current_target()))
                )
                .unwrap()
                .as_ref(),
                Some(&registry.digest)
            );
        }
        assert!(!transaction_path(&home).exists());
    }
    #[test]
    fn refreshes_legacy_aliases_when_the_cli_and_router_are_already_replaced() {
        let (temporary, home, journal) = fixture();
        fs::remove_file(transaction_path(&home)).unwrap();
        let directory = staging(&home.join("bin"), &journal);
        let cli = home.join("bin").join(cli_name(&current_target()));
        let shim = home.join("bin").join(shim_name(&current_target()));
        fs::copy(directory.join("source-cli"), &cli).unwrap();
        fs::copy(directory.join("source-shim"), &shim).unwrap();
        let mut before = journal.before.clone().unwrap();
        before.cli_digest = None;
        write_json(&home.join("state/shims.json"), &before).unwrap();

        let service = service(&home, temporary.path());
        service.install_shims_from(&cli, &shim).unwrap();
        let registry = registry_at(&home.join("state/shims.json"))
            .unwrap()
            .unwrap();
        assert_eq!(registry.digest, digest(b"new-shim"));
        assert_eq!(registry.cli_digest, Some(digest(b"new-cli")));
        for command in registry.commands.keys() {
            assert_eq!(
                fingerprint(
                    &home
                        .join("bin")
                        .join(executable_name(command, &current_target()))
                )
                .unwrap()
                .as_ref(),
                Some(&registry.digest)
            );
        }
        // Repeated installation and repair must retain the now-matching entries.
        service.install_shims_from(&cli, &shim).unwrap();
        assert_eq!(
            registry_at(&home.join("state/shims.json")).unwrap(),
            Some(registry)
        );
        assert!(!transaction_path(&home).exists());
    }
    #[test]
    #[cfg(unix)]
    fn legacy_unowned_cli_is_preserved_without_execution() {
        use std::os::unix::fs::PermissionsExt;
        let (temporary, home, journal) = fixture();
        fs::remove_file(transaction_path(&home)).unwrap();
        let directory = staging(&home.join("bin"), &journal);
        let marker = temporary.path().join("external-cli-was-executed");
        let cli = home.join("bin/pinset");
        let script = format!(
            "#!/bin/sh\ntouch '{}'\nprintf 'pinset 3.0.5\\n'\n",
            marker.display()
        );
        fs::write(&cli, &script).unwrap();
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        let mut before = journal.before.clone().unwrap();
        before.cli_digest = None;
        write_json(&home.join("state/shims.json"), &before).unwrap();
        let error = service(&home, temporary.path())
            .install_shims_from(
                &directory.join("source-cli"),
                &directory.join("source-shim"),
            )
            .unwrap_err();
        assert_eq!(error.code(), "PINSET_SHIM_EXTERNAL");
        assert_eq!(fs::read_to_string(cli).unwrap(), script);
        assert!(!marker.exists());
        assert!(!transaction_path(&home).exists());
    }
    #[test]
    #[cfg(unix)]
    fn linked_refresh_state_never_writes_to_the_external_directory() {
        use std::os::unix::fs::symlink;
        let (temporary, home, journal) = fixture();
        let pending = transaction_path(&home);
        fs::remove_file(&pending).unwrap();
        fs::remove_dir(pending.parent().unwrap()).unwrap();
        let external = temporary.path().join("external-state");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("sentinel"), b"user data").unwrap();
        symlink(&external, pending.parent().unwrap()).unwrap();
        let source = staging(&home.join("bin"), &journal);
        let registry = fs::read(home.join("state/shims.json")).unwrap();
        let error = service(&home, temporary.path())
            .install_shims_from(&source.join("source-cli"), &source.join("source-shim"))
            .unwrap_err();
        assert_eq!(error.code(), "PINSET_SHIM_RECOVERY");
        assert_eq!(fs::read(external.join("sentinel")).unwrap(), b"user data");
        assert_eq!(fs::read_dir(&external).unwrap().count(), 1);
        assert_eq!(fs::read(home.join("state/shims.json")).unwrap(), registry);
        for entry in &journal.entries {
            assert_eq!(
                fingerprint(&home.join("bin").join(&entry.name)).unwrap(),
                entry.before
            );
        }
    }
}
