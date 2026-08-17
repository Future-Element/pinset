use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    Error, Result, find_optional_project_config, global_config_path, load_optional_global_config,
    load_project_config, validate_exact_node_version,
};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledNodeVersion {
    pub version: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeVersionReference {
    pub scope: &'static str,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallNodeOutcome {
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

pub fn find_node_version_references(
    pinset_home: &Path,
    cwd: &Path,
    version: &str,
) -> Result<Vec<NodeVersionReference>> {
    validate_exact_node_version(version)?;
    let mut references = Vec::new();
    if let Some(path) = find_optional_project_config(cwd)? {
        let config = load_project_config(&path)?;
        if config
            .tools
            .get("node")
            .is_some_and(|selected| selected == version)
        {
            references.push(NodeVersionReference {
                scope: "project",
                path,
            });
        }
    }
    let path = global_config_path(pinset_home);
    if let Some(config) = load_optional_global_config(&path)? {
        if config
            .tools
            .get("node")
            .is_some_and(|selected| selected == version)
        {
            references.push(NodeVersionReference {
                scope: "global",
                path,
            });
        }
    }
    Ok(references)
}

pub fn uninstall_node_version(
    pinset_home: &Path,
    cwd: &Path,
    version: &str,
    force: bool,
) -> Result<UninstallNodeOutcome> {
    validate_exact_node_version(version)?;
    let references = find_node_version_references(pinset_home, cwd, version)?;
    if !force && !references.is_empty() {
        return Err(Error::NodeVersionInUse {
            version: version.to_owned(),
            references: references
                .iter()
                .map(|reference| format!("{}:{}", reference.scope, reference.path.display()))
                .collect::<Vec<_>>()
                .join(", "),
        });
    }

    let version_root = pinset_home.join("installs").join("node").join(version);
    let root_metadata = match fs::symlink_metadata(&version_root) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NodeVersionNotInstalled {
                version: version.to_owned(),
            });
        }
        Err(source) => {
            return Err(Error::ReadNodeInstallDirectory {
                path: version_root,
                source,
            });
        }
    };
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return Err(Error::UnsafeNodeInstallEntry { path: version_root });
    }

    let mut targets = Vec::new();
    for entry in fs::read_dir(&version_root).map_err(|source| Error::ReadNodeInstallDirectory {
        path: version_root.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ReadNodeInstallDirectory {
            path: version_root.clone(),
            source,
        })?;
        let file_type = entry
            .file_type()
            .map_err(|source| Error::ReadNodeInstallDirectory {
                path: entry.path(),
                source,
            })?;
        let target = entry.file_name().to_string_lossy().into_owned();
        if !file_type.is_dir()
            || file_type.is_symlink()
            || !has_matching_complete_receipt(&entry.path(), version, &target)
        {
            return Err(Error::UnsafeNodeInstallEntry { path: entry.path() });
        }
        targets.push((target, entry.path()));
    }
    if targets.is_empty() {
        return Err(Error::NodeVersionNotInstalled {
            version: version.to_owned(),
        });
    }
    targets.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, path) in &targets {
        fs::remove_dir_all(path).map_err(|source| Error::RemoveNodeInstall {
            path: path.clone(),
            source,
        })?;
    }
    fs::remove_dir(&version_root).map_err(|source| Error::RemoveNodeInstall {
        path: version_root,
        source,
    })?;
    Ok(UninstallNodeOutcome {
        version: version.to_owned(),
        targets: targets.into_iter().map(|(target, _)| target).collect(),
    })
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
    matches!(receipt.schema, 1 | 2)
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

    #[test]
    fn refuses_referenced_version_and_uninstalls_owned_targets_with_force() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        fs::write(
            project.path().join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"24.1.0\"\n",
        )
        .expect("project config");
        let global_path = global_config_path(home.path());
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(&global_path, "schema = 1\n[tools]\nnode = \"24.1.0\"\n").expect("global config");
        let directory = home.path().join("installs/node/24.1.0/linux-x86_64");
        fs::create_dir_all(&directory).expect("install directory");
        fs::write(
            directory.join(".pinset-install.toml"),
            "schema = 1\ncomplete = true\ntool = \"node\"\nversion = \"24.1.0\"\ntarget = \"linux-x86_64\"\n",
        )
        .expect("receipt");

        let error = uninstall_node_version(home.path(), project.path(), "24.1.0", false)
            .expect_err("referenced version");
        let Error::NodeVersionInUse { references, .. } = error else {
            panic!("unexpected error: {error}");
        };
        assert!(references.contains("project:"));
        assert!(references.contains("global:"));
        assert!(directory.is_dir());

        let outcome = uninstall_node_version(home.path(), project.path(), "24.1.0", true)
            .expect("forced uninstall");
        assert_eq!(outcome.targets, ["linux-x86_64"]);
        assert!(!home.path().join("installs/node/24.1.0").exists());
    }

    #[test]
    fn refuses_to_remove_unowned_or_incomplete_install_entries() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        let directory = home.path().join("installs/node/24.1.0/linux-x86_64");
        fs::create_dir_all(&directory).expect("install directory");
        fs::write(directory.join("unexpected.txt"), b"keep").expect("foreign file");

        assert!(matches!(
            uninstall_node_version(home.path(), project.path(), "24.1.0", true),
            Err(Error::UnsafeNodeInstallEntry { .. })
        ));
        assert!(directory.join("unexpected.txt").is_file());
    }
}
