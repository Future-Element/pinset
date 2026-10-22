pub use crate::process_tree::worker as process_worker;
use crate::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, env, path::Path, process::Command, time::Duration};
use zeroize::Zeroize;

pub(crate) fn prepare_command(
    plan: &CommandPlan,
    args: &[String],
    cwd: &Path,
    values: BTreeMap<String, String>,
) -> Command {
    let mut command = Command::new(&plan.executable);
    command
        .args(&plan.prefix)
        .args(args)
        .current_dir(cwd)
        .env_clear();
    for (k, v) in env::vars_os() {
        if !plan
            .remove_environment
            .iter()
            .any(|name| k.to_string_lossy().eq_ignore_ascii_case(name))
        {
            command.env(k, v);
        }
    }
    command.envs(&plan.environment);
    for (k, mut v) in values {
        command.env(k, &v);
        v.zeroize();
    }
    command
}
impl Services {
    pub fn exec(&self, args: &[String], profile: Option<&str>, disabled: bool) -> Result<i32> {
        let c = self.context(false)?;
        let plan = plan_command(&self.cwd, &self.home, &c, &args[0])?;
        let env = self.resolve_profile(&c, profile, disabled)?;
        let mut command = prepare_command(&plan, &args[1..], &self.cwd, env);
        if disabled {
            command.env("PINSET_NO_ENV", "1");
        }
        Ok(command.status()?.code().unwrap_or(1))
    }
    pub fn check(
        &self,
        global: bool,
        deep: bool,
        probe: bool,
        target: Option<&str>,
    ) -> Result<Value> {
        let c = self.context(global)?;
        if target.is_some() && c.global {
            return Err(service_error(
                "PINSET_PROJECT_REQUIRED",
                "build target checks require a project",
            ));
        }
        let (config, lock) = self.load(&c)?;
        let mut report = EnvironmentReport {
            protocol: PROTOCOL.into(),
            project: (!c.global).then(|| c.root.clone()),
            checks: vec![],
            evidence: vec![],
        };
        for tool in &lock.tools {
            let platform = locked_target(tool);
            let receipt = read_receipt(&self.home, tool, &platform);
            let mut check = EnvironmentCheck {
                tool: tool.name.clone(),
                expected_version: tool.version.clone(),
                actual_version: None,
                artifact_digest: ArtifactDigestCheck {
                    expected: tool
                        .artifact(&platform)
                        .map(|artifact| {
                            artifact
                                .artifact_integrity()
                                .map(|digest| digest.canonical())
                        })
                        .transpose()?,
                    receipt_matches_lock: receipt.is_ok(),
                    payload_verified: None,
                    passed: receipt.is_ok(),
                },
                passed: false,
                configured: true,
                installed: receipt.is_ok(),
                bound: false,
                actually_verified: false,
                detail: receipt
                    .as_ref()
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "exact installation receipt matches".into()),
            };
            if deep && let Ok(receipt) = &receipt {
                match check_payload(&self.home, tool, &platform, receipt) {
                    Ok(matches) => {
                        check.artifact_digest.payload_verified = Some(matches);
                        if !matches {
                            check.installed = false;
                            check.detail =
                                "installation payload digest, size or file count changed".into();
                        }
                    }
                    Err(error) => {
                        check.artifact_digest.payload_verified = Some(false);
                        check.installed = false;
                        check.detail = error.to_string();
                    }
                }
                check.artifact_digest.passed = check.artifact_digest.payload_verified == Some(true);
            }
            let command = if tool.name == "rust" {
                "rustc"
            } else {
                &tool.name
            };
            let plan_result = plan_command(&self.cwd, &self.home, &c, command);
            if let Ok(plan) = plan_result {
                check.bound = true;
                if probe && check.installed {
                    let args=match tool.name.as_str(){"python"=>vec!["-I".into(),"-c".into(),"import sys,site,json; print(json.dumps(dict(version=sys.version.split()[0], executable=sys.executable, prefix=sys.prefix, base_prefix=sys.base_prefix, base_executable=sys._base_executable, user_site=site.ENABLE_USER_SITE)))".into()],"java"=>vec!["-version".into()],"go"=>vec!["version".into()],_=>vec!["--version".into()]};
                    let mut cmd = prepare_command(&plan, &args, &self.cwd, BTreeMap::new());
                    cmd.env("PINSET_NO_ENV", "1");
                    let output =
                        crate::process_tree::run(cmd, Duration::from_secs(30), Some(1024 * 1024));
                    let (code, observed) = match output {
                        Ok(output) => (
                            output.code,
                            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                        ),
                        Err(error) => (-1, format!("PINSET_PROBE_FAILED: {error}")),
                    };
                    let version = (code != -1)
                        .then(|| probe_version(&tool.name, &observed))
                        .flatten();
                    let exact = version.as_deref().is_some_and(|v| {
                        if tool.name == "rust" {
                            rust_probe_matches(tool, &observed)
                        } else if tool.name == "python" {
                            tool.version.split('+').next() == Some(v)
                                && python_probe_matches(tool, &plan, &c, &observed)
                        } else if tool.is_generic() {
                            v == tool.version.strip_prefix('v').unwrap_or(&tool.version)
                        } else {
                            v == tool.version
                        }
                    });
                    check.actual_version = version;
                    check.actually_verified = code == 0 && exact;
                    report.evidence.push(ExecutionEvidence {
                        host: current_target(),
                        command: command.into(),
                        executable: plan.executable,
                        version: tool.version.clone(),
                        observed: observed.clone(),
                        timestamp: now(),
                        success: check.actually_verified,
                        scope: "explicit-entry-probe".into(),
                    });
                    if !check.actually_verified {
                        check.detail = if code == -1 {
                            observed.clone()
                        } else {
                            "probe failed or the observed version differs from the lock".into()
                        };
                    }
                }
            } else {
                check.detail = plan_result.unwrap_err().to_string();
            }
            check.passed = check.configured
                && check.installed
                && check.bound
                && check.artifact_digest.passed
                && (!probe || check.actually_verified);
            report.checks.push(check);
        }
        let java = if config.tools.contains_key("java") {
            Some(self.java_build_report(&c, &lock, probe)?)
        } else {
            None
        };
        let android = if target == Some("android") {
            Some(self.android_report(&c, &lock, probe)?)
        } else {
            None
        };
        let passed = report.checks.iter().all(|check| check.passed);
        let value = json!({"protocol":PROTOCOL,"schema_version":1,"passed":passed,"report":report,"java":java,"android":android,"target":target,"build_target":target.map(|requested|json!({"target":requested,"host":current_target(),"build_observed":false,"scope":"selected toolchain and prerequisites; no application build performed"}))});
        if probe {
            write_json(
                &self.home.join("state/evidence").join(format!(
                    "{}-{}.json",
                    config.project_id,
                    now()
                )),
                &value,
            )?;
        }
        Ok(value)
    }
}

