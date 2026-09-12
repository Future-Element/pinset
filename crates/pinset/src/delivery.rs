//! Explicit team/offline/network reports. No task or runtime is launched here.
use crate::readiness::ReportResult;
use clap::Args;
use pinset_core::{EnvironmentCheck, EnvironmentDescriptor, ReadinessState};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

#[derive(Debug, Args)]
pub struct Options {
    /// Show the complete team delivery checklist. Implies environment report 2.
    #[arg(long)]
    pub delivery: bool,
    /// Hash every required cached SDK artifact without making network requests.
    #[arg(long, conflicts_with = "network")]
    pub offline: bool,
    /// Explicitly probe selected sources in their configured fallback order.
    #[arg(long, conflicts_with = "offline")]
    pub network: bool,
    #[arg(long = "target", value_delimiter = ',')]
    pub targets: Vec<String>,
}

impl Options {
    pub fn requested(&self) -> bool {
        self.delivery || self.offline || self.network || !self.targets.is_empty()
    }
}

pub fn validate_targets(targets: &[String]) -> ReportResult<()> {
    if targets.len() > 5
        || targets
            .iter()
            .any(|target| !pinset_core::ENVIRONMENT_PLATFORMS.contains(&target.as_str()))
        || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
    {
        return Err("targets must be unique supported Pinset platforms".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    cwd: &Path,
    options: Options,
    probe: bool,
    json: bool,
    save: Option<&Path>,
    compare: Option<&Path>,
    profile: Option<&str>,
    no_env: bool,
) -> ReportResult<i32> {
    let home = pinset_core::pinset_home()?;
    let config_path = pinset_core::find_project_config(cwd)?;
    let config = pinset_core::load_effective_project_config(&config_path)?;
    let lock = pinset_core::load_lockfile(&pinset_core::lockfile_path(&config_path))?;
    pinset_core::validate_lock_matches_tools(&lock, &config.tools, &config_path)?;
    pinset_core::validate_lock_matches_tool_options(&lock, &config.tool_options, &config_path)?;
    let mut targets = options.targets;
    if targets.is_empty() {
        targets = config
            .requirements
            .as_ref()
            .map(|requirements| requirements.platforms.clone())
            .unwrap_or_default();
    }
    if targets.is_empty() {
        targets.push(pinset_core::current_target());
    }
    validate_targets(&targets)?;
    let mut report = crate::readiness::collect(cwd, profile, no_env)?;
    crate::readiness::verify_environment(cwd, &mut report, no_env);
    if probe {
        crate::readiness::probe_report(cwd, &mut report, profile, no_env)?;
    }
    let report = report.portable();
    let offline = options
        .offline
        .then(|| pinset_core::inspect_offline_delivery(&home, &lock, &targets));
    let mut attempts = Vec::new();
    let mut network_checks = Vec::new();
    if options.network {
        let sources = pinset_core::load_source_config(&pinset_core::source_config_path(&home))?;
        let items = crate::locked_prefetch_items_for_platforms(&lock, &sources, &targets)?;
        let client = pinset_core::http_client_builder()?.build()?;
        for (index, item) in items.iter().enumerate() {
            let mut state = ReadinessState::Fail;
            let mut reason = "selected_sources_unreachable";
            for source in &item.artifact.sources {
                if attempts.len() >= 32 {
                    state = ReadinessState::Unknown;
                    reason = "network_probe_budget_exhausted";
                    break;
                }
                let result = pinset_core::probe_source(&client, attempts.len(), &source.url);
                let passed = result.state == ReadinessState::Pass;
                let unknown = result.state == ReadinessState::Unknown;
                attempts.push(result);
                if passed {
                    state = ReadinessState::Pass;
                    reason = "selected_source_reachable_integrity_not_checked";
                    break;
                }
                if unknown {
                    state = ReadinessState::Unknown;
                    reason = "source_reachability_not_established";
                }
            }
            network_checks.push(EnvironmentCheck {
                id: format!("network.artifact.{index}"),
                state,
                reason: reason.to_owned(),
                next_step: Some(
                    "Review source attempts; prefetch verifies the archive content separately."
                        .to_owned(),
                ),
            });
        }
    }
    let comparison = compare
        .map(|path| read_report(path).map(|previous| compare_reports(&previous, &report)))
        .transpose()?;
    if let Some(path) = save {
        save_report(path, &report)?;
    }
    let delivery_passed = if let Some(offline) = &offline {
        offline.ready
    } else if options.network {
        !network_checks.is_empty()
            && network_checks
                .iter()
                .all(|check| check.state == ReadinessState::Pass)
    } else {
        report.environment_ready
    };
    let passed = delivery_passed && (!probe || report.execution_verified);
    if json {
        crate::print_json_success(
            "check",
            serde_json::json!({"report": report, "comparison": comparison, "offline": offline,
            "network": {"checks": network_checks, "attempts": attempts, "requests": attempts.len()}, "passed": passed,
            "project_dependencies_verified": false, "secret_values_compared": false}),
        )?;
    } else {
        println!(
            "Environment ready: {} | Delivery check passed: {}",
            report.environment_ready, passed
        );
        for check in report
            .checks
            .iter()
            .chain(report.runtimes.iter().flat_map(|runtime| &runtime.checks))
            .chain(network_checks.iter())
        {
            println!("{} [{:?}]: {}", check.id, check.state, check.reason);
        }
        if let Some(offline) = &offline {
            for artifact in &offline.artifacts {
                println!(
                    "{} {} overlay={} [{:?}]: {}",
                    artifact.tool,
                    artifact.target,
                    artifact.overlay,
                    artifact.state,
                    artifact.reason
                );
            }
        }
        for attempt in attempts {
            println!("{} [{:?}]: {}", attempt.id, attempt.state, attempt.reason);
        }
        println!(
            "SDK readiness does not verify npm/pip/Maven project dependencies or application builds."
        );
    }
    Ok(if passed { 0 } else { 1 })
}

pub fn save_report(path: &Path, report: &EnvironmentDescriptor) -> ReportResult<()> {
    if fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("report destination must be a regular file".into());
    }
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = atomic_write_file::AtomicWriteFile::options().open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&report.portable())?)?;
    file.commit()?;
    Ok(())
}

