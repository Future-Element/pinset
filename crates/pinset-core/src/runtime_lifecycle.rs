use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    Error, Result, find_optional_project_config, global_config_path, load_optional_global_config,
    load_project_config, runtime_provider,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledToolVersion {
    pub tool: String,
    pub version: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolVersionReference {
    pub scope: &'static str,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallToolOutcome {
    pub tool: String,
    pub version: String,
    pub targets: Vec<String>,
}

pub fn list_installed_tool_versions(
    pinset_home: &Path,
    tool: &str,
) -> Result<Vec<InstalledToolVersion>> {
    validate_tool_and_version(tool, "0.0.0")?;
    let root = pinset_home.join("installs").join(tool);
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: root,
                source,
            });
        }
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadToolInstallDirectory {
            tool: tool.to_owned(),
            path: root.clone(),
            source,
        })?;
        let version = entry.file_name().to_string_lossy().into_owned();
        if !valid_version_segment(&version) || !entry.path().is_dir() {
            continue;
        }
        let mut targets = Vec::new();
        for target_entry in
            fs::read_dir(entry.path()).map_err(|source| Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: entry.path(),
                source,
            })?
        {
            let target_entry = target_entry.map_err(|source| Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: entry.path(),
                source,
            })?;
            let target = target_entry.file_name().to_string_lossy().into_owned();
            if target_entry.path().is_dir()
                && has_matching_complete_receipt(&target_entry.path(), tool, &version, &target)
            {
                targets.push(target);
            }
        }
        targets.sort();
        if !targets.is_empty() {
            versions.push(InstalledToolVersion {
                tool: tool.to_owned(),
                version,
                targets,
            });
        }
    }
    versions.sort_by_key(|entry| Reverse(version_key(&entry.version)));
    Ok(versions)
}

pub fn find_tool_version_references(
    pinset_home: &Path,
    cwd: &Path,
    tool: &str,
    version: &str,
) -> Result<Vec<ToolVersionReference>> {
    validate_tool_and_version(tool, version)?;
    let mut references = Vec::new();
    if let Some(path) = find_optional_project_config(cwd)? {
        let config = load_project_config(&path)?;
        if config
            .tools
            .get(tool)
            .is_some_and(|selected| selected == version)
        {
            references.push(ToolVersionReference {
                scope: "project",
                path,
            });
        }
    }
    let path = global_config_path(pinset_home);
    if let Some(config) = load_optional_global_config(&path)?
        && config
            .tools
            .get(tool)
            .is_some_and(|selected| selected == version)
    {
        references.push(ToolVersionReference {
            scope: "global",
            path,
        });
    }
    Ok(references)
}

