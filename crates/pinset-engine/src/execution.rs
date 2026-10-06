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
            let command = if tool.name == "rust" {
                "rustc"
            } else {
                &tool.name
            };
            if let Ok(plan) = plan_command(&self.cwd, &self.home, &c, command) {
                check.bound = true;
                if deep {
                    let root = install_directory(&self.home, tool, &platform);
                    let (count, size) = install_payload_statistics(&root)?;
                    let r = receipt.as_ref().unwrap();
                    // Receipt itself is excluded by the install statistics routine.
                    if (tool.name != "flutter" && (count != r.file_count || size != r.total_size))
                        || install_payload_fingerprint(&root, &tool.name)? != r.payload_digest
                    {
                        check.installed = false;
                        check.detail = "installation payload size or file count changed".into();
                    }
                    if tool.name == "java" {
                        let sdk = sdk_root(&root, "java", &platform);
                        for entry in ["release", "include/jni.h"] {
                            if !sdk.join(entry).exists() {
                                check.installed = false;
                                check.detail = format!("JDK missing {entry}");
                            }
                        }
                    }
                }
                if probe && check.installed {
                    let args=match tool.name.as_str(){"python"=>vec!["-I".into(),"-c".into(),"import sys; print(sys.version.split()[0]); print(sys.executable); print(sys.prefix)".into()],"java"=>vec!["-version".into()],"go"=>vec!["version".into()],_=>vec!["--version".into()]};
                    let mut cmd = prepare_command(&plan, &args, &self.cwd, BTreeMap::new());
                    cmd.env("PINSET_NO_ENV", "1");
                    let output =
                        crate::process_tree::run(cmd, Duration::from_secs(30), Some(1024 * 1024))
                            .map_err(|e| service_error("PINSET_PROBE_FAILED", e.to_string()))?;
                    let observed = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    let version = if tool.name == "java" {
                        java_probe_version(&observed)
                    } else {
                        observed
                            .split_whitespace()
                            .find(|s| {
                                s.trim_start_matches(['v', 'g', 'o'])
                                    .starts_with(char::is_numeric)
                            })
                            .map(|s| s.trim_start_matches('v').to_owned())
                    };
                    let exact = version.as_deref().is_some_and(|v| {
                        if tool.name == "rust" {
                            rust_probe_matches(tool, &observed)
                        } else if tool.name == "python" {
                            tool.version.split('+').next() == Some(v)
                        } else if tool.name == "java" {
                            v == tool.version
                        } else {
                            v == tool.version || v.trim_start_matches("go") == tool.version
                        }
                    });
                    check.actually_verified = output.code == 0 && exact;
                    report.evidence.push(ExecutionEvidence {
                        host: current_target(),
                        command: command.into(),
                        executable: plan.executable,
                        version: tool.version.clone(),
                        observed,
                        timestamp: now(),
                        success: check.actually_verified,
                        scope: "explicit-entry-probe".into(),
                    });
                    if !check.actually_verified {
                        check.detail =
                            "probe failed or the observed version differs from the lock".into();
                    }
                }
            } else {
                check.detail = "selected command binding is unavailable".into();
            }
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
        let value = json!({"protocol":PROTOCOL,"report":report,"java":java,"android":android,"target":target,"build_target":target.map(|requested|json!({"target":requested,"host":current_target(),"build_observed":false,"scope":"selected toolchain and prerequisites; no application build performed"}))});
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
