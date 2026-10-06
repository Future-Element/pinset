//! A CLI/shim pair is one update unit. Backups are on the destination volume
//! so rollback remains possible for custom installation directories.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BinaryJournal {
    protocol: String,
    destination: PathBuf,
    backup: PathBuf,
    phase: String,
    names: Vec<String>,
}
fn rollback(journal: &BinaryJournal) -> Result<()> {
    for name in &journal.names {
        let backup = journal.backup.join(name);
        if backup.is_file() {
            let dst = journal.destination.join(name);
            if dst.exists() {
                if cfg!(windows) {
                    fs::rename(&dst, journal.backup.join(format!("failed-{name}")))?;
                } else {
                    fs::remove_file(&dst)?;
                }
            }
            fs::rename(backup, dst)?;
        }
    }
    Ok(())
}
pub(crate) fn replace_binary_pair(
    source: &Path,
    destination: &Path,
    home: &Path,
    names: &[String],
) -> Result<PathBuf> {
    let backup = destination.join(format!(".pinset-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&backup)?;
    let journal = BinaryJournal {
        protocol: PROTOCOL.into(),
        destination: destination.canonicalize()?,
        backup: backup.clone(),
        phase: "prepared".into(),
        names: names.to_vec(),
    };
    let path = home.join("state/self-update/transaction.json");
    write_json(&path, &journal)?;
    let result = (|| -> Result<()> {
        for name in names {
            let dst = destination.join(name);
            if !dst.is_file() || fs::symlink_metadata(&dst)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_PAIR",
                    "both owned regular binaries must exist",
                ));
            }
            fs::copy(&dst, backup.join(name))?;
        }
        for name in names {
            let staged = backup.join(format!("staged-{name}"));
            let input = source.join(name);
            if !input.is_file() || fs::symlink_metadata(&input)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_PAIR",
                    "replacement binaries must be regular files",
                ));
            }
            fs::copy(input, &staged)?;
            // Windows permits renaming an in-use executable. Keep its inode
            // until the old process exits, then atomically publish the stage.
            let dst = destination.join(name);
            if cfg!(windows) {
                fs::rename(&dst, backup.join(format!("running-{name}")))?;
            }
            fs::rename(staged, &dst)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        rollback(&journal)?;
        fs::remove_file(path)?;
        return Err(error);
    }
    let done = BinaryJournal {
        phase: "committed".into(),
        ..journal
    };
    write_json(&path, &done)?;
    Ok(backup)
}
impl Services {
    pub fn self_repair(&self, plan: bool) -> Result<serde_json::Value> {
        if !plan {
            self.ensure_home()?;
        }
        let transactions = self.recover_transactions(plan)?;
        // Release project guards before acquiring the binary-update guard: a
        // valid project identity may have the same name as this internal lock.
        let _guard = if plan {
            None
        } else {
            Some(self.guard("self-update")?)
        };
        let path = self.home.join("state/self-update/transaction.json");
        let mut binary_update = None;
        if path.exists() {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "binary journal is a symlink",
                ));
            }
            let journal: BinaryJournal = read_json(&path)?;
            let parent = std::env::current_exe()?.parent().unwrap().canonicalize()?;
            if journal.protocol != PROTOCOL
                || journal.destination != parent
                || !journal.backup.starts_with(&parent)
                || journal.backup.parent() != Some(parent.as_path())
                || journal.names
                    != [
                        executable_name("pinset", &current_target()),
                        executable_name("pinset-shim", &current_target()),
                    ]
            {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "invalid binary update journal",
                ));
            }
            if !matches!(journal.phase.as_str(), "prepared" | "committed") {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "unknown binary update phase",
                ));
            }
            binary_update = Some(journal.phase.clone());
            if !plan {
                if journal.phase == "prepared" {
                    rollback(&journal)?;
                }
                self.install_shims()?;
                fs::remove_file(path)?;
            }
        } else if !plan {
            self.install_shims()?;
        }
        Ok(
            serde_json::json!({"protocol":PROTOCOL,"plan":plan,"repaired":!plan,"transactions":transactions,"binary_update":binary_update}),
        )
    }
    pub(crate) fn validate_new_cli(&self, path: &Path, version: &str) -> Result<()> {
        let output = Command::new(path)
            .arg("--version")
            .env_remove("PINSET_IDENTITY")
            .output()?;
        if !output.status.success()
            || String::from_utf8_lossy(&output.stdout).trim() != format!("pinset {version}")
        {
            return Err(service_error(
                "PINSET_UPDATE_VERSION",
                "downloaded CLI does not match the requested release",
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_pair_keeps_both_backups_and_committed_journal() {
        let root = tempfile::tempdir().unwrap();
        let dst = root.path().join("bin");
        let source = root.path().join("new");
        fs::create_dir(&dst).unwrap();
        fs::create_dir(&source).unwrap();
        let names = vec!["pinset".into(), "pinset-shim".into()];
        for name in &names {
            fs::write(dst.join(name), format!("old-{name}")).unwrap();
            fs::write(source.join(name), format!("new-{name}")).unwrap();
        }
        let backup = replace_binary_pair(&source, &dst, root.path(), &names).unwrap();
        let journal: BinaryJournal =
            read_json(&root.path().join("state/self-update/transaction.json")).unwrap();
        assert_eq!(journal.phase, "committed");
        for name in &names {
            assert_eq!(
                fs::read_to_string(dst.join(name)).unwrap(),
                format!("new-{name}")
            );
            assert_eq!(
                fs::read_to_string(backup.join(name)).unwrap(),
                format!("old-{name}")
            );
        }
        // A prepared journal can restore the complete pair after an interrupted replacement.
        rollback(&journal).unwrap();
        for name in &names {
            assert_eq!(
                fs::read_to_string(dst.join(name)).unwrap(),
                format!("old-{name}")
            );
        }
    }
    #[test]
    fn partial_pair_replacement_rolls_back_both_original_binaries() {
        let root = tempfile::tempdir().unwrap();
        let dst = root.path().join("bin");
        let source = root.path().join("new");
        fs::create_dir(&dst).unwrap();
        fs::create_dir(&source).unwrap();
        for name in ["pinset", "pinset-shim"] {
            fs::write(dst.join(name), format!("old-{name}")).unwrap();
        }
        fs::write(source.join("pinset"), "new-cli").unwrap();
        assert!(
            replace_binary_pair(
                &source,
                &dst,
                root.path(),
                &["pinset".into(), "pinset-shim".into()]
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(dst.join("pinset")).unwrap(),
            "old-pinset"
        );
        assert_eq!(
            fs::read_to_string(dst.join("pinset-shim")).unwrap(),
            "old-pinset-shim"
        );
    }
}
