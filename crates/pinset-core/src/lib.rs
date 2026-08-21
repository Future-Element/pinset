mod config;
#[cfg(feature = "dotnet-metadata")]
mod dotnet_metadata;
#[cfg(feature = "dotnet-provider")]
mod dotnet_provider;
#[cfg(all(
    feature = "installer",
    feature = "dotnet-provider",
    feature = "lockfile"
))]
mod dotnet_runtime;
#[cfg(feature = "installer")]
mod download_cache;
mod environment_protocol;
mod error;
#[cfg(feature = "flutter-metadata")]
mod flutter_metadata;
#[cfg(feature = "flutter-provider")]
mod flutter_provider;
#[cfg(all(
    feature = "installer",
    feature = "flutter-provider",
    feature = "lockfile"
))]
mod flutter_runtime;
mod global_state;
#[cfg(feature = "go-metadata")]
mod go_metadata;
#[cfg(feature = "go-provider")]
mod go_provider;
#[cfg(all(feature = "installer", feature = "go-provider", feature = "lockfile"))]
mod go_runtime;
#[cfg(feature = "installer")]
mod installer;
#[cfg(any(feature = "installer", feature = "lockfile", feature = "npm-metadata"))]
mod integrity;
#[cfg(feature = "java-metadata")]
mod java_metadata;
#[cfg(feature = "java-provider")]
mod java_provider;
#[cfg(all(feature = "installer", feature = "java-provider", feature = "lockfile"))]
mod java_runtime;
#[cfg(all(feature = "installer", feature = "lockfile"))]
mod lock_audit;
#[cfg(feature = "lockfile")]
mod lockfile;
#[cfg(feature = "node-provider")]
mod node_lifecycle;
#[cfg(feature = "node-metadata")]
mod node_metadata;
#[cfg(feature = "node-provider")]
mod node_provider;
#[cfg(all(feature = "installer", feature = "lockfile"))]
mod node_runtime;
#[cfg(feature = "node-metadata")]
mod node_trust;
#[cfg(feature = "npm-metadata")]
mod npm_metadata;
#[cfg(all(feature = "installer", feature = "npm-metadata"))]
mod npm_runtime;
#[cfg(feature = "project-discovery")]
mod project_discovery;
mod provenance;
#[cfg(feature = "provider-registry")]
mod provider_registry;
#[cfg(feature = "python-metadata")]
mod python_metadata;
#[cfg(feature = "python-provider")]
mod python_provider;
#[cfg(all(
    feature = "installer",
    feature = "python-provider",
    feature = "lockfile"
))]
mod python_runtime;
mod python_venv;
mod resolver;
mod runtime_lifecycle;
mod runtime_provider;
#[cfg(feature = "rust-metadata")]
mod rust_metadata;
#[cfg(feature = "rust-provider")]
mod rust_provider;
#[cfg(all(feature = "installer", feature = "rust-provider", feature = "lockfile"))]
mod rust_runtime;
mod shim_install;
#[cfg(feature = "sources")]
mod source_config;
mod target;
mod user_settings;