pub fn uninstall_tool_version(
    pinset_home: &Path,
    cwd: &Path,
    tool: &str,
    version: &str,
    force: bool,
) -> Result<UninstallToolOutcome> {
    validate_tool_and_version(tool, version)?;
    let references = find_tool_version_references(pinset_home, cwd, tool, version)?;
    if !force && !references.is_empty() {
        return Err(Error::ToolVersionInUse {
            tool: tool.to_owned(),
            version: version.to_owned(),
            references: references
                .iter()
                .map(|reference| format!("{}:{}", reference.scope, reference.path.display()))
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    let version_root = pinset_home.join("installs").join(tool).join(version);
    let metadata = match fs::symlink_metadata(&version_root) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::ToolVersionNotInstalled {
                tool: tool.to_owned(),
                version: version.to_owned(),
            });
        }
        Err(source) => {
            return Err(Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: version_root,
                source,
            });
        }
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafeToolInstallEntry {
            tool: tool.to_owned(),
            path: version_root,
        });
    }
    let mut targets = Vec::new();
    for entry in fs::read_dir(&version_root).map_err(|source| Error::ReadToolInstallDirectory {
        tool: tool.to_owned(),
        path: version_root.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ReadToolInstallDirectory {
            tool: tool.to_owned(),
            path: version_root.clone(),
            source,
        })?;
        let file_type = entry
            .file_type()
            .map_err(|source| Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: entry.path(),
                source,
            })?;
        let target = entry.file_name().to_string_lossy().into_owned();
        if !file_type.is_dir()
            || file_type.is_symlink()
            || !has_matching_complete_receipt(&entry.path(), tool, version, &target)
        {
            return Err(Error::UnsafeToolInstallEntry {
                tool: tool.to_owned(),
                path: entry.path(),
            });
        }
        targets.push((target, entry.path()));
    }
    if targets.is_empty() {
        return Err(Error::ToolVersionNotInstalled {
            tool: tool.to_owned(),
            version: version.to_owned(),
        });
    }
    targets.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, path) in &targets {
        fs::remove_dir_all(path).map_err(|source| Error::RemoveToolInstall {
            tool: tool.to_owned(),
            path: path.clone(),
            source,
        })?;
    }
    fs::remove_dir(&version_root).map_err(|source| Error::RemoveToolInstall {
        tool: tool.to_owned(),
        path: version_root,
        source,
    })?;
    Ok(UninstallToolOutcome {
        tool: tool.to_owned(),
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

fn has_matching_complete_receipt(
    directory: &Path,
    tool: &str,
    version: &str,
    target: &str,
) -> bool {
    let Ok(content) = fs::read_to_string(directory.join(".pinset-install.toml")) else {
        return false;
    };
    let Ok(receipt) = toml::from_str::<InstallReceiptIdentity>(&content) else {
        return false;
    };
    matches!(receipt.schema, 1 | 2)
        && receipt.complete
        && receipt.tool == tool
        && receipt.version == version
        && receipt.target == target
}

fn validate_tool_and_version(tool: &str, version: &str) -> Result<()> {
    if runtime_provider(tool).is_none() {
        return Err(Error::UnsupportedRuntimeProvider {
            provider: tool.to_owned(),
        });
    }
    if !valid_version_segment(version) {
        return Err(Error::InvalidToolVersion {
            tool: tool.to_owned(),
            version: version.to_owned(),
        });
    }
    Ok(())
}

fn valid_version_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', ':'])
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

fn version_key(version: &str) -> (u64, u64, u64, u64, u64) {
    let (release, build) = version
        .split_once('+')
        .map_or((version, 0), |(release, build)| {
            (release, build.parse().unwrap_or(0))
        });
    let mut parts = release.split('.');
    (
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        build,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_schema_two_bun_install_receipts() {
        let home = tempfile::tempdir().expect("home");
        let directory = home.path().join("installs/bun/1.3.14/windows-x86_64-avx2");
        fs::create_dir_all(&directory).expect("directory");
        fs::write(
            directory.join(".pinset-install.toml"),
            "schema = 2\ncomplete = true\ntool = \"bun\"\nversion = \"1.3.14\"\ntarget = \"windows-x86_64-avx2\"\n",
        )
        .expect("receipt");
        let installed = list_installed_tool_versions(home.path(), "bun").expect("installed");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].version, "1.3.14");
    }

    #[test]
    fn lists_exact_python_distribution_versions_with_build_ids() {
        let home = tempfile::tempdir().expect("home");
        let directory = home
            .path()
            .join("installs/python/3.14.7+20260807/windows-x86_64");
        fs::create_dir_all(&directory).expect("directory");
        fs::write(
            directory.join(".pinset-install.toml"),
            "schema = 2\ncomplete = true\ntool = \"python\"\nversion = \"3.14.7+20260807\"\ntarget = \"windows-x86_64\"\n",
        )
        .expect("receipt");

        let versions = list_installed_tool_versions(home.path(), "python").expect("versions");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "3.14.7+20260807");
    }

    #[test]
    fn orders_java_builds_as_distinct_release_identities() {
        let home = tempfile::tempdir().expect("home");
        for version in ["21.0.8+8", "21.0.8+9"] {
            let directory = home
                .path()
                .join("installs")
                .join("java")
                .join(version)
                .join("linux-x86_64");
            fs::create_dir_all(&directory).expect("directory");
            fs::write(
                directory.join(".pinset-install.toml"),
                format!(
                    "schema = 2\ncomplete = true\ntool = \"java\"\nversion = \"{version}\"\ntarget = \"linux-x86_64\"\n"
                ),
            )
            .expect("receipt");
        }
        let versions = list_installed_tool_versions(home.path(), "java").expect("versions");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "21.0.8+9");
        assert_eq!(versions[1].version, "21.0.8+8");
    }
}
