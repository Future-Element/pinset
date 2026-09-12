use std::{
    collections::BTreeSet,
    fs,
    io::{Error as IoError, ErrorKind, Write},
    path::Path,
};

use atomic_write_file::AtomicWriteFile;
use pinset_core::{
    LockAuditCategory, LockAuditReasonCode, LockAuditSeverity, audit_project_lock, current_target,
    environment_selection, find_optional_project_config, load_effective_project_config,
    load_optional_lockfile, lockfile_path, pinset_home,
};
use serde::{Deserialize, Serialize};

const REPORT_SCHEMA: u32 = 1;
const MAX_REPORT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticReport {
    pub schema: u32,
    pub pinset_version: String,
    pub platform: DiagnosticPlatform,
    pub project: DiagnosticProject,
    pub environment: DiagnosticEnvironment,
    pub tools: Vec<DiagnosticTool>,
    pub summary: DiagnosticSummary,
    pub findings: Vec<DiagnosticFinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_preview: Vec<DiagnosticRepair>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPlatform {
    pub os: String,
    pub arch: String,
    pub target: String,
    pub ci: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticProject {
    pub configured: bool,
    pub config_schema: Option<u32>,
    pub lock_schema: Option<u32>,
    pub tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEnvironment {
    pub configured: bool,
    pub selected_profile: Option<String>,
    pub selection_source: String,
    pub profiles: usize,
    pub variables: usize,
    pub secret_variables: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticTool {
    pub name: String,
    pub requested: String,
    pub locked_version: Option<String>,
    pub provider: Option<String>,
    pub current_target_artifact: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSummary {
    pub passed: bool,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticFinding {
    pub code: String,
    pub severity: String,
    pub category: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRepair {
    pub code: String,
    pub action: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticComparison {
    pub changed: bool,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticOutput {
    pub report: DiagnosticReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<DiagnosticComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
}

pub fn collect(
    cwd: &Path,
    repair_preview: bool,
) -> Result<DiagnosticReport, Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let audit = audit_project_lock(&home, cwd);
    collect_with_audit(cwd, repair_preview, audit)
}

pub fn collect_environment(cwd: &Path) -> Result<DiagnosticReport, Box<dyn std::error::Error>> {
    let audit = pinset_core::audit_project_environment(&pinset_home()?, cwd);
    collect_with_audit(cwd, false, audit)
}

fn collect_with_audit(
    cwd: &Path,
    repair_preview: bool,
    audit: pinset_core::LockAuditReport,
) -> Result<DiagnosticReport, Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let config_path = find_optional_project_config(cwd)?;
    let config = config_path
        .as_deref()
        .map(load_effective_project_config)
        .transpose()?;
    let lock = config_path
        .as_deref()
        .map(lockfile_path)
        .as_deref()
        .map(load_optional_lockfile)
        .transpose()?
        .flatten();

    let environment = if let (Some(path), Some(config)) = (&config_path, &config) {
        let selection = environment_selection(&home, path, config, None)?;
        let declared = config.environment.as_ref();
        DiagnosticEnvironment {
            configured: declared.is_some(),
            selected_profile: selection.profile,
            selection_source: selection.source.to_owned(),
            profiles: declared.map_or(0, |value| value.profiles.len()),
            variables: declared.map_or(0, |value| value.variables.len()),
            secret_variables: declared.map_or(0, |value| {
                value
                    .variables
                    .values()
                    .filter(|contract| contract.secret)
                    .count()
            }),
        }
    } else {
        DiagnosticEnvironment {
            configured: false,
            selected_profile: None,
            selection_source: "none".to_owned(),
            profiles: 0,
            variables: 0,
            secret_variables: 0,
        }
    };

    let mut tools = Vec::new();
    if let Some(config) = &config {
        for (name, requested) in &config.tools {
            let locked = lock.as_ref().and_then(|value| value.tool(name));
            tools.push(DiagnosticTool {
                name: name.clone(),
                requested: requested.clone(),
                locked_version: locked.map(|value| value.version.clone()),
                provider: locked.map(|value| value.provider.clone()),
                current_target_artifact: locked.is_some_and(|value| {
                    let target = pinset_core::current_target_for_tool(name);
                    value
                        .artifacts
                        .iter()
                        .any(|artifact| artifact.target == target)
                }),
            });
        }
    }

    let findings = audit
        .findings
        .iter()
        .map(|finding| DiagnosticFinding {
            code: finding.reason_code.as_str().to_owned(),
            severity: severity_name(finding.severity).to_owned(),
            category: category_name(finding.category).to_owned(),
            subject: finding.subject.clone(),
        })
        .collect::<Vec<_>>();
    let repairs = if repair_preview {
        let mut seen = BTreeSet::new();
        audit
            .findings
            .iter()
            .filter_map(|finding| {
                let command = repair_command(finding.reason_code)?;
                let key = (finding.reason_code.as_str(), command);
                seen.insert(key).then(|| DiagnosticRepair {
                    code: finding.reason_code.as_str().to_owned(),
                    action: finding
                        .repair
                        .as_ref()
                        .map_or("Review the diagnostic finding", |repair| {
                            repair.action.as_str()
                        })
                        .to_owned(),
                    command: command.to_owned(),
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(DiagnosticReport {
        schema: REPORT_SCHEMA,
        pinset_version: pinset_core::pinset_version().to_owned(),
        platform: DiagnosticPlatform {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            target: current_target(),
            ci: is_ci(),
        },
        project: DiagnosticProject {
            configured: config.is_some(),
            config_schema: config.as_ref().map(|value| value.schema),
            lock_schema: lock.as_ref().map(|value| value.schema),
            tasks: config.as_ref().map_or(0, |value| value.tasks.len()),
        },
        environment,
        tools,
        summary: DiagnosticSummary {
            passed: audit.passed,
            errors: audit.summary.errors,
            warnings: audit.summary.warnings,
            info: audit.summary.info,
        },
        findings,
        repair_preview: repairs,
    })
}

pub fn save(path: &Path, report: &DiagnosticReport) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "diagnostic report destination must be a regular file",
        )
        .into());
    }
    if let Some(parent) = path.parent().filter(|value| !value.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(report)?;
    let mut file = AtomicWriteFile::options().open(path)?;
    file.write_all(&content)?;
    file.write_all(b"\n")?;
    file.commit()?;
    Ok(())
}

pub fn load(path: &Path) -> Result<DiagnosticReport, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_REPORT_BYTES
    {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "diagnostic report must be a regular file of at most 1 MiB",
        )
        .into());
    }
    let report: DiagnosticReport = serde_json::from_slice(&fs::read(path)?)?;
    if report.schema != REPORT_SCHEMA {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "unsupported diagnostic report schema {}; expected {REPORT_SCHEMA}",
                report.schema
            ),
        )
        .into());
    }
    Ok(report)
}

pub fn compare(previous: &DiagnosticReport, current: &DiagnosticReport) -> DiagnosticComparison {
    let previous = serde_json::to_value(previous).expect("diagnostic report is serializable");
    let current = serde_json::to_value(current).expect("diagnostic report is serializable");
    let mut paths = Vec::new();
    collect_changes("", &previous, &current, &mut paths);
    DiagnosticComparison {
        changed: !paths.is_empty(),
        paths,
    }
}

pub fn human_lines(output: &DiagnosticOutput) -> Vec<String> {
    let report = &output.report;
    let mut lines = vec![
        format!(
            "diagnostic schema={} version={} target={} ci={}",
            report.schema, report.pinset_version, report.platform.target, report.platform.ci
        ),
        format!(
            "project configured={} config-schema={} lock-schema={} tools={} tasks={}",
            report.project.configured,
            optional_number(report.project.config_schema),
            optional_number(report.project.lock_schema),
            report.tools.len(),
            report.project.tasks
        ),
        format!(
            "environment configured={} profile={} source={} profiles={} variables={} secrets={}",
            report.environment.configured,
            report
                .environment
                .selected_profile
                .as_deref()
                .unwrap_or("none"),
            report.environment.selection_source,
            report.environment.profiles,
            report.environment.variables,
            report.environment.secret_variables
        ),
        format!(
            "summary passed={} errors={} warnings={} info={}",
            report.summary.passed,
            report.summary.errors,
            report.summary.warnings,
            report.summary.info
        ),
    ];
    lines.extend(report.findings.iter().map(|finding| {
        format!(
            "finding {} {} {} {}",
            finding.severity, finding.code, finding.category, finding.subject
        )
    }));
    lines.extend(
        report
            .repair_preview
            .iter()
            .map(|repair| format!("repair {} command={}", repair.code, repair.command)),
    );
    if let Some(comparison) = &output.comparison {
        lines.push(format!(
            "comparison changed={} paths={}",
            comparison.changed,
            comparison.paths.len()
        ));
        lines.extend(
            comparison
                .paths
                .iter()
                .map(|path| format!("changed {path}")),
        );
    }
    if let Some(saved) = &output.saved {
        lines.push(format!("saved {saved}"));
    }
    lines
}

fn collect_changes(
    path: &str,
    previous: &serde_json::Value,
    current: &serde_json::Value,
    output: &mut Vec<String>,
) {
    match (previous, current) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => collect_changes(&child, left, right, output),
                    _ => output.push(child),
                }
            }
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                let child = format!("{path}/{index}");
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) => collect_changes(&child, left, right, output),
                    _ => output.push(child),
                }
            }
        }
        _ if previous != current => output.push(if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        }),
        _ => {}
    }
}

