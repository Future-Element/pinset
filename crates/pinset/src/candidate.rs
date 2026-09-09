use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use pinset_core::{
    LOCKFILE_SCHEMA, Lockfile, ProjectConfig, acquire_project_state_write_lock,
    effective_project_config, load_lockfile, load_project_config, lockfile_path, save_lockfile,
    validate_lock_matches_tool_options, validate_lock_matches_tools, validate_project_lock_policy,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CANDIDATE_SCHEMA: u32 = 1;
const ACTIVE_FILE: &str = "active.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateBaseline {
    pub config_sha256: String,
    pub effective_config_sha256: String,
    pub lock_sha256: String,
    pub git_head: Option<String>,
    pub git_dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateTestRecord {
    pub task: String,
    pub arguments: Vec<String>,
    pub candidate_sha256: String,
    pub git_head: Option<String>,
    pub git_dirty: Option<bool>,
    pub finished_unix_ms: u64,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateRecord {
    pub schema: u32,
    pub id: String,
    pub project_id: String,
    pub config_path: PathBuf,
    pub created_unix_ms: u64,
    pub baseline: CandidateBaseline,
    pub lock: Lockfile,
    #[serde(default)]
    pub tests: Vec<CandidateTestRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateHistoryRecord {
    pub schema: u32,
    pub id: String,
    pub candidate_id: String,
    pub project_id: String,
    pub config_path: PathBuf,
    pub applied_unix_ms: u64,
    pub config_sha256: String,
    pub effective_config_sha256: String,
    pub previous_lock: Lockfile,
    pub applied_lock: Lockfile,
    pub tested_git_head: Option<String>,
    pub tested_git_dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateTransaction {
    schema: u32,
    id: String,
    created_unix_ms: u64,
    entries: Vec<CandidateTransactionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateTransactionEntry {
    config_path: PathBuf,
    project_id: String,
    candidate_id: String,
    config_sha256: String,
    effective_config_sha256: String,
    previous_lock: Lockfile,
    applied_lock: Lockfile,
    tested_git_head: Option<String>,
    tested_git_dirty: Option<bool>,
}

pub fn capture_baseline(
    config_path: &Path,
) -> Result<CandidateBaseline, Box<dyn std::error::Error>> {
    let config = load_project_config(config_path)?;
    let effective = effective_project_config(config_path, &config)?;
    let lock_path = lockfile_path(config_path);
    Ok(CandidateBaseline {
        config_sha256: hash_file(config_path)?,
        effective_config_sha256: hash_serialized(&effective)?,
        lock_sha256: hash_file(&lock_path)?,
        git_head: git_head(config_path.parent().unwrap_or_else(|| Path::new(".")))?,
        git_dirty: git_dirty(config_path.parent().unwrap_or_else(|| Path::new(".")))?,
    })
}

pub fn new_record(
    config_path: PathBuf,
    project_id: String,
    baseline: CandidateBaseline,
    lock: Lockfile,
) -> Result<CandidateRecord, Box<dyn std::error::Error>> {
    Ok(CandidateRecord {
        schema: CANDIDATE_SCHEMA,
        id: uuid::Uuid::new_v4().to_string(),
        project_id,
        config_path,
        created_unix_ms: unix_time_ms()?,
        baseline,
        lock,
        tests: Vec::new(),
    })
}

pub fn candidate_digest(lock: &Lockfile) -> Result<String, serde_json::Error> {
    let mut normalized = lock.clone();
    normalized.schema = LOCKFILE_SCHEMA;
    hash_serialized(&normalized)
}

pub fn candidate_directory(
    home: &Path,
    config_path: &Path,
    project_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let canonical = fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(project_id.as_bytes());
    Ok(home
        .join("state")
        .join("candidates")
        .join(hex::encode(hasher.finalize())))
}

pub fn save_active(
    home: &Path,
    record: &CandidateRecord,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    validate_record(record)?;
    let directory = candidate_directory(home, &record.config_path, &record.project_id)?;
    fs::create_dir_all(&directory)?;
    let path = directory.join(ACTIVE_FILE);
    write_json(&path, record)?;
    Ok(path)
}

pub fn load_active(
    home: &Path,
    config_path: &Path,
    project_id: &str,
) -> Result<CandidateRecord, Box<dyn std::error::Error>> {
    let path = candidate_directory(home, config_path, project_id)?.join(ACTIVE_FILE);
    let record: CandidateRecord = read_json(&path)?;
    validate_record(&record)?;
    if record.project_id != project_id || !same_path(&record.config_path, config_path) {
        return Err(format!(
            "candidate record {} belongs to a different project",
            path.display()
        )
        .into());
    }
    Ok(record)
}

pub fn record_test(
    home: &Path,
    record: &mut CandidateRecord,
    task: &str,
    arguments: &[String],
    exit_code: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    record.tests.push(CandidateTestRecord {
        task: task.to_owned(),
        arguments: arguments.to_vec(),
        candidate_sha256: candidate_digest(&record.lock)?,
        git_head: git_head(
            record
                .config_path
                .parent()
                .unwrap_or_else(|| Path::new(".")),
        )?,
        git_dirty: git_dirty(
            record
                .config_path
                .parent()
                .unwrap_or_else(|| Path::new(".")),
        )?,
        finished_unix_ms: unix_time_ms()?,
        exit_code,
    });
    save_active(home, record)?;
    Ok(())
}

pub fn verify_candidate_for_test(
    record: &CandidateRecord,
) -> Result<ProjectConfig, Box<dyn std::error::Error>> {
    let config = verify_baseline(record, true)?;
    validate_candidate_lock(record, &config)?;
    Ok(config)
}

pub fn verify_candidate_for_apply(
    record: &CandidateRecord,
) -> Result<ProjectConfig, Box<dyn std::error::Error>> {
    let config = verify_baseline(record, true)?;
    validate_candidate_lock(record, &config)?;
    let digest = candidate_digest(&record.lock)?;
    let latest = record.tests.last().ok_or_else(|| {
        format!(
            "candidate {} has no test record; run `pinset candidate test <task>`",
            record.id
        )
    })?;
    if latest.exit_code != 0 || latest.candidate_sha256 != digest {
        return Err(format!(
            "candidate {} has no passing test for its current exact lock",
            record.id
        )
        .into());
    }
    let current_head = git_head(
        record
            .config_path
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    )?;
    let current_dirty = git_dirty(
        record
            .config_path
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    )?;
    if latest.git_head != current_head || latest.git_dirty != current_dirty {
        return Err(format!(
            "Git HEAD or worktree cleanliness changed after candidate {} was tested; test it again before applying",
            record.id
        )
        .into());
    }
    Ok(config)
}

pub fn apply_records(
    home: &Path,
    records: &[CandidateRecord],
) -> Result<Vec<CandidateHistoryRecord>, Box<dyn std::error::Error>> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::with_capacity(records.len());
    for record in records {
        let _config = verify_candidate_for_apply(record)?;
        let previous_lock = load_lockfile(&lockfile_path(&record.config_path))?;
        entries.push(CandidateTransactionEntry {
            config_path: record.config_path.clone(),
            project_id: record.project_id.clone(),
            candidate_id: record.id.clone(),
            config_sha256: record.baseline.config_sha256.clone(),
            effective_config_sha256: record.baseline.effective_config_sha256.clone(),
            previous_lock,
            applied_lock: record.lock.clone(),
            tested_git_head: record.tests.last().and_then(|test| test.git_head.clone()),
            tested_git_dirty: record.tests.last().and_then(|test| test.git_dirty),
        });
    }
    apply_transaction(home, entries)
}

pub fn list_history(
    home: &Path,
    config_path: &Path,
    project_id: &str,
) -> Result<Vec<CandidateHistoryRecord>, Box<dyn std::error::Error>> {
    let directory = candidate_directory(home, config_path, project_id)?.join("history");
    let mut records = Vec::new();
    match fs::read_dir(&directory) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let record: CandidateHistoryRecord = read_json(&entry.path())?;
                validate_history(&record)?;
                records.push(record);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    records.sort_by(|left, right| {
        (left.applied_unix_ms, left.id.as_str()).cmp(&(right.applied_unix_ms, right.id.as_str()))
    });
    records.reverse();
    Ok(records)
}

pub fn restore_history(
    home: &Path,
    config_path: &Path,
    project_id: &str,
    history_id: Option<&str>,
) -> Result<CandidateHistoryRecord, Box<dyn std::error::Error>> {
    let history = list_history(home, config_path, project_id)?;
    let selected = match history_id {
        Some(id) => history.into_iter().find(|record| record.id == id),
        None => history.into_iter().next(),
    }
    .ok_or_else(|| {
        format!(
            "candidate history {:?} was not found",
            history_id.unwrap_or("latest")
        )
    })?;
    let config = load_project_config(config_path)?;
    let effective = effective_project_config(config_path, &config)?;
    if hash_file(config_path)? != selected.config_sha256
        || hash_serialized(&effective)? != selected.effective_config_sha256
    {
        return Err(
            "project configuration changed after candidate application; refusing restore".into(),
        );
    }
    let current = load_lockfile(&lockfile_path(config_path))?;
    if candidate_digest(&current)? != candidate_digest(&selected.applied_lock)? {
        return Err(
            "current lock no longer matches the selected history entry; refusing restore".into(),
        );
    }
    let restored_candidate_id = format!("restore-{}", selected.id);
    let entries = vec![CandidateTransactionEntry {
        config_path: config_path.to_path_buf(),
        project_id: project_id.to_owned(),
        candidate_id: restored_candidate_id,
        config_sha256: selected.config_sha256.clone(),
        effective_config_sha256: selected.effective_config_sha256.clone(),
        previous_lock: current,
        applied_lock: selected.previous_lock.clone(),
        tested_git_head: git_head(config_path.parent().unwrap_or_else(|| Path::new(".")))?,
        tested_git_dirty: git_dirty(config_path.parent().unwrap_or_else(|| Path::new(".")))?,
    }];
    apply_transaction(home, entries)?
        .into_iter()
        .next()
        .ok_or_else(|| "restore transaction did not create history".into())
}

pub fn recover_transactions(home: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let directory = transaction_directory(home);
    let mut recovered = Vec::new();
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recovered),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let transaction: CandidateTransaction = read_json(&path)?;
        validate_transaction(&transaction)?;
        let mut lock_paths = transaction
            .entries
            .iter()
            .map(|entry| entry.config_path.clone())
            .collect::<Vec<_>>();
        lock_paths.sort();
        lock_paths.dedup();
        let mut guards = Vec::with_capacity(lock_paths.len());
        for config_path in &lock_paths {
            guards.push(acquire_project_state_write_lock(home, config_path)?);
        }
        let mut old = 0usize;
        let mut new = 0usize;
        for item in &transaction.entries {
            let config = load_project_config(&item.config_path)?;
            let effective = effective_project_config(&item.config_path, &config)?;
            if hash_file(&item.config_path)? != item.config_sha256
                || hash_serialized(&effective)? != item.effective_config_sha256
            {
                return Err(format!(
                    "candidate transaction {} conflicts with configuration {}",
                    transaction.id,
                    item.config_path.display()
                )
                .into());
            }
            let current = load_lockfile(&lockfile_path(&item.config_path))?;
            let digest = candidate_digest(&current)?;
            if digest == candidate_digest(&item.previous_lock)? {
                old += 1;
            } else if digest == candidate_digest(&item.applied_lock)? {
                new += 1;
            } else {
                return Err(format!(
                    "candidate transaction {} conflicts with {}",
                    transaction.id,
                    item.config_path.display()
                )
                .into());
            }
        }
        if new == transaction.entries.len() {
            persist_histories(home, &transaction)?;
        } else if old != transaction.entries.len() {
            for item in &transaction.entries {
                save_lockfile(&lockfile_path(&item.config_path), &item.previous_lock)?;
            }
        }
        fs::remove_file(&path)?;
        recovered.push(transaction.id);
    }
    Ok(recovered)
}

fn verify_baseline(
    record: &CandidateRecord,
    require_lock: bool,
) -> Result<ProjectConfig, Box<dyn std::error::Error>> {
    validate_record(record)?;
    let config = load_project_config(&record.config_path)?;
    let project_id = config
        .project_id
        .as_deref()
        .ok_or("candidate projects require project-id")?;
    if project_id != record.project_id {
        return Err("project identity changed after candidate preparation".into());
    }
    let effective = effective_project_config(&record.config_path, &config)?;
    if hash_file(&record.config_path)? != record.baseline.config_sha256
        || hash_serialized(&effective)? != record.baseline.effective_config_sha256
    {
        return Err("project configuration or inherited workspace defaults changed after candidate preparation".into());
    }
    if require_lock
        && hash_file(&lockfile_path(&record.config_path))? != record.baseline.lock_sha256
    {
        return Err(
            "project lock changed after candidate preparation; refusing to overwrite it".into(),
        );
    }
    Ok(config)
}

fn validate_candidate_lock(
    record: &CandidateRecord,
    config: &ProjectConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let effective = effective_project_config(&record.config_path, config)?;
    validate_lock_matches_tools(&record.lock, &effective.tools, &record.config_path)?;
    validate_lock_matches_tool_options(&record.lock, &effective.tool_options, &record.config_path)?;
    validate_project_lock_policy(&effective, &record.lock, SystemTime::now())?;
    Ok(())
}

fn apply_transaction(
    home: &Path,
    entries: Vec<CandidateTransactionEntry>,
) -> Result<Vec<CandidateHistoryRecord>, Box<dyn std::error::Error>> {
    let transaction = CandidateTransaction {
        schema: CANDIDATE_SCHEMA,
        id: uuid::Uuid::new_v4().to_string(),
        created_unix_ms: unix_time_ms()?,
        entries,
    };
    validate_transaction(&transaction)?;
    let mut lock_paths = transaction
        .entries
        .iter()
        .map(|entry| entry.config_path.clone())
        .collect::<Vec<_>>();
    lock_paths.sort();
    lock_paths.dedup();
    let mut guards = Vec::with_capacity(lock_paths.len());
    for path in &lock_paths {
        guards.push(acquire_project_state_write_lock(home, path)?);
    }
    verify_transaction_baselines(&transaction)?;
    let directory = transaction_directory(home);
    fs::create_dir_all(&directory)?;
    let journal = directory.join(format!("{}.json", transaction.id));
    write_json(&journal, &transaction)?;
    for (applied, entry) in transaction.entries.iter().enumerate() {
        if let Err(error) = save_lockfile(&lockfile_path(&entry.config_path), &entry.applied_lock) {
            for rollback in transaction.entries[..applied].iter().rev() {
                save_lockfile(
                    &lockfile_path(&rollback.config_path),
                    &rollback.previous_lock,
                )?;
            }
            fs::remove_file(&journal)?;
            return Err(error.into());
        }
    }
    let histories = persist_histories(home, &transaction)?;
    fs::remove_file(&journal)?;
    Ok(histories)
}

fn persist_histories(
    home: &Path,
    transaction: &CandidateTransaction,
) -> Result<Vec<CandidateHistoryRecord>, Box<dyn std::error::Error>> {
    let mut histories = Vec::new();
    for (index, entry) in transaction.entries.iter().enumerate() {
        let history = CandidateHistoryRecord {
            schema: CANDIDATE_SCHEMA,
            id: format!("{}-{index:04}", transaction.id),
            candidate_id: entry.candidate_id.clone(),
            project_id: entry.project_id.clone(),
            config_path: entry.config_path.clone(),
            applied_unix_ms: unix_time_ms()?,
            config_sha256: entry.config_sha256.clone(),
            effective_config_sha256: entry.effective_config_sha256.clone(),
            previous_lock: entry.previous_lock.clone(),
            applied_lock: entry.applied_lock.clone(),
            tested_git_head: entry.tested_git_head.clone(),
            tested_git_dirty: entry.tested_git_dirty,
        };
        let directory =
            candidate_directory(home, &history.config_path, &history.project_id)?.join("history");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.json", history.id));
        if !path.exists() {
            write_json(&path, &history)?;
        }
        histories.push(history);
    }
    Ok(histories)
}

fn verify_transaction_baselines(
    transaction: &CandidateTransaction,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in &transaction.entries {
        let config = load_project_config(&entry.config_path)?;
        let effective = effective_project_config(&entry.config_path, &config)?;
        if hash_file(&entry.config_path)? != entry.config_sha256
            || hash_serialized(&effective)? != entry.effective_config_sha256
        {
            return Err(format!(
                "candidate transaction {} found a conflicting configuration change in {}",
                transaction.id,
                entry.config_path.display()
            )
            .into());
        }
        let current = load_lockfile(&lockfile_path(&entry.config_path))?;
        if candidate_digest(&current)? != candidate_digest(&entry.previous_lock)? {
            return Err(format!(
                "candidate transaction {} found a conflicting lock change in {}",
                transaction.id,
                entry.config_path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn validate_record(record: &CandidateRecord) -> Result<(), Box<dyn std::error::Error>> {
    if record.schema != CANDIDATE_SCHEMA || record.id.is_empty() || record.project_id.is_empty() {
        return Err("invalid candidate record schema or identity".into());
    }
    Ok(())
}

fn validate_history(record: &CandidateHistoryRecord) -> Result<(), Box<dyn std::error::Error>> {
    if record.schema != CANDIDATE_SCHEMA || record.id.is_empty() || record.candidate_id.is_empty() {
        return Err("invalid candidate history record".into());
    }
    Ok(())
}

fn validate_transaction(
    transaction: &CandidateTransaction,
) -> Result<(), Box<dyn std::error::Error>> {
    if transaction.schema != CANDIDATE_SCHEMA
        || transaction.id.is_empty()
        || transaction.entries.is_empty()
    {
        return Err("invalid candidate transaction".into());
    }
    for (index, entry) in transaction.entries.iter().enumerate() {
        if transaction.entries[..index]
            .iter()
            .any(|previous| same_path(&previous.config_path, &entry.config_path))
        {
            return Err("candidate transaction contains duplicate project paths".into());
        }
    }
    Ok(())
}

fn transaction_directory(home: &Path) -> PathBuf {
    home.join("state").join("candidate-transactions")
}

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}

fn hash_serialized<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_vec(value).map(|bytes| hex::encode(Sha256::digest(bytes)))
}

fn git_head(root: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let head = String::from_utf8(output.stdout)?.trim().to_owned();
    Ok((!head.is_empty()).then_some(head))
}

fn git_dirty(root: &Path) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(!output.stdout.is_empty()))
}

fn unix_time_ms() -> Result<u64, std::time::SystemTimeError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = AtomicWriteFile::options().open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.commit()?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 16 * 1024 * 1024
    {
        return Err(format!("refusing unsafe candidate state {}", path.display()).into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn active_candidate_is_bound_to_config_lock_and_project_identity() {
        let root = tempdir().expect("candidate");
        let project = root.path().join("project");
        let home = root.path().join("home");
        fs::create_dir_all(&project).expect("project");
        let config_path = project.join(pinset_core::PROJECT_CONFIG_FILENAME);
        fs::write(
            &config_path,
            "schema = 5\nproject-id = \"4c5652e4-0000-4000-8000-000000000040\"\n\n[tools]\n",
        )
        .expect("config");
        let lock = Lockfile {
            schema: pinset_core::LOCKFILE_SCHEMA,
            generated_by: "pinset test".to_owned(),
            tools: Vec::new(),
        };
        save_lockfile(&lockfile_path(&config_path), &lock).expect("lock");
        let record = CandidateRecord {
            schema: CANDIDATE_SCHEMA,
            id: uuid::Uuid::new_v4().to_string(),
            project_id: "4c5652e4-0000-4000-8000-000000000040".to_owned(),
            config_path: config_path.clone(),
            created_unix_ms: unix_time_ms().expect("time"),
            baseline: capture_baseline(&config_path).expect("baseline"),
            lock,
            tests: Vec::new(),
        };
        save_active(&home, &record).expect("save active");
        assert_eq!(
            load_active(&home, &config_path, &record.project_id)
                .expect("load active")
                .id,
            record.id
        );
        fs::write(
            &config_path,
            "schema = 5\nproject-id = \"4c5652e4-0000-4000-8000-000000000040\"\n\n[tools]\n# changed\n",
        )
        .expect("change config");
        assert!(verify_candidate_for_test(&record).is_err());
    }

    #[test]
    fn mixed_workspace_transaction_recovers_every_member_to_previous_lock() {
        let root = tempdir().expect("transaction");
        let home = root.path().join("home");
        let mut transaction_entries = Vec::new();
        for (index, project_id) in [
            "4c5652e4-0000-4000-8000-000000000042",
            "4c5652e4-0000-4000-8000-000000000043",
        ]
        .into_iter()
        .enumerate()
        {
            let project = root.path().join(format!("member-{index}"));
            fs::create_dir_all(&project).expect("member");
            let config_path = project.join(pinset_core::PROJECT_CONFIG_FILENAME);
            fs::write(
                &config_path,
                format!("schema = 5\nproject-id = \"{project_id}\"\n\n[tools]\n"),
            )
            .expect("config");
            let previous_lock = empty_lock("previous");
            let applied_lock = empty_lock("candidate");
            save_lockfile(&lockfile_path(&config_path), &previous_lock).expect("previous lock");
            let baseline = capture_baseline(&config_path).expect("baseline");
            transaction_entries.push(CandidateTransactionEntry {
                config_path,
                project_id: project_id.to_owned(),
                candidate_id: format!("candidate-{index}"),
                config_sha256: baseline.config_sha256,
                effective_config_sha256: baseline.effective_config_sha256,
                previous_lock,
                applied_lock,
                tested_git_head: None,
                tested_git_dirty: None,
            });
        }
        let transaction = CandidateTransaction {
            schema: CANDIDATE_SCHEMA,
            id: "4c5652e4-0000-4000-8000-000000000044".to_owned(),
            created_unix_ms: unix_time_ms().expect("time"),
            entries: transaction_entries,
        };
        let directory = transaction_directory(&home);
        fs::create_dir_all(&directory).expect("transaction directory");
        write_json(
            &directory.join(format!("{}.json", transaction.id)),
            &transaction,
        )
        .expect("journal");
        save_lockfile(
            &lockfile_path(&transaction.entries[0].config_path),
            &transaction.entries[0].applied_lock,
        )
        .expect("first member applied");

        assert_eq!(
            recover_transactions(&home).expect("recover"),
            [transaction.id]
        );
        for entry in &transaction.entries {
            assert_eq!(
                load_lockfile(&lockfile_path(&entry.config_path)).expect("restored lock"),
                entry.previous_lock
            );
        }
    }

    #[test]
    fn completed_transaction_recovery_writes_idempotent_history() {
        let root = tempdir().expect("transaction");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(&project).expect("project");
        let project_id = "4c5652e4-0000-4000-8000-000000000045";
        let config_path = project.join(pinset_core::PROJECT_CONFIG_FILENAME);
        fs::write(
            &config_path,
            format!("schema = 5\nproject-id = \"{project_id}\"\n\n[tools]\n"),
        )
        .expect("config");
        let previous_lock = empty_lock("previous");
        let applied_lock = empty_lock("candidate");
        save_lockfile(&lockfile_path(&config_path), &applied_lock).expect("applied lock");
        let effective = load_project_config(&config_path).expect("config");
        let transaction = CandidateTransaction {
            schema: CANDIDATE_SCHEMA,
            id: "4c5652e4-0000-4000-8000-000000000046".to_owned(),
            created_unix_ms: unix_time_ms().expect("time"),
            entries: vec![CandidateTransactionEntry {
                config_path: config_path.clone(),
                project_id: project_id.to_owned(),
                candidate_id: "candidate-complete".to_owned(),
                config_sha256: hash_file(&config_path).expect("config hash"),
                effective_config_sha256: hash_serialized(&effective).expect("effective hash"),
                previous_lock,
                applied_lock,
                tested_git_head: None,
                tested_git_dirty: None,
            }],
        };
        let directory = transaction_directory(&home);
        fs::create_dir_all(&directory).expect("transaction directory");
        let journal = directory.join(format!("{}.json", transaction.id));
        write_json(&journal, &transaction).expect("journal");
        recover_transactions(&home).expect("first recovery");
        write_json(&journal, &transaction).expect("recreated journal");
        recover_transactions(&home).expect("repeated recovery");
        assert_eq!(
            list_history(&home, &config_path, project_id)
                .expect("history")
                .len(),
            1
        );
    }

    fn empty_lock(generated_by: &str) -> Lockfile {
        Lockfile {
            schema: pinset_core::LOCKFILE_SCHEMA,
            generated_by: generated_by.to_owned(),
            tools: Vec::new(),
        }
    }
}
