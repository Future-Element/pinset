//! Bounded content inventory and independent task work directories.
use crate::process_tree;
use pinset_core::{Lockfile, ProjectConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_FILES: usize = 20_000;
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputManifest {
    pub schema: u32,
    pub source: String,
    pub digest: String,
    pub files: Vec<InputFile>,
    pub excluded_sensitive_files: bool,
}

pub fn capture(root: &Path, config: &ProjectConfig) -> Result<InputManifest> {
    let root = fs::canonicalize(root)?;
    let mut excluded_directories = vec![root.join(".venv")];
    if let Some(python) = &config.python {
        excluded_directories.extend(
            python
                .environments
                .values()
                .map(|environment| root.join(&environment.path)),
        );
    }
    if let Ok(home) = pinset_core::pinset_home()
        && home.starts_with(&root)
    {
        excluded_directories.push(home);
    }
    let mut git = Command::new("git");
    process_tree::system_environment(&mut git);
    git.current_dir(&root).args([
        "-c",
        "core.fsmonitor=false",
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        ".",
    ]);
    git.env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .env(
        "GIT_CONFIG_SYSTEM",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    );
    let mut paths = BTreeSet::new();
    let mut excluded = false;
    let source = match process_tree::run(git, Duration::from_secs(15), Some(4 * 1024 * 1024)) {
        Ok(output) if output.code == 0 => {
            for bytes in output
                .stdout
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
            {
                let relative = std::str::from_utf8(bytes)?;
                check_relative(relative)?;
                gather(
                    &root,
                    &excluded_directories,
                    &root.join(relative),
                    &mut paths,
                    &mut excluded,
                    false,
                    0,
                )?;
            }
            "git-tracked-and-untracked"
        }
        Ok(output) if output.code == 124 || output.code == 130 => {
            return Err("candidate input inventory timed out or was canceled".into());
        }
        Ok(_) => {
            gather(
                &root,
                &excluded_directories,
                &root,
                &mut paths,
                &mut excluded,
                false,
                0,
            )?;
            "bounded-directory-inventory"
        }
        Err(error) => return Err(error),
    };
    if let Some(verification) = &config.verification {
        for relative in &verification.inputs {
            check_relative(relative)?;
            gather(
                &root,
                &excluded_directories,
                &root.join(relative),
                &mut paths,
                &mut excluded,
                true,
                0,
            )?;
        }
    }
    // Snapshot these declarations even when .gitignore excludes them.
    for relative in ["pinset.toml", "pinset.lock"] {
        paths.insert(relative.to_owned());
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for path in paths {
        let absolute = root.join(&path);
        let metadata = fs::symlink_metadata(&absolute)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FILE {
            return Err("candidate input is not a bounded regular file".into());
        }
        if !fs::canonicalize(&absolute)?.starts_with(&root) {
            return Err("candidate input escapes its work directory".into());
        }
        let bytes = read_bounded(&absolute)?;
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_BYTES || files.len() >= MAX_FILES {
            return Err("candidate inputs exceed 20,000 files or 256 MiB".into());
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(windows)]
        let executable = false;
        files.push(InputFile {
            path,
            sha256: hash(&bytes),
            bytes: bytes.len() as u64,
            executable,
        });
    }
    let digest = hash(&serde_json::to_vec(&files)?);
    Ok(InputManifest {
        schema: 1,
        source: source.into(),
        digest,
        files,
        excluded_sensitive_files: excluded,
    })
}

fn check_relative(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains(['\\', ':'])
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("candidate inputs must be relative paths inside the work directory".into());
    }
    Ok(())
}

fn gather(
    root: &Path,
    excluded_directories: &[PathBuf],
    path: &Path,
    files: &mut BTreeSet<String>,
    excluded: &mut bool,
    explicit: bool,
    depth: usize,
) -> Result<()> {
    if excluded_directories
        .iter()
        .any(|directory| path.starts_with(directory))
    {
        if explicit {
            return Err(
                "managed environments and Pinset state cannot be verification inputs".into(),
            );
        }
        return Ok(());
    }
    if depth > 64 || files.len() > MAX_FILES {
        return Err("candidate inputs exceed traversal limits".into());
    }
    let relative = path.strip_prefix(root)?;
    if !relative.as_os_str().is_empty() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name == ".git" || name.starts_with(".pinset-") {
            return Ok(());
        }
        if name == ".env" || name.starts_with(".env.") {
            *excluded = true;
            if explicit {
                return Err("secret/environment files are not snapshot inputs; use explicit encrypted profile injection".into());
            }
            return Ok(());
        }
        if !explicit
            && [
                "node_modules",
                ".venv",
                "target",
                "build",
                "dist",
                ".next",
                ".dart_tool",
                ".gradle",
                ".pnpm-store",
            ]
            .contains(&name.as_ref())
        {
            return Ok(());
        }
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        // Git's index may still list a locally deleted input. Its absence is
        // represented by its omission, so restoring it changes the manifest.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !explicit => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err("candidate input symlinks require a regular-file input instead".into());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            gather(
                root,
                excluded_directories,
                &entry?.path(),
                files,
                excluded,
                explicit,
                depth + 1,
            )?;
        }
    } else if metadata.is_file() {
        let value = relative
            .to_str()
            .ok_or("candidate input names must be UTF-8")?
            .replace('\\', "/");
        check_relative(&value)?;
        files.insert(value);
    } else {
        return Err("candidate input is a special file".into());
    }
    Ok(())
}

pub struct Snapshot {
    directory: tempfile::TempDir,
    workspace_cache: Option<PathBuf>,
}
impl Snapshot {
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn create(
        home: &Path,
        original: &Path,
        config: &ProjectConfig,
        lock: &Lockfile,
        inputs: &InputManifest,
    ) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("pinset-candidate-")
            .tempdir()?;
        for input in &inputs.files {
            let source = original.join(&input.path);
            let metadata = fs::symlink_metadata(&source)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAX_FILE
                || !fs::canonicalize(&source)?.starts_with(fs::canonicalize(original)?)
            {
                return Err("candidate input changed while snapshotting".into());
            }
            let bytes = read_bounded(&source)?;
            if hash(&bytes) != input.sha256 {
                return Err("candidate input changed while snapshotting".into());
            }
            let destination = directory.path().join(&input.path);
            fs::create_dir_all(destination.parent().ok_or("snapshot path has no parent")?)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            file.write_all(&bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(if input.executable {
                    0o700
                } else {
                    0o600
                }))?;
            }
        }
        let config_path = directory.path().join("pinset.toml");
        pinset_core::save_project_config(&config_path, config)?;
        pinset_core::save_lockfile(&directory.path().join("pinset.lock"), lock)?;
        let mut snapshot = Self {
            directory,
            workspace_cache: None,
        };
        if let Some(flutter) = lock.tool("flutter") {
            let cache = pinset_core::prepare_workspace_flutter(
                home,
                &config_path,
                &flutter.installation_version(),
                &pinset_core::current_target_for_tool("flutter"),
            )?;
            snapshot.workspace_cache = Some(cache);
        }
        Ok(snapshot)
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        if let Some(cache) = &self.workspace_cache
            && fs::symlink_metadata(cache)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        {
            // Only this freshly-created temporary directory's namespace is owned
            // by the snapshot. Task process trees are joined before it is dropped.
            let _ = fs::remove_dir_all(cache);
        }
    }
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("candidate input exceeds 16 MiB".into());
    }
    Ok(bytes)
}
