//! Machine-local directory identity. No files are written while resolving it.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// These identifiers are local state, never part of a portable environment report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkDirectoryIdentity {
    pub schema: u32,
    pub root: PathBuf,
    pub host: String,
    pub generation: String,
    pub namespace: String,
}

pub fn work_directory_identity(root: &Path) -> io::Result<WorkDirectoryIdentity> {
    let root = fs::canonicalize(root)?;
    let metadata = fs::metadata(&root)?;
    if !metadata.is_dir() {
        return Err(io::Error::other("environment root must be a directory"));
    }
    // File IDs alone may be reused after deletion. Birth time must be available;
    // an unsupported filesystem cannot safely inherit persistent local state.
    let created = metadata
        .created()?
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let mut generation = IdentityHasher(Sha256::new());
    same_file::Handle::from_path(&root)?.hash(&mut generation);
    generation.0.update(created.as_nanos().to_le_bytes());
    let generation = hex(&generation.0.finalize());
    let host = host_identity()?;
    let path = root.to_string_lossy();
    let path = if cfg!(windows) {
        path.to_ascii_lowercase()
    } else {
        path.into_owned()
    };
    let mut digest = Sha256::new();
    for part in ["pinset-directory-v1", &host, &path, &generation] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    Ok(WorkDirectoryIdentity {
        schema: 1,
        root,
        host,
        generation,
        namespace: hex(&digest.finalize()),
    })
}

fn host_identity() -> io::Result<String> {
    let mut digest = Sha256::new();
    digest.update(env::consts::OS.as_bytes());
    digest.update(env::consts::ARCH.as_bytes());
    let mut identified = false;
    if cfg!(unix) {
        for path in [
            "/etc/machine-id",
            "/var/lib/dbus/machine-id",
            "/etc/hostname",
        ] {
            if let Ok(metadata) = fs::metadata(path)
                && metadata.is_file()
                && metadata.len() <= 4096
            {
                let bytes = fs::read(path)?;
                if !bytes.is_empty() {
                    digest.update(path.as_bytes());
                    digest.update(bytes);
                    identified = true;
                }
            }
        }
    }
    for name in ["COMPUTERNAME", "HOSTNAME", "WSL_DISTRO_NAME"] {
        if let Some(value) = env::var_os(name).filter(|value| !value.is_empty()) {
            digest.update(name.as_bytes());
            digest.update(value.to_string_lossy().as_bytes());
            identified = true;
        }
    }
    // macOS shells need not export HOSTNAME. Its /private directory identity is
    // host-local and survives normal shell/SSH sessions without executing a tool.
    if cfg!(target_os = "macos") {
        let mut handle = IdentityHasher(Sha256::new());
        same_file::Handle::from_path("/private")?.hash(&mut handle);
        digest.update(handle.0.finalize());
        identified = true;
    }
    if !identified {
        return Err(io::Error::other(
            "cannot identify the local environment host",
        ));
    }
    Ok(hex(&digest.finalize()))
}

struct IdentityHasher(Sha256);
impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        u64::from_le_bytes(
            self.0.clone().finalize()[..8]
                .try_into()
                .expect("digest length"),
        )
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_edits_keep_identity_but_directory_replacement_does_not() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let original = work_directory_identity(&root).unwrap();
        fs::write(root.join("pinset.toml"), "schema=6").unwrap();
        assert_eq!(original, work_directory_identity(&root).unwrap());
        fs::rename(&root, temp.path().join("previous")).unwrap();
        fs::create_dir(&root).unwrap();
        assert_ne!(
            original.namespace,
            work_directory_identity(&root).unwrap().namespace
        );
        assert_ne!(
            original.namespace,
            work_directory_identity(&temp.path().join("previous"))
                .unwrap()
                .namespace
        );
    }
    #[cfg(unix)]
    #[test]
    fn symbolic_directory_aliases_resolve_to_one_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert_eq!(
            work_directory_identity(&root).unwrap(),
            work_directory_identity(&alias).unwrap()
        );
    }
}
