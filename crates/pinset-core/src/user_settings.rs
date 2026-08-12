use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[cfg(feature = "state-write")]
use std::io::Write;

#[cfg(feature = "state-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const USER_SETTINGS_SCHEMA: u32 = 1;
pub const USER_SETTINGS_FILENAME: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserSettings {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            schema: USER_SETTINGS_SCHEMA,
            language: None,
        }
    }
}

pub fn user_settings_path(pinset_home: &Path) -> PathBuf {
    pinset_home.join(USER_SETTINGS_FILENAME)
}

pub fn load_user_settings(path: &Path) -> Result<UserSettings> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(UserSettings::default()),
        Err(source) => {
            return Err(Error::ReadUserSettings {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let settings: UserSettings =
        toml::from_str(&content).map_err(|source| Error::ParseUserSettings {
            path: path.to_path_buf(),
            source,
        })?;
    validate(&settings)?;
    Ok(settings)
}

#[cfg(feature = "state-write")]
pub fn save_user_settings(path: &Path, settings: &UserSettings) -> Result<()> {
    validate(settings)?;
    let serialized = toml::to_string_pretty(settings)
        .map_err(|source| Error::SerializeUserSettings { source })?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::CreateUserSettingsDirectory {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                ErrorKind::InvalidInput,
                "user settings path has no parent directory",
            ),
        })?;
    fs::create_dir_all(parent).map_err(|source| Error::CreateUserSettingsDirectory {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteUserSettings {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteUserSettings {
            path: path.to_path_buf(),
            source,
        })
}

fn validate(settings: &UserSettings) -> Result<()> {
    if settings.schema != USER_SETTINGS_SCHEMA {
        return Err(Error::UnsupportedUserSettingsSchema {
            actual: settings.schema,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn missing_settings_are_read_only_defaults() {
        let root = tempdir().expect("temporary root");
        let path = user_settings_path(&root.path().join("home"));

        assert_eq!(
            load_user_settings(&path).expect("settings"),
            UserSettings::default()
        );
        assert!(!path.exists());
    }

    #[cfg(feature = "state-write")]
    #[test]
    fn atomically_persists_language_preference() {
        let root = tempdir().expect("temporary root");
        let path = user_settings_path(&root.path().join("home"));
        let settings = UserSettings {
            schema: USER_SETTINGS_SCHEMA,
            language: Some("zh-CN".to_owned()),
        };

        save_user_settings(&path, &settings).expect("save settings");

        assert_eq!(load_user_settings(&path).expect("settings"), settings);
        assert_eq!(
            fs::read_dir(path.parent().expect("settings directory"))
                .expect("read settings directory")
                .count(),
            1
        );
    }

    #[test]
    fn rejects_unknown_fields_and_schema() {
        let root = tempdir().expect("temporary root");
        let path = root.path().join(USER_SETTINGS_FILENAME);
        fs::write(&path, "schema = 1\nunknown = true\n").expect("invalid settings");
        assert!(matches!(
            load_user_settings(&path),
            Err(Error::ParseUserSettings { .. })
        ));

        fs::write(&path, "schema = 2\n").expect("unsupported settings");
        assert!(matches!(
            load_user_settings(&path),
            Err(Error::UnsupportedUserSettingsSchema { actual: 2 })
        ));
    }
}