fn repair_command(code: LockAuditReasonCode) -> Option<&'static str> {
    use LockAuditReasonCode::*;
    Some(match code {
        ConfigSchemaLegacy | LockSchemaLegacy => "pinset migrate --dry-run",
        CacheEntryCorrupt | CacheEntryUnsafe | CacheEntryUnreadable => {
            "pinset cache repair --dry-run"
        }
        ConfigMissing | ConfigInvalid | ProviderUnsupported | ProviderAuditUnsupported => {
            return None;
        }
        _ => "pinset install --locked",
    })
}

fn severity_name(value: LockAuditSeverity) -> &'static str {
    match value {
        LockAuditSeverity::Error => "error",
        LockAuditSeverity::Warning => "warning",
        LockAuditSeverity::Info => "info",
    }
}

fn category_name(value: LockAuditCategory) -> &'static str {
    match value {
        LockAuditCategory::Configuration => "configuration",
        LockAuditCategory::Lock => "lock",
        LockAuditCategory::PlatformArtifact => "platform_artifact",
        LockAuditCategory::Cache => "cache",
        LockAuditCategory::InstallReceipt => "install_receipt",
        LockAuditCategory::Ownership => "ownership",
        LockAuditCategory::Provenance => "provenance",
    }
}

fn optional_number(value: Option<u32>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn is_ci() -> bool {
    ["CI", "GITHUB_ACTIONS", "GITLAB_CI", "TF_BUILD"]
        .iter()
        .any(|name| {
            std::env::var(name).is_ok_and(|value| {
                !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_reports_paths_without_values() {
        let mut left = sample();
        let mut right = left.clone();
        left.environment.selected_profile = Some("secret-profile-a".to_owned());
        right.environment.selected_profile = Some("secret-profile-b".to_owned());
        let comparison = compare(&left, &right);
        assert_eq!(comparison.paths, ["/environment/selected_profile"]);
        let json = serde_json::to_string(&comparison).unwrap();
        assert!(!json.contains("secret-profile-a"));
        assert!(!json.contains("secret-profile-b"));
    }
    fn sample() -> DiagnosticReport {
        DiagnosticReport {
            schema: REPORT_SCHEMA,
            pinset_version: "2.4.0".to_owned(),
            platform: DiagnosticPlatform {
                os: "linux".to_owned(),
                arch: "x86_64".to_owned(),
                target: "linux-x86_64".to_owned(),
                ci: false,
            },
            project: DiagnosticProject {
                configured: true,
                config_schema: Some(5),
                lock_schema: Some(3),
                tasks: 0,
            },
            environment: DiagnosticEnvironment {
                configured: false,
                selected_profile: None,
                selection_source: "none".to_owned(),
                profiles: 0,
                variables: 0,
                secret_variables: 0,
            },
            tools: Vec::new(),
            summary: DiagnosticSummary {
                passed: true,
                ..DiagnosticSummary::default()
            },
            findings: Vec::new(),
            repair_preview: Vec::new(),
        }
    }
}
