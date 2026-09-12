//! Opt-in bounded probes. No project tasks or lifecycle scripts are implicitly executed.
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::readiness::ReportResult;
use pinset_core::{
    EnvironmentDescriptor, ExecutionEvidence, ReadinessState, managed_runtime_arguments,
};

const LIMIT: u64 = 64 * 1024;

pub fn collect(cwd: &Path, report: &mut EnvironmentDescriptor) -> ReportResult<()> {
    for runtime in &report.runtimes {
        let Some(executable) = runtime.executable.as_deref().map(Path::new) else {
            if matches!(runtime.tool.as_str(), "node" | "python" | "flutter") {
                report.evidence.push(evidence(
                    report.context_fingerprint.as_deref(),
                    "managed-command",
                    &runtime.tool,
                    ReadinessState::Unknown,
                    None,
                    None,
                    None,
                    "managed_runtime_unavailable",
                ));
            }
            continue;
        };
        let arguments: Vec<&str> = match runtime.tool.as_str() {
            "node" => vec![
                "-e",
                "console.log(JSON.stringify({executable:process.execPath,version:process.versions.node}))",
            ],
            "python" => vec![
                "-E",
                "-S",
                "-c",
                "import sys,json; print(json.dumps(dict(executable=sys.executable,version='.'.join(map(str,sys.version_info[:3])),prefix=sys.prefix,base_prefix=getattr(sys,'base_prefix',sys.prefix))))",
            ],
            // Flutter version checks can refresh/write its cache. Require a declared task instead.
            "flutter" => {
                report.evidence.push(evidence(
                    report.context_fingerprint.as_deref(),
                    "managed-command",
                    &runtime.tool,
                    ReadinessState::Unknown,
                    Some(executable),
                    None,
                    None,
                    "flutter_sdk_probe_requires_explicit_task",
                ));
                continue;
            }
            _ => continue,
        };
        let context = pinset_core::execution_context(
            &runtime.tool,
            executable,
            cwd,
            &pinset_core::pinset_home()?,
        )?;
        let mut command = Command::new(executable);
        command.current_dir(cwd).env("PATH", context.path);
        for value in context.environment {
            command.env(value.name, value.value);
        }
        for name in context.remove_environment {
            command.env_remove(name);
        }
        // Avoid arbitrary startup code from inherited Node/Python configuration.
        for name in [
            "NODE_OPTIONS",
            "NODE_PATH",
            "PYTHONSTARTUP",
            "PYTHONPATH",
            "PYTHONINSPECT",
        ] {
            command.env_remove(name);
        }
        if runtime.tool == "python" {
            command.env("PYTHONNOUSERSITE", "1");
        }
        command.args(managed_runtime_arguments(
            &runtime.tool,
            &runtime.tool,
            &arguments
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
        ));
        let result = capture(command, Duration::from_secs(10));
        let observation = result
            .as_ref()
            .ok()
            .and_then(|output| serde_json::from_slice::<serde_json::Value>(output).ok());
        let actual = observation
            .as_ref()
            .and_then(|value| value.get("executable"))
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from);
        let version = observation
            .as_ref()
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let matches = actual.as_deref().is_some_and(|actual| {
            if runtime.tool == "python" {
                same_python_invocation(executable, actual)
            } else {
                same_executable(executable, actual)
            }
        }) && version
            .as_deref()
            .zip(runtime.locked_version.as_deref())
            .is_some_and(|(actual, locked)| {
                locked == actual || locked.starts_with(&format!("{actual}+"))
            });
        report.evidence.push(evidence(
            report.context_fingerprint.as_deref(),
            "managed-command",
            &runtime.tool,
            if matches {
                ReadinessState::Pass
            } else {
                ReadinessState::Fail
            },
            Some(executable),
            actual.as_deref(),
            version,
            if matches {
                "observed_selected_executable"
            } else {
                "probe_failed_or_executable_mismatch"
            },
        ));
    }
    report.update_readiness();
    Ok(())
}

fn same_executable(expected: &Path, actual: &Path) -> bool {
    let expected = std::fs::canonicalize(expected).ok();
    let actual = std::fs::canonicalize(actual).ok();
    expected.is_some() && expected == actual
}

// A venv executable can be a symlink to its base interpreter. Resolving that final symlink
// would incorrectly accept a process launched directly through the base interpreter.
fn same_python_invocation(expected: &Path, actual: &Path) -> bool {
    let parent = |path: &Path| {
        path.parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok())
    };
    let filename = |path: &Path| {
        path.file_name().map(|name| {
            let name = name.to_string_lossy();
            if cfg!(windows) {
                name.to_ascii_lowercase()
            } else {
                name.into_owned()
            }
        })
    };
    parent(expected).is_some()
        && parent(expected) == parent(actual)
        && filename(expected) == filename(actual)
}

#[allow(clippy::too_many_arguments)]
fn evidence(
    fingerprint: Option<&str>,
    entry: &str,
    tool: &str,
    state: ReadinessState,
    expected: Option<&Path>,
    actual: Option<&Path>,
    version: Option<String>,
    reason: &str,
) -> ExecutionEvidence {
    ExecutionEvidence {
        entry: entry.to_owned(),
        tool: tool.to_owned(),
        state,
        expected_executable: expected.map(|path| path.display().to_string()),
        observed_executable: actual.map(|path| path.display().to_string()),
        observed_version: version,
        observed_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        context_fingerprint: fingerprint.map(str::to_owned),
        reason: reason.to_owned(),
    }
}

fn terminate(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Readers are bounded and communicate over a channel; inherited pipes cannot block timeout.
fn capture(mut command: Command, timeout: Duration) -> ReportResult<Vec<u8>> {
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
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let output = child.stdout.take().ok_or("missing probe stdout")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let started = Instant::now();
    let mut bytes = None;
    loop {
        if let Ok(result) = receiver.try_recv() {
            let value = match result {
                Ok(value) => value,
                Err(error) => {
                    terminate(&mut child);
                    return Err(error.into());
                }
            };
            if value.len() as u64 > LIMIT {
                terminate(&mut child);
                return Err("probe output exceeded 64 KiB".into());
            }
            bytes = Some(value);
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate(&mut child);
                return Err(error.into());
            }
        };
        if let Some(status) = status {
            if !status.success() {
                terminate(&mut child);
                return Err("probe process failed".into());
            }
            if let Some(bytes) = bytes {
                return Ok(bytes);
            }
        }
        if started.elapsed() >= timeout {
            terminate(&mut child);
            return Err("probe timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_paths_never_count_as_matching() {
        assert!(!same_executable(
            Path::new("missing-first-probe"),
            Path::new("missing-second-probe")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn venv_and_base_interpreter_are_distinct_even_when_symlinked() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("base");
        let venv = root.path().join("venv");
        std::fs::create_dir(&base).unwrap();
        std::fs::create_dir(&venv).unwrap();
        let base_python = base.join("python");
        let venv_python = venv.join("python");
        std::fs::write(&base_python, "fixture").unwrap();
        std::os::unix::fs::symlink(&base_python, &venv_python).unwrap();
        assert!(same_executable(&venv_python, &base_python));
        assert!(!same_python_invocation(&venv_python, &base_python));
        assert!(same_python_invocation(&venv_python, &venv_python));
    }

    #[cfg(unix)]
    #[test]
    fn probe_timeout_kills_inherited_pipe_processes() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 10 & wait"]);
        let start = Instant::now();
        assert!(capture(command, Duration::from_millis(100)).is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn probe_output_is_bounded() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "yes x"]);
        assert!(capture(command, Duration::from_secs(2)).is_err());
    }
}