pub fn read_report(path: &Path) -> ReportResult<EnvironmentDescriptor> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("environment report must be a regular file of at most 1 MiB".into());
    }
    let report: EnvironmentDescriptor = serde_json::from_slice(&fs::read(path)?)?;
    if report.schema != 2 {
        return Err("unsupported environment report schema".into());
    }
    let unique = report
        .runtimes
        .iter()
        .map(|runtime| &runtime.tool)
        .collect::<BTreeSet<_>>();
    if unique.len() != report.runtimes.len() {
        return Err("environment report contains duplicate runtime identities".into());
    }
    Ok(report)
}

pub fn compare_reports(
    previous: &EnvironmentDescriptor,
    current: &EnvironmentDescriptor,
) -> serde_json::Value {
    let mut changes = Vec::new();
    for runtime in &current.runtimes {
        match previous
            .runtimes
            .iter()
            .find(|old| old.tool == runtime.tool)
        {
            None => changes.push(format!("added:{}", runtime.tool)),
            Some(old) => {
                if old.locked_version != runtime.locked_version {
                    changes.push(format!("version:{}", runtime.tool));
                }
                if old.options != runtime.options
                    || (old.locked_version == runtime.locked_version
                        && old.installation_identity != runtime.installation_identity)
                {
                    changes.push(format!("options:{}", runtime.tool));
                }
                if !old.artifacts.is_empty()
                    && !runtime.artifacts.is_empty()
                    && sorted_artifacts(&old.artifacts) != sorted_artifacts(&runtime.artifacts)
                {
                    changes.push(format!("artifacts:{}", runtime.tool));
                }
            }
        }
    }
    for runtime in &previous.runtimes {
        if !current
            .runtimes
            .iter()
            .any(|item| item.tool == runtime.tool)
        {
            changes.push(format!("removed:{}", runtime.tool));
        }
    }
    if previous.profile != current.profile {
        changes.push("environment:profile".to_owned());
    }
    if previous.project_id != current.project_id {
        changes.push("environment:project-id".to_owned());
    }
    if sorted_requirements(previous.requirements.clone())
        != sorted_requirements(current.requirements.clone())
    {
        changes.push("environment:requirements".to_owned());
    }
    if sorted_variables(&previous.variables) != sorted_variables(&current.variables) {
        changes.push("environment:variable-contract".to_owned());
    }
    let changed = previous.target != current.target;
    let allowed = current.requirements.as_ref().is_some_and(|requirements| {
        requirements.platforms.contains(&previous.target)
            && requirements.platforms.contains(&current.target)
    });
    let mut execution = Vec::new();
    for observed in &current.evidence {
        let old = previous
            .evidence
            .iter()
            .find(|old| old.tool == observed.tool && old.entry == observed.entry);
        execution.push(serde_json::json!({"tool": observed.tool, "entry": observed.entry,
            "comparison": if old.is_some_and(|old| old.state == ReadinessState::Pass && observed.state == ReadinessState::Pass && old.observed_version == observed.observed_version) { "same_observed_version_independent_runs" } else { "different_or_unverified" }}));
    }
    serde_json::json!({"changes": changes, "platform_changed": changed, "platform_difference": if !changed { "same" } else if allowed { "allowed_by_requirements" } else { "not_declared" },
        "execution": execution, "execution_comparable": false, "secret_values_compared": false,
        "artifact_comparison_complete": previous.runtimes.iter().chain(&current.runtimes).all(|runtime| !runtime.artifacts.is_empty())})
}

