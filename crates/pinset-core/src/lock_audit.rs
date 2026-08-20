//! Read-only, offline auditing for selected configuration and lock state.
//!
//! The audit deliberately treats configuration, lock, cache, receipt, and ownership
//! problems as report findings instead of mutating state or contacting a Provider.

use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{
    ArtifactIntegrity, Error, GLOBAL_STATE_SCHEMA, LOCKFILE_SCHEMA, LockedArtifact, Lockfile,
    MinimumReleaseAge, PROJECT_CONFIG_FILENAME, PROJECT_CONFIG_SCHEMA, RuntimeLockAuditKind,
    VerificationStrength, current_target_for_tool, download_cache::verify_download_cache_integrity,
    find_optional_project_config, global_config_path, global_lockfile_path, load_global_config,
    load_lockfile, load_project_config, load_project_python_environment, lockfile_path,
    runtime_provider,
};

const MAX_AUDIT_RECEIPT_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAuditScope {
    Project,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAuditSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAuditCategory {
    Configuration,
    Lock,
    PlatformArtifact,
    Cache,
    InstallReceipt,
    Ownership,
    Provenance,
}

/// Stable identifiers intended for scripts and policy checks. Messages and repair text are
/// human-facing and may be refined without changing these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAuditReasonCode {
    ConfigMissing,
    ConfigInvalid,
    ConfigSchemaLegacy,
    LockMissing,
    LockInvalid,
    LockSchemaLegacy,
    LockToolMissing,
    LockToolUnconfigured,
    LockSelectorMismatch,
    ProviderUnsupported,
    ProviderAuditUnsupported,
    PlatformArtifactMissing,
    PlatformArtifactInvalid,
    CacheEntryMissing,
    CacheEntryCorrupt,
    CacheEntryUnsafe,
    CacheEntryUnreadable,
    InstallMissing,
    InstallPathUnsafe,
    ReceiptMissing,
    ReceiptUnreadable,
    ReceiptInvalid,
    ReceiptSchemaLegacy,
    ReceiptSchemaUnsupported,
    ReceiptIncomplete,
    ReceiptIdentityMismatch,
    ReceiptIntegrityMissing,
    ReceiptIntegrityMismatch,
    ReceiptOverlayMismatch,
    PythonEnvironmentMissing,
    PythonEnvironmentOwnershipInvalid,
    VerificationBelowPolicy,
    ReleaseAgeUnavailable,
    ReleaseTooNew,
}

