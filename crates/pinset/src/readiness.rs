//! Project readiness uses the same resolver as normal execution. Collection never executes tools.
use std::{fs, path::Path};

use pinset_core::{
    EnvironmentCheck, EnvironmentDescriptor, ReadinessState, RuntimeDescriptor,
    current_target, current_target_for_tool, environment_selection, find_optional_project_config,
    load_effective_project_config, load_optional_lockfile, lockfile_path, pinset_home,
    resolve_command, resolve_tool_selection, runtime_provider,
};
use sha2::{Digest, Sha256};

pub type ReportResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn check(id: &str, state: ReadinessState, reason: &str, next: Option<&str>) -> EnvironmentCheck {
    EnvironmentCheck {
        id: id.to_owned(), state, reason: reason.to_owned(), next_step: next.map(str::to_owned),
    }
}

pub fn host() -> &'static str {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() { "wsl" }
    else if std::env::var_os("SSH_CONNECTION").is_some() { "ssh" }
    else if Path::new("/.dockerenv").exists() { "container" }
    else { "local" }
}

/// Local state identity excludes decrypted values and includes effective member configuration.
pub fn fingerprint(cwd: &Path, profile: Option<&str>, no_env: bool) -> ReportResult<String> {
    let root = fs::canonicalize(cwd)?;
    let mut digest = Sha256::new();
    let root_text = root.to_string_lossy();
    let target = current_target();
    for part in [root_text.as_bytes(), target.as_bytes(),
        profile.unwrap_or("").as_bytes(), if no_env { b"disabled" } else { b"enabled" }] {
        digest.update(part.len().to_le_bytes()); digest.update(part);
    }
    if let Some(path) = find_optional_project_config(&root)? {
        let config = load_effective_project_config(&path)?;
        digest.update(serde_json::to_vec(&config)?);
        if let Some(lock) = load_optional_lockfile(&lockfile_path(&path))? {
            digest.update(serde_json::to_vec(&lock)?);
        }
    } else {
        digest.update(serde_json::to_vec(&pinset_core::scan_project_sources(&root)?)?);
    }
    Ok(hex::encode(digest.finalize()))
}

#[allow(clippy::too_many_arguments)]
pub fn run(command: &'static str, cwd: &Path, json: bool, save: Option<&Path>, compare: Option<&Path>, probe: bool, profile: Option<&str>, no_env: bool) -> ReportResult<i32> {
    use std::io::Write;
    let mut report = collect(cwd, profile, no_env)?;
    if probe {
        verify_environment(cwd, &mut report, no_env);
        // A failed trust/identity/contract check must not launch project-context probes.
        if !report.checks.iter().any(|item| item.id == "environment" && item.state == ReadinessState::Fail) {
            crate::probes::collect(cwd, &mut report)?;
        }
    }
    let report = report.portable();
    let comparison = if let Some(path) = compare {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err("environment report must be a regular file of at most 1 MiB".into());
        }
        let previous: EnvironmentDescriptor = serde_json::from_slice(&fs::read(path)?)?;
        if previous.schema != 2 { return Err("unsupported environment report schema".into()); }
        let mut changes = Vec::new();
        for runtime in &report.runtimes {
            let old = previous.runtimes.iter().find(|old| old.tool == runtime.tool);
            if old.is_none_or(|old| old.locked_version != runtime.locked_version || old.installation_identity != runtime.installation_identity) {
                changes.push(format!("runtime:{}", runtime.tool));
            }
        }
        for old in &previous.runtimes {
            if !report.runtimes.iter().any(|runtime| runtime.tool == old.tool) { changes.push(format!("removed:{}", old.tool)); }
        }
        if previous.profile != report.profile { changes.push("environment:profile".to_owned()); }
        Some(serde_json::json!({"changes": changes, "platform_changed": previous.target != report.target,
            "execution_comparable": false, "secret_values_compared": false}))
    } else { None };
    if let Some(path) = save {
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file()) {
            return Err("report destination must be a regular file".into());
        }
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) { fs::create_dir_all(parent)?; }
        let mut file = atomic_write_file::AtomicWriteFile::options().open(path)?;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?; file.commit()?;
    }
    if json { crate::print_json_success(command, serde_json::json!({"report": report, "comparison": comparison}))?; }
    else {
        println!("Environment: {} | Execution: {}", if report.environment_ready { "ready" } else { "needs attention" },
            if report.execution_verified { "requested probes verified" } else { "not verified" });
        for runtime in &report.runtimes { println!("{} {} [{}]", runtime.tool, runtime.locked_version.as_deref().unwrap_or("unlocked"), runtime.selection_source); }
        for item in report.checks.iter().chain(report.runtimes.iter().flat_map(|runtime| &runtime.checks)) {
            if matches!(item.state, ReadinessState::Fail | ReadinessState::Unknown) { println!("{}: {}{}", item.id, item.reason, item.next_step.as_ref().map(|next| format!("; {next}")).unwrap_or_default()); }
        }
        for evidence in &report.evidence { println!("{} {}: {:?} ({})", evidence.tool, evidence.entry, evidence.state, evidence.reason); }
    }
    Ok(if command == "check" && (!report.environment_ready || (probe && !report.execution_verified)) { 1 } else { 0 })
}

