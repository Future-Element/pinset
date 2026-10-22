//! Recoverable project venv replacement. Python always builds at the final path.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const PENDING: &str = "venv-transaction.json";
const CREATING: &str = ".pinset-creating.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VenvJournal {
    protocol: String,
    id: String,
    root: PathBuf,
    project_id: String,
    directory: WorkDirectoryIdentity,
    phase: String,
    worker_pid: Option<u32>,
    worker_birth: Option<String>,
    before: Option<VenvOwner>,
    next: VenvOwner,
}

fn spawn_venv_worker(interpreter: &Path, destination: &Path) -> Result<crate::process_tree::Tree> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__process-worker")
        .arg(interpreter)
        .args(["-I", "-m", "venv"])
        .arg(destination)
        .stdin(Stdio::piped())
        .env_remove("PINSET_IDENTITY")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(crate::process_tree::Tree::new(command.spawn()?)?)
}

fn process_birth(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
            let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
            if fields.first() == Some(&"Z") {
                return None;
            }
            return fields.get(19).map(|value| (*value).to_owned());
        }
        None
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let result = Command::new("/bin/ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env_remove("PINSET_IDENTITY")
            .output()
            .ok()?;
        let birth = String::from_utf8(result.stdout).ok()?.trim().to_owned();
        (result.status.success() && !birth.is_empty()).then_some(birth)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        // SAFETY: query-only handle; no signal or mutation of external processes.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut exit = 0;
            let mut created = std::mem::zeroed();
            let mut exited = std::mem::zeroed();
            let mut kernel = std::mem::zeroed();
            let mut user = std::mem::zeroed();
            let valid = GetExitCodeProcess(handle, &mut exit) != 0
                && exit == 259
                && GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
            CloseHandle(handle);
            valid.then(|| format!("{}:{}", created.dwHighDateTime, created.dwLowDateTime))
        }
    }
}

fn unsafe_venv(message: &str) -> Error {
    service_error("PINSET_VENV_RECOVERY", message)
}

fn regular_file(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(unsafe_venv("venv state must be a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn safe_local(c: &ProjectContext, path: &Path) -> Result<()> {
    if !path.starts_with(&c.local) {
        return Err(unsafe_venv("venv transaction escaped local state"));
    }
    for parent in path.ancestors() {
        if fs::symlink_metadata(parent).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(unsafe_venv("venv transaction path is linked"));
        }
        if parent == c.local {
            break;
        }
    }
    Ok(())
}

fn owner(path: &Path) -> Result<VenvOwner> {
    if fs::symlink_metadata(path)?.file_type().is_symlink()
        || !regular_file(&path.join(VENV_MARKER))?
    {
        return Err(service_error(
            "PINSET_VENV_EXTERNAL",
            "external venv cannot be replaced",
        ));
    }
    Ok(toml::from_str(&fs::read_to_string(
        path.join(VENV_MARKER),
    )?)?)
}

fn same_owner(a: &VenvOwner, b: &VenvOwner) -> Result<bool> {
    Ok(serde_json::to_vec(a)? == serde_json::to_vec(b)?)
}

fn owns_creation(path: &Path, journal: &VenvJournal) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(unsafe_venv(
            "venv transaction directory is linked or external",
        ));
    }
    let marker = path.join(CREATING);
    if !regular_file(&marker)? {
        return Ok(false);
    }
    let token: serde_json::Value = read_json(&marker)?;
    Ok(token["protocol"] == PROTOCOL && token["id"] == journal.id)
}

impl VenvJournal {
    fn backup(&self, c: &ProjectContext) -> PathBuf {
        c.local.join("venv-backups").join(&self.id)
    }
    fn staging(&self, c: &ProjectContext) -> PathBuf {
        c.local.join("venv-staging").join(&self.id)
    }
    fn validate(&self, c: &ProjectContext, config: &ProjectConfig) -> Result<()> {
        validate_id(&self.id)?;
        if self.protocol != PROTOCOL
            || self.root != c.root
            || self.project_id != config.project_id
            || self.directory != c.directory
            || !matches!(
                self.phase.as_str(),
                "prepared" | "creating" | "committed" | "rolled-back"
            )
            || self.next.protocol != PROTOCOL
            || self.next.project_id != self.project_id
            || self.next.directory != self.directory
            || self.next.platform != current_target_for_tool("python")
            || self.worker_pid.is_some() != self.worker_birth.is_some()
        {
            return Err(unsafe_venv(
                "venv journal does not match this project and directory",
            ));
        }
        if let Some(before) = &self.before
            && (before.protocol != PROTOCOL
                || before.project_id != self.project_id
                || before.directory != self.directory)
        {
            return Err(unsafe_venv("venv backup belongs to another project"));
        }
        safe_local(c, &self.backup(c))?;
        safe_local(c, &self.staging(c))?;
        let marker = c.local.join(PENDING);
        if regular_file(&marker)? {
            let token: serde_json::Value = read_json(&marker)?;
            if token["protocol"] != PROTOCOL || token["id"] != self.id {
                return Err(unsafe_venv("venv pending marker changed"));
            }
        }
        Ok(())
    }
}