fn sorted_artifacts(
    artifacts: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    artifacts
        .iter()
        .map(|(target, values)| {
            let mut values = values.clone();
            values.sort();
            (target.clone(), values)
        })
        .collect()
}

fn sorted_requirements(
    requirements: Option<pinset_core::ProjectRequirements>,
) -> Option<pinset_core::ProjectRequirements> {
    requirements.map(|mut requirements| {
        requirements.platforms.sort();
        requirements.build_targets.sort();
        requirements.disabled_rules.sort();
        requirements
    })
}

fn sorted_variables(
    variables: &std::collections::BTreeMap<String, pinset_core::VariableRequirement>,
) -> std::collections::BTreeMap<String, pinset_core::VariableRequirement> {
    variables
        .iter()
        .map(|(name, requirement)| {
            let mut requirement = requirement.clone();
            requirement.profiles.sort();
            (name.clone(), requirement)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn report() -> EnvironmentDescriptor {
        serde_json::from_value(json!({
            "schema":2,"cli_version":"2.14.0","project_id":"fixture","project_root":"/private/alice/project",
            "target":"linux-x86_64","host":"local","profile":"dev","profile_source":"local", "context_fingerprint":"private-context",
            "runtimes":[{"tool":"rust","requested":"1.97.1","locked_version":"1.97.1","installation_identity":"1.97.1+options",
                "selection_source":"project","target":"linux-x86_64","commands":["rustc"],"executable":"/private/alice/rustc","checks":[],
                "options":{"profile":"default"},"artifacts":{"linux-x86_64":["sha256:base","sha256:overlay"]}}],
            "checks":[],"evidence":[],"environment_ready":false,"execution_verified":false,
            "requirements":{"platforms":["linux-x86_64","windows-x86_64"]},
            "variables":{"TOKEN":{"kind":"string","required":true,"secret":true,"profiles":["dev","test"]}}
        })).unwrap()
    }
    #[test]
    fn comparison_ignores_machine_paths_order_and_allowed_platform_differences() {
        let previous = report();
        let mut current = previous.clone();
        current.project_root = Some("C:/Bob/project".to_owned());
        current.target = "windows-x86_64".to_owned();
        current.runtimes[0].executable = Some("C:/Bob/rustc.exe".to_owned());
        current.runtimes[0].target = current.target.clone();
        current.requirements.as_mut().unwrap().platforms.reverse();
        current
            .variables
            .get_mut("TOKEN")
            .unwrap()
            .profiles
            .reverse();
        current.runtimes[0]
            .artifacts
            .get_mut("linux-x86_64")
            .unwrap()
            .reverse();
        let comparison = compare_reports(&previous, &current);
        assert_eq!(comparison["changes"], json!([]));
        assert_eq!(comparison["platform_difference"], "allowed_by_requirements");
        assert_eq!(comparison["secret_values_compared"], false);
        current.runtimes[0]
            .options
            .insert("profile".to_owned(), "minimal".to_owned());
        current.runtimes[0]
            .artifacts
            .get_mut("linux-x86_64")
            .unwrap()
            .pop();
        assert_eq!(
            compare_reports(&previous, &current)["changes"],
            json!(["options:rust", "artifacts:rust"])
        );
    }
    #[test]
    fn portable_export_and_legacy_comparison_preserve_evidence_limits() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("report.json");
        save_report(&path, &report()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("/private/") && !text.contains("private-context"));
        let mut previous = read_report(&path).unwrap();
        previous.runtimes[0].artifacts.clear();
        assert_eq!(
            compare_reports(&previous, &report())["artifact_comparison_complete"],
            false
        );
        previous.schema = 99;
        fs::write(&path, serde_json::to_vec(&previous).unwrap()).unwrap();
        assert!(read_report(&path).is_err());
        let mut duplicate = report();
        duplicate.runtimes.push(duplicate.runtimes[0].clone());
        fs::write(&path, serde_json::to_vec(&duplicate).unwrap()).unwrap();
        assert!(read_report(&path).is_err());
    }
}
