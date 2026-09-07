//! Shared, secret-free profile selection for the CLI and command shims.
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, ProjectConfig, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvironmentSelection {
    pub profile: Option<String>,
    pub source: &'static str,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalEnvironment {
    schema: u32,
    root: String,
    project_id: String,
    profile: String,
}

pub fn environment_selection(
    home: &Path,
    config_path: &Path,
    config: &ProjectConfig,
    explicit: Option<&str>,
) -> Result<EnvironmentSelection> {
    let process_profile = env::var("PINSET_ENV_PROFILE").ok();
    let ci = ["CI", "GITHUB_ACTIONS", "GITLAB_CI", "TF_BUILD"]
        .iter()
        .any(|name| {
            env::var(name).is_ok_and(|value| {
                !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
            })
        });
    select_environment(
        home,
        config_path,
        config,
        explicit,
        process_profile.as_deref(),
        ci,
    )
}

/// Explicit/process choices bypass local preferences; CI never reads machine-local state.
pub fn select_environment(
    home: &Path,
    config_path: &Path,
    config: &ProjectConfig,
    explicit: Option<&str>,
    process_profile: Option<&str>,
    ci: bool,
) -> Result<EnvironmentSelection> {
    let (profile, source) = if let Some(value) = explicit {
        (Some(value.to_owned()), "argument")
    } else if let Some(value) = process_profile {
        (Some(value.to_owned()), "process")
    } else if let Some(value) = if ci {
        None
    } else {
        load_local_environment(home, config_path, config)?
    } {
        (Some(value), "local")
    } else {
        (
            config
                .environment
                .as_ref()
                .and_then(|value| value.auto_profile.clone()),
            "project",
        )
    };
    if let Some(profile) = &profile
        && !config
            .environment
            .as_ref()
            .is_some_and(|value| value.profiles.contains_key(profile))
    {
        return invalid(
            config_path,
            format!(
                "profile {profile:?} is not declared; select an existing profile or run `pinset env use --reset`"
            ),
        );
    }
    Ok(EnvironmentSelection {
        source: if profile.is_some() { source } else { "none" },
        profile,
    })
}

fn identity(config_path: &Path) -> Result<String> {
    let root = config_path.parent().unwrap_or_else(|| Path::new("."));
    let canonical = fs::canonicalize(root).map_err(|error| local_error(config_path, error))?;
    let value = canonical.to_string_lossy().into_owned();
    Ok(if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    })
}

fn state_path(home: &Path, root: &str) -> PathBuf {
    let digest = Sha256::digest(root.as_bytes());
    let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    home.join("state")
        .join("environments")
        .join(format!("{name}.toml"))
}

fn validate_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            invalid(path, "expected a regular directory")
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(local_error(path, error)),
    }
}