pub fn collect(cwd: &Path, profile: Option<&str>, no_env: bool) -> ReportResult<EnvironmentDescriptor> {
    let home = pinset_home()?;
    let path = find_optional_project_config(cwd)?;
    let mut report = EnvironmentDescriptor {
        schema: 2, cli_version: pinset_core::pinset_version().to_owned(), project_id: None,
        project_root: None, target: current_target(), host: host().to_owned(),
        profile: None, profile_source: if no_env { "disabled" } else { "none" }.to_owned(),
        context_fingerprint: None, runtimes: Vec::new(), checks: Vec::new(), evidence: Vec::new(),
        environment_ready: false, execution_verified: false,
    };
    let Some(path) = path else {
        report.checks.push(check("project", ReadinessState::Fail, "project_not_configured", Some("pinset setup")));
        return Ok(report);
    };
    let config = load_effective_project_config(&path)?;
    let root = path.parent().ok_or("project configuration has no parent")?;
    report.project_root = Some(fs::canonicalize(root)?.display().to_string());
    report.project_id = config.project_id.clone();
    if !no_env {
        let selection = environment_selection(&home, &path, &config, profile)?;
        report.profile = selection.profile;
        report.profile_source = selection.source.to_owned();
    }
    report.context_fingerprint = Some(fingerprint(root, report.profile.as_deref(), no_env)?);
    let lock = load_optional_lockfile(&lockfile_path(&path))?;
    report.checks.push(check("project", ReadinessState::Pass, "configuration_loaded", None));
    report.checks.push(check("lock", if lock.is_some() { ReadinessState::Pass } else { ReadinessState::Fail },
        if lock.is_some() { "lock_loaded" } else { "lock_missing" }, if lock.is_none() { Some("pinset setup") } else { None }));
    for (name, requested) in &config.tools {
        let locked = lock.as_ref().and_then(|lock| lock.tool(name));
        let provider = runtime_provider(name);
        let commands = provider.map(|provider| provider.commands.iter().map(|value| (*value).to_owned()).collect()).unwrap_or_default();
        let command = provider.map_or(name.as_str(), |provider| provider.commands[0]);
        let resolution = resolve_command(command, cwd, &home);
        let selection = resolve_tool_selection(name, cwd, &home).ok();
        let target = current_target_for_tool(name);
        let available = locked.is_some_and(|tool| tool.artifacts.iter().any(|artifact| artifact.target == target));
        let mut checks = vec![check("artifact", if available { ReadinessState::Pass } else { ReadinessState::Fail },
            if available { "target_artifact_locked" } else { "target_artifact_missing" }, None)];
        checks.push(check("routing", if resolution.is_ok() { ReadinessState::Pass } else { ReadinessState::Fail },
            if resolution.is_ok() { "managed_command_resolved" } else { "managed_command_unavailable" },
            if resolution.is_err() { Some("pinset doctor --deep") } else { None }));
        report.runtimes.push(RuntimeDescriptor {
            tool: name.clone(), requested: requested.clone(), locked_version: locked.map(|tool| tool.version.clone()),
            installation_identity: locked.map(|tool| tool.installation_version()),
            selection_source: selection.map_or_else(|| "project".to_owned(), |selection| selection.source.as_str().to_owned()),
            target, commands, executable: resolution.ok().map(|resolution| resolution.executable.display().to_string()), checks,
        });
    }
    let audit = pinset_core::audit_project_lock(&home, cwd);
    for finding in audit.findings {
        if finding.severity == pinset_core::LockAuditSeverity::Error {
            report.checks.push(check(finding.reason_code.as_str(), ReadinessState::Fail, "project_audit_failed", Some("pinset doctor --deep")));
        }
    }
    let environment = if no_env || config.environment.is_none() {
        check("environment", ReadinessState::NotApplicable, "project_environment_not_requested", None)
    } else if report.profile.is_some() {
        check("environment", ReadinessState::Unknown, "encrypted_environment_not_opened", Some("pinset env check"))
    } else if config.environment.as_ref().is_some_and(|environment| environment.variables.values().any(|variable| variable.required)) {
        check("environment", ReadinessState::Fail, "required_environment_profile_not_selected", Some("pinset env use <profile>"))
    } else {
        check("environment", ReadinessState::NotApplicable, "no_profile_selected", None)
    };
    report.checks.push(environment);
    report.update_readiness();
    Ok(report)
}

/// Explicit execution may check a trusted environment; background collection never decrypts it.
pub fn verify_environment(cwd: &Path, report: &mut EnvironmentDescriptor, no_env: bool) {
    if no_env || report.profile.is_none() { return; }
    let result = crate::environment::resolve_environment(cwd, report.profile.as_deref()).map(|(_, mut values)| {
        use zeroize::Zeroize;
        for value in values.values_mut() { value.zeroize(); }
    });
    let reason = match &result {
        Ok(()) => "trusted_environment_contract_valid",
        Err(error) => error.downcast_ref::<crate::environment::ContractError>().map_or_else(
            || crate::json_error(error.as_ref()).0, |error| error.reason()),
    };
    let valid = result.is_ok();
    if let Some(item) = report.checks.iter_mut().find(|item| item.id == "environment") {
        *item = check("environment", if valid { ReadinessState::Pass } else { ReadinessState::Fail },
            reason,
            if valid { None } else { Some("pinset env check") });
    }
    report.update_readiness();
}
