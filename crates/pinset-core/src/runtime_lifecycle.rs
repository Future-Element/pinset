//! Ownership-aware inspection, uninstall, and pruning for managed runtimes.
//!
//! SAFETY: a matching complete install receipt is the authority to delete a target directory.
//! Selection references are an additional protection layer; `--force` may bypass references but
//! never receipt or path ownership checks.

use std::{
    cmp::Reverse,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result, find_optional_project_config, global_config_path, load_optional_global_config,
    load_project_config, runtime_provider, runtime_providers,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledToolVersion {
    pub tool: String,
    pub version: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolVersionReference {
    pub scope: &'static str,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UninstallToolOutcome {
    pub tool: String,
    pub version: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PruneToolCandidate {
    pub tool: String,
    pub version: String,
    pub targets: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PruneToolPlan {
    pub candidates: Vec<PruneToolCandidate>,
    pub protected: Vec<ProtectedToolVersion>,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectedToolVersion {
    pub tool: String,
    pub version: String,
    pub references: Vec<ToolVersionReference>,
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

pub fn list_all_installed_tool_versions(pinset_home: &Path) -> Result<Vec<InstalledToolVersion>> {
    let mut installed = Vec::new();
    for provider in runtime_providers() {
        installed.extend(list_installed_tool_versions(pinset_home, provider.tool)?);
    }
    Ok(installed)
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

pub fn find_tool_version_references_in_projects(
    pinset_home: &Path,
    project_roots: &[PathBuf],
    tool: &str,
    version: &str,
) -> Result<Vec<ToolVersionReference>> {
    validate_tool_and_version(tool, version)?;
    let mut references = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for root in project_roots {
        let Some(path) = find_optional_project_config(root)? else {
            continue;
        };
        if !seen_paths.insert(path.clone()) {
            continue;
        }
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

pub fn plan_prune_tool_versions(
    pinset_home: &Path,
    project_roots: &[PathBuf],
) -> Result<PruneToolPlan> {
    let mut candidates = Vec::new();
    let mut protected = Vec::new();
    let mut bytes = 0_u64;
    for installed in list_all_installed_tool_versions(pinset_home)? {
        let references = find_tool_version_references_in_projects(
            pinset_home,
            project_roots,
            &installed.tool,
            &installed.version,
        )?;
        if references.is_empty() {
            let (version_root, targets) = validate_owned_tool_install_layout(
                pinset_home,
                &installed.tool,
                &installed.version,
            )?;
            let candidate_bytes =
                directory_size_without_following_links(&version_root, &installed.tool)?;
            bytes = bytes.saturating_add(candidate_bytes);
            candidates.push(PruneToolCandidate {
                tool: installed.tool,
                version: installed.version,
                targets,
                bytes: candidate_bytes,
            });
        } else {
            protected.push(ProtectedToolVersion {
                tool: installed.tool,
                version: installed.version,
                references,
            });
        }
    }
    Ok(PruneToolPlan {
        candidates,
        protected,
        bytes,
    })
}

pub fn plan_uninstall_tool_version(
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
    let (_, targets) = validate_owned_tool_install_layout(pinset_home, tool, version)?;
    Ok(UninstallToolOutcome {
        tool: tool.to_owned(),
        version: version.to_owned(),
        targets,
    })
}

fn validate_owned_tool_install_layout(
    pinset_home: &Path,
    tool: &str,
    version: &str,
) -> Result<(PathBuf, Vec<String>)> {
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
        targets.push(target);
    }
    if targets.is_empty() {
        return Err(Error::ToolVersionNotInstalled {
            tool: tool.to_owned(),
            version: version.to_owned(),
        });
    }
    targets.sort();
    Ok((version_root, targets))
}

pub fn uninstall_tool_version(
    pinset_home: &Path,
    cwd: &Path,
    tool: &str,
    version: &str,
    force: bool,
) -> Result<UninstallToolOutcome> {
    let outcome = plan_uninstall_tool_version(pinset_home, cwd, tool, version, force)?;
    let version_root = pinset_home.join("installs").join(tool).join(version);
    for target in &outcome.targets {
        let path = version_root.join(target);
        fs::remove_dir_all(&path).map_err(|source| Error::RemoveToolInstall {
            tool: tool.to_owned(),
            path,
            source,
        })?;
    }
    fs::remove_dir(&version_root).map_err(|source| Error::RemoveToolInstall {
        tool: tool.to_owned(),
        path: version_root,
        source,
    })?;
    Ok(outcome)
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
    // Legacy schema 1 receipts remain readable, but every identity field must still agree with
    // the directory being inspected before the directory is considered Pinset-owned.
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

fn directory_size_without_following_links(path: &Path, tool: &str) -> Result<u64> {
    let mut size = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries =
            fs::read_dir(&directory).map_err(|source| Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: directory.clone(),
                source,
            })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::ReadToolInstallDirectory {
                tool: tool.to_owned(),
                path: directory.clone(),
                source,
            })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|source| {
                Error::ReadToolInstallDirectory {
                    tool: tool.to_owned(),
                    path: entry.path(),
                    source,
                }
            })?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                size = size.saturating_add(metadata.len());
            }
        }
    }
    Ok(size)
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

    #[test]
    fn prune_plan_protects_global_and_supplied_project_selections() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        fs::write(
            project.path().join("pinset.toml"),
            "schema = 2\n[tools]\npnpm = \"11.21.0\"\n",
        )
        .expect("project config");
        let global_path = global_config_path(home.path());
        fs::create_dir_all(global_path.parent().expect("state directory")).expect("state");
        fs::write(&global_path, "schema = 2\n[tools]\nbun = \"1.3.14\"\n").expect("global config");
        for (tool, version) in [("pnpm", "11.21.0"), ("pnpm", "10.0.0"), ("bun", "1.3.14")] {
            let target = "linux-x86_64";
            let directory = home
                .path()
                .join("installs")
                .join(tool)
                .join(version)
                .join(target);
            fs::create_dir_all(&directory).expect("install directory");
            fs::write(
                directory.join(".pinset-install.toml"),
                format!(
                    "schema = 2\ncomplete = true\ntool = \"{tool}\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
                ),
            )
            .expect("receipt");
        }

        let plan = plan_prune_tool_versions(home.path(), &[project.path().to_path_buf()])
            .expect("prune plan");

        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(plan.candidates[0].tool, "pnpm");
        assert_eq!(plan.candidates[0].version, "10.0.0");
        assert_eq!(plan.protected.len(), 2);
        assert!(plan.bytes > 0);
    }
}
