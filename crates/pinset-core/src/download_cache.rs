use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

const CACHE_DIRECTORY: &str = "downloads";
const SHA256_DIRECTORY: &str = "sha256";
const ARCHIVE_SUFFIX: &str = ".archive";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCacheEntry {
    pub sha256: String,
    pub size: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCacheCleanOutcome {
    pub entries: usize,
    pub bytes: u64,
}

pub fn download_cache_path(pinset_home: &Path, sha256: &str) -> Result<PathBuf> {
    if !is_sha256(sha256) {
        return Err(Error::InvalidSha256 {
            value: sha256.to_owned(),
        });
    }
    Ok(pinset_home
        .join(CACHE_DIRECTORY)
        .join(SHA256_DIRECTORY)
        .join(format!("{}.archive", sha256.to_ascii_lowercase())))
}

pub fn list_download_cache(pinset_home: &Path) -> Result<Vec<DownloadCacheEntry>> {
    let root = pinset_home.join(CACHE_DIRECTORY).join(SHA256_DIRECTORY);
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(Error::ReadDownloadCache { path: root, source }),
    };
    let mut cached = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadDownloadCache {
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(sha256) = name.strip_suffix(ARCHIVE_SUFFIX) else {
            continue;
        };
        if !is_sha256(sha256) {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|source| Error::ReadDownloadCache {
                path: entry.path(),
                source,
            })?;
        if !metadata.is_file()
            || !entry
                .file_type()
                .map_err(|source| Error::ReadDownloadCache {
                    path: entry.path(),
                    source,
                })?
                .is_file()
        {
            return Err(Error::UnsafeDownloadCacheEntry { path: entry.path() });
        }
        cached.push(DownloadCacheEntry {
            sha256: sha256.to_owned(),
            size: metadata.len(),
            path: entry.path(),
        });
    }
    cached.sort_by_key(|entry| Reverse(entry.sha256.clone()));
    Ok(cached)
}

pub fn clean_download_cache(pinset_home: &Path) -> Result<DownloadCacheCleanOutcome> {
    let entries = list_download_cache(pinset_home)?;
    let mut outcome = DownloadCacheCleanOutcome {
        entries: 0,
        bytes: 0,
    };
    for entry in entries {
        fs::remove_file(&entry.path).map_err(|source| Error::RemoveDownloadCacheEntry {
            path: entry.path,
            source,
        })?;
        outcome.entries += 1;
        outcome.bytes = outcome.bytes.saturating_add(entry.size);
    }
    remove_if_empty(&pinset_home.join(CACHE_DIRECTORY).join(SHA256_DIRECTORY))?;
    remove_if_empty(&pinset_home.join(CACHE_DIRECTORY))?;
    Ok(outcome)
}

fn remove_if_empty(path: &Path) -> Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(source)
            if matches!(
                source.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(source) => Err(Error::RemoveDownloadCacheEntry {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_cleans_only_content_addressed_cache_entries() {
        let home = tempfile::tempdir().expect("home");
        let root = home.path().join("downloads/sha256");
        fs::create_dir_all(&root).expect("cache root");
        let hash = "a".repeat(64);
        fs::write(root.join(format!("{hash}.archive")), b"cached").expect("cache file");
        fs::write(root.join("notes.txt"), b"leave me").expect("unknown file");

        let entries = list_download_cache(home.path()).expect("cache entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sha256, hash);
        assert_eq!(entries[0].size, 6);

        let outcome = clean_download_cache(home.path()).expect("clean cache");
        assert_eq!(outcome.entries, 1);
        assert_eq!(outcome.bytes, 6);
        assert!(root.join("notes.txt").is_file());
    }

    #[test]
    fn missing_cache_is_empty_and_is_not_created() {
        let home = tempfile::tempdir().expect("home");
        let root = home.path().join("downloads");
        assert!(list_download_cache(home.path()).expect("cache").is_empty());
        assert_eq!(clean_download_cache(home.path()).expect("clean").entries, 0);
        assert!(!root.exists());
    }
}
