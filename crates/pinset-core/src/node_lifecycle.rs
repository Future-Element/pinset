use std::{cmp::Reverse, fs, path::Path};

use crate::{Error, Result, validate_exact_node_version};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledNodeVersion {
    pub version: String,
    pub targets: Vec<String>,
}

pub fn list_installed_node_versions(pinset_home: &Path) -> Result<Vec<InstalledNodeVersion>> {
    let root = pinset_home.join("installs").join("node");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadNodeInstallDirectory { path: root, source });
        }
    };

    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadNodeInstallDirectory {
            path: root.clone(),
            source,
        })?;
        let version = entry.file_name().to_string_lossy().into_owned();
        if validate_exact_node_version(&version).is_err() || !entry.path().is_dir() {
            continue;
        }
        let mut targets = Vec::new();
        let target_entries =
            fs::read_dir(entry.path()).map_err(|source| Error::ReadNodeInstallDirectory {
                path: entry.path(),
                source,
            })?;
        for target_entry in target_entries {
            let target_entry = target_entry.map_err(|source| Error::ReadNodeInstallDirectory {
                path: entry.path(),
                source,
            })?;
            let target = target_entry.file_name().to_string_lossy().into_owned();
            if target_entry.path().is_dir()
                && has_matching_complete_receipt(&target_entry.path(), &version, &target)
            {
                targets.push(target);
            }
        }
        targets.sort();
        if !targets.is_empty() {
            versions.push(InstalledNodeVersion { version, targets });
        }
    }
    versions.sort_by_key(|entry| Reverse(version_key(&entry.version)));
    Ok(versions)
}

#[derive(Debug, Deserialize)]
struct InstallReceiptIdentity {
    schema: u32,
    complete: bool,
    tool: String,
    version: String,
    target: String,
}

fn has_matching_complete_receipt(directory: &Path, version: &str, target: &str) -> bool {
    let Ok(content) = fs::read_to_string(directory.join(".pinset-install.toml")) else {
        return false;
    };
    let Ok(receipt) = toml::from_str::<InstallReceiptIdentity>(&content) else {
        return false;
    };
    receipt.schema == 1
        && receipt.complete
        && receipt.tool == "node"
        && receipt.version == version
        && receipt.target == target
}

fn version_key(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.');
    (
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_complete_pinset_installations_in_version_order() {
        let home = tempfile::tempdir().expect("home");
        for (version, target, complete) in [
            ("22.12.0", "linux-x86_64", true),
            ("24.1.0", "windows-x86_64", true),
            ("24.1.0", "linux-x86_64", true),
            ("25.0.0", "linux-x86_64", false),
        ] {
            let directory = home
                .path()
                .join("installs")
                .join("node")
                .join(version)
                .join(target);
            fs::create_dir_all(&directory).expect("target directory");
            if complete {
                fs::write(
                    directory.join(".pinset-install.toml"),
                    format!(
                        "schema = 1\ncomplete = true\ntool = \"node\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
                    ),
                )
                .expect("receipt");
            }
        }
        fs::create_dir_all(home.path().join("installs/node/not-a-version/linux-x86_64"))
            .expect("invalid directory");

        let installed = list_installed_node_versions(home.path()).expect("installed versions");
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].version, "24.1.0");
        assert_eq!(installed[0].targets, ["linux-x86_64", "windows-x86_64"]);
        assert_eq!(installed[1].version, "22.12.0");
    }

    #[test]
    fn missing_install_root_is_an_empty_list() {
        let home = tempfile::tempdir().expect("home");
        assert!(
            list_installed_node_versions(home.path())
                .expect("installed versions")
                .is_empty()
        );
    }
}