fn rollback(c: &ProjectContext, journal: &VenvJournal) -> Result<()> {
    let current = c.root.join(".venv");
    let backup = journal.backup(c);
    let staging = journal.staging(c);
    // Validate all deletion/restore inputs before mutating any directory.
    let backup_exists = fs::symlink_metadata(&backup).is_ok();
    if backup_exists {
        let before = journal
            .before
            .as_ref()
            .ok_or_else(|| unsafe_venv("unexpected venv backup"))?;
        if !same_owner(&owner(&backup)?, before)? {
            return Err(unsafe_venv("venv backup identity changed"));
        }
    }
    let current_exists = fs::symlink_metadata(&current).is_ok();
    let remove_current = current_exists && owns_creation(&current, journal)?;
    if current_exists && !remove_current {
        let Some(before) = &journal.before else {
            return Err(unsafe_venv("refusing to remove an external venv"));
        };
        if backup_exists || !same_owner(&owner(&current)?, before)? {
            return Err(unsafe_venv(
                "venv directory was replaced outside this transaction",
            ));
        }
    }
    if staging.exists() && !owns_creation(&staging, journal)? {
        return Err(unsafe_venv("refusing to remove external venv staging"));
    }
    if remove_current {
        fs::remove_dir_all(&current)?;
    }
    if backup_exists {
        fs::rename(&backup, &current)?;
    }
    if staging.exists() {
        fs::remove_dir_all(staging)?;
    }
    Ok(())
}

