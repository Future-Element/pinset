use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[cfg(feature = "state-write")]
use std::{io::Write, sync::Mutex};

#[cfg(feature = "state-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};
#[cfg(all(feature = "state-write", feature = "lockfile"))]
use crate::{Lockfile, save_lockfile, validate_lock_matches_tools};

pub const GLOBAL_STATE_SCHEMA: u32 = 2;
pub const GLOBAL_CONFIG_FILENAME: &str = "global.toml";
pub const GLOBAL_LOCKFILE_FILENAME: &str = "global.lock";

#[cfg(feature = "state-write")]
static GLOBAL_STATE_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    pub schema: u32,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            schema: GLOBAL_STATE_SCHEMA,
            tools: BTreeMap::new(),
        }
    }
}

impl GlobalConfig {
    pub fn set_tool(&mut self, tool: &str, version: &str) {
        self.tools.insert(tool.to_owned(), version.to_owned());
    }
}

pub fn global_state_dir(pinset_home: &Path) -> PathBuf {
    pinset_home.join("state")
}

pub fn global_config_path(pinset_home: &Path) -> PathBuf {
    global_state_dir(pinset_home).join(GLOBAL_CONFIG_FILENAME)
}

pub fn global_lockfile_path(pinset_home: &Path) -> PathBuf {
    global_state_dir(pinset_home).join(GLOBAL_LOCKFILE_FILENAME)
}

