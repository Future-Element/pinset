use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShimInstallMethod {
    HardLink,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShimInstallResult {
    pub command: String,
    pub destination: PathBuf,
    pub method: ShimInstallMethod,
}

pub fn install_shims(
    shim_binary: &Path,
    destination_dir: &Path,
    commands: &[String],
) -> Result<Vec<ShimInstallResult>> {
    if !shim_binary.is_file() {
        return Err(Error::InvalidShimSource {
            path: shim_binary.to_path_buf(),
        });
    }

    fs::create_dir_all(destination_dir).map_err(|source| Error::CreateShimDirectory {
        path: destination_dir.to_path_buf(),
        source,
    })?;

    let mut seen = HashSet::new();
    let mut destinations = Vec::with_capacity(commands.len());
    for command in commands {
        let filename = shim_filename(command)?;
        if !seen.insert(filename.clone()) {
            return Err(Error::DuplicateShimCommand {
                command: command.clone(),
            });
        }
        let destination = destination_dir.join(filename);
        if destination.exists() {
            return Err(Error::ShimDestinationExists { path: destination });
        }
        destinations.push((command, destination));
    }

    let mut installed = Vec::with_capacity(destinations.len());
    for (command, destination) in destinations {
        match install_one(shim_binary, command, destination) {
            Ok(result) => installed.push(result),
            Err(error) => {
                for result in &installed {
                    let _ = fs::remove_file(&result.destination);
                }
                return Err(error);
            }
        }
    }
    Ok(installed)
}

fn install_one(
    shim_binary: &Path,
    command: &str,
    destination: PathBuf,
) -> Result<ShimInstallResult> {
    let method = match fs::hard_link(shim_binary, &destination) {
        Ok(()) => ShimInstallMethod::HardLink,
        Err(_) => {
            copy_create_new(shim_binary, &destination).map_err(|source| Error::InstallShim {
                source_path: shim_binary.to_path_buf(),
                destination: destination.clone(),
                source,
            })?;
            ShimInstallMethod::Copy
        }
    };

    Ok(ShimInstallResult {
        command: command.to_owned(),
        destination,
        method,
    })
}

fn shim_filename(command: &str) -> Result<String> {
    if command.is_empty() || command == "." || command == ".." || command.contains(['/', '\\', ':'])
    {
        return Err(Error::InvalidShimCommand {
            command: command.to_owned(),
        });
    }

    Ok(if cfg!(windows) {
        format!("{command}.exe")
    } else {
        command.to_owned()
    })
}

fn copy_create_new(source: &Path, destination: &Path) -> io::Result<()> {
    let permissions = fs::metadata(source)?.permissions();
    let mut source_file = File::open(source)?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    if let Err(error) = io::copy(&mut source_file, &mut destination_file) {
        drop(destination_file);
        drop(source_file);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    drop(destination_file);
    drop(source_file);
    if let Err(error) = fs::set_permissions(destination, permissions) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn installs_requested_shims_without_admin_paths() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        fs::write(&source, b"fake shim").expect("source");

        let results = install_shims(
            &source,
            &root.path().join("shims"),
            &["node".to_owned(), "npm".to_owned()],
        )
        .expect("install");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.destination.is_file()));
    }

    #[test]
    fn refuses_to_overwrite_existing_command() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        let shims = root.path().join("shims");
        fs::create_dir_all(&shims).expect("shims");
        fs::write(&source, b"fake shim").expect("source");
        let existing = shims.join(if cfg!(windows) { "node.exe" } else { "node" });
        fs::write(&existing, b"user executable").expect("existing");

        let error =
            install_shims(&source, &shims, &["node".to_owned()]).expect_err("must not overwrite");
        assert!(matches!(error, Error::ShimDestinationExists { .. }));
        assert_eq!(
            fs::read(&existing).expect("existing content"),
            b"user executable"
        );
    }

    #[test]
    fn rejects_path_like_and_duplicate_commands() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        fs::write(&source, b"fake shim").expect("source");

        let path_error =
            install_shims(&source, &root.path().join("shims"), &["../node".to_owned()])
                .expect_err("path command");
        assert!(matches!(path_error, Error::InvalidShimCommand { .. }));

        let duplicate_error = install_shims(
            &source,
            &root.path().join("other-shims"),
            &["node".to_owned(), "node".to_owned()],
        )
        .expect_err("duplicate command");
        assert!(matches!(
            duplicate_error,
            Error::DuplicateShimCommand { .. }
        ));
    }
}