impl Services {
    fn venv_journals(&self) -> Result<Vec<(PathBuf, VenvJournal)>> {
        let directory = self.home.join("state/venv-transactions");
        if !directory.exists() {
            return Ok(vec![]);
        }
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            return Err(unsafe_venv("venv journal directory is linked"));
        }
        let mut journals = Vec::new();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if !regular_file(&path)? {
                continue;
            }
            let journal: VenvJournal = read_json(&path)?;
            validate_id(&journal.id)?;
            if journal.protocol != PROTOCOL
                || path.file_stem().and_then(|s| s.to_str()) != Some(&journal.id)
            {
                return Err(unsafe_venv("invalid venv transaction journal"));
            }
            journals.push((path, journal));
        }
        Ok(journals)
    }

    fn finish_venv_journal(&self, c: &ProjectContext, path: &Path, id: &str) -> Result<()> {
        let marker = c.local.join(PENDING);
        if regular_file(&marker)? {
            let token: serde_json::Value = read_json(&marker)?;
            if token["protocol"] != PROTOCOL || token["id"] != id {
                return Err(unsafe_venv("venv pending marker changed"));
            }
            fs::remove_file(marker)?;
        }
        fs::remove_file(path)?;
        Ok(())
    }

    pub(crate) fn protected_venv_interpreters(&self) -> Result<BTreeSet<String>> {
        let mut protected = BTreeSet::new();
        for (_, journal) in self.venv_journals()? {
            protected.insert(journal.next.interpreter_identity);
            if let Some(before) = journal.before {
                protected.insert(before.interpreter_identity);
            }
        }
        Ok(protected)
    }

    pub(crate) fn recover_venv_transactions(&self, plan: bool) -> Result<Vec<String>> {
        let Some(c) = find_project_context(&self.cwd)? else {
            return Ok(vec![]);
        };
        let config: ProjectConfig = toml::from_str(&fs::read_to_string(&c.config_path)?)?;
        config.validate()?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&config.project_id)?)
        };
        let current: ProjectConfig = toml::from_str(&fs::read_to_string(&c.config_path)?)?;
        current.validate()?;
        if current.project_id != config.project_id {
            return Err(unsafe_venv(
                "project identity changed while waiting for venv recovery",
            ));
        }
        let journals = self.venv_journals()?;
        let relevant: Vec<_> = journals
            .into_iter()
            .filter(|(_, j)| j.root == c.root)
            .collect();
        if relevant.len() > 1 {
            return Err(unsafe_venv("multiple pending venv transactions"));
        }
        let mut recovered = Vec::new();
        for (path, _) in relevant {
            if !path.exists() {
                continue;
            }
            let mut journal: VenvJournal = read_json(&path)?;
            journal.validate(&c, &config)?;
            if !plan
                && journal
                    .worker_pid
                    .zip(journal.worker_birth.as_deref())
                    .is_some_and(|(pid, birth)| process_birth(pid).as_deref() == Some(birth))
            {
                return Err(service_error(
                    "PINSET_VENV_BUSY",
                    "the recorded venv worker is still running; retry self repair after it exits",
                ));
            }
            if !plan {
                if journal.phase == "committed" {
                    let current = c.root.join(".venv");
                    if !same_owner(&owner(&current)?, &journal.next)? {
                        return Err(unsafe_venv("committed venv identity changed"));
                    }
                    let creating = current.join(CREATING);
                    if regular_file(&creating)? {
                        if !owns_creation(&current, &journal)? {
                            return Err(unsafe_venv("venv creation token changed"));
                        }
                        fs::remove_file(creating)?;
                    }
                } else {
                    rollback(&c, &journal)?;
                    journal.phase = "rolled-back".into();
                    write_json(&path, &journal)?;
                }
                self.finish_venv_journal(&c, &path, &journal.id)?;
            }
            recovered.push(journal.id);
        }
        Ok(recovered)
    }

    pub fn create_venv(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        tool: &LockedTool,
        recreate: bool,
    ) -> Result<()> {
        if regular_file(&c.local.join(PENDING))?
            || self.venv_journals()?.iter().any(|(_, j)| j.root == c.root)
        {
            return Err(service_error(
                "PINSET_VENV_TRANSACTION_PENDING",
                "recover the existing venv transaction with pinset self repair",
            ));
        }
        let target = current_target_for_tool("python");
        let current = c.root.join(".venv");
        let before = if fs::symlink_metadata(&current).is_ok() {
            let before = owner(&current)?;
            if !recreate
                || before.protocol != PROTOCOL
                || before.project_id != config.project_id
                || before.directory != c.directory
            {
                return Err(service_error(
                    "PINSET_VENV_EXTERNAL",
                    "only an owned Pinset 3 venv in this directory can be recreated",
                ));
            }
            Some(before)
        } else {
            None
        };
        let mut journal = VenvJournal {
            protocol: PROTOCOL.into(),
            id: uuid::Uuid::new_v4().to_string(),
            root: c.root.clone(),
            project_id: config.project_id.clone(),
            directory: c.directory.clone(),
            phase: "prepared".into(),
            worker_pid: None,
            worker_birth: None,
            before,
            next: VenvOwner {
                protocol: PROTOCOL.into(),
                project_id: config.project_id.clone(),
                directory: c.directory.clone(),
                interpreter_identity: tool.installation_version(&target),
                platform: target.clone(),
            },
        };
        journal.validate(c, config)?;
        let path = self
            .home
            .join("state/venv-transactions")
            .join(format!("{}.json", journal.id));
        let token = serde_json::json!({"protocol":PROTOCOL,"id":journal.id});
        // Prepare ownership before publishing a recovery journal. A kill while
        // preparing this empty directory leaves the original venv untouched.
        let staging = journal.staging(c);
        fs::create_dir_all(staging.parent().unwrap())?;
        fs::create_dir(&staging)?;
        write_json(&staging.join(CREATING), &token)?;
        write_json(&path, &journal)?;
        write_json(&c.local.join(PENDING), &token)?;
        let result = (|| -> Result<()> {
            if journal.before.is_some() {
                let backup = journal.backup(c);
                fs::create_dir_all(backup.parent().unwrap())?;
                fs::rename(&current, backup)?;
            }
            fs::rename(staging, &current)?;
            journal.phase = "creating".into();
            write_json(&path, &journal)?;
            let interpreter =
                interpreter_path(&install_directory(&self.home, tool, &target), &target);
            if !interpreter.is_file() {
                return Err(service_error(
                    "PINSET_VENV_CREATE",
                    "the locked interpreter is missing",
                ));
            }
            let mut worker = spawn_venv_worker(&interpreter, &current).map_err(|_| {
                service_error(
                    "PINSET_VENV_CREATE",
                    "could not start the locked interpreter for venv creation",
                )
            })?;
            journal.worker_pid = Some(worker.child.id());
            journal.worker_birth = Some(
                process_birth(worker.child.id())
                    .ok_or_else(|| unsafe_venv("could not record venv worker birth identity"))?,
            );
            write_json(&path, &journal)?;
            worker
                .child
                .stdin
                .take()
                .ok_or_else(|| unsafe_venv("missing venv worker handshake"))?
                .write_all(b"G")?;
            let status = worker.child.wait()?;
            drop(worker);
            if !status.success() {
                return Err(service_error(
                    "PINSET_VENV_CREATE",
                    "standard library venv creation failed",
                ));
            }
            write_atomic(
                &current.join(VENV_MARKER),
                toml::to_string(&journal.next)?.as_bytes(),
            )?;
            validate_venv_contents(c, config, tool, &target, &self.home)?;
            journal.phase = "committed".into();
            write_json(&path, &journal)?;
            fs::remove_file(current.join(CREATING))?;
            self.finish_venv_journal(c, &path, &journal.id)
        })();
        if let Err(error) = result {
            if journal.phase != "committed" {
                rollback(c, &journal)?;
                journal.phase = "rolled-back".into();
                write_json(&path, &journal)?;
                self.finish_venv_journal(c, &path, &journal.id)?;
            }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        tempfile::TempDir,
        Services,
        ProjectContext,
        ProjectConfig,
        VenvJournal,
    ) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("project");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let service = Services {
            cwd: root.clone(),
            home: temporary.path().join("home/v3"),
            progress: ProgressReporter::default(),
        };
        service.init().unwrap();
        let c = ProjectContext::at(&root, false).unwrap();
        let config = c.load().unwrap();
        let old = VenvOwner {
            protocol: PROTOCOL.into(),
            project_id: config.project_id.clone(),
            directory: c.directory.clone(),
            interpreter_identity: "3.13.0--old".into(),
            platform: current_target_for_tool("python"),
        };
        let current = root.join(".venv");
        fs::create_dir(&current).unwrap();
        write_atomic(
            &current.join(VENV_MARKER),
            toml::to_string(&old).unwrap().as_bytes(),
        )
        .unwrap();
        fs::write(current.join("preserved-package"), "original package").unwrap();
        let journal = VenvJournal {
            protocol: PROTOCOL.into(),
            id: uuid::Uuid::new_v4().to_string(),
            root,
            project_id: config.project_id.clone(),
            directory: c.directory.clone(),
            phase: "prepared".into(),
            worker_pid: None,
            worker_birth: None,
            before: Some(old.clone()),
            next: VenvOwner {
                interpreter_identity: "3.14.0--new".into(),
                ..old
            },
        };
        (temporary, service, c, config, journal)
    }

    fn publish_journal(service: &Services, c: &ProjectContext, journal: &VenvJournal) -> PathBuf {
        let path = service
            .home
            .join("state/venv-transactions")
            .join(format!("{}.json", journal.id));
        write_json(&path, journal).unwrap();
        write_json(
            &c.local.join(PENDING),
            &serde_json::json!({"protocol":PROTOCOL,"id":journal.id}),
        )
        .unwrap();
        path
    }

    fn interrupt_creation(c: &ProjectContext, journal: &VenvJournal, marker_written: bool) {
        let current = c.root.join(".venv");
        let backup = journal.backup(c);
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::rename(&current, backup).unwrap();
        fs::create_dir(&current).unwrap();
        write_json(
            &current.join(CREATING),
            &serde_json::json!({"protocol":PROTOCOL,"id":journal.id}),
        )
        .unwrap();
        fs::write(current.join("partial-build"), "unfinished").unwrap();
        if marker_written {
            write_atomic(
                &current.join(VENV_MARKER),
                toml::to_string(&journal.next).unwrap().as_bytes(),
            )
            .unwrap();
        }
    }

    #[test]
    fn interrupted_creation_and_owner_write_failure_restore_original_packages() {
        for marker_written in [false, true] {
            let (_temporary, service, c, _config, mut journal) = fixture();
            journal.phase = "creating".into();
            let path = publish_journal(&service, &c, &journal);
            interrupt_creation(&c, &journal, marker_written);
            let before = fs::read(&path).unwrap();
            assert_eq!(
                service.recover_venv_transactions(true).unwrap(),
                vec![journal.id.clone()]
            );
            assert_eq!(fs::read(&path).unwrap(), before);
            let protected = service.protected_venv_interpreters().unwrap();
            assert!(protected.contains("3.13.0--old") && protected.contains("3.14.0--new"));
            assert_eq!(
                service.recover_venv_transactions(false).unwrap(),
                vec![journal.id]
            );
            assert_eq!(
                fs::read_to_string(c.root.join(".venv/preserved-package")).unwrap(),
                "original package"
            );
            assert!(!c.root.join(".venv/partial-build").exists());
            assert!(!path.exists() && !c.local.join(PENDING).exists());
            assert!(service.recover_venv_transactions(false).unwrap().is_empty());
        }
    }

    #[test]
    fn journal_without_pending_marker_recovers_a_kill_between_atomic_writes() {
        let (_temporary, service, c, _config, journal) = fixture();
        let path = publish_journal(&service, &c, &journal);
        fs::remove_file(c.local.join(PENDING)).unwrap();
        assert_eq!(
            service.recover_venv_transactions(false).unwrap(),
            vec![journal.id]
        );
        assert!(!path.exists());
        assert!(c.root.join(".venv/preserved-package").exists());
    }

    #[test]
    fn external_replacement_is_rejected_without_deleting_original_backup() {
        let (_temporary, service, c, _config, mut journal) = fixture();
        journal.phase = "creating".into();
        publish_journal(&service, &c, &journal);
        interrupt_creation(&c, &journal, false);
        fs::remove_file(c.root.join(".venv").join(CREATING)).unwrap();
        assert!(service.recover_venv_transactions(false).is_err());
        assert!(journal.backup(&c).join("preserved-package").exists());
        assert!(c.root.join(".venv/partial-build").exists());
    }

    #[test]
    fn failed_interpreter_start_rolls_back_environment_and_clears_journal() {
        let (_temporary, service, c, config, _journal) = fixture();
        let tool = LockedTool {
            name: "python".into(),
            requested: "3.14.0".into(),
            version: "3.14.0".into(),
            provider: "python-build-standalone".into(),
            released_at: None,
            metadata: Default::default(),
            options: Default::default(),
            artifacts: vec![],
        };
        assert_eq!(
            service
                .create_venv(&c, &config, &tool, true)
                .unwrap_err()
                .code(),
            "PINSET_VENV_CREATE"
        );
        assert!(c.root.join(".venv/preserved-package").exists());
        assert!(!c.local.join(PENDING).exists());
        assert!(service.venv_journals().unwrap().is_empty());
    }

    #[test]
    fn committed_venv_recovery_preserves_new_environment_and_retained_backup() {
        let (_temporary, service, c, _config, mut journal) = fixture();
        journal.phase = "committed".into();
        publish_journal(&service, &c, &journal);
        interrupt_creation(&c, &journal, true);
        service.recover_venv_transactions(false).unwrap();
        assert_eq!(
            owner(&c.root.join(".venv")).unwrap().interpreter_identity,
            "3.14.0--new"
        );
        assert!(journal.backup(&c).join("preserved-package").exists());
        assert!(!c.root.join(".venv").join(CREATING).exists());
    }

    #[test]
    fn live_writer_blocks_recovery_but_reused_pid_does_not() {
        let (_temporary, service, c, _config, mut journal) = fixture();
        journal.phase = "creating".into();
        journal.worker_pid = Some(std::process::id());
        journal.worker_birth = Some(process_birth(std::process::id()).unwrap());
        let path = publish_journal(&service, &c, &journal);
        interrupt_creation(&c, &journal, false);
        assert_eq!(
            service.recover_venv_transactions(false).unwrap_err().code(),
            "PINSET_VENV_BUSY"
        );
        assert!(journal.backup(&c).join("preserved-package").exists());
        journal.worker_birth = Some("a different process birth".into());
        write_json(&path, &journal).unwrap();
        service.recover_venv_transactions(false).unwrap();
        assert!(c.root.join(".venv/preserved-package").exists());
    }
}