fn check_payload(
    home: &Path,
    tool: &LockedTool,
    platform: &str,
    receipt: &InstallReceipt,
) -> Result<bool> {
    let root = install_directory(home, tool, platform);
    let (count, size) = install_payload_statistics(&root)?;
    // Mutable Flutter state is excluded by its fingerprint routine.
    let mut matches = (tool.name == "flutter"
        || (count == receipt.file_count && size == receipt.total_size))
        && install_payload_fingerprint(&root, &tool.name)? == receipt.payload_digest;
    if tool.name == "java" {
        let sdk = sdk_root(&root, "java", platform);
        matches &= ["release", "include/jni.h"]
            .iter()
            .all(|entry| sdk.join(entry).exists());
    }
    Ok(matches)
}

fn probe_version(tool: &str, observed: &str) -> Option<String> {
    match tool {
        "python" => serde_json::from_str::<Value>(observed)
            .ok()
            .and_then(|value| value["version"].as_str().map(str::to_owned)),
        "java" => java_probe_version(observed),
        _ => observed.split_whitespace().find_map(|token| {
            let version = if tool == "go" {
                token.strip_prefix("go").unwrap_or(token)
            } else {
                token.trim_start_matches('v')
            };
            version
                .starts_with(char::is_numeric)
                .then(|| version.to_owned())
        }),
    }
}

fn same_path(observed: &str, expected: &Path) -> bool {
    Path::new(observed)
        .canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(a, b)| a == b)
}
fn python_probe_matches(
    tool: &LockedTool,
    plan: &CommandPlan,
    context: &ProjectContext,
    observed: &str,
) -> bool {
    let Ok(info) = serde_json::from_str::<Value>(observed) else {
        return false;
    };
    let Some(sdk) = &plan.sdk else {
        return false;
    };
    let prefix = if context.global {
        sdk.clone()
    } else {
        context.root.join(".venv")
    };
    let expected_base = interpreter_path(sdk, &locked_target(tool));
    info["user_site"] == false
        && info["executable"]
            .as_str()
            .is_some_and(|p| same_path(p, &plan.executable))
        && info["prefix"]
            .as_str()
            .is_some_and(|p| same_path(p, &prefix))
        && info["base_prefix"]
            .as_str()
            .is_some_and(|p| same_path(p, sdk))
        && info["base_executable"]
            .as_str()
            .is_some_and(|p| same_path(p, &expected_base))
}

