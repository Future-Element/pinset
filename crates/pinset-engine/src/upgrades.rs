use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE_LIMIT: usize = 20_000;
const FILE_BYTES: u64 = 16 * 1024 * 1024;
const TOTAL_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Default)]
struct Snapshot {
    files: Vec<(PathBuf, Vec<u8>, fs::Permissions)>,
    bytes: u64,
}
fn excluded(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str().to_str().is_some_and(|s| {
            matches!(
                s,
                ".git"
                    | ".venv"
                    | "node_modules"
                    | "target"
                    | "build"
                    | ".dart_tool"
                    | ".gradle"
                    | "__pycache__"
                    | ".pytest_cache"
                    | ".mypy_cache"
            ) || s == ".env"
                || s.starts_with(".env.")
        })
    }) || path.starts_with(".pinset/local")
        || path.starts_with("output")
}
fn collect(root: &Path, relative: &Path, depth: usize, snapshot: &mut Snapshot) -> Result<()> {
    if depth > 64 {
        return Err(service_error(
            "PINSET_SNAPSHOT_LIMIT",
            "snapshot exceeds depth 64",
        ));
    }
    let mut entries = fs::read_dir(root.join(relative))?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let rel = relative.join(entry.file_name());
        if excluded(&rel) {
            continue;
        }
        let m = fs::symlink_metadata(entry.path())?;
        if m.file_type().is_symlink() {
            return Err(service_error(
                "PINSET_SNAPSHOT_LINK",
                format!("snapshot refuses symbolic link {}", rel.display()),
            ));
        }
        if m.is_dir() {
            collect(root, &rel, depth + 1, snapshot)?;
        } else if m.is_file() {
            if m.len() > FILE_BYTES
                || snapshot.files.len() >= FILE_LIMIT
                || snapshot
                    .bytes
                    .checked_add(m.len())
                    .is_none_or(|n| n > TOTAL_BYTES)
            {
                return Err(service_error(
                    "PINSET_SNAPSHOT_LIMIT",
                    "snapshot exceeds its file count or byte limits",
                ));
            }
            let bytes = fs::read(entry.path())?;
            snapshot.bytes += bytes.len() as u64;
            snapshot.files.push((rel, bytes, m.permissions()));
        } else {
            return Err(service_error(
                "PINSET_SNAPSHOT_SPECIAL",
                "snapshot contains a special file",
            ));
        }
    }
    Ok(())
}
fn fingerprint(snapshot: &Snapshot) -> String {
    let mut hash = Sha256::new();
    for (path, bytes, permissions) in &snapshot.files {
        let path = path.to_string_lossy().replace('\\', "/");
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
        hash.update([u8::from(permissions.readonly())]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update(permissions.mode().to_le_bytes());
        }
    }
    hex::encode(hash.finalize())
}
fn snapshot(root: &Path) -> Result<Snapshot> {
    let mut value = Snapshot::default();
    collect(root, Path::new(""), 0, &mut value)?;
    Ok(value)
}
fn write_snapshot(value: &Snapshot, directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)?;
    for (relative, bytes, permissions) in &value.files {
        let dst = directory.join(relative);
        fs::create_dir_all(dst.parent().unwrap())?;
        fs::write(&dst, bytes)?;
        fs::set_permissions(&dst, permissions.clone())?;
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeHistory {
    pub protocol: String,
    pub id: String,
    pub candidate_id: String,
    pub root: PathBuf,
    pub project_id: String,
    pub old_config: ProjectConfig,
    pub old_lock: Lockfile,
    pub new_config: ProjectConfig,
    pub new_lock: Lockfile,
    pub applied_at: u64,
    pub restored: bool,
    pub transaction: String,
}
impl Services {
    fn candidate_file(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self
            .home
            .join("state/candidates")
            .join(id)
            .join("record.json"))
    }
    pub fn candidate(&self, id: &str) -> Result<CandidateRecord> {
        let r: CandidateRecord = read_json(&self.candidate_file(id)?)?;
        let c = self.project()?;
        let config = c.load()?;
        if r.protocol != PROTOCOL
            || r.id != id
            || r.root != c.root
            || r.project_id != config.project_id
        {
            return Err(service_error(
                "PINSET_CANDIDATE_PROJECT",
                "candidate belongs to a different project",
            ));
        }
        Ok(r)
    }
    fn state_fingerprint(&self, c: &ProjectContext) -> Result<String> {
        let mut bytes = fs::read(&c.config_path)?;
        bytes.extend(fs::read(&c.lock_path)?);
        Ok(digest(&bytes))
    }
    fn fresh(&self, r: &CandidateRecord) -> Result<()> {
        let c = self.project()?;
        if self.state_fingerprint(&c)? != r.state_fingerprint
            || fingerprint(&snapshot(&c.root)?) != r.source_fingerprint
        {
            return Err(service_error(
                "PINSET_CANDIDATE_STALE",
                "project source, configuration or lock changed after preparation",
            ));
        }
        Ok(())
    }
    pub fn upgrade_prepare(&self, specs: &[String], plan: bool) -> Result<Value> {
        let c = self.project()?;
        let initial = c.load()?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&initial.project_id)?)
        };
        let (mut config, baseline) = self.load(&c)?;
        let mut candidate = baseline.clone();
        let specs = if specs.is_empty() {
            config.tools.keys().cloned().collect::<Vec<_>>()
        } else {
            specs.to_vec()
        };
        let mut seen = std::collections::BTreeSet::new();
        for spec in specs {
            let (name, selector) = spec
                .split_once('@')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .unwrap_or_else(|| {
                    (
                        spec.clone(),
                        config.tools.get(&spec).cloned().unwrap_or_default(),
                    )
                });
            validate_tool(&name)?;
            if selector.is_empty() || !seen.insert(name.clone()) {
                return Err(service_error(
                    "PINSET_SELECTION_DUPLICATE",
                    "candidate contains an unselected tool or a duplicate",
                ));
            }
            config.tools.insert(name.clone(), selector.clone());
            let tool = self.resolve(&name, &selector, &config)?;
            if let Some(old) = candidate.tool(&name) {
                validate_verification_transition(old, &tool)?;
            }
            candidate.upsert_tool(tool)?;
        }
        config.validate_lock(&candidate)?;
        if baseline.tools == candidate.tools {
            return Ok(json!({"protocol":PROTOCOL,"changed":false,"candidate":null}));
        }
        for input in &config.verification.inputs {
            let p = c.root.join(input);
            if excluded(Path::new(input)) || !p.exists() || !p.canonicalize()?.starts_with(&c.root)
            {
                return Err(service_error(
                    "PINSET_SNAPSHOT_INPUT",
                    "verification input is missing, excluded or outside this project",
                ));
            }
        }
        let snap = snapshot(&c.root)?;
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"baseline":baseline,"candidate":candidate,"snapshot_files":snap.files.len(),"snapshot_bytes":snap.bytes}),
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        let directory = self.home.join("state/candidates").join(&id);
        write_snapshot(&snap, &directory.join("source"))?;
        let record = CandidateRecord {
            protocol: PROTOCOL.into(),
            id: id.clone(),
            project_id: config.project_id.clone(),
            root: c.root.clone(),
            source_fingerprint: fingerprint(&snap),
            state_fingerprint: self.state_fingerprint(&c)?,
            baseline,
            candidate,
            config,
            created_at: now(),
            last_test: None,
        };
        write_json(&directory.join("record.json"), &record)?;
        Ok(json!({"protocol":PROTOCOL,"changed":true,"candidate":record}))
    }
    pub fn upgrade_test(
        &self,
        id: &str,
        compare: bool,
        args: &[String],
        profile: Option<&str>,
        disabled: bool,
    ) -> Result<i32> {
        let initial = self.candidate(id)?;
        let _guard = self.guard(&initial.project_id)?;
        let mut r = self.candidate(id)?;
        r.last_test = Some(CandidateTest {
            command: args.to_vec(),
            baseline_exit: None,
            candidate_exit: 125,
            fingerprint: r.source_fingerprint.clone(),
            timestamp: now(),
            limited: vec![],
            timed_out: false,
        });
        write_json(&self.candidate_file(id)?, &r)?;
        self.fresh(&r)?;
        let original = self.project()?;
        let values = self.resolve_profile(&original, profile, disabled)?;
        let mut limited = r.config.verification.external_state.clone();
        if !values.is_empty() {
            limited.push("encrypted environment was supplied; secret-dependent external effects are outside the snapshot".into());
        }
        let source = self.home.join("state/candidates").join(id).join("source");
        let snap = snapshot(&source)?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let runroot = self
            .home
            .join("state/candidates")
            .join(id)
            .join("runs")
            .join(&run_id);
        let mut baseline_exit = None;
        for (name, lock) in [("baseline", &r.baseline), ("candidate", &r.candidate)] {
            if name == "baseline" && !compare {
                continue;
            }
            let directory = runroot.join(name);
            write_snapshot(&snap, &directory)?;
            let c = ProjectContext::at(&directory, false)?;
            let config = if name == "baseline" {
                let mut old = r.config.clone();
                old.tools = r
                    .baseline
                    .tools
                    .iter()
                    .map(|t| (t.name.clone(), t.requested.clone()))
                    .collect();
                old
            } else {
                r.config.clone()
            };
            write_atomic(&c.config_path, toml::to_string(&config)?.as_bytes())?;
            write_atomic(&c.lock_path, toml::to_string(lock)?.as_bytes())?;
            self.install_sdk(lock, None, false, false)?;
            self.bind_local(&c, &config, lock, false, false)?;
            let mut scoped = values.clone();
            if disabled || std::env::var_os("PINSET_NO_ENV").is_some() {
                scoped.insert("PINSET_NO_ENV".into(), "1".into());
            }
            if let Some(marker) = scoped.get("PINSET_ENV_RESOLVED").cloned() {
                let name = marker.rsplit_once(':').map(|(_, name)| name).unwrap_or("");
                let fingerprint = self.environment_fingerprint(&c)?;
                pinset_env::trust_project(&self.home, &c.root, &config.project_id, &fingerprint)?;
                scoped.insert(
                    "PINSET_ENV_RESOLVED".into(),
                    format!("{fingerprint}:{name}"),
                );
            }
            let result = self.verification_command(&c, args, scoped, r.config.verification.timeout);
            let code = match result {
                Ok(c) => c,
                Err(error) => {
                    r.last_test = Some(CandidateTest {
                        command: args.to_vec(),
                        baseline_exit,
                        candidate_exit: 125,
                        fingerprint: r.source_fingerprint.clone(),
                        timestamp: now(),
                        limited: limited.clone(),
                        timed_out: false,
                    });
                    write_json(&self.candidate_file(id)?, &r)?;
                    return Err(error);
                }
            };
            if name == "baseline" {
                baseline_exit = Some(code);
            } else {
                r.last_test = Some(CandidateTest {
                    command: args.to_vec(),
                    baseline_exit,
                    candidate_exit: code,
                    fingerprint: r.source_fingerprint.clone(),
                    timestamp: now(),
                    limited: limited.clone(),
                    timed_out: code == 124,
                });
            }
        }
        if let Err(error) = self.fresh(&r) {
            r.last_test.as_mut().unwrap().candidate_exit = 125;
            write_json(&self.candidate_file(id)?, &r)?;
            return Err(error);
        }
        write_json(&self.candidate_file(id)?, &r)?;
        let test = r.last_test.as_ref().unwrap();
        Ok(if test.baseline_exit.is_some_and(|c| c != 0) {
            test.baseline_exit.unwrap()
        } else {
            test.candidate_exit
        })
    }
    pub fn upgrade_status(&self, id: Option<&str>, history: bool) -> Result<Value> {
        if let Some(id) = id {
            return Ok(serde_json::to_value(self.candidate(id)?)?);
        }
        let c = self.project()?;
        let root = self.home.join(if history {
            "state/history"
        } else {
            "state/candidates"
        });
        let mut records = vec![];
        if root.exists() {
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let p = if history {
                    entry.path()
                } else {
                    entry.path().join("record.json")
                };
                if let Ok(value) = read_json::<Value>(&p)
                    && value["root"] == c.root.display().to_string()
                {
                    records.push(value);
                }
            }
        }
        Ok(json!({"protocol":PROTOCOL,"history":history,"records":records}))
    }
    pub fn upgrade_apply(&self, id: &str, allow_limited: bool, plan: bool) -> Result<Value> {
        let context = self.project()?;
        let initial = context.load()?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&initial.project_id)?)
        };
        let r = self.candidate(id)?;
        self.fresh(&r)?;
        let test = r.last_test.as_ref().ok_or_else(|| {
            service_error(
                "PINSET_CANDIDATE_UNTESTED",
                "candidate has no validation result",
            )
        })?;
        if test.candidate_exit != 0 || test.baseline_exit.is_some_and(|c| c != 0) || test.timed_out
        {
            return Err(service_error(
                "PINSET_CANDIDATE_FAILED",
                "the most recent validation failed",
            ));
        }
        if test.fingerprint != r.source_fingerprint || now().saturating_sub(test.timestamp) > 86_400
        {
            return Err(service_error(
                "PINSET_CANDIDATE_STALE",
                "validation result is stale",
            ));
        }
        if !test.limited.is_empty() && !allow_limited {
            return Err(service_error(
                "PINSET_CANDIDATE_LIMITED",
                "limited evidence requires --allow-limited",
            ));
        }
        let c = self.project()?;
        let (old_config, old_lock) = self.load(&c)?;
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"config":r.config,"lock":r.candidate,"limited":test.limited}),
            );
        }
        self.fresh(&r)?;
        self.install_sdk(&r.candidate, None, false, false)?;
        let history_id = uuid::Uuid::new_v4().to_string();
        let transaction =
            self.begin_commit(&c, &r.config, &r.candidate, Some(history_id.clone()))?;
        let notes = self.bind_local(&c, &r.config, &r.candidate, false, true)?;
        let history = UpgradeHistory {
            protocol: PROTOCOL.into(),
            id: history_id.clone(),
            candidate_id: id.into(),
            root: c.root.clone(),
            project_id: r.project_id,
            old_config,
            old_lock,
            new_config: r.config,
            new_lock: r.candidate,
            applied_at: now(),
            restored: false,
            transaction: transaction.clone(),
        };
        write_json(
            &self
                .home
                .join("state/history")
                .join(format!("{history_id}.json")),
            &history,
        )?;
        self.finish_commit(&c, &transaction)?;
        Ok(json!({"protocol":PROTOCOL,"applied":id,"history_id":history_id,"notes":notes}))
    }
    pub fn upgrade_restore(&self, id: &str, plan: bool) -> Result<Value> {
        validate_id(id)?;
        let path = self.home.join("state/history").join(format!("{id}.json"));
        let c = self.project()?;
        let initial = c.load()?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&initial.project_id)?)
        };
        let mut h: UpgradeHistory = read_json(&path)?;
        let (config, lock) = self.load(&c)?;
        if h.protocol != PROTOCOL
            || h.root != c.root
            || h.project_id != config.project_id
            || h.restored
        {
            return Err(service_error(
                "PINSET_HISTORY_INVALID",
                "history cannot be restored in this project",
            ));
        }
        if lock != h.new_lock || config != h.new_config {
            return Err(service_error(
                "PINSET_HISTORY_STALE",
                "project selections changed after this upgrade",
            ));
        }
        if !plan {
            self.install_sdk(&h.old_lock, None, true, false)?;
            let transaction = self.begin_commit(&c, &h.old_config, &h.old_lock, Some(id.into()))?;
            self.bind_local(&c, &h.old_config, &h.old_lock, false, true)?;
            h.restored = true;
            write_json(&path, &h)?;
            self.finish_commit(&c, &transaction)?;
        }
        Ok(json!({"protocol":PROTOCOL,"plan":plan,"restored":!plan,"history":id}))
    }
    pub fn upgrade_recover(&self, plan: bool) -> Result<Value> {
        let c = self.project()?;
        let marker = c.local.join("transaction.json");
        let state: Value = if marker.exists() {
            read_json(&marker)?
        } else {
            return Ok(json!({"protocol":PROTOCOL,"plan":plan,"transactions":[]}));
        };
        let id = state["id"].as_str().ok_or_else(|| {
            service_error("PINSET_RECOVERY_INVALID", "missing transaction identity")
        })?;
        validate_id(id)?;
        let path = self
            .home
            .join("state/transactions")
            .join(format!("{id}.json"));
        let mut journal: TransactionJournal = read_json(&path)?;
        if journal.protocol != PROTOCOL
            || journal.root != c.root
            || !matches!(journal.phase.as_str(), "prepared" | "committed")
        {
            return Err(service_error(
                "PINSET_RECOVERY_INVALID",
                "journal does not match project",
            ));
        }
        if !plan {
            let _guard = self.guard(&journal.project_id)?;
            if journal.phase == "prepared" {
                if journal.old_config.is_empty() {
                    return Err(service_error(
                        "PINSET_RECOVERY_INVALID",
                        "journal has no prior project state",
                    ));
                }
                write_atomic(&c.config_path, journal.old_config.as_bytes())?;
                for (name, before) in &journal.profile_before {
                    validate_id(name)?;
                    let path = c.root.join(format!(".pinset/env/{name}.env"));
                    if let Some(bytes) = before {
                        write_atomic(&path, bytes.as_bytes())?;
                    } else if path.exists() {
                        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                            return Err(service_error(
                                "PINSET_RECOVERY_INVALID",
                                "profile is a symlink",
                            ));
                        }
                        fs::remove_file(path)?;
                    }
                }
                if let Some(lock) = &journal.old_lock {
                    write_atomic(&c.lock_path, lock.as_bytes())?;
                } else if c.lock_path.exists() {
                    fs::remove_file(&c.lock_path)?;
                }
                if let Some(history) = &journal.history_id {
                    let h = self
                        .home
                        .join("state/history")
                        .join(format!("{history}.json"));
                    if let Some(before) = &journal.history_before {
                        write_atomic(&h, before.as_bytes())?;
                    } else if h.exists() {
                        fs::remove_file(h)?;
                    }
                }
                let config: ProjectConfig = toml::from_str(&journal.old_config)?;
                let lock = journal
                    .old_lock
                    .as_ref()
                    .map(|text| toml::from_str(text))
                    .transpose()?
                    .unwrap_or_else(|| Lockfile::empty(config.project_id.clone()));
                self.bind_local(&c, &config, &lock, false, true)?;
                journal.phase = "recovered".into();
                write_json(&path, &journal)?;
            }
            fs::remove_file(marker)?;
        }
        Ok(json!({"protocol":PROTOCOL,"plan":plan,"transactions":[id],"phase":journal.phase}))
    }
}