#[cfg(all(feature = "project-write", feature = "lockfile"))]
pub use config::save_project_state;
#[cfg(feature = "lockfile")]
pub use config::validate_project_lock_policy;
pub use config::{
    EnvironmentCollision, EnvironmentProfile, PROJECT_CONFIG_FILENAME, PROJECT_CONFIG_SCHEMA,
    ProjectBoundary, ProjectConfig, ProjectContext, ProjectEnvironment, ProjectPolicy,
    find_optional_project_config, find_project_config, find_project_context, load_project_config,
};
#[cfg(feature = "project-write")]
pub use config::{create_project_config, save_project_config};
#[cfg(feature = "dotnet-metadata")]
pub use dotnet_metadata::{DotnetMetadataClient, DotnetRelease};
#[cfg(feature = "dotnet-provider")]
pub use dotnet_provider::{
    DOTNET_TARGETS, DotnetArchiveFormat, DotnetArtifactPlan, DotnetVersion, dotnet_rid,
    plan_dotnet_artifact, validate_exact_dotnet_version,
};
#[cfg(all(
    feature = "installer",
    feature = "dotnet-provider",
    feature = "lockfile"
))]
pub use dotnet_runtime::install_locked_dotnet;
#[cfg(feature = "installer")]
pub use download_cache::{
    DownloadCacheCleanOutcome, DownloadCacheEntry, DownloadCacheInfo, DownloadCacheVerification,
    DownloadCacheVerificationEntry, clean_download_cache, download_cache_info, download_cache_path,
    import_download_cache, import_download_cache_with_integrity, list_download_cache,
    repair_download_cache, verify_download_cache,
};
pub use environment_protocol::{decode_environment, encode_environment};
pub use error::{Error, Result};
#[cfg(feature = "flutter-metadata")]
pub use flutter_metadata::{FlutterMetadataClient, FlutterRelease};
#[cfg(feature = "flutter-provider")]
pub use flutter_provider::{
    FLUTTER_TARGETS, FlutterArchiveFormat, FlutterArtifactPlan, plan_flutter_artifact,
    validate_exact_flutter_version,
};
#[cfg(all(
    feature = "installer",
    feature = "flutter-provider",
    feature = "lockfile"
))]
pub use flutter_runtime::install_locked_flutter;
#[cfg(feature = "state-write")]
pub use global_state::save_global_config;
#[cfg(all(feature = "state-write", feature = "lockfile"))]
pub use global_state::save_global_state;
pub use global_state::{
    GLOBAL_CONFIG_FILENAME, GLOBAL_LOCKFILE_FILENAME, GLOBAL_STATE_SCHEMA, GlobalConfig,
    global_config_path, global_lockfile_path, global_state_dir, load_global_config,
    load_optional_global_config,
};
#[cfg(feature = "go-metadata")]
pub use go_metadata::{GoMetadataClient, GoRelease};
#[cfg(feature = "go-provider")]
pub use go_provider::{
    GO_TARGETS, GoArchiveFormat, GoArtifactPlan, plan_go_artifact, validate_exact_go_version,
};
#[cfg(all(feature = "installer", feature = "go-provider", feature = "lockfile"))]
pub use go_runtime::install_locked_go;
#[cfg(feature = "installer")]
pub use installer::{
    ArtifactFormat, ArtifactInstallSpec, ArtifactSource, ArtifactSourceKind, ArtifactSpec,
    DownloadProgressEvent, InstallAlias, InstallLimits, InstallOutcome, InstallRequest, Installer,
    install_payload_statistics, sha256_hex,
};
#[cfg(any(feature = "installer", feature = "lockfile", feature = "npm-metadata"))]
pub use integrity::{ArtifactIntegrity, IntegrityAlgorithm};
#[cfg(feature = "java-metadata")]
pub use java_metadata::{JavaMetadataClient, JavaRelease};
#[cfg(feature = "java-provider")]
pub use java_provider::{
    JAVA_TARGETS, JavaArchiveFormat, JavaArtifactPlan, JavaVersion, plan_java_artifact,
    validate_exact_java_version,
};
#[cfg(all(feature = "installer", feature = "java-provider", feature = "lockfile"))]
pub use java_runtime::install_locked_java;
#[cfg(all(feature = "installer", feature = "lockfile"))]
pub use lock_audit::{
    LockAuditCategory, LockAuditFinding, LockAuditReasonCode, LockAuditRepair, LockAuditReport,
    LockAuditScope, LockAuditSeverity, LockAuditSummary, audit_global_lock, audit_project_lock,
};
#[cfg(feature = "lockfile")]
pub use lockfile::{
    LOCKFILE_FILENAME, LOCKFILE_SCHEMA, LockedArtifact, LockedArtifactFormat,
    LockedArtifactOverlay, LockedTool, Lockfile, MVP_NODE_TARGETS, load_lockfile,
    load_optional_lockfile, lockfile_path, save_lockfile, validate_lock_matches_project,
    validate_lock_matches_selection, validate_lock_matches_tool, validate_lock_matches_tools,
};
#[cfg(feature = "node-provider")]
pub use node_lifecycle::{
    InstalledNodeVersion, NodeVersionReference, UninstallNodeOutcome, find_node_version_references,
    list_installed_node_versions, uninstall_node_version,
};
#[cfg(feature = "node-metadata")]
pub use node_metadata::{NodeMetadataClient, NodeRelease};
#[cfg(feature = "node-provider")]
pub use node_provider::{
    NodeArchiveFormat, NodeArtifactPlan, plan_node_artifact, validate_exact_node_version,
};
#[cfg(all(feature = "installer", feature = "lockfile"))]
pub use node_runtime::{install_locked_node, node_command_directory};
#[cfg(feature = "npm-metadata")]
pub use npm_metadata::{
    BUN_TARGETS, NpmMetadataClient, NpmToolRelease, NpmToolTarget, PNPM_TARGETS, tool_targets,
    validate_exact_npm_tool_version,
};
#[cfg(all(feature = "installer", feature = "npm-metadata"))]
pub use npm_runtime::install_locked_npm_tool;
#[cfg(feature = "project-discovery")]
pub use project_discovery::{
    DiscoveryFinding, DiscoveryKind, DiscoveryReport, DiscoveryStatus, scan_project_sources,
};
pub use provenance::{
    MinimumReleaseAge, VerificationMethod, VerificationStrength, tool_verification_strength,
    valid_release_time, validate_tool_policy, validate_verification_transition,
    verification_method,
};
#[cfg(feature = "provider-registry")]
pub use provider_registry::{
    DeclarativeProvenanceCapabilities, DeclarativeProviderCapabilities,
    DeclarativeProviderManifest, ProviderRegistryDocument, VerifiedProviderRegistry,
    embedded_provider_registry, load_signed_provider_registry, verify_signed_provider_registry,
};
#[cfg(feature = "python-metadata")]
pub use python_metadata::{PythonMetadataClient, PythonRelease};
#[cfg(feature = "python-provider")]
pub use python_provider::{
    PYTHON_TARGETS, PYTHON_VARIANT, PythonArtifactPlan, is_exact_python_version,
    parse_python_distribution, plan_python_artifact, validate_exact_python_version,
};
#[cfg(all(
    feature = "installer",
    feature = "python-provider",
    feature = "lockfile"
))]
pub use python_runtime::install_locked_python;
pub use python_venv::{
    PYTHON_ENVIRONMENT_DIR, PYTHON_ENVIRONMENT_MARKER, ProjectPythonEnvironment,
    create_project_python_environment, load_project_python_environment,
    project_python_command_candidates, project_python_environment_path,
};
pub use resolver::{
    CommandResolution, RuntimeEnvironmentVariable, SelectionSource, ToolSelection, command_tool,
    find_system_commands, java_home_for_install, managed_runtime_arguments,
    path_with_selected_runtime, path_with_selected_tools, pinset_home, pinset_home_from_env,
    resolve_command, resolve_command_with_path, resolve_from_env, resolve_project_python_command,
    resolve_tool_selection, runtime_command_candidates, runtime_command_directory,
    runtime_environment_for_install, selected_runtime_environment,
    validate_managed_runtime_invocation,
};
pub use runtime_lifecycle::{
    InstalledToolVersion, ProtectedToolVersion, PruneToolCandidate, PruneToolPlan,
    ToolVersionReference, UninstallToolOutcome, find_tool_version_references,
    find_tool_version_references_in_projects, list_all_installed_tool_versions,
    list_installed_tool_versions, plan_prune_tool_versions, plan_uninstall_tool_version,
    uninstall_tool_version,
};
pub use runtime_provider::{
    RuntimeCommandLayout, RuntimeDiscoveryKind, RuntimeDiscoveryRule, RuntimeEnvironmentKind,
    RuntimeInstallKind, RuntimeLockAuditKind, RuntimeMetadataKind, RuntimeProvenanceCapabilities,
    RuntimeProvider, RuntimeProviderCapabilities, provider_dependency_order, runtime_provider,
    runtime_provider_for_command, runtime_providers, selected_provider_order,
    validate_provider_selections,
};
#[cfg(feature = "rust-metadata")]
pub use rust_metadata::{RustMetadataClient, RustRelease};
#[cfg(feature = "rust-provider")]
pub use rust_provider::{
    RUST_COMPONENTS, RUST_PROFILE, RUST_TARGETS, RustArchiveFormat, RustArtifactPlan, RustVersion,
    plan_rust_artifact, rust_target_triple, validate_exact_rust_version,
};
#[cfg(all(feature = "installer", feature = "rust-provider", feature = "lockfile"))]
pub use rust_runtime::install_locked_rust;
pub use shim_install::{
    ShimInstallMethod, ShimInstallResult, ensure_shims, install_shims, is_managed_command_shim,
    is_managed_shim,
};
#[cfg(feature = "sources")]
pub use source_config::{
    ResolvedArtifactSource, SUPPORTED_SOURCE_PROVIDERS, SourceConfig, SourceKind, SourceView,
    load_source_config, save_source_config, source_config_path,
};
pub use target::{current_target, current_target_for_tool};
#[cfg(feature = "state-write")]
pub use user_settings::save_user_settings;
pub use user_settings::{UserSettings, load_user_settings, user_settings_path};

pub fn pinset_version() -> &'static str {
    let version = option_env!("PINSET_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    version.strip_prefix('v').unwrap_or(version)
}
