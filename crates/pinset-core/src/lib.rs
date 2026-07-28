mod config;
mod error;
#[cfg(feature = "installer")]
mod installer;
#[cfg(feature = "lockfile")]
mod lockfile;
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

pub use config::{
    PROJECT_CONFIG_FILENAME, ProjectConfig, find_project_config, load_project_config,
};
#[cfg(feature = "project-write")]
pub use config::{create_project_config, save_project_config};
pub use error::{Error, Result};
#[cfg(feature = "installer")]
pub use installer::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, InstallLimits,
    InstallOutcome, InstallRequest, Installer, sha256_hex,
};
#[cfg(feature = "lockfile")]
pub use lockfile::{
    LOCKFILE_FILENAME, LOCKFILE_SCHEMA, LockedArtifact, LockedArtifactFormat, LockedTool, Lockfile,
    MVP_NODE_TARGETS, load_lockfile, load_optional_lockfile, lockfile_path, save_lockfile,
    validate_lock_matches_project,
};
#[cfg(feature = "node-metadata")]
pub use node_metadata::NodeMetadataClient;
#[cfg(feature = "node-provider")]
pub use node_provider::{NodeArchiveFormat, NodeArtifactPlan, plan_node_artifact};
#[cfg(all(feature = "installer", feature = "lockfile"))]
pub use node_runtime::{install_locked_node, node_command_directory};
pub use resolver::{
    CommandResolution, command_tool, pinset_home, pinset_home_from_env, resolve_command,
    resolve_from_env,
};
pub use shim_install::{ShimInstallMethod, ShimInstallResult, install_shims};
#[cfg(feature = "sources")]
pub use source_config::{
    ResolvedArtifactSource, SUPPORTED_SOURCE_PROVIDERS, SourceConfig, SourceKind, SourceView,
    load_source_config, save_source_config, source_config_path,
};
pub use target::current_target;
