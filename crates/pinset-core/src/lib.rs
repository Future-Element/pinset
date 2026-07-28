mod config;
mod error;
#[cfg(feature = "installer")]
mod installer;
#[cfg(feature = "node-provider")]
mod node_provider;
mod resolver;
mod shim_install;
#[cfg(feature = "sources")]
mod source_config;
mod target;

#[cfg(feature = "project-write")]
pub use config::create_project_config;
pub use config::{
    PROJECT_CONFIG_FILENAME, ProjectConfig, find_project_config, load_project_config,
};
pub use error::{Error, Result};
#[cfg(feature = "installer")]
pub use installer::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, InstallLimits,
    InstallOutcome, InstallRequest, Installer, sha256_hex,
};
#[cfg(feature = "node-provider")]
pub use node_provider::{NodeArchiveFormat, NodeArtifactPlan, plan_node_artifact};
pub use resolver::{
    CommandResolution, command_tool, pinset_home_from_env, resolve_command, resolve_from_env,
};
pub use shim_install::{ShimInstallMethod, ShimInstallResult, install_shims};
#[cfg(feature = "sources")]
pub use source_config::{
    ResolvedArtifactSource, SUPPORTED_SOURCE_PROVIDERS, SourceConfig, SourceKind, SourceView,
    load_source_config, save_source_config, source_config_path,
};
pub use target::current_target;
