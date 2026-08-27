use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[cfg(feature = "project-write")]
use std::io::Write;

#[cfg(feature = "project-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
#[cfg(feature = "project-write")]
use sha2::{Digest, Sha256};

use crate::{Error, Result, global_state_dir};

const PROJECT_REGISTRY_SCHEMA: u32 = 1;
const MAX_PROJECT_RECORD_BYTES: u64 = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRecord {
    schema: u32,
    config: PathBuf,
}

#[cfg(feature = "project-write")]
pub fn register_project_config(pinset_home: &Path, config_path: &Path) -> Result<()> {
    let canonical = fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    let identity = if cfg!(windows) {
        canonical.to_string_lossy().to_ascii_lowercase()
    } else {
        canonical.to_string_lossy().into_owned()
    };
    let digest = Sha256::digest(identity.as_bytes());
    let filename = format!("{}.toml", hex_lower(&digest));
    let directory = project_registry_dir(pinset_home);
    fs::create_dir_all(&directory).map_err(|source| Error::CreateProjectRegistryDirectory {
        path: directory.clone(),
        source,
    })?;
    let path = directory.join(filename);
    let serialized = toml::to_string_pretty(&ProjectRecord {
        schema: PROJECT_REGISTRY_SCHEMA,
        config: canonical,
    })
    .map_err(|source| Error::InvalidProjectRegistry {
        path: path.clone(),
        reason: source.to_string(),
    })?;
    let mut file =
        AtomicWriteFile::options()
            .open(&path)
            .map_err(|source| Error::WriteProjectRegistry {
                path: path.clone(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteProjectRegistry { path, source })
}

pub fn registered_project_configs(pinset_home: &Path) -> Result<Vec<PathBuf>> {
    let directory = project_registry_dir(pinset_home);
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(Error::InvalidProjectRegistry {
                path: directory,
                reason: "registry path must be a regular directory".to_owned(),
            });
        }
        Ok(_) => {}
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadProjectRegistry {
                path: directory,
                source,
            });
        }
    }
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadProjectRegistry {
                path: directory,
                source,
            });
        }
    };
    let mut configs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadProjectRegistry {
            path: directory.clone(),
            source,
        })?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|source| Error::ReadProjectRegistry {
                path: path.clone(),
                source,
            })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(Error::InvalidProjectRegistry {
                path,
                reason: "record must be a regular file".to_owned(),
            });
        }
        if metadata.len() > MAX_PROJECT_RECORD_BYTES {
            return Err(Error::InvalidProjectRegistry {
                path,
                reason: format!("record exceeds {MAX_PROJECT_RECORD_BYTES} bytes"),
            });
        }
        let content = fs::read_to_string(&path).map_err(|source| Error::ReadProjectRegistry {
            path: path.clone(),
            source,
        })?;
        let record: ProjectRecord =
            toml::from_str(&content).map_err(|source| Error::InvalidProjectRegistry {
                path: path.clone(),
                reason: source.to_string(),
            })?;
        if record.schema != PROJECT_REGISTRY_SCHEMA {
            return Err(Error::InvalidProjectRegistry {
                path,
                reason: format!("unsupported schema {}", record.schema),
            });
        }
        match fs::symlink_metadata(&record.config) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(Error::InvalidProjectRegistry {
                    path,
                    reason: format!(
                        "registered config is not a regular file: {}",
                        record.config.display()
                    ),
                });
            }
            Ok(_) => configs.push(record.config),
            Err(source) if source.kind() == ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::ReadProjectRegistry {
                    path: record.config,
                    source,
                });
            }
        }
    }
    configs.sort();
    configs.dedup();
    Ok(configs)
}

fn project_registry_dir(pinset_home: &Path) -> PathBuf {
    global_state_dir(pinset_home).join("projects")
}

#[cfg(feature = "project-write")]
fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "project-write")]
    #[test]
    fn registers_and_lists_existing_project_configs() {
        let root = tempfile::tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(&project).expect("project directory");
        let config = project.join("pinset.toml");
        fs::write(&config, "schema = 1\n[tools]\n").expect("project config");

        register_project_config(&home, &config).expect("register project");

        assert_eq!(
            registered_project_configs(&home).expect("registered projects"),
            vec![fs::canonicalize(config).expect("canonical config")]
        );
    }
}