pub fn load_global_config(path: &Path) -> Result<GlobalConfig> {
    let content = fs::read_to_string(path).map_err(|source| {
        if source.kind() == ErrorKind::NotFound {
            Error::GlobalConfigNotFound {
                path: path.to_path_buf(),
            }
        } else {
            Error::ReadGlobalConfig {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    let config: GlobalConfig =
        toml::from_str(&content).map_err(|source| Error::ParseGlobalConfig {
            path: path.to_path_buf(),
            source,
        })?;
    validate_global_config(&config)?;
    Ok(config)
}

pub fn load_optional_global_config(path: &Path) -> Result<Option<GlobalConfig>> {
    match load_global_config(path) {
        Ok(config) => Ok(Some(config)),
        Err(Error::GlobalConfigNotFound { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn validate_global_config(config: &GlobalConfig) -> Result<()> {
    if !matches!(config.schema, 1 | GLOBAL_STATE_SCHEMA) {
        return Err(Error::UnsupportedGlobalConfigSchema {
            actual: config.schema,
        });
    }
    Ok(())
}

#[cfg(feature = "state-write")]
pub fn save_global_config(path: &Path, config: &GlobalConfig) -> Result<()> {
    validate_global_config(config)?;
    let mut normalized = config.clone();
    normalized.schema = GLOBAL_STATE_SCHEMA;
    let serialized = toml::to_string_pretty(&normalized)
        .map_err(|source| Error::SerializeGlobalConfig { source })?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::CreateGlobalStateDirectory {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                ErrorKind::InvalidInput,
                "global config path has no parent directory",
            ),
        })?;
    fs::create_dir_all(parent).map_err(|source| Error::CreateGlobalStateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteGlobalConfig {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteGlobalConfig {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(all(feature = "state-write", feature = "lockfile"))]
pub fn save_global_state(
    pinset_home: &Path,
    config: &GlobalConfig,
    lockfile: &Lockfile,
) -> Result<()> {
    let config_path = global_config_path(pinset_home);
    validate_lock_matches_tools(lockfile, &config.tools, &config_path)?;

    let _guard = GLOBAL_STATE_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state_dir = global_state_dir(pinset_home);
    fs::create_dir_all(&state_dir).map_err(|source| Error::CreateGlobalStateDirectory {
        path: state_dir,
        source,
    })?;

    // Commit the lock first. If the second atomic write is interrupted, the previous
    // selection remains active and lock-dependent operations fail until this is retried.
    save_lockfile(&global_lockfile_path(pinset_home), lockfile)?;
    save_global_config(&config_path, config)
}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "state-write", feature = "lockfile"))]
    use std::sync::{Arc, Barrier};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn derives_global_paths_without_creating_state() {
        let root = tempdir().expect("temporary root");
        let home = root.path().join("home");

        assert_eq!(global_state_dir(&home), home.join("state"));
        assert_eq!(
            global_config_path(&home),
            home.join("state").join(GLOBAL_CONFIG_FILENAME)
        );
        assert_eq!(
            global_lockfile_path(&home),
            home.join("state").join(GLOBAL_LOCKFILE_FILENAME)
        );
        assert_eq!(
            load_optional_global_config(&global_config_path(&home))
                .expect("optional global config"),
            None
        );
        assert!(!home.exists());
    }

    #[test]
    fn rejects_unknown_fields_and_schema() {
        let root = tempdir().expect("temporary root");
        let path = root.path().join(GLOBAL_CONFIG_FILENAME);
        fs::write(&path, "schema = 1\nunknown = true\n[tools]\n").expect("invalid config");
        assert!(matches!(
            load_global_config(&path),
            Err(Error::ParseGlobalConfig { .. })
        ));

        fs::write(&path, "schema = 3\n[tools]\n").expect("unsupported config");
        assert!(matches!(
            load_global_config(&path),
            Err(Error::UnsupportedGlobalConfigSchema { actual: 3 })
        ));
    }

    #[cfg(feature = "state-write")]
    #[test]
    fn atomically_creates_and_updates_global_config() {
        let root = tempdir().expect("temporary root");
        let path = global_config_path(root.path());
        let mut config = GlobalConfig::default();
        config.set_tool("node", "20.0.0");
        save_global_config(&path, &config).expect("save global config");

        config.set_tool("node", "24.0.0");
        save_global_config(&path, &config).expect("update global config");

        assert_eq!(
            load_global_config(&path)
                .expect("load global config")
                .tools
                .get("node")
                .map(String::as_str),
            Some("24.0.0")
        );
        assert_eq!(
            fs::read_dir(path.parent().expect("state directory"))
                .expect("read state directory")
                .count(),
            1
        );
    }

    #[cfg(all(feature = "state-write", feature = "lockfile"))]
    #[test]
    fn rejects_mismatched_state_without_writing_files() {
        let root = tempdir().expect("temporary root");
        let mut config = GlobalConfig::default();
        config.set_tool("node", "20.0.0");

        let error = save_global_state(root.path(), &config, &node_lockfile("24.0.0"))
            .expect_err("mismatched state");

        assert!(matches!(error, Error::LockfileMismatch { .. }));
        assert!(!global_state_dir(root.path()).exists());
    }

    #[cfg(all(feature = "state-write", feature = "lockfile"))]
    #[test]
    fn concurrent_writes_leave_a_matching_global_state_pair() {
        let root = tempdir().expect("temporary root");
        let home = Arc::new(root.path().join("home"));
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();

        for version in ["20.0.0", "24.0.0"] {
            let home = Arc::clone(&home);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                let mut config = GlobalConfig::default();
                config.set_tool("node", version);
                let lockfile = node_lockfile(version);
                barrier.wait();
                save_global_state(&home, &config, &lockfile).expect("save concurrent state");
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().expect("writer thread");
        }

        let config_path = global_config_path(&home);
        let config = load_global_config(&config_path).expect("global config");
        let selected = config.tools.get("node").expect("node selection");
        let lockfile = crate::load_lockfile(&global_lockfile_path(&home)).expect("global lockfile");
        crate::validate_lock_matches_selection(&lockfile, selected, &config_path)
            .expect("matching global state");
    }

    #[cfg(all(feature = "state-write", feature = "lockfile"))]
    fn node_lockfile(version: &str) -> Lockfile {
        let artifacts = crate::MVP_NODE_TARGETS
            .into_iter()
            .map(|target| {
                let plan =
                    crate::plan_node_artifact(&crate::SourceConfig::default(), version, target)
                        .expect("artifact plan");
                crate::LockedArtifact {
                    target: target.to_owned(),
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: "ab".repeat(32),
                    integrity: None,
                    format: match plan.format {
                        crate::NodeArchiveFormat::Zip => crate::LockedArtifactFormat::Zip,
                        crate::NodeArchiveFormat::TarXz => crate::LockedArtifactFormat::TarXz,
                    },
                    archive_root: plan.archive_root,
                    verification: "nodejs-shasums-https".to_owned(),
                    overlays: Vec::new(),
                }
            })
            .collect();
        Lockfile::new_node(
            "pinset global state test".to_owned(),
            version.to_owned(),
            artifacts,
        )
    }
}
