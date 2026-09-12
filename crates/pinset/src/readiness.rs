//! Project readiness uses the same resolver as normal execution. Collection never executes tools.
use std::{fs, path::Path};

use pinset_core::{
    EnvironmentCheck, EnvironmentDescriptor, ReadinessState, RuntimeDescriptor, current_target,
    current_target_for_tool, environment_selection, find_optional_project_config,
    load_effective_project_config, load_optional_lockfile, lockfile_path, pinset_home,
    resolve_command, resolve_tool_selection, runtime_provider,
};
use sha2::{Digest, Sha256};

pub type ReportResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn check(
    id: &str,
    state: ReadinessState,
    reason: &str,
    next: Option<&str>,
) -> EnvironmentCheck {
    EnvironmentCheck {
        id: id.to_owned(),
        state,
        reason: reason.to_owned(),
        next_step: next.map(str::to_owned),
    }
}

pub fn host() -> &'static str {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        "wsl"
    } else if std::env::var_os("SSH_CONNECTION").is_some() {
        "ssh"
    } else if Path::new("/.dockerenv").exists() {
        "container"
    } else {
        "local"
    }
}

/// Local state identity excludes decrypted values and includes effective member configuration.
pub fn fingerprint(cwd: &Path, profile: Option<&str>, no_env: bool) -> ReportResult<String> {
    let no_env = no_env || std::env::var_os("PINSET_ENV_DISABLE").is_some_and(|value| value == "1");
    let root = fs::canonicalize(cwd)?;
    let mut digest = Sha256::new();
    let root_text = root.to_string_lossy();
    let target = current_target();
    for part in [
        root_text.as_bytes(),
        target.as_bytes(),
        profile.unwrap_or("").as_bytes(),
        if no_env { b"disabled" } else { b"enabled" },
    ] {
        digest.update(part.len().to_le_bytes());
        digest.update(part);
    }
    if let Some(path) = find_optional_project_config(&root)? {
        let config = load_effective_project_config(&path)?;
        digest.update(serde_json::to_vec(&config)?);
        if let Some(lock) = load_optional_lockfile(&lockfile_path(&path))? {
            digest.update(serde_json::to_vec(&lock)?);
        }
        for relative in [
            "package.json",
            "pyproject.toml",
            "go.mod",
            "go.work",
            "rust-toolchain.toml",
            "rust-toolchain",
            "global.json",
            "gradle/wrapper/gradle-wrapper.properties",
            "android/gradle/wrapper/gradle-wrapper.properties",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "android/build.gradle",
            "android/build.gradle.kts",
            "android/settings.gradle",
            "android/settings.gradle.kts",
        ] {
            digest.update(relative.as_bytes());
            let declaration = root.join(relative);
            let safe = fs::symlink_metadata(&declaration).is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 1024 * 1024
            }) && fs::canonicalize(&declaration)
                .is_ok_and(|path| path.starts_with(&root));
            if safe {
                digest.update(fs::read(declaration)?);
            } else {
                digest.update(b"unavailable-or-unsafe");
            }
        }
    } else {
        digest.update(serde_json::to_vec(&pinset_core::scan_project_sources(
            &root,
        )?)?);
    }
    Ok(hex::encode(digest.finalize()))
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    command: &'static str,
    cwd: &Path,
    json: bool,
    save: Option<&Path>,
    compare: Option<&Path>,
    probe: bool,
    profile: Option<&str>,
    no_env: bool,
) -> ReportResult<i32> {
    let mut report = collect(cwd, profile, no_env)?;
    if probe {
        verify_environment(cwd, &mut report, no_env);
        probe_report(cwd, &mut report, profile, no_env)?;
    }
    let report = report.portable();
    let comparison = compare
        .map(|path| {
            crate::delivery::read_report(path)
                .map(|previous| crate::delivery::compare_reports(&previous, &report))
        })
        .transpose()?;
    if let Some(path) = save {
        crate::delivery::save_report(path, &report)?;
    }
    if json {
        crate::print_json_success(
            command,
            serde_json::json!({"report": report, "comparison": comparison}),
        )?;
    } else {
        println!(
            "Environment: {} | Execution: {}",
            if report.environment_ready {
                "ready"
            } else {
                "needs attention"
            },
            if report.execution_verified {
                "requested probes verified"
            } else {
                "not verified"
            }
        );
        for runtime in &report.runtimes {
            println!(
                "{} {} [{}]",
                runtime.tool,
                runtime.locked_version.as_deref().unwrap_or("unlocked"),
                runtime.selection_source
            );
        }
        for item in report
            .checks
            .iter()
            .chain(report.runtimes.iter().flat_map(|runtime| &runtime.checks))
        {
            if matches!(item.state, ReadinessState::Fail | ReadinessState::Unknown) {
                println!(
                    "{}: {}{}",
                    item.id,
                    item.reason,
                    item.next_step
                        .as_ref()
                        .map(|next| format!("; {next}"))
                        .unwrap_or_default()
                );
            }
        }
        for evidence in &report.evidence {
            println!(
                "{} {}: {:?} ({})",
                evidence.tool, evidence.entry, evidence.state, evidence.reason
            );
        }
    }
    Ok(
        if command == "check"
            && (!report.environment_ready || (probe && !report.execution_verified))
        {
            1
        } else {
            0
        },
    )
}

