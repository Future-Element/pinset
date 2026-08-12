use std::{
    cmp::Reverse,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use tempfile::Builder;

use crate::{ArtifactIntegrity, Error, IntegrityAlgorithm, Result};

const CACHE_DIRECTORY: &str = "downloads";
const PARTIAL_DIRECTORY: &str = "partial";
const ARCHIVE_SUFFIX: &str = ".archive";
const MAX_CACHE_IMPORT_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCacheEntry {
    pub integrity: String,
    pub size: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCacheCleanOutcome {
    pub entries: usize,
    pub bytes: u64,
}

pub fn download_cache_path(pinset_home: &Path, sha256: &str) -> Result<PathBuf> {
    let integrity = ArtifactIntegrity::parse(sha256).map_err(|_| Error::InvalidSha256 {
        value: sha256.to_owned(),
    })?;
    if integrity.algorithm() != IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidSha256 {
            value: sha256.to_owned(),
        });
    }
    download_cache_path_for_integrity(pinset_home, &integrity)
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

pub fn list_download_cache(pinset_home: &Path) -> Result<Vec<DownloadCacheEntry>> {
    let mut cached = Vec::new();
    for algorithm in [IntegrityAlgorithm::Sha256, IntegrityAlgorithm::Sha512] {
        list_download_cache_algorithm(pinset_home, algorithm, &mut cached)?;
    }
    cached.sort_by_key(|entry| Reverse(entry.integrity.clone()));
    Ok(cached)
}

fn list_download_cache_algorithm(
    pinset_home: &Path,
    algorithm: IntegrityAlgorithm,
    cached: &mut Vec<DownloadCacheEntry>,
) -> Result<()> {
    let root = pinset_home.join(CACHE_DIRECTORY).join(algorithm.as_str());
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(Error::ReadDownloadCache { path: root, source }),
    };
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadDownloadCache {
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(hash) = name.strip_suffix(ARCHIVE_SUFFIX) else {
            continue;
        };
        let candidate = format!("{}:{hash}", algorithm.as_str());
        let Ok(integrity) = ArtifactIntegrity::parse(&candidate) else {
            continue;
        };
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
            integrity: integrity.canonical(),
            size: metadata.len(),
            path: entry.path(),
        });
    }
    Ok(())
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
    let partial_root = pinset_home.join(CACHE_DIRECTORY).join(PARTIAL_DIRECTORY);
    for algorithm in [IntegrityAlgorithm::Sha256, IntegrityAlgorithm::Sha512] {
        let root = partial_root.join(algorithm.as_str());
        for entry in list_partial_downloads(&root, algorithm)? {
            fs::remove_file(&entry.path).map_err(|source| Error::RemoveDownloadCacheEntry {
                path: entry.path,
                source,
            })?;
            outcome.entries += 1;
            outcome.bytes = outcome.bytes.saturating_add(entry.size);
        }
        remove_if_empty(&root)?;
    }
    for algorithm in [IntegrityAlgorithm::Sha256, IntegrityAlgorithm::Sha512] {
        remove_if_empty(&pinset_home.join(CACHE_DIRECTORY).join(algorithm.as_str()))?;
    }
    remove_if_empty(&partial_root)?;
    remove_if_empty(&pinset_home.join(CACHE_DIRECTORY))?;
    Ok(outcome)
}

fn list_partial_downloads(
    root: &Path,
    algorithm: IntegrityAlgorithm,
) -> Result<Vec<DownloadCacheEntry>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadDownloadCache {
                path: root.to_path_buf(),
                source,
            });
        }
    };
    let mut partials = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadDownloadCache {
            path: root.to_path_buf(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(hash) = name.strip_suffix(".part") else {
            continue;
        };
        let candidate = format!("{}:{hash}", algorithm.as_str());
        let Ok(integrity) = ArtifactIntegrity::parse(&candidate) else {
            continue;
        };
        let file_type = entry
            .file_type()
            .map_err(|source| Error::ReadDownloadCache {
                path: entry.path(),
                source,
            })?;
        if !file_type.is_file() {
            return Err(Error::UnsafeDownloadCacheEntry { path: entry.path() });
        }
        let metadata = entry
            .metadata()
            .map_err(|source| Error::ReadDownloadCache {
                path: entry.path(),
                source,
            })?;
        partials.push(DownloadCacheEntry {
            integrity: integrity.canonical(),
            size: metadata.len(),
            path: entry.path(),
        });
    }
    Ok(partials)
}

#[cfg(test)]
pub(crate) fn download_partial_path(pinset_home: &Path, sha256: &str) -> Result<PathBuf> {
    let integrity = ArtifactIntegrity::parse(sha256).map_err(|_| Error::InvalidSha256 {
        value: sha256.to_owned(),
    })?;
    if integrity.algorithm() != IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidSha256 {
            value: sha256.to_owned(),
        });
    }
    download_partial_path_for_integrity(pinset_home, &integrity)
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

pub fn import_download_cache(
    pinset_home: &Path,
    archive: &Path,
    expected_sha256: &str,
) -> Result<DownloadCacheEntry> {
    let integrity =
        ArtifactIntegrity::parse(expected_sha256).map_err(|_| Error::InvalidSha256 {
            value: expected_sha256.to_owned(),
        })?;
    if integrity.algorithm() != IntegrityAlgorithm::Sha256 {
        return Err(Error::InvalidSha256 {
            value: expected_sha256.to_owned(),
        });
    }
    import_download_cache_with_integrity(pinset_home, archive, &integrity)
}

pub fn import_download_cache_with_integrity(
    pinset_home: &Path,
    archive: &Path,
    integrity: &ArtifactIntegrity,
) -> Result<DownloadCacheEntry> {
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
        let (actual, size) = hash_file(&destination, integrity)?;
        if actual != expected {
            return Err(Error::ChecksumMismatch { expected, actual });
        }
        return Ok(DownloadCacheEntry {
            integrity: actual,
            size,
            path: destination,
        });
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
    Ok(DownloadCacheEntry {
        integrity: actual,
        size,
        path: destination,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

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
        assert_eq!(entries[0].integrity, format!("sha256:{hash}"));
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

    #[test]
    fn clean_removes_resumable_partial_downloads() {
        let home = tempfile::tempdir().expect("home");
        let sha256 = "a".repeat(64);
        let partial = download_partial_path(home.path(), &sha256).expect("partial path");
        fs::create_dir_all(partial.parent().expect("partial parent")).expect("partial root");
        fs::write(&partial, b"partial").expect("partial");

        let outcome = clean_download_cache(home.path()).expect("clean");

        assert_eq!(outcome.entries, 1);
        assert_eq!(outcome.bytes, 7);
        assert!(!partial.exists());
    }

    #[test]
    fn imports_only_an_archive_matching_the_declared_sha256() {
        let home = tempfile::tempdir().expect("home");
        let source = home.path().join("node.tar.xz");
        fs::write(&source, b"offline node archive").expect("source");
        let expected = hex::encode(Sha256::digest(b"offline node archive"));

        let imported =
            import_download_cache(home.path(), &source, &expected).expect("cache import");
        assert_eq!(imported.integrity, format!("sha256:{expected}"));
        assert_eq!(
            fs::read(&imported.path).expect("cached"),
            b"offline node archive"
        );
        assert!(matches!(
            import_download_cache(home.path(), &source, &"0".repeat(64)),
            Err(Error::ChecksumMismatch { .. })
        ));
    }
}
