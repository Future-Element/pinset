//! Flutter writes caches inside its SDK. Each work directory receives an owned
//! copy; immutable download archives and the pristine installation stay shared.
use crate::{Error, Result};
use sha2::{Digest, Sha256};
#[cfg(feature = "project-write")]
use std::io;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn workspace_flutter_directory(
    home: &Path,
    config_path: &Path,
    installation: &str,
    target: &str,
) -> Result<PathBuf> {
    Ok(workspace_flutter_spec(home, config_path, installation, target)?.0)
}

fn workspace_flutter_spec(
    home: &Path,
    config_path: &Path,
    installation: &str,
    target: &str,
) -> Result<(PathBuf, String)> {
    if [installation, target].iter().any(|value| {
        value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    }) {
        return Err(Error::InvalidProjectConfig {
            reason: "unsafe workspace runtime identity".into(),
        });
    }
    let root = config_path.parent().unwrap_or(Path::new("."));
    let identity = crate::work_directory_identity(root).map_err(|error| failure(root, error))?;
    let source = home
        .join("installs/flutter")
        .join(installation)
        .join(target);
    let receipt = source.join(".pinset-install.toml");
    let metadata = fs::symlink_metadata(&receipt).map_err(|error| failure(&receipt, error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err(failure(&receipt, "expected a bounded regular SDK receipt"));
    }
    let bytes = fs::read(&receipt).map_err(|error| failure(&receipt, error))?;
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let marker = format!(
        "2\n{}\n{digest}\n{installation}\n{target}\n",
        identity.namespace
    );
    // Compact path components keep Windows batch launch paths usable. The
    // marker retains both full digests; a truncated-name collision fails closed.
    Ok((
        home.join("state/workspaces")
            .join(&identity.namespace[..32])
            .join("flutter")
            .join(&digest[..32]),
        marker,
    ))
}

pub fn prepared_workspace_flutter(
    home: &Path,
    config_path: &Path,
    installation: &str,
    target: &str,
) -> Result<PathBuf> {
    let (path, expected_marker) = workspace_flutter_spec(home, config_path, installation, target)?;
    let marker = path.join(".pinset-workspace-runtime");
    if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        && fs::symlink_metadata(&marker).is_ok_and(|m| {
            m.is_file() && !m.file_type().is_symlink() && m.len() == expected_marker.len() as u64
        })
        && fs::read(&marker).is_ok_and(|bytes| bytes == expected_marker.as_bytes())
    {
        Ok(path)
    } else {
        Err(failure(
            &path,
            "Flutter's local SDK/cache is not prepared; run `pinset setup` or `pinset install --locked`",
        ))
    }
}

#[cfg(feature = "project-write")]
pub fn prepare_workspace_flutter(
    home: &Path,
    config_path: &Path,
    installation: &str,
    target: &str,
) -> Result<PathBuf> {
    use fs4::FileExt;
    let (path, expected_marker) = workspace_flutter_spec(home, config_path, installation, target)?;
    let parent = path.parent().expect("SDK parent");
    fs::create_dir_all(home).map_err(|error| failure(home, error))?;
    // Reject redirected ancestors before creating files in the local namespace.
    let mut ancestor = home.to_path_buf();
    for component in parent
        .strip_prefix(home)
        .map_err(|error| failure(parent, error))?
        .components()
    {
        ancestor.push(component);
        match fs::symlink_metadata(&ancestor) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&ancestor).map_err(|error| failure(&ancestor, error))?
            }
            Err(error) => return Err(failure(&ancestor, error)),
            Ok(_) => {}
        }
        if fs::symlink_metadata(&ancestor)
            .map_err(|error| failure(&ancestor, error))?
            .file_type()
            .is_symlink()
        {
            return Err(failure(
                &ancestor,
                "workspace runtime ancestors must be regular directories",
            ));
        }
    }
    let lock_path = parent.join(".prepare.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_file()) {
        return Err(failure(
            &lock_path,
            "workspace runtime lock must be a regular file",
        ));
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| failure(&lock_path, error))?;
    FileExt::lock(&lock).map_err(|error| failure(&lock_path, error))?;
    if path.exists() {
        return prepared_workspace_flutter(home, config_path, installation, target);
    }
    let source = fs::canonicalize(
        home.join("installs/flutter")
            .join(installation)
            .join(target),
    )
    .map_err(|error| failure(home, error))?;
    let temp = tempfile::Builder::new()
        .prefix(".prepare-")
        .tempdir_in(parent)
        .map_err(|error| failure(parent, error))?;
    let output = temp.path().join("sdk");
    copy_sdk(&source, &source, &output, &mut (0, 0), 0).map_err(|error| failure(&source, error))?;
    fs::write(output.join(".pinset-workspace-runtime"), expected_marker)
        .map_err(|error| failure(&output, error))?;
    fs::rename(&output, &path).map_err(|error| failure(&path, error))?;
    Ok(path)
}