impl LockAuditReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigMissing => "config_missing",
            Self::ConfigInvalid => "config_invalid",
            Self::ConfigSchemaLegacy => "config_schema_legacy",
            Self::LockMissing => "lock_missing",
            Self::LockInvalid => "lock_invalid",
            Self::LockSchemaLegacy => "lock_schema_legacy",
            Self::LockToolMissing => "lock_tool_missing",
            Self::LockToolUnconfigured => "lock_tool_unconfigured",
            Self::LockSelectorMismatch => "lock_selector_mismatch",
            Self::ProviderUnsupported => "provider_unsupported",
            Self::ProviderAuditUnsupported => "provider_audit_unsupported",
            Self::PlatformArtifactMissing => "platform_artifact_missing",
            Self::PlatformArtifactInvalid => "platform_artifact_invalid",
            Self::CacheEntryMissing => "cache_entry_missing",
            Self::CacheEntryCorrupt => "cache_entry_corrupt",
            Self::CacheEntryUnsafe => "cache_entry_unsafe",
            Self::CacheEntryUnreadable => "cache_entry_unreadable",
            Self::InstallMissing => "install_missing",
            Self::InstallPathUnsafe => "install_path_unsafe",
            Self::ReceiptMissing => "receipt_missing",
            Self::ReceiptUnreadable => "receipt_unreadable",
            Self::ReceiptInvalid => "receipt_invalid",
            Self::ReceiptSchemaLegacy => "receipt_schema_legacy",
            Self::ReceiptSchemaUnsupported => "receipt_schema_unsupported",
            Self::ReceiptIncomplete => "receipt_incomplete",
            Self::ReceiptIdentityMismatch => "receipt_identity_mismatch",
            Self::ReceiptIntegrityMissing => "receipt_integrity_missing",
            Self::ReceiptIntegrityMismatch => "receipt_integrity_mismatch",
            Self::ReceiptOverlayMismatch => "receipt_overlay_mismatch",
            Self::PythonEnvironmentMissing => "python_environment_missing",
            Self::PythonEnvironmentOwnershipInvalid => "python_environment_ownership_invalid",
            Self::VerificationBelowPolicy => "verification_below_policy",
            Self::ReleaseAgeUnavailable => "release_age_unavailable",
            Self::ReleaseTooNew => "release_too_new",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockAuditRepair {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockAuditFinding {
    pub reason_code: LockAuditReasonCode,
    pub severity: LockAuditSeverity,
    pub category: LockAuditCategory,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair: Option<LockAuditRepair>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LockAuditSummary {
    pub tools: usize,
    pub platform_artifacts: usize,
    pub cache_entries: usize,
    pub receipts: usize,
    pub owned_installs: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub action_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockAuditReport {
    pub scope: LockAuditScope,
    pub offline: bool,
    pub passed: bool,
    pub config: PathBuf,
    pub lockfile: PathBuf,
    pub summary: LockAuditSummary,
    pub findings: Vec<LockAuditFinding>,
}

impl LockAuditReport {
    pub fn action_required(&self) -> bool {
        self.summary.action_required
    }

    fn push(&mut self, finding: LockAuditFinding) {
        match finding.severity {
            LockAuditSeverity::Error => self.summary.errors += 1,
            LockAuditSeverity::Warning => self.summary.warnings += 1,
            LockAuditSeverity::Info => self.summary.info += 1,
        }
        self.findings.push(finding);
    }

    fn finish(&mut self) {
        self.summary.action_required = self.summary.errors > 0 || self.summary.warnings > 0;
        self.passed = !self.summary.action_required;
    }
}

#[derive(Debug)]
struct ConfigSelection {
    schema: u32,
    tools: BTreeMap<String, String>,
    verification_strength: Option<VerificationStrength>,
    minimum_release_age: Option<MinimumReleaseAge>,
}

#[derive(Debug, Deserialize)]
struct AuditInstallReceipt {
    #[serde(default)]
    schema: u32,
    #[serde(default)]
    complete: bool,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    canonical_url: Option<String>,
    #[serde(default)]
    selected_source: Option<String>,
    #[serde(default)]
    selected_source_kind: Option<String>,
    #[serde(default)]
    selected_url: Option<String>,
    #[serde(default)]
    artifact_integrity: Option<String>,
    #[serde(default)]
    artifact_sha256: Option<String>,
    #[serde(default)]
    artifact_format: Option<String>,
    #[serde(default)]
    base_artifact_integrities: Vec<String>,
    #[serde(default)]
    bytes_downloaded: Option<u64>,
}

pub fn audit_project_lock(pinset_home: &Path, cwd: &Path) -> LockAuditReport {
    let config = project_config_path_for_audit(cwd);
    let lockfile = lockfile_path(&config);
    audit_lock_paths(pinset_home, LockAuditScope::Project, config, lockfile)
}

pub fn audit_global_lock(pinset_home: &Path) -> LockAuditReport {
    let config = global_config_path(pinset_home);
    let lockfile = global_lockfile_path(pinset_home);
    audit_lock_paths(pinset_home, LockAuditScope::Global, config, lockfile)
}

fn audit_lock_paths(
    pinset_home: &Path,
    scope: LockAuditScope,
    config_path: PathBuf,
    lock_path: PathBuf,
) -> LockAuditReport {
    let mut report = LockAuditReport {
        scope,
        offline: true,
        passed: false,
        config: config_path.clone(),
        lockfile: lock_path.clone(),
        summary: LockAuditSummary::default(),
        findings: Vec::new(),
    };

    let config = load_config_selection(scope, &config_path, &mut report);
    let lockfile = load_audited_lock(&lock_path, &mut report);
    if let (Some(config), Some(lockfile)) = (&config, &lockfile) {
        audit_config_lock_pair(
            pinset_home,
            scope,
            &config_path,
            config,
            lockfile,
            &mut report,
        );
    }
    report.finish();
    report
}

fn load_config_selection(
    scope: LockAuditScope,
    path: &Path,
    report: &mut LockAuditReport,
) -> Option<ConfigSelection> {
    let result = match scope {
        LockAuditScope::Project => load_project_config(path).map(|config| ConfigSelection {
            schema: config.schema,
            tools: config.tools,
            verification_strength: config.policy.verification_strength,
            minimum_release_age: config.policy.minimum_release_age,
        }),
        LockAuditScope::Global => load_global_config(path).map(|config| ConfigSelection {
            schema: config.schema,
            tools: config.tools,
            verification_strength: None,
            minimum_release_age: None,
        }),
    };
    match result {
        Ok(config) => {
            let expected_schema = match scope {
                LockAuditScope::Project => PROJECT_CONFIG_SCHEMA,
                LockAuditScope::Global => GLOBAL_STATE_SCHEMA,
            };
            if config.schema < expected_schema {
                report.push(finding(
                    LockAuditReasonCode::ConfigSchemaLegacy,
                    LockAuditSeverity::Warning,
                    LockAuditCategory::Configuration,
                    "configuration",
                    Some(path),
                    format!(
                        "configuration schema {} remains readable but should be migrated to schema {}",
                        config.schema, expected_schema
                    ),
                    Some(repair(
                        "migrate the configuration and lock pair",
                        Some(migrate_command(scope, path)),
                    )),
                ));
            }
            Some(config)
        }
        Err(error) if config_missing(&error) => {
            report.push(finding(
                LockAuditReasonCode::ConfigMissing,
                LockAuditSeverity::Error,
                LockAuditCategory::Configuration,
                "configuration",
                Some(path),
                error.to_string(),
                Some(repair(
                    match scope {
                        LockAuditScope::Project => "create a project configuration",
                        LockAuditScope::Global => "select a global runtime",
                    },
                    Some(match scope {
                        LockAuditScope::Project => "pinset init".to_owned(),
                        LockAuditScope::Global => "pinset global <tool>@<selector>".to_owned(),
                    }),
                )),
            ));
            None
        }
        Err(error) => {
            report.push(finding(
                LockAuditReasonCode::ConfigInvalid,
                LockAuditSeverity::Error,
                LockAuditCategory::Configuration,
                "configuration",
                Some(path),
                error.to_string(),
                Some(repair(
                    "review the configuration before regenerating its lock",
                    None,
                )),
            ));
            None
        }
    }
}

fn load_audited_lock(path: &Path, report: &mut LockAuditReport) -> Option<Lockfile> {
    match load_lockfile(path) {
        Ok(lockfile) => {
            if lockfile.schema < LOCKFILE_SCHEMA {
                report.push(finding(
                    LockAuditReasonCode::LockSchemaLegacy,
                    LockAuditSeverity::Warning,
                    LockAuditCategory::Lock,
                    "lockfile",
                    Some(path),
                    format!(
                        "lockfile schema {} remains readable but should be migrated to schema {}",
                        lockfile.schema, LOCKFILE_SCHEMA
                    ),
                    Some(repair("migrate the configuration and lock pair", None)),
                ));
            }
            Some(lockfile)
        }
        Err(Error::ReadLockfile { source, .. }) if source.kind() == ErrorKind::NotFound => {
            report.push(finding(
                LockAuditReasonCode::LockMissing,
                LockAuditSeverity::Error,
                LockAuditCategory::Lock,
                "lockfile",
                Some(path),
                format!("lockfile does not exist: {}", path.display()),
                Some(repair(
                    "generate an exact lock for every configured runtime",
                    None,
                )),
            ));
            None
        }
        Err(error) => {
            let (reason_code, category) = lock_error_classification(&error);
            report.push(finding(
                reason_code,
                LockAuditSeverity::Error,
                category,
                "lockfile",
                Some(path),
                error.to_string(),
                Some(repair("review and regenerate the lockfile", None)),
            ));
            None
        }
    }
}

fn lock_error_classification(error: &Error) -> (LockAuditReasonCode, LockAuditCategory) {
    let artifact_related = match error {
        Error::InvalidArtifactIntegrity { .. } | Error::InvalidSha256 { .. } => true,
        Error::InvalidLockfile { reason } => [
            "artifact",
            "archive",
            "checksum",
            "integrity",
            "target",
            "URL",
            "verification",
        ]
        .into_iter()
        .any(|keyword| reason.contains(keyword)),
        _ => false,
    };
    if artifact_related {
        (
            LockAuditReasonCode::PlatformArtifactInvalid,
            LockAuditCategory::PlatformArtifact,
        )
    } else {
        (LockAuditReasonCode::LockInvalid, LockAuditCategory::Lock)
    }
}

fn audit_config_lock_pair(
    pinset_home: &Path,
    scope: LockAuditScope,
    config_path: &Path,
    config: &ConfigSelection,
    lockfile: &Lockfile,
    report: &mut LockAuditReport,
) {
    let lock_path = report.lockfile.clone();
    for (tool, requested) in &config.tools {
        report.summary.tools += 1;
        let Some(provider) = runtime_provider(tool) else {
            report.push(finding(
                LockAuditReasonCode::ProviderUnsupported,
                LockAuditSeverity::Error,
                LockAuditCategory::Configuration,
                tool,
                Some(config_path),
                format!("configuration selects unsupported Provider {tool:?}"),
                Some(repair(
                    "remove the unsupported selection or install a supported Provider",
                    None,
                )),
            ));
            continue;
        };
        if provider.capabilities.lock_audit != RuntimeLockAuditKind::ArtifactReceipt {
            report.push(finding(
                LockAuditReasonCode::ProviderAuditUnsupported,
                LockAuditSeverity::Error,
                LockAuditCategory::Lock,
                tool,
                Some(config_path),
                format!("Provider {tool:?} does not declare a lock audit capability"),
                None,
            ));
            continue;
        }
        let Some(locked) = lockfile.tool(tool) else {
            report.push(finding(
                LockAuditReasonCode::LockToolMissing,
                LockAuditSeverity::Error,
                LockAuditCategory::Lock,
                tool,
                Some(&lock_path),
                format!("lockfile has no record for configured selection {tool}@{requested}"),
                Some(repair("regenerate the lockfile from configuration", None)),
            ));
            continue;
        };
        if locked.requested != *requested {
            report.push(finding(
                LockAuditReasonCode::LockSelectorMismatch,
                LockAuditSeverity::Error,
                LockAuditCategory::Lock,
                tool,
                Some(&lock_path),
                format!(
                    "configuration requests {tool}@{requested}, but the lock records {} -> {}",
                    locked.requested, locked.version
                ),
                Some(repair("regenerate the lockfile from configuration", None)),
            ));
            continue;
        }
        if let Err(error) = crate::validate_tool_policy(
            locked,
            config.verification_strength,
            config.minimum_release_age,
            SystemTime::now(),
        ) {
            let (reason_code, action) = match error {
                Error::VerificationPolicyViolation { .. } => (
                    LockAuditReasonCode::VerificationBelowPolicy,
                    "select a release with stronger verification evidence or lower the explicit policy",
                ),
                Error::ReleaseAgeUnavailable { .. } => (
                    LockAuditReasonCode::ReleaseAgeUnavailable,
                    "remove the minimum release age policy or select a Provider that publishes release time",
                ),
                Error::ReleaseTooNew { .. } => (
                    LockAuditReasonCode::ReleaseTooNew,
                    "wait until the release satisfies the configured minimum age or select an older release",
                ),
                _ => (
                    LockAuditReasonCode::LockInvalid,
                    "regenerate the lockfile from trusted metadata",
                ),
            };
            report.push(finding(
                reason_code,
                LockAuditSeverity::Error,
                LockAuditCategory::Provenance,
                tool,
                Some(&lock_path),
                error.to_string(),
                Some(repair(action, None)),
            ));
        }
        audit_locked_tool(pinset_home, scope, config_path, locked, report);
    }

    for locked in &lockfile.tools {
        if !config.tools.contains_key(&locked.name) {
            report.push(finding(
                LockAuditReasonCode::LockToolUnconfigured,
                LockAuditSeverity::Error,
                LockAuditCategory::Lock,
                &locked.name,
                Some(&lock_path),
                format!(
                    "lockfile records {}@{}, but configuration does not select it",
                    locked.name, locked.version
                ),
                Some(repair("regenerate the lockfile from configuration", None)),
            ));
        }
    }
}

fn audit_locked_tool(
    pinset_home: &Path,
    scope: LockAuditScope,
    config_path: &Path,
    locked: &crate::LockedTool,
    report: &mut LockAuditReport,
) {
    let lock_path = report.lockfile.clone();
    let target = current_target_for_tool(&locked.name);
    let subject = format!("{}@{}:{target}", locked.name, locked.version);
    let Some(artifact) = locked.artifact(&target) else {
        report.push(finding(
            LockAuditReasonCode::PlatformArtifactMissing,
            LockAuditSeverity::Error,
            LockAuditCategory::PlatformArtifact,
            &subject,
            Some(&lock_path),
            format!(
                "lockfile has no {} artifact for the current target {target}",
                locked.name
            ),
            Some(repair(
                "regenerate the lock with a Pinset release that supports this platform",
                None,
            )),
        ));
        return;
    };
    report.summary.platform_artifacts += 1;
    audit_artifact_cache(pinset_home, &subject, artifact, report);
    audit_install_receipt(
        pinset_home,
        scope,
        config_path,
        &subject,
        locked,
        &target,
        artifact,
        report,
    );
    if scope == LockAuditScope::Project && locked.name == "python" {
        audit_python_environment(config_path, locked, &target, report);
    }
}

fn audit_artifact_cache(
    pinset_home: &Path,
    subject: &str,
    artifact: &LockedArtifact,
    report: &mut LockAuditReport,
) {
    let lock_path = report.lockfile.clone();
    let mut identities = Vec::with_capacity(1 + artifact.overlays.len());
    match artifact.artifact_integrity() {
        Ok(integrity) => identities.push(("primary", integrity)),
        Err(error) => {
            report.push(finding(
                LockAuditReasonCode::PlatformArtifactInvalid,
                LockAuditSeverity::Error,
                LockAuditCategory::PlatformArtifact,
                subject,
                Some(&lock_path),
                error.to_string(),
                Some(repair("regenerate the invalid lockfile", None)),
            ));
            return;
        }
    }
    for (index, overlay) in artifact.overlays.iter().enumerate() {
        match overlay.artifact_integrity() {
            Ok(integrity) => identities.push((
                if index == 0 {
                    "overlay"
                } else {
                    "overlay-extra"
                },
                integrity,
            )),
            Err(error) => report.push(finding(
                LockAuditReasonCode::PlatformArtifactInvalid,
                LockAuditSeverity::Error,
                LockAuditCategory::PlatformArtifact,
                subject,
                Some(&lock_path),
                error.to_string(),
                Some(repair("regenerate the invalid lockfile", None)),
            )),
        }
    }
    for (kind, integrity) in identities {
        let cache_subject = format!("{subject}:{kind}:{}", integrity.canonical());
        match verify_download_cache_integrity(pinset_home, &integrity) {
            Ok(Some(entry)) => {
                report.summary.cache_entries += 1;
                if !entry.valid {
                    report.push(finding(
                        LockAuditReasonCode::CacheEntryCorrupt,
                        LockAuditSeverity::Error,
                        LockAuditCategory::Cache,
                        &cache_subject,
                        Some(&entry.path),
                        format!(
                            "cached artifact identity is {}, but its bytes hash to {}",
                            entry.integrity, entry.actual
                        ),
                        Some(repair(
                            "remove corrupt cache bytes after review and reinstall from a trusted source",
                            Some("pinset cache repair".to_owned()),
                        )),
                    ));
                }
            }
            Ok(None) => report.push(finding(
                LockAuditReasonCode::CacheEntryMissing,
                LockAuditSeverity::Info,
                LockAuditCategory::Cache,
                &cache_subject,
                None,
                "the locked artifact is not present in the optional offline cache".to_owned(),
                Some(repair(
                    "import a reviewed copy when offline installation readiness is required",
                    Some(format!(
                        "pinset cache import <archive> --integrity {}",
                        integrity.canonical()
                    )),
                )),
            )),
            Err(Error::UnsafeDownloadCacheEntry { path }) => report.push(finding(
                LockAuditReasonCode::CacheEntryUnsafe,
                LockAuditSeverity::Error,
                LockAuditCategory::Cache,
                &cache_subject,
                Some(&path),
                "the cache identity resolves to a non-regular file or symbolic link".to_owned(),
                Some(repair(
                    "review the unsafe cache path before removing it",
                    None,
                )),
            )),
            Err(error) => report.push(finding(
                LockAuditReasonCode::CacheEntryUnreadable,
                LockAuditSeverity::Error,
                LockAuditCategory::Cache,
                &cache_subject,
                None,
                error.to_string(),
                Some(repair("restore read access to the cache entry", None)),
            )),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_install_receipt(
    pinset_home: &Path,
    scope: LockAuditScope,
    config_path: &Path,
    subject: &str,
    locked: &crate::LockedTool,
    target: &str,
    artifact: &LockedArtifact,
    report: &mut LockAuditReport,
) {
    let install_dir = pinset_home
        .join("installs")
        .join(&locked.name)
        .join(&locked.version)
        .join(target);
    match validate_install_directory_chain(pinset_home, &locked.name, &locked.version, target) {
        Ok(false) => {
            report.push(finding(
                LockAuditReasonCode::InstallMissing,
                LockAuditSeverity::Warning,
                LockAuditCategory::Ownership,
                subject,
                Some(&install_dir),
                "the locked runtime is not installed for the current platform".to_owned(),
                Some(repair(
                    "install the exact locked runtime",
                    Some(install_command(scope, config_path)),
                )),
            ));
            return;
        }
        Ok(true) => {}
        Err(path) => {
            report.push(finding(
                LockAuditReasonCode::InstallPathUnsafe,
                LockAuditSeverity::Error,
                LockAuditCategory::Ownership,
                subject,
                Some(&path),
                "an installation path component is not a regular directory or is a symbolic link"
                    .to_owned(),
                Some(repair(
                    "review the unsafe installation path; Pinset will not take ownership",
                    None,
                )),
            ));
            return;
        }
    }

    let receipt_path = install_dir.join(".pinset-install.toml");
    let receipt_metadata = match fs::symlink_metadata(&receipt_path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == ErrorKind::NotFound => {
            report.push(finding(
                LockAuditReasonCode::ReceiptMissing,
                LockAuditSeverity::Error,
                LockAuditCategory::Ownership,
                subject,
                Some(&receipt_path),
                "the installation has no Pinset ownership receipt".to_owned(),
                Some(repair(
                    "move the unowned directory aside and reinstall the locked runtime",
                    None,
                )),
            ));
            return;
        }
        Err(source) => {
            report.push(finding(
                LockAuditReasonCode::ReceiptUnreadable,
                LockAuditSeverity::Error,
                LockAuditCategory::InstallReceipt,
                subject,
                Some(&receipt_path),
                source.to_string(),
                Some(repair(
                    "restore read access to the installation receipt",
                    None,
                )),
            ));
            return;
        }
    };
    if !receipt_metadata.file_type().is_file() || receipt_metadata.file_type().is_symlink() {
        report.push(finding(
            LockAuditReasonCode::InstallPathUnsafe,
            LockAuditSeverity::Error,
            LockAuditCategory::Ownership,
            subject,
            Some(&receipt_path),
            "the ownership receipt is not a regular file".to_owned(),
            Some(repair(
                "review the unsafe receipt path; Pinset will not take ownership",
                None,
            )),
        ));
        return;
    }
    if receipt_metadata.len() > MAX_AUDIT_RECEIPT_BYTES {
        report.push(finding(
            LockAuditReasonCode::ReceiptInvalid,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            format!("the ownership receipt exceeds the {MAX_AUDIT_RECEIPT_BYTES} byte audit limit"),
            Some(repair(
                "review the oversized receipt before replacing the installation",
                None,
            )),
        ));
        return;
    }
    let content = match fs::read_to_string(&receipt_path) {
        Ok(content) => content,
        Err(source) => {
            report.push(finding(
                LockAuditReasonCode::ReceiptUnreadable,
                LockAuditSeverity::Error,
                LockAuditCategory::InstallReceipt,
                subject,
                Some(&receipt_path),
                source.to_string(),
                Some(repair(
                    "restore read access to the installation receipt",
                    None,
                )),
            ));
            return;
        }
    };
    let receipt = match toml::from_str::<AuditInstallReceipt>(&content) {
        Ok(receipt) => receipt,
        Err(error) => {
            report.push(finding(
                LockAuditReasonCode::ReceiptInvalid,
                LockAuditSeverity::Error,
                LockAuditCategory::InstallReceipt,
                subject,
                Some(&receipt_path),
                error.to_string(),
                Some(repair(
                    "move the invalid installation aside and reinstall",
                    None,
                )),
            ));
            return;
        }
    };
    report.summary.receipts += 1;
    if !matches!(receipt.schema, 1 | 2) {
        report.push(finding(
            LockAuditReasonCode::ReceiptSchemaUnsupported,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            format!("install receipt schema {} is unsupported", receipt.schema),
            Some(repair(
                "move the unsupported installation aside and reinstall",
                None,
            )),
        ));
        return;
    }
    if receipt.schema == 1 {
        report.push(finding(
            LockAuditReasonCode::ReceiptSchemaLegacy,
            LockAuditSeverity::Warning,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            "legacy receipt schema 1 proves identity but records less integrity evidence"
                .to_owned(),
            Some(repair(
                "reinstall the runtime to write a schema 2 receipt",
                None,
            )),
        ));
    }
    if !receipt.complete {
        report.push(finding(
            LockAuditReasonCode::ReceiptIncomplete,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            "the install receipt is not marked complete".to_owned(),
            Some(repair(
                "move the incomplete installation aside and reinstall",
                None,
            )),
        ));
        return;
    }
    if receipt.tool != locked.name || receipt.version != locked.version || receipt.target != target
    {
        report.push(finding(
            LockAuditReasonCode::ReceiptIdentityMismatch,
            LockAuditSeverity::Error,
            LockAuditCategory::Ownership,
            subject,
            Some(&receipt_path),
            format!(
                "receipt identifies {}@{}:{}, not {subject}",
                receipt.tool, receipt.version, receipt.target
            ),
            Some(repair(
                "review the foreign installation; Pinset will not take ownership",
                None,
            )),
        ));
        return;
    }
    report.summary.owned_installs += 1;

    if receipt.schema == 2
        && (receipt.canonical_url.as_deref() != Some(artifact.canonical_url.as_str())
            || receipt.selected_source.as_deref().is_none_or(str::is_empty)
            || !matches!(
                receipt.selected_source_kind.as_deref(),
                Some("official" | "mirror" | "cache")
            )
            || receipt.selected_url.as_deref().is_none_or(str::is_empty)
            || receipt.artifact_format.as_deref() != Some(artifact.format.as_str())
            || receipt.bytes_downloaded.is_none())
    {
        report.push(finding(
            LockAuditReasonCode::ReceiptInvalid,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            "schema 2 receipt metadata does not describe the locked artifact and selected source"
                .to_owned(),
            Some(repair(
                "move the malformed installation aside and reinstall the locked runtime",
                None,
            )),
        ));
    }

    let expected = match artifact.artifact_integrity() {
        Ok(integrity) => integrity.canonical(),
        Err(_) => return,
    };
    let recorded = receipt
        .artifact_integrity
        .as_deref()
        .or(receipt.artifact_sha256.as_deref())
        .and_then(|value| ArtifactIntegrity::parse(value).ok())
        .map(|integrity| integrity.canonical());
    let Some(recorded) = recorded else {
        report.push(finding(
            LockAuditReasonCode::ReceiptIntegrityMissing,
            if receipt.schema == 1 {
                LockAuditSeverity::Warning
            } else {
                LockAuditSeverity::Error
            },
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            "the receipt does not record a valid primary artifact integrity".to_owned(),
            Some(repair(
                "reinstall the runtime to bind its receipt to the locked artifact",
                None,
            )),
        ));
        return;
    };
    if recorded != expected {
        report.push(finding(
            LockAuditReasonCode::ReceiptIntegrityMismatch,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            format!("receipt records {recorded}, but the lock requires {expected}"),
            Some(repair(
                "move the mismatched installation aside and reinstall",
                None,
            )),
        ));
    }
    let expected_overlays = artifact
        .overlays
        .iter()
        .filter_map(|overlay| overlay.artifact_integrity().ok())
        .map(|integrity| integrity.canonical())
        .collect::<Vec<_>>();
    let recorded_overlays = receipt
        .base_artifact_integrities
        .iter()
        .map(|value| ArtifactIntegrity::parse(value).map(|integrity| integrity.canonical()))
        .collect::<Result<Vec<_>, _>>();
    if !recorded_overlays.is_ok_and(|recorded| recorded == expected_overlays) {
        report.push(finding(
            LockAuditReasonCode::ReceiptOverlayMismatch,
            LockAuditSeverity::Error,
            LockAuditCategory::InstallReceipt,
            subject,
            Some(&receipt_path),
            "receipt overlay integrities do not match the locked platform artifact".to_owned(),
            Some(repair(
                "move the mismatched installation aside and reinstall",
                None,
            )),
        ));
    }
}

fn audit_python_environment(
    config_path: &Path,
    locked: &crate::LockedTool,
    target: &str,
    report: &mut LockAuditReport,
) {
    match load_project_python_environment(config_path, &locked.version, target) {
        Ok(_) => {}
        Err(Error::PythonEnvironmentMissing { path }) => report.push(finding(
            LockAuditReasonCode::PythonEnvironmentMissing,
            LockAuditSeverity::Warning,
            LockAuditCategory::Ownership,
            "python-environment",
            Some(&path),
            "the project Python environment has not been created".to_owned(),
            Some(repair(
                "create the Pinset-owned project environment",
                Some("pinset venv create".to_owned()),
            )),
        )),
        Err(
            error @ (Error::PythonEnvironmentNotOwned { .. }
            | Error::PythonEnvironmentMismatch { .. }
            | Error::InvalidPythonEnvironmentMarker { .. }),
        ) => report.push(finding(
            LockAuditReasonCode::PythonEnvironmentOwnershipInvalid,
            LockAuditSeverity::Error,
            LockAuditCategory::Ownership,
            "python-environment",
            None,
            error.to_string(),
            Some(repair(
                "review the foreign environment before recreating it",
                None,
            )),
        )),
        Err(error) => report.push(finding(
            LockAuditReasonCode::PythonEnvironmentOwnershipInvalid,
            LockAuditSeverity::Error,
            LockAuditCategory::Ownership,
            "python-environment",
            None,
            error.to_string(),
            Some(repair("review the project Python environment", None)),
        )),
    }
}

fn validate_install_directory_chain(
    pinset_home: &Path,
    tool: &str,
    version: &str,
    target: &str,
) -> Result<bool, PathBuf> {
    let mut path = pinset_home.to_path_buf();
    for segment in ["installs", tool, version, target] {
        path.push(segment);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(path),
        };
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(path);
        }
    }
    Ok(true)
}

fn project_config_path_for_audit(cwd: &Path) -> PathBuf {
    match find_optional_project_config(cwd) {
        Ok(Some(path)) => return path,
        Ok(None) => {}
        Err(Error::ReadProjectConfig { path, .. } | Error::ParseProjectConfig { path, .. }) => {
            return path;
        }
        Err(_) => {}
    }
    let start = if cwd.is_file() {
        cwd.parent().unwrap_or(cwd)
    } else {
        cwd
    };
    let git_root = start
        .ancestors()
        .find(|directory| {
            let marker = directory.join(".git");
            marker.is_file() || marker.is_dir()
        })
        .unwrap_or(start);
    for directory in start
        .ancestors()
        .take_while(|directory| directory.starts_with(git_root))
    {
        let candidate = directory.join(PROJECT_CONFIG_FILENAME);
        if fs::symlink_metadata(&candidate).is_ok() {
            return candidate;
        }
    }
    start.join(PROJECT_CONFIG_FILENAME)
}

fn config_missing(error: &Error) -> bool {
    match error {
        Error::GlobalConfigNotFound { .. } => true,
        Error::ReadProjectConfig { source, .. } => source.kind() == ErrorKind::NotFound,
        _ => false,
    }
}

fn install_command(scope: LockAuditScope, config_path: &Path) -> String {
    match scope {
        LockAuditScope::Project => format!(
            "pinset install --locked --cwd \"{}\"",
            config_path.parent().unwrap_or(config_path).display()
        ),
        LockAuditScope::Global => "pinset install --locked --global".to_owned(),
    }
}

fn migrate_command(scope: LockAuditScope, config_path: &Path) -> String {
    match scope {
        LockAuditScope::Project => format!(
            "pinset migrate --cwd \"{}\"",
            config_path.parent().unwrap_or(config_path).display()
        ),
        LockAuditScope::Global => "pinset migrate --global".to_owned(),
    }
}

fn repair(action: &str, command: Option<String>) -> LockAuditRepair {
    LockAuditRepair {
        action: action.to_owned(),
        command,
    }
}

fn finding(
    reason_code: LockAuditReasonCode,
    severity: LockAuditSeverity,
    category: LockAuditCategory,
    subject: impl Into<String>,
    path: Option<&Path>,
    message: String,
    repair: Option<LockAuditRepair>,
) -> LockAuditFinding {
    LockAuditFinding {
        reason_code,
        severity,
        category,
        subject: subject.into(),
        path: path.map(Path::to_path_buf),
        message,
        repair,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        LockedArtifact, LockedArtifactFormat, MVP_NODE_TARGETS, NodeArchiveFormat, SourceConfig,
        plan_node_artifact, save_lockfile,
    };
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn missing_project_state_is_reported_without_creating_files() {
        let root = tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("project");

        let report = audit_project_lock(&home, &project);

        assert!(report.action_required());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| { finding.reason_code == LockAuditReasonCode::ConfigMissing })
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| { finding.reason_code == LockAuditReasonCode::LockMissing })
        );
        assert!(!home.exists());
        assert_eq!(fs::read_dir(&project).expect("project entries").count(), 0);
    }

    #[test]
    fn reason_codes_have_stable_snake_case_serialization() {
        assert_eq!(
            LockAuditReasonCode::ReceiptIntegrityMismatch.as_str(),
            "receipt_integrity_mismatch"
        );
    }

    #[test]
    fn selector_drift_has_a_specific_action_required_reason_code() {
        let root = tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("project");
        fs::write(
            project.join(PROJECT_CONFIG_FILENAME),
            "schema = 3\n\n[policy]\ninherit-global = false\nsystem-fallback = false\nboundary = \"git\"\n\n[tools]\nnode = \"22.0.0\"\n",
        )
        .expect("project config");
        save_lockfile(&project.join("pinset.lock"), &node_lockfile("24.0.0")).expect("lockfile");

        let report = audit_project_lock(&home, &project);

        assert!(report.action_required());
        assert!(
            report.findings.iter().any(|finding| {
                finding.reason_code == LockAuditReasonCode::LockSelectorMismatch
            })
        );
        assert!(!home.exists());
    }

    #[test]
    fn provenance_policy_has_a_stable_audit_reason_code() {
        let root = tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("project");
        fs::write(
            project.join(PROJECT_CONFIG_FILENAME),
            "schema = 3\n\n[policy]\nverification-strength = \"provenance\"\n\n[tools]\nnode = \"24.0.0\"\n",
        )
        .expect("project config");
        save_lockfile(&project.join("pinset.lock"), &node_lockfile("24.0.0")).expect("lockfile");

        let report = audit_project_lock(&home, &project);

        assert!(report.findings.iter().any(|finding| {
            finding.reason_code == LockAuditReasonCode::VerificationBelowPolicy
                && finding.category == LockAuditCategory::Provenance
        }));
    }

    #[test]
    fn matching_receipt_passes_while_an_optional_cache_miss_remains_informational() {
        let root = tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("project");
        fs::write(
            project.join(PROJECT_CONFIG_FILENAME),
            "schema = 3\n\n[policy]\ninherit-global = false\nsystem-fallback = false\nboundary = \"git\"\n\n[tools]\nnode = \"24.0.0\"\n",
        )
        .expect("project config");
        save_lockfile(&project.join("pinset.lock"), &node_lockfile("24.0.0")).expect("lockfile");

        let target = current_target_for_tool("node");
        let install = home
            .join("installs")
            .join("node")
            .join("24.0.0")
            .join(&target);
        fs::create_dir_all(&install).expect("install directory");
        let plan = plan_node_artifact(&SourceConfig::default(), "24.0.0", &target)
            .expect("Node artifact plan");
        let format = match plan.format {
            NodeArchiveFormat::Zip => "zip",
            NodeArchiveFormat::TarXz => "tar.xz",
        };
        fs::write(
            install.join(".pinset-install.toml"),
            format!(
                "schema = 2\ncomplete = true\ntool = \"node\"\nversion = \"24.0.0\"\ntarget = \"{target}\"\ncanonical_url = \"{url}\"\nselected_source = \"fixture\"\nselected_source_kind = \"official\"\nselected_url = \"{url}\"\nartifact_integrity = \"sha256:{integrity}\"\nartifact_format = \"{format}\"\nbytes_downloaded = 0\n",
                url = plan.canonical_url,
                integrity = "ab".repeat(32),
            ),
        )
        .expect("receipt");

        let report = audit_project_lock(&home, &project);

        assert!(report.passed);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.summary.warnings, 0);
        assert_eq!(report.summary.owned_installs, 1);
        assert!(report.findings.iter().any(|finding| {
            finding.reason_code == LockAuditReasonCode::CacheEntryMissing
                && finding.severity == LockAuditSeverity::Info
        }));
    }

    fn node_lockfile(version: &str) -> Lockfile {
        let artifacts = MVP_NODE_TARGETS
            .into_iter()
            .map(|target| {
                let plan = plan_node_artifact(&SourceConfig::default(), version, target)
                    .expect("Node artifact plan");
                LockedArtifact {
                    target: target.to_owned(),
                    canonical_url: plan.canonical_url,
                    artifact_path: plan.artifact_path,
                    sha256: "ab".repeat(32),
                    integrity: None,
                    format: match plan.format {
                        NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                        NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                    },
                    archive_root: plan.archive_root,
                    verification: "nodejs-openpgp-sha256".to_owned(),
                    overlays: Vec::new(),
                }
            })
            .collect();
        Lockfile::new_node(
            "pinset lock audit test".to_owned(),
            version.to_owned(),
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356".to_owned(),
            "official".to_owned(),
            artifacts,
        )
    }
}
