use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Read},
    path::{Path, PathBuf},
};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShimInstallMethod {
    Symlink,
    Wrapper,
    HardLink,
    Copy,
    Existing,
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
        if let Some(existing) =
            existing_command_entry(destination_dir, command).map_err(|source| {
                Error::InstallShim {
                    source_path: shim_binary.to_path_buf(),
                    destination: destination.clone(),
                    source,
                }
            })?
        {
            return Err(Error::ShimDestinationExists { path: existing });
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

pub fn ensure_shims(
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
    let mut existing = Vec::new();
    let mut missing = Vec::new();
    for command in commands {
        let filename = shim_filename(command)?;
        if !seen.insert(filename.clone()) {
            return Err(Error::DuplicateShimCommand {
                command: command.clone(),
            });
        }
        let destination = destination_dir.join(filename);
        if let Some(existing) =
            existing_alternate_command_entry(destination_dir, command, &destination).map_err(
                |source| Error::InstallShim {
                    source_path: shim_binary.to_path_buf(),
                    destination: destination.clone(),
                    source,
                },
            )?
        {
            return Err(Error::ShimDestinationExists { path: existing });
        }
        if path_entry_exists(&destination).map_err(|source| Error::InstallShim {
            source_path: shim_binary.to_path_buf(),
            destination: destination.clone(),
            source,
        })? {
            if is_managed_command_shim(shim_binary, &destination, command).map_err(|source| {
                Error::InstallShim {
                    source_path: shim_binary.to_path_buf(),
                    destination: destination.clone(),
                    source,
                }
            })? {
                existing.push(ShimInstallResult {
                    command: command.clone(),
                    destination,
                    method: ShimInstallMethod::Existing,
                });
            } else {
                return Err(Error::ShimDestinationExists { path: destination });
            }
        } else {
            missing.push((command, destination));
        }
    }

    let mut installed = Vec::with_capacity(missing.len());
    for (command, destination) in missing {
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
    existing.extend(installed);
    Ok(existing)
}

fn install_one(
    shim_binary: &Path,
    command: &str,
    destination: PathBuf,
) -> Result<ShimInstallResult> {
    #[cfg(windows)]
    {
        write_windows_wrapper(shim_binary, command, &destination).map_err(|source| {
            Error::InstallShim {
                source_path: shim_binary.to_path_buf(),
                destination: destination.clone(),
                source,
            }
        })?;
        Ok(ShimInstallResult {
            command: command.to_owned(),
            destination,
            method: ShimInstallMethod::Wrapper,
        })
    }

    #[cfg(unix)]
    if std::os::unix::fs::symlink(shim_binary, &destination).is_ok() {
        return Ok(ShimInstallResult {
            command: command.to_owned(),
            destination,
            method: ShimInstallMethod::Symlink,
        });
    }

    #[cfg(not(windows))]
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

    #[cfg(not(windows))]
    Ok(ShimInstallResult {
        command: command.to_owned(),
        destination,
        method,
    })
}

fn shim_filename(command: &str) -> Result<String> {
    if command.is_empty()
        || command == "."
        || command == ".."
        || !command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(Error::InvalidShimCommand {
            command: command.to_owned(),
        });
    }

    Ok(if cfg!(windows) {
        format!("{command}.cmd")
    } else {
        command.to_owned()
    })
}

fn command_entry_paths(directory: &Path, command: &str) -> Result<Vec<PathBuf>> {
    let primary = directory.join(shim_filename(command)?);
    if cfg!(windows) {
        Ok([
            format!("{command}.exe"),
            format!("{command}.cmd"),
            format!("{command}.bat"),
            command.to_owned(),
        ]
        .into_iter()
        .map(|name| directory.join(name))
        .collect())
    } else {
        Ok(vec![primary])
    }
}

fn existing_command_entry(directory: &Path, command: &str) -> io::Result<Option<PathBuf>> {
    command_entry_paths(directory, command)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error.to_string()))?
        .into_iter()
        .find_map(|path| match path_entry_exists(&path) {
            Ok(true) => Some(Ok(path)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .transpose()
}

fn existing_alternate_command_entry(
    directory: &Path,
    command: &str,
    primary: &Path,
) -> io::Result<Option<PathBuf>> {
    command_entry_paths(directory, command)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error.to_string()))?
        .into_iter()
        .filter(|path| path != primary)
        .find_map(|path| match path_entry_exists(&path) {
            Ok(true) => Some(Ok(path)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .transpose()
}

#[cfg(windows)]
fn write_windows_wrapper(source: &Path, command: &str, destination: &Path) -> io::Result<()> {
    let wrapper = windows_wrapper(source, command);
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    if let Err(error) = io::Write::write_all(&mut destination_file, wrapper.as_bytes()) {
        drop(destination_file);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn windows_wrapper(source: &Path, command: &str) -> String {
    let source = source.to_string_lossy().replace('%', "%%");
    format!(
        "@echo off\r\nsetlocal DisableDelayedExpansion\r\n\"{source}\" --as {command} -- %*\r\nexit /b %ERRORLEVEL%\r\n"
    )
}

#[cfg(not(windows))]
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

fn path_entry_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Returns whether `destination` is another entry for the same Pinset shim binary.
///
/// This recognizes symbolic links, hard links, and byte-identical copy fallbacks. It is
/// intentionally read-only so diagnostics and migration code can distinguish Pinset-owned
/// command routes from foreign files without relying on their directory name.
pub fn is_managed_shim(source: &Path, destination: &Path) -> io::Result<bool> {
    if same_file::is_same_file(source, destination).unwrap_or(false) {
        return Ok(true);
    }
    let source_metadata = fs::metadata(source)?;
    let destination_metadata = match fs::metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !source_metadata.is_file()
        || !destination_metadata.is_file()
        || source_metadata.len() != destination_metadata.len()
    {
        return Ok(false);
    }

    let mut source = File::open(source)?;
    let mut destination = File::open(destination)?;
    let mut source_buffer = [0_u8; 8192];
    let mut destination_buffer = [0_u8; 8192];
    loop {
        let source_read = source.read(&mut source_buffer)?;
        let destination_read = destination.read(&mut destination_buffer)?;
        if source_read != destination_read
            || source_buffer[..source_read] != destination_buffer[..destination_read]
        {
            return Ok(false);
        }
        if source_read == 0 {
            return Ok(true);
        }
    }
}

/// Returns whether a command entry is managed by Pinset for the given command name.
///
/// Windows routes are stable `.cmd` wrappers that point at the companion router path, so updating
/// `pinset-shim.exe` does not leave hard-linked or copied command binaries on an older version.
pub fn is_managed_command_shim(
    source: &Path,
    destination: &Path,
    _command: &str,
) -> io::Result<bool> {
    #[cfg(windows)]
    {
        if fs::read_to_string(destination)
            .is_ok_and(|content| content == windows_wrapper(source, _command))
        {
            return Ok(true);
        }
    }
    is_managed_shim(source, destination)
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
    fn ensure_is_idempotent_for_managed_entries() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        let shims = root.path().join("shims");
        fs::write(&source, b"fake shim").expect("source");

        ensure_shims(&source, &shims, &["node".to_owned(), "npm".to_owned()])
            .expect("initial shims");
        let results = ensure_shims(&source, &shims, &["node".to_owned(), "npm".to_owned()])
            .expect("ensure shims");

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|result| result.method == ShimInstallMethod::Existing)
        );
    }

    #[test]
    fn managed_entry_inspection_rejects_foreign_content() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        let managed = root.path().join("managed");
        let foreign = root.path().join("foreign");
        fs::write(&source, b"fake shim").expect("source");
        fs::write(&managed, b"fake shim").expect("managed");
        fs::write(&foreign, b"another executable").expect("foreign");

        assert!(is_managed_shim(&source, &managed).expect("managed inspection"));
        assert!(!is_managed_shim(&source, &foreign).expect("foreign inspection"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_wrapper_remains_managed_after_router_binary_update() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join("pinset-shim.exe");
        let shims = root.path().join("shims");
        fs::write(&source, b"router version one").expect("source");
        let initial = ensure_shims(&source, &shims, &["node".to_owned()]).expect("initial");
        assert_eq!(initial[0].method, ShimInstallMethod::Wrapper);

        fs::write(&source, b"router version two").expect("updated source");
        let repeated = ensure_shims(&source, &shims, &["node".to_owned()]).expect("repeat");
        assert_eq!(repeated[0].method, ShimInstallMethod::Existing);
        assert!(is_managed_command_shim(&source, &repeated[0].destination, "node").unwrap());
    }

    #[test]
    fn ensure_rejects_foreign_entries_before_creating_any_commands() {
        let root = tempdir().expect("temp directory");
        let source = root.path().join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        let shims = root.path().join("shims");
        fs::create_dir_all(&shims).expect("shims");
        fs::write(&source, b"fake shim").expect("source");
        let existing = shims.join(if cfg!(windows) { "npm.exe" } else { "npm" });
        fs::write(&existing, b"foreign entry").expect("existing");

        let error = ensure_shims(&source, &shims, &["npm".to_owned(), "node".to_owned()])
            .expect_err("foreign entry must stop registration");

        assert!(matches!(error, Error::ShimDestinationExists { .. }));
        assert_eq!(fs::read(existing).expect("existing"), b"foreign entry");
        assert!(
            !shims
                .join(if cfg!(windows) { "node.exe" } else { "node" })
                .exists()
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

        let shell_error = install_shims(
            &source,
            &root.path().join("unsafe-shims"),
            &["node & echo unsafe".to_owned()],
        )
        .expect_err("shell metacharacters");
        assert!(matches!(shell_error, Error::InvalidShimCommand { .. }));

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