pub fn collect(
    cwd: &Path,
    profile: Option<&str>,
    no_env: bool,
) -> ReportResult<EnvironmentDescriptor> {
    let no_env = no_env || std::env::var_os("PINSET_ENV_DISABLE").is_some_and(|value| value == "1");
    let home = pinset_home()?;
    let path = find_optional_project_config(cwd)?;
    let mut report = EnvironmentDescriptor {
        schema: 2,
        cli_version: pinset_core::pinset_version().to_owned(),
        project_id: None,
        project_root: None,
        target: current_target(),
        host: host().to_owned(),
        profile: None,
        profile_source: if no_env { "disabled" } else { "none" }.to_owned(),
        context_fingerprint: None,
        runtimes: Vec::new(),
        checks: Vec::new(),
        evidence: Vec::new(),
        environment_ready: false,
        execution_verified: false,
        requirements: None,
        variables: Default::default(),
    };
    let Some(path) = path else {
        report.checks.push(check(
            "project",
            ReadinessState::Fail,
            "project_not_configured",
            Some("pinset setup"),
        ));
        return Ok(report);
    };
    let config = load_effective_project_config(&path)?;
    let root = path.parent().ok_or("project configuration has no parent")?;
    report.project_root = Some(fs::canonicalize(root)?.display().to_string());
    report.project_id = config.project_id.clone();
    report.requirements = config.requirements.clone();
    if let Some(environment) = &config.environment {
        report.variables = environment
            .variables
            .iter()
            .map(|(name, variable)| {
                (
                    name.clone(),
                    pinset_core::VariableRequirement {
                        kind: variable.kind,
                        required: variable.required,
                        secret: variable.secret,
                        profiles: variable.profiles.clone(),
                    },
                )
            })
            .collect();
    }
    if !no_env {
        let selection = environment_selection(&home, &path, &config, profile)?;
        report.profile = selection.profile;
        report.profile_source = selection.source.to_owned();
    }
    report.context_fingerprint = Some(fingerprint(root, report.profile.as_deref(), no_env)?);
    let lock = load_optional_lockfile(&lockfile_path(&path))?;
    report.checks.push(check(
        "project",
        ReadinessState::Pass,
        "configuration_loaded",
        None,
    ));
    report.checks.push(check(
        "lock",
        if lock.is_some() {
            ReadinessState::Pass
        } else {
            ReadinessState::Fail
        },
        if lock.is_some() {
            "lock_loaded"
        } else {
            "lock_missing"
        },
        if lock.is_none() {
            Some("pinset setup")
        } else {
            None
        },
    ));
    for (name, requested) in &config.tools {
        let locked = lock.as_ref().and_then(|lock| lock.tool(name));
        let provider = runtime_provider(name);
        let commands = provider
            .map(|provider| {
                provider
                    .commands
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let command = provider.map_or(name.as_str(), |provider| provider.commands[0]);
        let resolution = resolve_command(command, cwd, &home);
        let selection = resolve_tool_selection(name, cwd, &home).ok();
        let target = current_target_for_tool(name);
        let available = locked.is_some_and(|tool| {
            tool.artifacts
                .iter()
                .any(|artifact| artifact.target == target)
        });
        let mut checks = vec![check(
            "artifact",
            if available {
                ReadinessState::Pass
            } else {
                ReadinessState::Fail
            },
            if available {
                "target_artifact_locked"
            } else {
                "target_artifact_missing"
            },
            None,
        )];
        checks.push(check(
            "routing",
            if resolution.is_ok() {
                ReadinessState::Pass
            } else {
                ReadinessState::Fail
            },
            if resolution.is_ok() {
                "managed_command_resolved"
            } else {
                "managed_command_unavailable"
            },
            if resolution.is_err() {
                Some("pinset doctor --deep")
            } else {
                None
            },
        ));
        if name == "node"
            && let Some(node) = locked
        {
            checks.push(pinset_core::check_bundled_npm(
                &home
                    .join("installs/node")
                    .join(node.installation_version())
                    .join(&target),
                &node.version,
                &target,
            ));
        }
        report.runtimes.push(RuntimeDescriptor {
            tool: name.clone(),
            requested: requested.clone(),
            locked_version: locked.map(|tool| tool.version.clone()),
            installation_identity: locked.map(|tool| tool.installation_version()),
            selection_source: selection.map_or_else(
                || "project".to_owned(),
                |selection| selection.source.as_str().to_owned(),
            ),
            target,
            commands,
            executable: resolution
                .ok()
                .map(|resolution| resolution.executable.display().to_string()),
            checks,
            options: locked.map(|tool| tool.options.clone()).unwrap_or_default(),
            artifacts: locked
                .map(|tool| {
                    tool.artifacts
                        .iter()
                        .map(|artifact| {
                            let mut identities = vec![
                                artifact
                                    .artifact_integrity()
                                    .map(|value| value.canonical())
                                    .unwrap_or_default(),
                            ];
                            identities.extend(artifact.overlays.iter().map(|overlay| {
                                overlay
                                    .artifact_integrity()
                                    .map(|value| value.canonical())
                                    .unwrap_or_default()
                            }));
                            identities.sort();
                            (artifact.target.clone(), identities)
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    if let Some(lock) = &lock {
        let overrides = [
            "GOTOOLCHAIN",
            "GOWORK",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ]
        .into_iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| (name.to_owned(), value))
        })
        .collect();
        report
            .checks
            .extend(pinset_core::check_compatibility(root, lock, &overrides));
        if let Some(requirements) = &config.requirements {
            for platform in &requirements.platforms {
                for tool in &lock.tools {
                    let present = !pinset_core::artifacts_for_platform(tool, platform).is_empty();
                    report.checks.push(check(
                        &format!("platform.{}.{}", tool.name, platform),
                        if present {
                            ReadinessState::Pass
                        } else {
                            ReadinessState::Fail
                        },
                        if present {
                            "required_platform_artifact_locked_execution_not_verified"
                        } else {
                            "required_platform_artifact_missing"
                        },
                        Some("pinset check --delivery --offline"),
                    ));
                }
            }
        }
    }
    let audit = pinset_core::audit_project_environment(&home, cwd);
    for finding in audit.findings {
        if finding.severity == pinset_core::LockAuditSeverity::Error {
            report.checks.push(check(
                finding.reason_code.as_str(),
                ReadinessState::Fail,
                "project_audit_failed",
                Some("pinset doctor --deep"),
            ));
        }
    }
    let environment = if no_env || config.environment.is_none() {
        check(
            "environment",
            ReadinessState::NotApplicable,
            "project_environment_not_requested",
            None,
        )
    } else if report.profile.is_some() {
        check(
            "environment",
            ReadinessState::Unknown,
            "encrypted_environment_not_opened",
            Some("pinset env check"),
        )
    } else if config.environment.as_ref().is_some_and(|environment| {
        environment
            .variables
            .values()
            .any(|variable| variable.required)
    }) {
        check(
            "environment",
            ReadinessState::Fail,
            "required_environment_profile_not_selected",
            Some("pinset env use <profile>"),
        )
    } else {
        check(
            "environment",
            ReadinessState::NotApplicable,
            "no_profile_selected",
            None,
        )
    };
    report.checks.push(environment);
    if !no_env
        && let Some(profile) = report.profile.as_deref()
        && let Some(environment) = &config.environment
    {
        let trust = config.project_id.as_deref().map(|id| {
            pinset_env::verify_project_trust(
                &home,
                root,
                id,
                &toml::to_string(environment).unwrap_or_default(),
            )
        });
        let (trusted, reason) = match trust {
            Some(Ok(())) => (ReadinessState::Pass, "project_trusted"),
            Some(Err(pinset_env::Error::TrustMissing)) => (ReadinessState::Fail, "trust_missing"),
            Some(Err(pinset_env::Error::TrustChanged)) => (ReadinessState::Fail, "trust_changed"),
            _ => (ReadinessState::Unknown, "trust_not_established"),
        };
        report.checks.push(check(
            "environment.trust",
            trusted,
            reason,
            Some("pinset trust add --project-id <reviewed-project-id>"),
        ));
        let identity = std::env::var_os("PINSET_IDENTITY").is_some_and(|value| !value.is_empty())
            || std::env::var_os("PINSET_IDENTITY_FILE").is_some()
            || pinset_env::list_identities(&home).is_ok_and(|identities| !identities.is_empty());
        report.checks.push(check(
            "environment.identity",
            if identity {
                ReadinessState::Unknown
            } else {
                ReadinessState::Fail
            },
            if identity {
                "identity_registered_not_opened"
            } else {
                "identity_missing"
            },
            Some("pinset env identity list"),
        ));
        for (name, variable) in &environment.variables {
            if variable.profiles.is_empty()
                || variable.profiles.iter().any(|value| value == profile)
            {
                report.checks.push(check(
                    &format!("environment.variable.{name}"),
                    ReadinessState::Unknown,
                    "variable_not_decrypted",
                    Some("pinset env check"),
                ));
            }
        }
    }
    if let Some(requirements) = &config.requirements {
        let values = [
            "ANDROID_HOME",
            "ANDROID_SDK_ROOT",
            "LOCALAPPDATA",
            "HOME",
            "ProgramFiles(x86)",
            "VSINSTALLDIR",
            "WindowsSdkDir",
            "DEVELOPER_DIR",
            "PATH",
        ]
        .into_iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| (name.to_owned(), value))
        })
        .collect();
        report.checks.extend(pinset_core::check_build_conditions(
            &requirements.build_targets,
            &report.target,
            &values,
        ));
        for check in report.checks.iter_mut().chain(
            report
                .runtimes
                .iter_mut()
                .flat_map(|runtime| &mut runtime.checks),
        ) {
            if requirements.disabled_rules.contains(&check.id) {
                check.state = ReadinessState::NotApplicable;
                check.reason = "individual_rule_disabled_by_project_requirements".to_owned();
            }
        }
    }
    report.update_readiness();
    Ok(report)
}

/// Explicit execution may check a trusted environment; background collection never decrypts it.
pub fn verify_environment(cwd: &Path, report: &mut EnvironmentDescriptor, no_env: bool) {
    if no_env || report.profile_source == "disabled" || report.profile.is_none() {
        return;
    }
    let result = crate::environment::resolve_environment(cwd, report.profile.as_deref()).map(
        |(_, mut values)| {
            use zeroize::Zeroize;
            for value in values.values_mut() {
                value.zeroize();
            }
        },
    );
    let reason = match &result {
        Ok(()) => "trusted_environment_contract_valid",
        Err(error) => error
            .downcast_ref::<crate::environment::ContractError>()
            .map_or_else(
                || crate::json_error(error.as_ref()).0,
                |error| error.reason(),
            ),
    };
    let valid = result.is_ok();
    let issues = result
        .as_ref()
        .err()
        .and_then(|error| error.downcast_ref::<crate::environment::ContractError>());
    if valid || issues.is_some() {
        for item in &mut report.checks {
            if item.id == "environment.identity" {
                item.state = ReadinessState::Pass;
                item.reason = "identity_decrypted_selected_profile".to_owned();
            }
            if let Some(name) = item.id.strip_prefix("environment.variable.") {
                let issue = issues.and_then(|issues| issues.issue_for(name));
                item.state = if issue.is_some() {
                    ReadinessState::Fail
                } else {
                    ReadinessState::Pass
                };
                item.reason = issue.unwrap_or("variable_contract_satisfied").to_owned();
            }
        }
    }
    if let Some(item) = report
        .checks
        .iter_mut()
        .find(|item| item.id == "environment")
    {
        *item = check(
            "environment",
            if valid {
                ReadinessState::Pass
            } else {
                ReadinessState::Fail
            },
            reason,
            if valid {
                None
            } else {
                Some("pinset env check")
            },
        );
    }
    report.update_readiness();
}

pub fn probe_report(
    cwd: &Path,
    report: &mut EnvironmentDescriptor,
    profile: Option<&str>,
    no_env: bool,
) -> ReportResult<()> {
    // A failed trust/identity/contract check must not launch project-context probes.
    if report
        .checks
        .iter()
        .any(|item| item.id == "environment" && item.state == ReadinessState::Fail)
    {
        return Ok(());
    }
    crate::probes::collect(cwd, report)?;
    let current = collect(cwd, profile, no_env)?;
    if current.context_fingerprint != report.context_fingerprint {
        for item in &mut report.evidence {
            item.state = ReadinessState::Unknown;
            item.reason = "project_changed_during_probe".to_owned();
        }
        report.checks.push(check(
            "probe_context",
            ReadinessState::Fail,
            "project_changed_during_probe",
            Some("pinset check --probe"),
        ));
        report.update_readiness();
    }
    Ok(())
}