#[cfg(feature = "project-write")]
fn copy_sdk(
    root: &Path,
    source: &Path,
    destination: &Path,
    budget: &mut (u64, u64),
    depth: usize,
) -> io::Result<()> {
    budget.0 += 1;
    if depth > 64 || budget.0 > 250_000 {
        return Err(io::Error::other("workspace SDK exceeds copy limits"));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        let resolved = fs::canonicalize(source)?;
        if !resolved.starts_with(root) {
            return Err(io::Error::other(
                "SDK link escapes the verified installation",
            ));
        }
        #[cfg(unix)]
        {
            let mut relative = PathBuf::new();
            for _ in source
                .parent()
                .expect("SDK entry parent")
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .components()
            {
                relative.push("..");
            }
            relative.push(resolved.strip_prefix(root).map_err(io::Error::other)?);
            return std::os::unix::fs::symlink(relative, destination);
        }
        #[cfg(windows)]
        {
            return copy_sdk(root, &resolved, destination, budget, depth + 1);
        }
    }
    if metadata.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_sdk(
                root,
                &entry.path(),
                &destination.join(entry.file_name()),
                budget,
                depth + 1,
            )?;
        }
    } else if metadata.is_file() {
        budget.1 = budget.1.saturating_add(metadata.len());
        if budget.1 > 16 * 1024 * 1024 * 1024 {
            return Err(io::Error::other("workspace SDK exceeds 16 GiB"));
        }
        fs::copy(source, destination)?;
    } else {
        return Err(io::Error::other("SDK contains a special file"));
    }
    Ok(())
}

fn failure(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::LocalEnvironment {
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

#[cfg(all(test, feature = "project-write"))]
mod tests {
    use super::*;
    #[test]
    fn two_work_directories_never_share_flutter_cache_files() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let source = home.join("installs/flutter/3.35.3/linux-x86_64");
        fs::create_dir_all(source.join("bin/cache")).unwrap();
        fs::write(source.join(".pinset-install.toml"), "verified receipt").unwrap();
        fs::write(source.join("bin/cache/stamp"), "pristine").unwrap();
        let one = temp.path().join("one");
        let two = temp.path().join("two");
        fs::create_dir(&one).unwrap();
        fs::create_dir(&two).unwrap();
        let first =
            prepare_workspace_flutter(&home, &one.join("pinset.toml"), "3.35.3", "linux-x86_64")
                .unwrap();
        let second =
            prepare_workspace_flutter(&home, &two.join("pinset.toml"), "3.35.3", "linux-x86_64")
                .unwrap();
        fs::write(first.join("bin/cache/stamp"), "worktree-one").unwrap();
        assert_eq!(
            fs::read_to_string(second.join("bin/cache/stamp")).unwrap(),
            "pristine"
        );
        assert_eq!(
            fs::read_to_string(source.join("bin/cache/stamp")).unwrap(),
            "pristine"
        );
        assert_eq!(
            first,
            prepare_workspace_flutter(&home, &one.join("pinset.toml"), "3.35.3", "linux-x86_64")
                .unwrap()
        );
        assert!(first.strip_prefix(&home).unwrap().to_string_lossy().len() < 100);
        fs::write(
            first.join(".pinset-workspace-runtime"),
            fs::read(second.join(".pinset-workspace-runtime")).unwrap(),
        )
        .unwrap();
        assert!(
            prepared_workspace_flutter(&home, &one.join("pinset.toml"), "3.35.3", "linux-x86_64")
                .is_err(),
            "compact directory names never replace full ownership checks"
        );
        assert!(
            prepared_workspace_flutter(&home, &two.join("pinset.toml"), "3.35.3", "linux-x86_64")
                .is_ok()
        );
    }
}
