mod config;
#[cfg(feature = "installer")]
mod download_cache;
mod error;
mod global_state;
#[cfg(feature = "installer")]
mod installer;
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
mod resolver;
mod shim_install;
#[cfg(feature = "sources")]
mod source_config;
mod target;
mod user_settings;

pub use config::{
    PROJECT_CONFIG_FILENAME, ProjectConfig, find_optional_project_config, find_project_config,
    load_project_config,
};
#[cfg(feature = "project-write")]
pub use config::{create_project_config, save_project_config};
#[cfg(feature = "installer")]
pub use download_cache::{
    DownloadCacheCleanOutcome, DownloadCacheEntry, clean_download_cache, download_cache_path,
    list_download_cache,
};
pub use error::{Error, Result};
#[cfg(feature = "state-write")]
pub use global_state::save_global_config;
#[cfg(all(feature = "state-write", feature = "lockfile"))]
pub use global_state::save_global_state;
pub use global_state::{
    GLOBAL_CONFIG_FILENAME, GLOBAL_LOCKFILE_FILENAME, GLOBAL_STATE_SCHEMA, GlobalConfig,
    global_config_path, global_lockfile_path, global_state_dir, load_global_config,
    load_optional_global_config,
};
#[cfg(feature = "installer")]
pub use installer::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, InstallLimits,
    InstallOutcome, InstallRequest, Installer, sha256_hex,
};
#[cfg(feature = "lockfile")]
pub use lockfile::{
    LOCKFILE_FILENAME, LOCKFILE_SCHEMA, LockedArtifact, LockedArtifactFormat, LockedTool, Lockfile,
    MVP_NODE_TARGETS, load_lockfile, load_optional_lockfile, lockfile_path, save_lockfile,
    validate_lock_matches_project, validate_lock_matches_selection,
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
pub use resolver::{
    CommandResolution, SelectionSource, ToolSelection, command_tool, find_system_commands,
    path_with_selected_runtime, pinset_home, pinset_home_from_env, resolve_command,
    resolve_command_with_path, resolve_from_env, resolve_tool_selection,
};
pub use shim_install::{ShimInstallMethod, ShimInstallResult, install_shims};
#[cfg(feature = "sources")]
pub use source_config::{
    ResolvedArtifactSource, SUPPORTED_SOURCE_PROVIDERS, SourceConfig, SourceKind, SourceView,
    load_source_config, save_source_config, source_config_path,
};
pub use target::current_target;
#[cfg(feature = "state-write")]
pub use user_settings::save_user_settings;
pub use user_settings::{UserSettings, load_user_settings, user_settings_path};