fn rust_probe_matches(tool: &LockedTool, observed: &str) -> bool {
    observed
        .trim()
        .strip_prefix("rustc ")
        .is_some_and(|version| {
            tool.metadata
                .get("compiler_version")
                .is_some_and(|expected| expected == version)
        })
}

pub fn java_probe_version(output: &str) -> Option<String> {
    let build = output
        .lines()
        .find(|line| line.contains("Runtime Environment") && line.contains("(build "))?
        .split("(build ")
        .nth(1)?
        .split(')')
        .next()?;
    if let Some(old) = build.strip_prefix("1.8.0_") {
        let (update, build) = old.split_once("-b")?;
        Some(format!("8.0.{update}+{}", build.parse::<u64>().ok()?))
    } else {
        let value = build.split('-').next()?;
        let (version, number) = value.split_once('+')?;
        let mut parts = version.split('.').collect::<Vec<_>>();
        while parts.len() < 3 {
            parts.push("0");
        }
        let normalized = format!("{}+{number}", parts.join("."));
        crate::JavaVersion::parse(&normalized)
            .ok()
            .map(|v| v.to_string())
    }
}
#[cfg(test)]
mod probe_tests {
    use super::*;
    #[test]
    fn normalizes_observed_versions_without_claiming_unparseable_output() {
        assert_eq!(
            probe_version("go", "go version go1.26.0 linux/amd64"),
            Some("1.26.0".into())
        );
        assert_eq!(probe_version("node", "v22.12.0"), Some("22.12.0".into()));
        assert_eq!(
            probe_version("terraform", "Terraform v1.9.0\non linux_amd64"),
            Some("1.9.0".into())
        );
        assert_eq!(
            probe_version("rust", "rustc 1.97.1-nightly (123456abc 2026-07-15)"),
            Some("1.97.1-nightly".into())
        );
        assert_eq!(
            probe_version("python", r#"{"version":"3.14.0"}"#),
            Some("3.14.0".into())
        );
        assert_eq!(
            probe_version("example", "unsupported flag: --version"),
            None
        );
        assert_eq!(probe_version("python", "Python failed to initialize"), None);
    }
    #[test]
    fn rust_probe_rejects_another_nightly_with_the_same_release_number() {
        let mut tool = LockedTool {
            name: "rust".into(),
            requested: "nightly-2026-07-16".into(),
            version: "1.97.1".into(),
            provider: "rust-official".into(),
            released_at: None,
            artifacts: vec![],
            options: BTreeMap::new(),
            metadata: BTreeMap::from([(
                "compiler_version".into(),
                "1.97.1-nightly (123456abc 2026-07-15)".into(),
            )]),
        };
        assert!(rust_probe_matches(
            &tool,
            "rustc 1.97.1-nightly (123456abc 2026-07-15)\n"
        ));
        assert!(!rust_probe_matches(
            &tool,
            "rustc 1.97.1-nightly (987654fed 2026-07-16)"
        ));
        assert!(!rust_probe_matches(
            &tool,
            "rustc 1.97.1 (123456abc 2026-07-15)"
        ));
        tool.metadata.clear();
        assert!(!rust_probe_matches(
            &tool,
            "rustc 1.97.1-nightly (123456abc 2026-07-15)"
        ));
    }

    #[test]
    fn java_probe_includes_build_and_normalizes_eight() {
        assert_eq!(
            java_probe_version(
                "openjdk version \"1.8.0_504\"\nOpenJDK Runtime Environment (Temurin)(build 1.8.0_504-b01)"
            ),
            Some("8.0.504+1".into())
        );
        assert_eq!(
            java_probe_version(
                "OpenJDK Runtime Environment Temurin-21.0.12.1+1 (build 21.0.12.1+1-LTS)"
            ),
            Some("21.0.12.1+1".into())
        );
        assert!(java_probe_version("openjdk version \"21.0.12\"").is_none());
        assert_eq!(
            java_probe_version("OpenJDK Runtime Environment Temurin-27+35 (build 27+35)"),
            Some("27.0.0+35".into())
        );
    }
}
