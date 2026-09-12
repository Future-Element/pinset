use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use fs4::FileExt;
#[cfg(feature = "project-write")]
use sha2::{Digest, Sha256};

use crate::{Error, Result, global_state_dir};

pub struct StateWriteLock {
    file: File,
}

impl Drop for StateWriteLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn acquire_global_state_write_lock(pinset_home: &Path) -> Result<StateWriteLock> {
    acquire(pinset_home, "global.lock")
}

pub fn acquire_self_update_lock(pinset_home: &Path) -> Result<StateWriteLock> {
    acquire(pinset_home, "self-update.lock")
}

#[cfg(feature = "project-write")]
pub fn acquire_setup_state_write_lock(pinset_home: &Path, root: &Path) -> Result<StateWriteLock> {
    let canonical = fs::canonicalize(root).map_err(|source| Error::OpenStateWriteLock { path: root.to_path_buf(), source })?;
    let identity = if cfg!(windows) { canonical.to_string_lossy().to_lowercase() } else { canonical.to_string_lossy().into_owned() };
    acquire(pinset_home, &format!("setup-{}.lock", hex_lower(&Sha256::digest(identity.as_bytes()))))
}

#[cfg(feature = "project-write")]
pub fn acquire_project_state_write_lock(
    pinset_home: &Path,
    config_path: &Path,
) -> Result<StateWriteLock> {
    let canonical = fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    let identity = if cfg!(windows) {
        canonical.to_string_lossy().to_ascii_lowercase()
    } else {
        canonical.to_string_lossy().into_owned()
    };
    let digest = Sha256::digest(identity.as_bytes());
    let name = format!("project-{}.lock", hex_lower(&digest));
    acquire(pinset_home, &name)
}

fn acquire(pinset_home: &Path, name: &str) -> Result<StateWriteLock> {
    let directory = state_write_lock_dir(pinset_home);
    fs::create_dir_all(&directory).map_err(|source| Error::CreateStateWriteLockDirectory {
        path: directory.clone(),
        source,
    })?;
    let directory_metadata =
        fs::symlink_metadata(&directory).map_err(|source| Error::OpenStateWriteLock {
            path: directory.clone(),
            source,
        })?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(Error::OpenStateWriteLock {
            path: directory,
            source: std::io::Error::other("state write-lock directory is not a regular directory"),
        });
    }
    let path = directory.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(Error::OpenStateWriteLock {
            path,
            source: std::io::Error::other("state write lock is not a regular file"),
        });
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|source| Error::OpenStateWriteLock {
            path: path.clone(),
            source,
        })?;
    FileExt::lock(&file).map_err(|source| Error::AcquireStateWriteLock { path, source })?;
    Ok(StateWriteLock { file })
}

fn state_write_lock_dir(pinset_home: &Path) -> PathBuf {
    global_state_dir(pinset_home).join("write-locks")
}

#[cfg(feature = "project-write")]
fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