fn load_local_environment(
    home: &Path,
    config_path: &Path,
    config: &ProjectConfig,
) -> Result<Option<String>> {
    let root = identity(config_path)?;
    let path = state_path(home, &root);
    validate_directory(&home.join("state"))?;
    validate_directory(path.parent().expect("state parent"))?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(local_error(&path, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 65536 {
        return invalid(&path, "expected a regular state file of at most 64 KiB");
    }
    let content = fs::read_to_string(&path).map_err(|error| local_error(&path, error))?;
    let record: LocalEnvironment =
        toml::from_str(&content).map_err(|_| Error::LocalEnvironment {
            path: path.clone(),
            reason: "invalid state file; run `pinset env use --reset`".to_owned(),
        })?;
    if record.schema != 1
        || record.root != root
        || Some(record.project_id.as_str()) != config.project_id.as_deref()
    {
        return invalid(
            &path,
            "project identity changed; run `pinset env use --reset`",
        );
    }
    Ok(Some(record.profile))
}

/// Writes only local state, under the same project write lock as configuration changes.
#[cfg(feature = "project-write")]
pub fn save_local_environment(
    home: &Path,
    config_path: &Path,
    profile: Option<&str>,
) -> Result<()> {
    use std::io::Write;
    let _guard = crate::acquire_project_state_write_lock(home, config_path)?;
    let config = crate::load_project_config(config_path)?;
    let root = identity(config_path)?;
    let path = state_path(home, &root);
    validate_directory(&home.join("state"))?;
    let directory = path.parent().expect("state parent");
    validate_directory(directory)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return invalid(&path, "refusing to replace a non-regular state file");
        }
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(local_error(&path, error)),
        _ => {}
    }
    let Some(profile) = profile else {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(local_error(&path, error)),
        };
    };
    select_environment(home, config_path, &config, Some(profile), None, false)?;
    let project_id = config.project_id.ok_or_else(|| Error::LocalEnvironment {
        path: config_path.to_path_buf(),
        reason: "project-id is required; run `pinset migrate` first".to_owned(),
    })?;
    let record = LocalEnvironment {
        schema: 1,
        root,
        project_id,
        profile: profile.to_owned(),
    };
    let content = toml::to_string(&record).map_err(|error| local_error(&path, error))?;
    fs::create_dir_all(directory).map_err(|error| local_error(directory, error))?;
    validate_directory(directory)?;
    let mut file = atomic_write_file::AtomicWriteFile::open(&path)
        .map_err(|error| local_error(&path, error))?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|error| local_error(&path, error))
}

fn local_error(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::LocalEnvironment {
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

fn invalid<T>(path: &Path, reason: impl Into<String>) -> Result<T> {
    Err(Error::LocalEnvironment {
        path: path.to_path_buf(),
        reason: reason.into(),
    })
}

#[cfg(all(test, feature = "project-write"))]
mod tests {
    use super::*;
    use crate::{EnvironmentProfile, ProjectEnvironment};
    use std::collections::BTreeMap;

    #[test]
    fn local_choices_are_bound_to_project_identity_and_ci_is_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let config_path = crate::create_project_config(dir.path()).unwrap();
        let mut config = crate::load_project_config(&config_path).unwrap();
        config.environment = Some(ProjectEnvironment {
            auto_profile: Some("dev".into()),
            profiles: ["dev", "test"]
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        EnvironmentProfile {
                            file: format!("pinset.env/{name}.age"),
                            recipients: vec!["age1example".into()],
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            ..Default::default()
        });
        // Recipient cryptography is tested by pinset-env; this fixture tests selection only.
        fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
        std::thread::scope(|scope| {
            for profile in ["dev", "test"] {
                let home = &home;
                let config_path = &config_path;
                scope.spawn(move || {
                    for _ in 0..4 {
                        save_local_environment(home, config_path, Some(profile)).unwrap();
                        let current = crate::load_project_config(config_path).unwrap();
                        assert!(
                            select_environment(home, config_path, &current, None, None, false)
                                .unwrap()
                                .profile
                                .is_some()
                        );
                    }
                });
            }
        });
        save_local_environment(&home, &config_path, Some("test")).unwrap();
        assert_eq!(
            select_environment(&home, &config_path, &config, None, None, false)
                .unwrap()
                .source,
            "local"
        );
        assert_eq!(
            select_environment(&home, &config_path, &config, None, None, true)
                .unwrap()
                .profile
                .as_deref(),
            Some("dev")
        );
        assert_eq!(
            select_environment(
                &home,
                &config_path,
                &config,
                Some("dev"),
                Some("test"),
                false
            )
            .unwrap()
            .source,
            "argument"
        );
        assert_eq!(
            select_environment(&home, &config_path, &config, None, Some("dev"), false)
                .unwrap()
                .source,
            "process"
        );
        config.project_id = Some("4c5652e4-0000-4000-8000-000000000001".into());
        assert!(select_environment(&home, &config_path, &config, None, None, false).is_err());
        assert!(select_environment(&home, &config_path, &config, None, None, true).is_ok());
        save_local_environment(&home, &config_path, None).unwrap();
        assert_eq!(
            select_environment(&home, &config_path, &config, None, None, false)
                .unwrap()
                .source,
            "project"
        );
    }
}
