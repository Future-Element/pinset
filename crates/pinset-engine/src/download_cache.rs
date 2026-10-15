use crate::{ArtifactIntegrity, Error, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tempfile::Builder;
const CACHE_DIRECTORY: &str = "cache/downloads";
const PARTIAL_DIRECTORY: &str = "partial";
const MAX_CACHE_IMPORT_BYTES: u64 = 1_073_741_824;

pub(crate) struct DownloadGuard(File);
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.0);
    }
}

pub(crate) fn reject_linked_managed_path(home: &Path, path: &Path) -> Result<()> {
    if !path.starts_with(home) {
        return Err(crate::service_error(
            "PINSET_PATH_UNSAFE",
            "managed path escaped Pinset home",
        ));
    }
    for parent in path.ancestors() {
        if fs::symlink_metadata(parent).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(crate::service_error(
                "PINSET_PATH_UNSAFE",
                "managed path contains a linked ancestor",
            ));
        }
        if parent == home {
            break;
        }
    }
    Ok(())
}

pub(crate) fn acquire_download_lock(
    home: &Path,
    integrity: &ArtifactIntegrity,
) -> Result<DownloadGuard> {
    let directory = home.join("state/locks/downloads");
    for path in [
        directory.clone(),
        home.join(CACHE_DIRECTORY)
            .join(integrity.algorithm().as_str()),
        home.join(CACHE_DIRECTORY)
            .join(PARTIAL_DIRECTORY)
            .join(integrity.algorithm().as_str()),
    ] {
        reject_linked_managed_path(home, &path)?;
    }
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!(
        "{}-{}.lock",
        integrity.algorithm().as_str(),
        integrity.cache_key()
    ));
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Error::UnsafeDownloadCacheEntry { path });
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    fs4::FileExt::lock(&file)?;
    Ok(DownloadGuard(file))
}

pub(crate) fn download_cache_path_for_integrity(
    pinset_home: &Path,
    integrity: &ArtifactIntegrity,
) -> Result<PathBuf> {
    Ok(pinset_home
        .join(CACHE_DIRECTORY)
        .join(integrity.algorithm().as_str())
        .join(format!("{}.archive", integrity.cache_key())))
}

pub(crate) fn download_partial_path_for_integrity(
    pinset_home: &Path,
    integrity: &ArtifactIntegrity,
) -> Result<PathBuf> {
    Ok(pinset_home
        .join(CACHE_DIRECTORY)
        .join(PARTIAL_DIRECTORY)
        .join(integrity.algorithm().as_str())
        .join(format!("{}.part", integrity.cache_key())))
}

pub fn import_download_cache_with_integrity(
    pinset_home: &Path,
    archive: &Path,
    integrity: &ArtifactIntegrity,
) -> Result<PathBuf> {
    let _guard = acquire_download_lock(pinset_home, integrity)?;
    let destination = download_cache_path_for_integrity(pinset_home, integrity)?;
    let expected = integrity.canonical();
    let metadata = fs::symlink_metadata(archive).map_err(|source| Error::ReadDownloadCache {
        path: archive.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafeDownloadCacheEntry {
            path: archive.to_path_buf(),
        });
    }
    if metadata.len() > MAX_CACHE_IMPORT_BYTES {
        return Err(Error::DownloadTooLarge {
            url: archive.display().to_string(),
            limit: MAX_CACHE_IMPORT_BYTES,
        });
    }

    if destination.exists() {
        let (actual, _size) = hash_file(&destination, integrity)?;
        if actual != expected {
            return Err(Error::ChecksumMismatch { expected, actual });
        }
        return Ok(destination);
    }

    let parent = destination
        .parent()
        .expect("download cache path always has a parent");
    fs::create_dir_all(parent).map_err(|source| Error::WriteDownload {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut source = File::open(archive).map_err(|source| Error::ReadDownloadCache {
        path: archive.to_path_buf(),
        source,
    })?;
    let mut temporary = Builder::new()
        .prefix(".cache-import-")
        .tempfile_in(parent)
        .map_err(|source| Error::WriteDownload {
            path: destination.clone(),
            source,
        })?;
    let mut hasher = integrity.hasher();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|source| Error::ReadDownloadCache {
                path: archive.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(count as u64);
        if size > MAX_CACHE_IMPORT_BYTES {
            return Err(Error::DownloadTooLarge {
                url: archive.display().to_string(),
                limit: MAX_CACHE_IMPORT_BYTES,
            });
        }
        hasher.update(&buffer[..count]);
        temporary
            .write_all(&buffer[..count])
            .map_err(|source| Error::WriteDownload {
                path: destination.clone(),
                source,
            })?;
    }
    let actual = integrity.canonical_digest(&hasher.finalize());
    if actual != expected {
        return Err(Error::ChecksumMismatch { expected, actual });
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| Error::WriteDownload {
            path: destination.clone(),
            source,
        })?;
    match temporary.persist_noclobber(&destination) {
        Ok(_) => {}
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let (existing_hash, _) = hash_file(&destination, integrity)?;
            if existing_hash != actual {
                return Err(Error::ChecksumMismatch {
                    expected: actual,
                    actual: existing_hash,
                });
            }
        }
        Err(error) => {
            return Err(Error::WriteDownload {
                path: destination,
                source: error.error,
            });
        }
    }
    Ok(destination)
}

fn hash_file(path: &Path, integrity: &ArtifactIntegrity) -> Result<(String, u64)> {
    let metadata = fs::symlink_metadata(path).map_err(|source| Error::ReadDownloadCache {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafeDownloadCacheEntry {
            path: path.to_path_buf(),
        });
    }
    let mut file = File::open(path).map_err(|source| Error::ReadDownloadCache {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = integrity.hasher();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| Error::ReadDownloadCache {
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(count as u64);
        hasher.update(&buffer[..count]);
    }
    Ok((integrity.canonical_digest(&hasher.finalize()), size))
}

#[cfg(test)]
pub(crate) fn download_cache_path(home: &Path, hash: &str) -> Result<PathBuf> {
    download_cache_path_for_integrity(home, &ArtifactIntegrity::parse(hash)?)
}
#[cfg(test)]
pub(crate) fn download_partial_path(home: &Path, hash: &str) -> Result<PathBuf> {
    download_partial_path_for_integrity(home, &ArtifactIntegrity::parse(hash)?)
}
