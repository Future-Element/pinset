use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsString,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{self, Command},
    sync::Mutex,
    time::{Duration, Instant},
};

mod i18n;

#[cfg(windows)]
use std::ffi::OsStr;

use clap::{Parser, Subcommand, ValueEnum, error::ErrorKind};
use pinset_core::{
    ArtifactIntegrity, DiscoveryReport, DiscoveryStatus, DotnetMetadataClient,
    DownloadProgressEvent, Error, FlutterMetadataClient, GlobalConfig, GoMetadataClient,
    InstallLimits, Installer, JavaMetadataClient, LockAuditReport, LockAuditScope,
    LockAuditSeverity, LockedTool, Lockfile, NodeMetadataClient, NpmMetadataClient,
    PROJECT_CONFIG_SCHEMA, ProjectConfig, PythonMetadataClient, RuntimeInstallKind,
    RuntimeMetadataKind, RustMetadataClient, SUPPORTED_SOURCE_PROVIDERS, ShimInstallMethod,
    SourceView, audit_global_lock, audit_project_lock, clean_download_cache, command_tool,
    create_project_config, create_project_python_environment, current_target_for_tool,
    download_cache_info, ensure_shims, find_optional_project_config, find_project_config,
    find_project_context, global_config_path, global_lockfile_path, import_download_cache,
    import_download_cache_with_integrity, install_locked_dotnet, install_locked_flutter,
    install_locked_go, install_locked_java, install_locked_node, install_locked_npm_tool,
    install_locked_python, install_locked_rust, is_managed_command_shim,
    list_all_installed_tool_versions, list_download_cache, list_installed_tool_versions,
    load_global_config, load_lockfile, load_optional_global_config, load_optional_lockfile,
    load_project_config, load_project_python_environment, load_source_config, load_user_settings,
    lockfile_path, managed_runtime_arguments, path_with_selected_tools, pinset_home,
    plan_prune_tool_versions, plan_uninstall_tool_version, project_python_environment_path,
    provider_dependency_order, repair_download_cache, resolve_command,
    resolve_project_python_command, resolve_tool_selection, runtime_command_candidates,
    runtime_command_directory, runtime_environment_for_install, runtime_provider,
    save_global_config, save_global_state, save_lockfile, save_project_config, save_project_state,
    save_source_config, save_user_settings, scan_project_sources, selected_runtime_environment,
    source_config_path, uninstall_node_version, uninstall_tool_version, user_settings_path,
    validate_exact_dotnet_version, validate_exact_flutter_version, validate_exact_go_version,
    validate_exact_java_version, validate_exact_node_version, validate_exact_npm_tool_version,
    validate_exact_python_version, validate_exact_rust_version, validate_lock_matches_selection,
    validate_lock_matches_tool, validate_lock_matches_tools, validate_managed_runtime_invocation,
    validate_project_lock_policy, verify_download_cache,
};
use serde::Serialize;
use terminal_size::{Width, terminal_size_of};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::i18n::{Catalog, Language};

#[derive(Debug, Parser)]
#[command(
    name = "pinset",
    version,
    about = "Predictable runtime version management for multilingual projects"
)]
struct Cli {
    /// UI language for this command. Without a subcommand, save it as the default.
    #[arg(long, global = true, value_name = "LANG")]
    lang: Option<Language>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a minimal pinset.toml in the current directory.
    Init,
    /// Detect traditional runtime version files without network or writes.
    Detect {
        /// Directory from which repository-bounded discovery starts.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Emit a stable machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Import traditional runtime selections into pinset.toml and pinset.lock.
    Import {
        /// Directory from which repository-bounded discovery starts.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Replace conflicting versions already selected by Pinset.
        #[arg(long)]
        force: bool,
        /// Write configuration and lock metadata without installing runtimes.
        #[arg(long)]
        no_install: bool,
    },
    /// Show or set a default runtime version used outside projects.
    Global {
        /// Selection such as node@lts, pnpm@11, bun@1.3, go@1.25, python@3.14, java@21, rust@stable or dotnet@lts.
        selection: Option<String>,
        /// Update the global selection and lock without downloading the runtime.
        #[arg(long, requires = "selection")]
        no_install: bool,
    },
    /// Select and lock a runtime version for the current project or globally.
    Use {
        /// Selection such as node@24, pnpm@11, bun@1.3, go@1.25, python@3.14, java@lts, rust@1.97 or dotnet@10.
        selection: String,
        /// Update the selection and lock without downloading the runtime.
        #[arg(long)]
        no_install: bool,
        /// Save the selection under PINSET_HOME instead of pinset.toml.
        #[arg(long)]
        global: bool,
    },
    /// Clear a project or global runtime selection without uninstalling anything.
    Unset {
        /// Tool to clear: node, pnpm, bun, go, python, flutter, java, rust or dotnet.
        tool: String,
        /// Clear the global default instead of the nearest project selection.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset configuration is updated.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
    },
    /// Install an explicit runtime version or every project/global lockfile target.
    Install {
        /// Install a runtime version without changing project or global selection.
        #[arg(conflicts_with_all = ["locked", "global", "cwd"])]
        selection: Option<String>,
        /// Require the selected config and lockfile to match. This is the default.
        #[arg(long)]
        locked: bool,
        /// Install the globally selected runtime.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
    },
    /// Print the exact runtime executable selected for a command.
    Which {
        command: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Explain the project/global/system candidate chain.
        #[arg(long)]
        explain: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Print the effective project, global or system selection and executable path.
    Current {
        /// Tool to inspect. Defaults to node.
        tool: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Explain the project/global/system candidate chain.
        #[arg(long)]
        explain: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// List installed or officially available runtime versions.
    List {
        /// Tool to list: node, pnpm, bun, go, python, flutter, java, rust or dotnet.
        tool: Option<String>,
        /// Query the official provider index instead of local installations.
        #[arg(long, requires = "tool")]
        available: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Check selected project and global runtimes against the latest stable releases.
    Outdated {
        /// Limit the check to one runtime provider.
        tool: Option<String>,
        /// Check only the global selections.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset configuration is checked.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Re-resolve configured selectors and update exact lock records without installing.
    Update {
        /// Limit the update to one runtime provider.
        tool: Option<String>,
        /// Update only global selections.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset configuration is updated.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
        /// Report the proposed lock changes without writing them.
        #[arg(long)]
        dry_run: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Upgrade project or global configuration and lock data to schema 3.
    Migrate {
        /// Migrate the global selection state instead of a project.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset state is migrated.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
        /// Report the schema change without writing it.
        #[arg(long)]
        dry_run: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Uninstall an exact runtime version owned by Pinset.
    Uninstall {
        /// Exact selection such as node@24.0.0.
        selection: String,
        /// Ignore current project and global selection references.
        #[arg(long)]
        force: bool,
        /// Project directory used for reference protection.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Show what would be removed without changing the installation.
        #[arg(long)]
        dry_run: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Remove installed runtime versions not selected globally or by the supplied projects.
    Prune {
        /// Project directory whose nearest Pinset configuration is protected.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Protect additional project selections. May be supplied more than once.
        #[arg(long, value_name = "PATH")]
        project: Vec<PathBuf>,
        /// Show what would be removed without changing installations.
        #[arg(long)]
        dry_run: bool,
        /// Emit a stable machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Audit configuration, locks, cached artifacts, receipts, and ownership without writes.
    Lock {
        #[command(subcommand)]
        command: LockCommands,
    },
    /// Inspect or clean verified runtime download archives.
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
    /// Execute through the selected runtime without enabling direct command routing.
    Exec {
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
    /// Install and execute one verified runtime selection without changing project state.
    X {
        /// Selection such as node@24, pnpm@11, bun@1.3, go@1.25, python@3.14, java@21, rust@stable or dotnet@lts.
        selection: String,
        /// Directory used for dependency selection and command execution.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Runtime command and arguments, normally separated with `--`.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
    /// Report project, lockfile, installation and PATH state without modifying anything.
    Doctor {
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Emit a stable machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Manage the Pinset-owned project Python environment without shell activation.
    Venv {
        #[command(subcommand)]
        command: VenvCommands,
    },
    /// Inspect or repair Provider command routing in a user-owned directory.
    Shim {
        #[command(subcommand)]
        command: ShimCommands,
    },
    /// Print shell code that enables provider command routing through Pinset.
    Activate {
        #[arg(value_enum)]
        shell: ActivationShell,
    },
    /// Generate lightweight command completion for a supported shell.
    Completions {
        #[arg(value_enum)]
        shell: ActivationShell,
    },
    /// Manage local download sources without changing project lock files.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Inspect the signed declarative Provider Registry preview.
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
}

#[derive(Debug, Subcommand)]
enum VenvCommands {
    /// Install the selected CPython runtime and create or validate .venv.
    Create {
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Show the selected CPython distribution and managed .venv path.
    Status {
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Delete and recreate .venv after verifying Pinset ownership.
    Recreate {
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum ShimCommands {
    /// Print the user-owned directory containing Pinset command shims.
    Path,
    /// Repair provider command shims without overwriting existing files.
    Install {
        /// pinset-shim binary. Defaults to the binary next to pinset.
        #[arg(long)]
        binary: Option<PathBuf>,
        /// Destination directory. Defaults to the active Pinset command-routing directory.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Install every command declared by this runtime provider.
        #[arg(long, conflicts_with = "commands")]
        provider: Option<String>,
        /// Advanced override: install these command names instead of a provider manifest.
        #[arg(value_name = "COMMAND", conflicts_with = "provider")]
        commands: Vec<String>,
    },
    /// Register configured provider commands in the current routing directory and preserve old entries.
    Migrate {
        /// Migrate every command declared by this runtime provider.
        #[arg(long)]
        provider: Option<String>,
        /// Destination directory. Defaults to the active Pinset command-routing directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ActivationShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

#[derive(Debug, Subcommand)]
enum CacheCommands {
    /// List content-addressed archives in the Pinset download cache.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show complete and partial download cache usage.
    Info {
        #[arg(long)]
        json: bool,
    },
    /// Hash every complete archive and compare it with its cache identity.
    Verify {
        #[arg(long)]
        json: bool,
    },
    /// Remove corrupt complete archives so a later install can download them again.
    Repair {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove content-addressed archives from the Pinset download cache.
    Clean {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Import a verified runtime archive into the content-addressed offline cache.
    Import {
        archive: PathBuf,
        /// Expected SHA-256 from a reviewed pinset.lock or upstream manifest.
        #[arg(long, conflicts_with = "integrity")]
        sha256: Option<String>,
        /// Expected SRI or canonical integrity, for example sha512-<base64>.
        #[arg(long, conflicts_with = "sha256")]
        integrity: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum LockCommands {
    /// Audit one project or global lock without network access or state changes.
    Audit {
        /// Audit the global selection instead of the nearest project.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset configuration is audited.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
        /// Emit stable reason codes in the JSON schema 1 envelope.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SourceCommands {
    /// List built-in and custom sources.
    List {
        /// Limit output to node, go, python or flutter.
        provider: Option<String>,
    },
    /// Add a custom source. HTTPS is required by default.
    Add {
        provider: String,
        alias: String,
        #[arg(long)]
        base_url: String,
        /// Allow an HTTP source, intended only for explicitly trusted LAN services.
        #[arg(long)]
        allow_insecure: bool,
        /// Trust this HTTPS source for provider metadata as well as archives.
        #[arg(long, conflicts_with = "allow_insecure")]
        trust_metadata: bool,
    },
    /// Select the active source.
    Use { provider: String, alias: String },
    /// Replace the ordered fallback list. Pass no aliases to clear it.
    Fallback {
        provider: String,
        aliases: Vec<String>,
    },
    /// Remove an inactive custom source.
    Remove { provider: String, alias: String },
    /// Read-only connectivity and provider metadata validation for one source.
    Test {
        provider: String,
        /// Source alias. Defaults to the active source.
        alias: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommands {
    /// List manifests from the embedded signed Registry.
    List {
        /// Emit the verified Registry using the stable JSON envelope.
        #[arg(long)]
        json: bool,
    },
    /// Verify an official clear-signed Registry file without activating Providers.
    Verify {
        /// Registry file. Omit it to verify the copy embedded in this binary.
        registry: Option<PathBuf>,
        /// Emit the verified Registry using the stable JSON envelope.
        #[arg(long)]
        json: bool,
    },
}

impl Cli {
    fn json_command(&self) -> Option<&'static str> {
        self.command.as_ref()?.json_command()
    }
}

impl Commands {
    fn json_command(&self) -> Option<&'static str> {
        match self {
            Self::Detect { json: true, .. } => Some("detect"),
            Self::Which { json: true, .. } => Some("which"),
            Self::Current { json: true, .. } => Some("current"),
            Self::List { json: true, .. } => Some("list"),
            Self::Outdated { json: true, .. } => Some("outdated"),
            Self::Update { json: true, .. } => Some("update"),
            Self::Migrate { json: true, .. } => Some("migrate"),
            Self::Uninstall { json: true, .. } => Some("uninstall"),
            Self::Prune { json: true, .. } => Some("prune"),
            Self::Doctor { json: true, .. } => Some("doctor"),
            Self::Lock { command } => command.json_command(),
            Self::Cache { command } => command.json_command(),
            Self::Provider { command } => command.json_command(),
            _ => None,
        }
    }
}

impl ProviderCommands {
    fn json_command(&self) -> Option<&'static str> {
        match self {
            Self::List { json: true } => Some("provider.list"),
            Self::Verify { json: true, .. } => Some("provider.verify"),
            _ => None,
        }
    }
}

impl LockCommands {
    fn json_command(&self) -> Option<&'static str> {
        match self {
            Self::Audit { json: true, .. } => Some("lock.audit"),
            _ => None,
        }
    }
}

impl CacheCommands {
    fn json_command(&self) -> Option<&'static str> {
        match self {
            Self::List { json: true } => Some("cache.list"),
            Self::Info { json: true } => Some("cache.info"),
            Self::Verify { json: true } => Some("cache.verify"),
            Self::Repair { json: true, .. } => Some("cache.repair"),
            Self::Clean { json: true, .. } => Some("cache.clean"),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct JsonSuccess<T> {
    schema: u32,
    command: &'static str,
    ok: bool,
    data: T,
}

#[derive(Serialize)]
struct JsonFailure<'a> {
    schema: u32,
    command: &'a str,
    ok: bool,
    error: JsonErrorBody<'a>,
}

#[derive(Serialize)]
struct JsonErrorBody<'a> {
    code: &'static str,
    message: &'a str,
    details: serde_json::Value,
}

fn print_json_success<T: Serialize>(
    command: &'static str,
    data: T,
) -> Result<(), serde_json::Error> {
    // INVARIANT: schema, command, ok, and data are the v1 automation boundary. Human messages
    // remain localized outside this envelope and may evolve independently.
    println!(
        "{}",
        serde_json::to_string_pretty(&JsonSuccess {
            schema: 1,
            command,
            ok: true,
            data,
        })?
    );
    Ok(())
}

fn print_json_failure(
    command: &str,
    code: &'static str,
    message: &str,
    details: serde_json::Value,
) {
    let output = serde_json::to_string_pretty(&JsonFailure {
        schema: 1,
        command,
        ok: false,
        error: JsonErrorBody {
            code,
            message,
            details,
        },
    })
    .expect("the fixed JSON error envelope is serializable");
    println!("{output}");
}

fn requested_json_command(arguments: &[OsString]) -> Option<String> {
    if !arguments.iter().any(|value| value == "--json") {
        return None;
    }
    let values = arguments
        .iter()
        .skip(1)
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>();
    let top_level = values.iter().position(|value| {
        matches!(
            value.as_ref(),
            "detect"
                | "which"
                | "current"
                | "list"
                | "outdated"
                | "update"
                | "migrate"
                | "uninstall"
                | "prune"
                | "doctor"
                | "lock"
                | "cache"
                | "provider"
        )
    });
    let Some(index) = top_level else {
        return Some("pinset".to_owned());
    };
    if values[index] != "cache" && values[index] != "lock" && values[index] != "provider" {
        return Some(values[index].as_ref().to_owned());
    }
    let group = values[index].as_ref();
    let subcommand = values[index + 1..].iter().find(|value| match group {
        "cache" => matches!(
            value.as_ref(),
            "list" | "info" | "verify" | "repair" | "clean"
        ),
        "lock" => value.as_ref() == "audit",
        "provider" => matches!(value.as_ref(), "list" | "verify"),
        _ => false,
    });
    Some(match subcommand {
        Some(subcommand) => format!("{group}.{subcommand}"),
        None => group.to_owned(),
    })
}

fn json_error(error: &(dyn std::error::Error + 'static)) -> (&'static str, serde_json::Value) {
    if error.downcast_ref::<std::io::Error>().is_some() {
        return ("io_error", serde_json::json!({}));
    }
    let Some(error) = error.downcast_ref::<Error>() else {
        return ("internal_error", serde_json::json!({}));
    };
    let code = match error {
        Error::UnsupportedSourceProvider { .. }
        | Error::UnsupportedRuntimeProvider { .. }
        | Error::UnsupportedNodeTarget { .. }
        | Error::UnsupportedGoTarget { .. }
        | Error::UnsupportedFlutterTarget { .. }
        | Error::UnsupportedPythonTarget { .. }
        | Error::UnsupportedJavaTarget { .. }
        | Error::UnsupportedRustTarget { .. }
        | Error::UnsupportedDotnetTarget { .. } => "unsupported_provider",
        Error::ProviderDependencyMissing { .. }
        | Error::ProviderDependencyUnknown { .. }
        | Error::ProviderDependencyCycle { .. } => "provider_dependency_failed",
        Error::InvalidNodeSelector { .. }
        | Error::InvalidGoSelector { .. }
        | Error::InvalidFlutterSelector { .. }
        | Error::InvalidPythonSelector { .. }
        | Error::InvalidJavaSelector { .. }
        | Error::InvalidRustSelector { .. }
        | Error::InvalidDotnetSelector { .. }
        | Error::InvalidNpmToolSelector { .. }
        | Error::InvalidNodeVersion { .. }
        | Error::InvalidGoVersion { .. }
        | Error::InvalidFlutterVersion { .. }
        | Error::InvalidPythonVersion { .. }
        | Error::InvalidJavaVersion { .. }
        | Error::InvalidRustVersion { .. }
        | Error::InvalidDotnetVersion { .. }
        | Error::InvalidToolVersion { .. } => "invalid_selector",
        Error::NodeSelectorNotFound { .. }
        | Error::GoSelectorNotFound { .. }
        | Error::FlutterSelectorNotFound { .. }
        | Error::PythonSelectorNotFound { .. }
        | Error::JavaSelectorNotFound { .. }
        | Error::RustSelectorNotFound { .. }
        | Error::DotnetSelectorNotFound { .. }
        | Error::NpmToolSelectorNotFound { .. }
        | Error::ToolSelectionNotFound { .. }
        | Error::CommandSelectionNotFound { .. }
        | Error::ToolNotConfigured { .. }
        | Error::ProjectToolSelectionRequired { .. }
        | Error::LockedToolMissing { .. }
        | Error::LockedArtifactMissing { .. } => "selection_missing",
        Error::ReadProjectConfig { .. }
        | Error::ParseProjectConfig { .. }
        | Error::UnsupportedSchema { .. }
        | Error::GlobalConfigNotFound { .. }
        | Error::ReadGlobalConfig { .. }
        | Error::ParseGlobalConfig { .. }
        | Error::UnsupportedGlobalConfigSchema { .. }
        | Error::ReadSourceConfig { .. }
        | Error::ParseSourceConfig { .. }
        | Error::UnsupportedSourceSchema { .. }
        | Error::ReadUserSettings { .. }
        | Error::ParseUserSettings { .. }
        | Error::UnsupportedUserSettingsSchema { .. } => "config_error",
        Error::ReadLockfile { .. }
        | Error::ParseLockfile { .. }
        | Error::UnsupportedLockfileSchema { .. }
        | Error::InvalidLockfile { .. }
        | Error::LockfileMismatch { .. } => "lockfile_error",
        Error::RuntimeCommandNotFound { .. }
        | Error::RuntimeCommandDirectoryMissing { .. }
        | Error::NodeVersionNotInstalled { .. }
        | Error::ToolVersionNotInstalled { .. }
        | Error::PythonEnvironmentMissing { .. }
        | Error::PythonEnvironmentSelectionMissing { .. } => "runtime_missing",
        Error::NodeMetadataRequest { .. }
        | Error::NodeMetadataRead { .. }
        | Error::GoMetadataRequest { .. }
        | Error::GoMetadataRead { .. }
        | Error::FlutterMetadataRequest { .. }
        | Error::FlutterMetadataRead { .. }
        | Error::PythonMetadataRequest { .. }
        | Error::PythonMetadataRead { .. }
        | Error::JavaMetadataRequest { .. }
        | Error::JavaMetadataRead { .. }
        | Error::RustMetadataRequest { .. }
        | Error::RustMetadataRead { .. }
        | Error::DotnetMetadataRequest { .. }
        | Error::DotnetMetadataRead { .. }
        | Error::NpmMetadataRequest { .. }
        | Error::NpmMetadataRead { .. }
        | Error::HttpClient { .. } => "metadata_request_failed",
        Error::NodeMetadataTooLarge { .. }
        | Error::NodeIndexTooLarge { .. }
        | Error::InvalidNodeIndex { .. }
        | Error::InvalidNodeShasums { .. }
        | Error::NodeChecksumMissing { .. }
        | Error::GoMetadataTooLarge { .. }
        | Error::InvalidGoIndex { .. }
        | Error::FlutterMetadataTooLarge { .. }
        | Error::InvalidFlutterIndex { .. }
        | Error::PythonMetadataTooLarge { .. }
        | Error::InvalidPythonIndex { .. }
        | Error::JavaMetadataTooLarge { .. }
        | Error::InvalidJavaIndex { .. }
        | Error::RustMetadataTooLarge { .. }
        | Error::InvalidRustIndex { .. }
        | Error::DotnetMetadataTooLarge { .. }
        | Error::InvalidDotnetIndex { .. }
        | Error::NpmMetadataTooLarge { .. }
        | Error::InvalidNpmMetadata { .. } => "metadata_invalid",
        Error::NodeSignatureInvalid { .. }
        | Error::NodeTrustStoreInvalid { .. }
        | Error::NpmSignatureVerification { .. } => "signature_invalid",
        Error::NodeSignerUntrusted { .. } => "signature_untrusted",
        Error::ProviderRegistrySignatureInvalid { .. } => "signature_invalid",
        Error::ReadProviderRegistry { .. } | Error::ProviderRegistryInvalid { .. } => {
            "provider_registry_invalid"
        }
        Error::VerificationPolicyViolation { .. } => "verification_policy_failed",
        Error::VerificationDowngrade { .. } => "verification_downgrade",
        Error::ReleaseAgeUnavailable { .. } | Error::ReleaseTooNew { .. } => {
            "release_age_policy_failed"
        }
        Error::InvalidSha256 { .. }
        | Error::InvalidArtifactIntegrity { .. }
        | Error::ChecksumMismatch { .. } => "artifact_integrity_failed",
        Error::NodeVersionInUse { .. }
        | Error::ToolVersionInUse { .. }
        | Error::SourceInUse { .. } => "in_use",
        Error::UnsafeNodeInstallEntry { .. }
        | Error::UnsafeToolInstallEntry { .. }
        | Error::UnsafeDownloadCacheEntry { .. }
        | Error::UnsafeArchiveEntry { .. }
        | Error::InvalidRequiredPath { .. }
        | Error::InvalidShimSource { .. }
        | Error::PythonEnvironmentNotOwned { .. }
        | Error::InvalidPythonEnvironmentMarker { .. } => "unsafe_path",
        Error::DownloadCacheCorrupt { .. } => "cache_corrupt",
        Error::DownloadRequest { .. }
        | Error::DownloadRead { .. }
        | Error::DownloadTooLarge { .. }
        | Error::ArtifactSourcesExhausted { .. }
        | Error::RequiredPathMissing { .. }
        | Error::InstallAlreadyExists { .. }
        | Error::OpenZip { .. }
        | Error::ReadZipEntry { .. }
        | Error::ReadTarArchive { .. }
        | Error::DuplicateArchiveEntry { .. }
        | Error::TooManyArchiveEntries { .. }
        | Error::ArchiveTooLarge { .. }
        | Error::ExtractArchiveEntry { .. }
        | Error::CommitInstall { .. } => "install_failed",
        Error::UnsupportedCommand { .. } => "usage_error",
        _ => "io_error",
    };
    let details = match error {
        Error::UnsupportedRuntimeProvider { provider }
        | Error::UnsupportedSourceProvider { provider } => {
            serde_json::json!({ "provider": provider })
        }
        Error::NodeSignerUntrusted { signer } => serde_json::json!({ "signer": signer }),
        Error::DownloadCacheCorrupt { entries } => serde_json::json!({ "entries": entries }),
        Error::UnsupportedCommand { command } => serde_json::json!({ "command": command }),
        _ => serde_json::json!({}),
    };
    (code, details)
}

fn main() {
    process::exit(main_exit_code());
}

fn main_exit_code() -> i32 {
    let arguments = env::args_os().collect::<Vec<_>>();
    let raw_json_command = requested_json_command(&arguments);
    let requested_language = match language_from_arguments(&arguments)
        .and_then(|language| language.map_or_else(language_from_env, |language| Ok(Some(language))))
    {
        Ok(language) => language,
        Err(error) => {
            let message = Catalog::new(Language::default()).error(error);
            if let Some(command) = &raw_json_command {
                print_json_failure(command, "usage_error", &message, serde_json::json!({}));
            } else {
                eprintln!("{message}");
            }
            return 2;
        }
    };
    let language = match resolve_language(requested_language) {
        Ok(language) => language,
        Err(error) => {
            let catalog = Catalog::new(requested_language.unwrap_or_default());
            let message = catalog.error(error);
            if let Some(command) = &raw_json_command {
                print_json_failure(command, "usage_error", &message, serde_json::json!({}));
            } else {
                eprintln!("{message}");
            }
            return 2;
        }
    };
    let catalog = Catalog::new(language);
    let help_command = requested_help_command(&arguments);
    if language == Language::SimplifiedChinese && help_command.is_some() {
        println!("{}", catalog.command_help(help_command.flatten()));
        return 0;
    }
    let cli = match Cli::try_parse_from(&arguments) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return 0;
        }
        Err(error) if raw_json_command.is_some() => {
            let command = raw_json_command.as_deref().expect("checked");
            let message = if language == Language::SimplifiedChinese {
                let kind = match error.kind() {
                    ErrorKind::MissingRequiredArgument => "missing",
                    ErrorKind::UnknownArgument | ErrorKind::InvalidSubcommand => "unknown",
                    ErrorKind::InvalidValue | ErrorKind::ValueValidation => "invalid",
                    ErrorKind::ArgumentConflict => "conflict",
                    _ => "other",
                };
                catalog.argument_error(kind).to_owned()
            } else {
                error.to_string()
            };
            print_json_failure(command, "usage_error", &message, serde_json::json!({}));
            return 2;
        }
        Err(error) if language == Language::SimplifiedChinese => {
            let kind = match error.kind() {
                ErrorKind::MissingRequiredArgument => "missing",
                ErrorKind::UnknownArgument | ErrorKind::InvalidSubcommand => "unknown",
                ErrorKind::InvalidValue | ErrorKind::ValueValidation => "invalid",
                ErrorKind::ArgumentConflict => "conflict",
                _ => "other",
            };
            eprintln!("{}", catalog.argument_error(kind));
            eprintln!(
                "\n{}",
                catalog.command_help(command_from_arguments(&arguments))
            );
            return 2;
        }
        Err(error) => {
            let _ = error.print();
            return 2;
        }
    };
    let json_command = cli.json_command();
    match run(cli, catalog) {
        Ok(code) => code,
        Err(error) => {
            let message = catalog.command_error(error.as_ref());
            if let Some(command) = json_command {
                let (code, details) = json_error(error.as_ref());
                print_json_failure(command, code, &message, details);
            } else {
                eprintln!("{message}");
            }
            2
        }
    }
}

fn run(cli: Cli, catalog: Catalog) -> Result<i32, Box<dyn std::error::Error>> {
    let Some(command) = cli.command else {
        if let Some(language) = cli.lang {
            let home = pinset_home()?;
            let path = user_settings_path(&home);
            let mut settings = load_user_settings(&path)?;
            settings.language = Some(language.as_str().to_owned());
            save_user_settings(&path, &settings)?;
            println!("{}", catalog.language_saved(&path));
        } else {
            println!("{}", catalog.top_level_help());
        }
        return Ok(0);
    };

    match command {
        Commands::Init => {
            let path = create_project_config(&env::current_dir()?)?;
            println!("{}", catalog.created(&path));
        }
        Commands::Detect { cwd, json } => {
            let report = scan_project_sources(&effective_cwd(cwd)?)?;
            if json {
                print_json_success("detect", report)?;
            } else {
                print_discovery_report(&report, catalog);
            }
        }
        Commands::Import {
            cwd,
            force,
            no_install,
        } => run_project_import(&effective_cwd(cwd)?, force, no_install, catalog)?,
        Commands::Global {
            selection,
            no_install,
        } => {
            if let Some(selection) = selection {
                let cwd = env::current_dir()?;
                select_tool(&selection, true, no_install, false, &cwd, catalog)?;
                print_project_override(&cwd, &pinset_home()?, catalog)?;
            } else {
                print_global_current(catalog)?;
                print_project_override(&env::current_dir()?, &pinset_home()?, catalog)?;
            }
        }
        Commands::Use {
            selection,
            no_install,
            global,
        } => {
            let cwd = env::current_dir()?;
            select_tool(&selection, global, no_install, false, &cwd, catalog)?
        }
        Commands::Unset { tool, global, cwd } => {
            require_provider(&tool)?;
            unset_tool(&tool, global, &effective_cwd(cwd)?, catalog)?;
        }
        Commands::Install {
            selection,
            locked: _,
            global,
            cwd,
        } => {
            if let Some(selection) = selection {
                install_tool_selection(&selection, catalog)?;
            } else if global {
                install_global(&pinset_home()?, catalog)?;
            } else {
                install_project(&effective_cwd(cwd)?, catalog)?;
            }
        }
        Commands::Which {
            command,
            cwd,
            explain,
            json,
        } => {
            let cwd = effective_cwd(cwd)?;
            let resolution = match resolve_command(&command, &cwd, &pinset_home()?) {
                Ok(resolution) => resolution,
                Err(error) => {
                    if explain && !json {
                        if let Some(tool) = command_tool(&command) {
                            if let Ok(explanation) = resolution_explanation(&cwd, tool, "none") {
                                print_resolution_explanation(&explanation);
                            }
                        }
                    }
                    return Err(error.into());
                }
            };
            let explanation = explain
                .then(|| resolution_explanation(&cwd, &resolution.tool, resolution.source.as_str()))
                .transpose()?;
            if json {
                print_json_success(
                    "which",
                    WhichReport {
                        command,
                        tool: resolution.tool,
                        requested: resolution.requested,
                        version: resolution.version,
                        source: resolution.source.as_str(),
                        executable: resolution.executable,
                        config: resolution.selection_path,
                        explanation,
                    },
                )?;
            } else {
                if let Some(explanation) = explanation.as_ref() {
                    print_resolution_explanation(explanation);
                }
                println!("{}", resolution.executable.display());
            }
        }
        Commands::Current {
            tool,
            cwd,
            explain,
            json,
        } => {
            let cwd = effective_cwd(cwd)?;
            let tool = tool.as_deref().unwrap_or("node");
            if json {
                print_json_success("current", current_report(&cwd, tool, explain)?)?;
            } else {
                print_current(&cwd, tool, explain, catalog)?;
            }
        }
        Commands::List {
            tool,
            available,
            json,
        } => run_list(tool.as_deref(), available, json, catalog)?,
        Commands::Outdated {
            tool,
            global,
            cwd,
            json,
        } => run_outdated(tool.as_deref(), global, cwd, json)?,
        Commands::Update {
            tool,
            global,
            cwd,
            dry_run,
            json,
        } => run_update(tool.as_deref(), global, cwd, dry_run, json)?,
        Commands::Migrate {
            global,
            cwd,
            dry_run,
            json,
        } => run_migrate(global, cwd, dry_run, json)?,
        Commands::Uninstall {
            selection,
            force,
            cwd,
            dry_run,
            json,
        } => run_uninstall(&selection, force, cwd, dry_run, json, catalog)?,
        Commands::Prune {
            cwd,
            project,
            dry_run,
            json,
        } => run_prune(cwd, &project, dry_run, json)?,
        Commands::Lock { command } => return run_lock_command(command, catalog),
        Commands::Cache { command } => run_cache(command, catalog)?,
        Commands::Exec { cwd, command } => {
            let cwd = effective_cwd(cwd)?;
            return execute_selected(&cwd, &command, false, catalog);
        }
        Commands::X {
            selection,
            cwd,
            command,
        } => {
            let cwd = effective_cwd(cwd)?;
            let mut selected_command = Vec::with_capacity(command.len() + 1);
            selected_command.push(OsString::from(selection));
            selected_command.extend(command);
            return execute_selected(&cwd, &selected_command, true, catalog);
        }
        Commands::Doctor { cwd, json } => {
            let cwd = effective_cwd(cwd)?;
            if json {
                print_json_success("doctor", doctor_report(&cwd)?)?;
            } else {
                run_doctor(&cwd, catalog)?;
            }
        }
        Commands::Venv { command } => run_venv_command(command, catalog)?,
        Commands::Shim { command } => match command {
            ShimCommands::Path => {
                println!("{}", command_routing_directory(&pinset_home()?)?.display())
            }
            ShimCommands::Install {
                binary,
                dir,
                provider,
                commands,
            } => {
                let binary = binary
                    .as_deref()
                    .map(absolutize)
                    .transpose()?
                    .unwrap_or(default_shim_binary()?);
                let dir = dir
                    .as_deref()
                    .map(absolutize)
                    .transpose()?
                    .unwrap_or(command_routing_directory(&pinset_home()?)?);
                let commands = manual_shim_commands(
                    provider.as_deref(),
                    &commands,
                    &env::current_dir()?,
                    &pinset_home()?,
                )?;
                for result in ensure_shims(&binary, &dir, &commands)? {
                    let method = match result.method {
                        ShimInstallMethod::Symlink => "symbolic-link",
                        ShimInstallMethod::Wrapper => "wrapper",
                        ShimInstallMethod::HardLink => "hard-link",
                        ShimInstallMethod::Copy => "copy",
                        ShimInstallMethod::Existing => "existing",
                    };
                    println!(
                        "{}",
                        catalog.shim_installed(&result.command, &result.destination, method)
                    );
                }
                println!("{}", catalog.shim_path_ready(&dir));
            }
            ShimCommands::Migrate { provider, dir } => {
                migrate_provider_shims(provider.as_deref(), dir.as_deref(), catalog)?;
            }
        },
        Commands::Activate { shell } => {
            println!(
                "{}",
                activation_script(shell, &command_routing_directory(&pinset_home()?)?)
            );
        }
        Commands::Completions { shell } => print_completions(shell),
        Commands::Source { command } => run_source_command(command, catalog)?,
        Commands::Provider { command } => run_provider_command(command)?,
    }

    Ok(0)
}

#[derive(Debug, Serialize)]
struct WhichReport {
    command: String,
    tool: String,
    requested: Option<String>,
    version: String,
    source: &'static str,
    executable: PathBuf,
    config: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<ResolutionExplanation>,
}

#[derive(Debug, Serialize)]
struct CurrentReport {
    command: String,
    tool: String,
    requested: Option<String>,
    version: String,
    source: &'static str,
    installed: bool,
    executable: Option<PathBuf>,
    expected_directory: Option<PathBuf>,
    config: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<ResolutionExplanation>,
}

#[derive(Debug, Serialize)]
struct ResolutionExplanation {
    start: PathBuf,
    boundary: PathBuf,
    project_config: Option<PathBuf>,
    project_strict: bool,
    global_eligible: bool,
    system_eligible: bool,
    selected_source: String,
    fallback_used: bool,
    candidates: Vec<ResolutionCandidate>,
    traditional_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ResolutionCandidate {
    source: &'static str,
    config: Option<PathBuf>,
    requested: Option<String>,
    resolved: Option<String>,
    status: &'static str,
    reason: String,
}

#[derive(Debug, Serialize)]
struct AvailableVersionReport {
    tool: String,
    version: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    details: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct OutdatedReport {
    scope: &'static str,
    config: PathBuf,
    tool: String,
    requested: String,
    current: String,
    latest_compatible: String,
    latest: String,
    update_available: bool,
    upgrade_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedRuntime {
    scope: &'static str,
    config: PathBuf,
    tool: String,
    requested: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct UpdateReport {
    scope: &'static str,
    config: PathBuf,
    tool: String,
    requested: String,
    previous: String,
    resolved: String,
    changed: bool,
}

#[derive(Debug, Serialize)]
struct MigrationReport {
    scope: &'static str,
    config: PathBuf,
    lockfile: PathBuf,
    from_config_schema: u32,
    from_lock_schema: u32,
    to_schema: u32,
    changed: bool,
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct UninstallReport {
    dry_run: bool,
    tool: String,
    version: String,
    targets: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PruneExecutionReport {
    dry_run: bool,
    candidates: Vec<pinset_core::PruneToolCandidate>,
    protected: Vec<pinset_core::ProtectedToolVersion>,
    bytes: u64,
    removed: usize,
}

#[derive(Debug, Serialize)]
struct CacheMutationReport {
    dry_run: bool,
    entries: usize,
    bytes: u64,
}

fn current_report(
    cwd: &Path,
    requested: &str,
    explain: bool,
) -> Result<CurrentReport, Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let provider = runtime_provider(requested)
        .or_else(|| command_tool(requested).and_then(runtime_provider))
        .ok_or_else(|| Error::UnsupportedCommand {
            command: requested.to_owned(),
        })?;
    let command = if provider.commands.contains(&requested) {
        requested
    } else {
        provider
            .commands
            .first()
            .copied()
            .ok_or_else(|| Error::UnsupportedCommand {
                command: requested.to_owned(),
            })?
    };
    match resolve_command(command, cwd, &home) {
        Ok(resolution) => {
            let explanation = explain
                .then(|| resolution_explanation(cwd, provider.tool, resolution.source.as_str()))
                .transpose()?;
            Ok(CurrentReport {
                command: command.to_owned(),
                tool: resolution.tool,
                requested: resolution.requested,
                version: resolution.version,
                source: resolution.source.as_str(),
                installed: true,
                executable: Some(resolution.executable),
                expected_directory: None,
                config: resolution.selection_path,
                explanation,
            })
        }
        Err(Error::RuntimeCommandNotFound { .. }) => {
            let selection = resolve_tool_selection(provider.tool, cwd, &home)?;
            let install_dir = home
                .join("installs")
                .join(provider.tool)
                .join(&selection.version)
                .join(current_target_for_tool(provider.tool));
            Ok(CurrentReport {
                command: command.to_owned(),
                tool: provider.tool.to_owned(),
                requested: Some(selection.requested),
                version: selection.version,
                source: selection.source.as_str(),
                installed: false,
                executable: None,
                expected_directory: Some(runtime_command_directory(provider.tool, &install_dir)),
                config: Some(selection.config_path),
                explanation: explain
                    .then(|| resolution_explanation(cwd, provider.tool, selection.source.as_str()))
                    .transpose()?,
            })
        }
        Err(error) => Err(error.into()),
    }
}

fn resolution_explanation(
    cwd: &Path,
    tool: &str,
    selected_source: &str,
) -> Result<ResolutionExplanation, Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let context = find_project_context(cwd)?;
    let mut candidates = Vec::new();
    let mut project_strict = false;
    let mut global_eligible = true;
    let mut system_eligible = true;
    if let Some(config_path) = context.config_path.as_ref() {
        let config = load_project_config(config_path)?;
        project_strict = !config.policy.inherit_global && !config.policy.system_fallback;
        global_eligible = config.policy.inherit_global;
        system_eligible = config.policy.system_fallback;
        if let Some(requested) = config.tools.get(tool) {
            let resolved = selected_version_from_lock(
                tool,
                requested,
                config.schema,
                config_path,
                &lockfile_path(config_path),
            )?;
            candidates.push(ResolutionCandidate {
                source: "project",
                config: Some(config_path.clone()),
                requested: Some(requested.clone()),
                resolved: Some(resolved),
                status: if selected_source == "project" {
                    "selected"
                } else {
                    "available"
                },
                reason: "nearest project selection".to_owned(),
            });
        } else {
            candidates.push(ResolutionCandidate {
                source: "project",
                config: Some(config_path.clone()),
                requested: None,
                resolved: None,
                status: "missing",
                reason: if project_strict {
                    "strict project does not declare this tool"
                } else if config.policy.inherit_global {
                    "project allows global inheritance"
                } else {
                    "project allows system fallback"
                }
                .to_owned(),
            });
        }
    } else {
        candidates.push(ResolutionCandidate {
            source: "project",
            config: None,
            requested: None,
            resolved: None,
            status: "not-found",
            reason: "no Pinset project exists inside the effective boundary".to_owned(),
        });
    }

    let global_path = global_config_path(&home);
    if let Some(global) = load_optional_global_config(&global_path)? {
        if let Some(requested) = global.tools.get(tool) {
            let resolved = selected_version_from_lock(
                tool,
                requested,
                global.schema,
                &global_path,
                &global_lockfile_path(&home),
            )?;
            candidates.push(ResolutionCandidate {
                source: "global",
                config: Some(global_path.clone()),
                requested: Some(requested.clone()),
                resolved: Some(resolved),
                status: if selected_source == "global" {
                    "selected"
                } else if !global_eligible {
                    "suppressed"
                } else {
                    "not-selected"
                },
                reason: if context.config_path.is_some() && !global_eligible {
                    "project policy does not allow global inheritance"
                } else {
                    "global selection is eligible"
                }
                .to_owned(),
            });
        } else {
            candidates.push(ResolutionCandidate {
                source: "global",
                config: Some(global_path.clone()),
                requested: None,
                resolved: None,
                status: "missing",
                reason: "global configuration does not declare this tool".to_owned(),
            });
        }
    } else {
        candidates.push(ResolutionCandidate {
            source: "global",
            config: Some(global_path),
            requested: None,
            resolved: None,
            status: "not-found",
            reason: "global configuration does not exist".to_owned(),
        });
    }
    candidates.push(ResolutionCandidate {
        source: "system",
        config: None,
        requested: None,
        resolved: None,
        status: if selected_source == "system" {
            "selected"
        } else if !system_eligible {
            "suppressed"
        } else {
            "not-selected"
        },
        reason: if context.config_path.is_some() && !system_eligible {
            "project policy does not allow system fallback"
        } else {
            "system PATH is the final eligible fallback"
        }
        .to_owned(),
    });

    let traditional_sources = scan_project_sources(cwd)?
        .findings
        .into_iter()
        .map(|finding| {
            format!(
                "{}:{}:{}",
                finding.tool,
                finding.source,
                discovery_status_name(finding.status, Language::English)
            )
        })
        .collect();
    Ok(ResolutionExplanation {
        start: context.start,
        boundary: context.boundary,
        project_config: context.config_path,
        project_strict,
        global_eligible,
        system_eligible,
        selected_source: selected_source.to_owned(),
        fallback_used: selected_source != "project",
        candidates,
        traditional_sources,
    })
}

fn print_resolution_explanation(explanation: &ResolutionExplanation) {
    println!("resolution start={}", explanation.start.display());
    println!("resolution boundary={}", explanation.boundary.display());
    println!(
        "resolution policy project-strict={} global-eligible={} system-eligible={}",
        explanation.project_strict, explanation.global_eligible, explanation.system_eligible
    );
    for candidate in &explanation.candidates {
        println!(
            "candidate source={} status={} requested={} resolved={} config={} reason={}",
            candidate.source,
            candidate.status,
            candidate.requested.as_deref().unwrap_or("-"),
            candidate.resolved.as_deref().unwrap_or("-"),
            candidate
                .config
                .as_deref()
                .map_or_else(|| "-".to_owned(), |path| path.display().to_string()),
            candidate.reason
        );
    }
    for source in &explanation.traditional_sources {
        println!("traditional-source {source} (explicit detect/import only)");
    }
}

fn run_list(
    tool: Option<&str>,
    available: bool,
    json: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if available {
        let tool = tool.ok_or("list --available requires a runtime provider")?;
        require_provider(tool)?;
        let releases = available_version_reports(tool)?;
        if json {
            print_json_success("list", serde_json::json!({ "versions": releases }))?;
        } else {
            for release in releases {
                if release.tool == "node" {
                    println!(
                        "{}",
                        catalog.available_node(
                            &release.version,
                            release.details.get("date").map_or("-", String::as_str),
                            release.details.get("lts").map(String::as_str),
                            release
                                .details
                                .get("security")
                                .is_some_and(|value| value == "true"),
                        )
                    );
                    continue;
                }
                let details = release
                    .details
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if details.is_empty() {
                    println!("{}@{}", release.tool, release.version);
                } else {
                    println!("{}@{} {details}", release.tool, release.version);
                }
            }
        }
        return Ok(());
    }

    let installed = if let Some(tool) = tool {
        require_provider(tool)?;
        list_installed_tool_versions(&pinset_home()?, tool)?
    } else {
        list_all_installed_tool_versions(&pinset_home()?)?
    };
    if json {
        print_json_success("list", serde_json::json!({ "versions": installed }))?;
    } else if installed.is_empty() {
        if tool == Some("node") {
            println!("{}", catalog.no_installed_node());
        } else if let Some(tool) = tool {
            println!("no Pinset-managed {tool} versions are installed");
        } else {
            println!("no Pinset-managed runtime versions are installed");
        }
    } else {
        for entry in installed {
            if entry.tool == "node" && tool == Some("node") {
                println!(
                    "{}",
                    catalog.installed_node(&entry.version, &entry.targets.join(","))
                );
            } else {
                println!(
                    "{}@{} [{}]",
                    entry.tool,
                    entry.version,
                    entry.targets.join(",")
                );
            }
        }
    }
    Ok(())
}

fn available_version_reports(
    tool: &str,
) -> Result<Vec<AvailableVersionReport>, Box<dyn std::error::Error>> {
    let provider = runtime_provider(tool).expect("required provider exists");
    let mut reports = Vec::new();
    match provider.capabilities.metadata {
        RuntimeMetadataKind::Node => {
            for release in node_metadata_client(&pinset_home()?)?.available_releases()? {
                let mut details = BTreeMap::new();
                details.insert("date".to_owned(), release.date);
                details.insert("security".to_owned(), release.security.to_string());
                if let Some(lts) = release.lts {
                    details.insert("lts".to_owned(), lts);
                }
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details,
                });
            }
        }
        RuntimeMetadataKind::Npm => {
            for release in NpmMetadataClient::official()?.available_releases(tool)? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::new(),
                });
            }
        }
        RuntimeMetadataKind::Go => {
            for release in go_metadata_client(&pinset_home()?)?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::new(),
                });
            }
        }
        RuntimeMetadataKind::Flutter => {
            for release in flutter_metadata_client(&pinset_home()?)?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::from([
                        ("channel".to_owned(), "stable".to_owned()),
                        ("dart".to_owned(), release.dart_version),
                    ]),
                });
            }
        }
        RuntimeMetadataKind::Python => {
            for release in PythonMetadataClient::official()?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: format!("{}+{}", release.version, release.build_id),
                    details: BTreeMap::from([
                        ("date".to_owned(), release.date),
                        ("distribution".to_owned(), release.distribution),
                    ]),
                });
            }
        }
        RuntimeMetadataKind::Java => {
            for release in JavaMetadataClient::official()?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::from([
                        ("date".to_owned(), release.date),
                        ("distribution".to_owned(), "temurin".to_owned()),
                        (
                            "release".to_owned(),
                            (if release.lts { "lts" } else { "ga" }).to_owned(),
                        ),
                    ]),
                });
            }
        }
        RuntimeMetadataKind::Rust => {
            for release in RustMetadataClient::official()?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::from([
                        ("channel".to_owned(), "stable".to_owned()),
                        ("date".to_owned(), release.date),
                    ]),
                });
            }
        }
        RuntimeMetadataKind::Dotnet => {
            for release in DotnetMetadataClient::official()?.available_releases()? {
                reports.push(AvailableVersionReport {
                    tool: tool.to_owned(),
                    version: release.version,
                    details: BTreeMap::from([
                        ("channel".to_owned(), release.channel),
                        ("date".to_owned(), release.date),
                        ("release".to_owned(), release.release_type),
                        ("support".to_owned(), release.support_phase),
                    ]),
                });
            }
        }
    }
    Ok(reports)
}

fn run_outdated(
    tool: Option<&str>,
    global_only: bool,
    cwd: Option<PathBuf>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tool) = tool {
        require_provider(tool)?;
    }
    let home = pinset_home()?;
    let cwd = effective_cwd(cwd)?;
    let selected = selected_runtimes_for_outdated(&home, &cwd, tool, global_only)?;
    let mut reports = Vec::new();
    let mut latest_versions: BTreeMap<String, String> = BTreeMap::new();
    for selection in selected {
        let latest_compatible = resolve_locked_tool(&selection.tool, &selection.requested)?.version;
        let latest = if let Some(latest) = latest_versions.get(&selection.tool) {
            latest.clone()
        } else {
            let latest =
                resolve_locked_tool(&selection.tool, latest_stable_selector(&selection.tool))?
                    .version;
            latest_versions.insert(selection.tool.clone(), latest.clone());
            latest
        };
        reports.push(OutdatedReport {
            scope: selection.scope,
            config: selection.config,
            tool: selection.tool,
            requested: selection.requested,
            update_available: selection.version != latest_compatible,
            upgrade_available: latest_compatible != latest,
            current: selection.version,
            latest_compatible,
            latest,
        });
    }
    if json {
        print_json_success("outdated", serde_json::json!({ "runtimes": reports }))?;
    } else {
        let mut count = 0;
        for report in reports
            .iter()
            .filter(|report| report.update_available || report.upgrade_available)
        {
            count += 1;
            println!(
                "{}@{} requested={} compatible={} latest={} scope={} config={}",
                report.tool,
                report.current,
                report.requested,
                report.latest_compatible,
                report.latest,
                report.scope,
                report.config.display()
            );
        }
        if count == 0 {
            println!("all selected runtimes are current");
        }
    }
    Ok(())
}

fn selected_runtimes_for_outdated(
    home: &Path,
    cwd: &Path,
    tool: Option<&str>,
    global_only: bool,
) -> Result<Vec<SelectedRuntime>, Box<dyn std::error::Error>> {
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    if !global_only {
        if let Some(path) = find_optional_project_config(cwd)? {
            let config = load_project_config(&path)?;
            let lock_path = lockfile_path(&path);
            for (selected_tool, requested) in config.tools {
                if tool.is_some_and(|tool| tool != selected_tool.as_str()) {
                    continue;
                }
                require_provider(&selected_tool)?;
                let version = selected_version_from_lock(
                    &selected_tool,
                    &requested,
                    config.schema,
                    &path,
                    &lock_path,
                )?;
                if seen.insert(("project", selected_tool.clone(), path.clone())) {
                    selected.push(SelectedRuntime {
                        scope: "project",
                        config: path.clone(),
                        tool: selected_tool,
                        requested,
                        version,
                    });
                }
            }
        }
    }
    let global_path = global_config_path(home);
    if let Some(global) = load_optional_global_config(&global_path)? {
        let lock_path = global_lockfile_path(home);
        for (selected_tool, requested) in global.tools {
            if tool.is_some_and(|tool| tool != selected_tool.as_str()) {
                continue;
            }
            require_provider(&selected_tool)?;
            let version = selected_version_from_lock(
                &selected_tool,
                &requested,
                global.schema,
                &global_path,
                &lock_path,
            )?;
            if seen.insert(("global", selected_tool.clone(), global_path.clone())) {
                selected.push(SelectedRuntime {
                    scope: "global",
                    config: global_path.clone(),
                    tool: selected_tool,
                    requested,
                    version,
                });
            }
        }
    }
    Ok(selected)
}

fn selected_version_from_lock(
    tool: &str,
    requested: &str,
    config_schema: u32,
    config_path: &Path,
    lock_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(lockfile) = load_optional_lockfile(lock_path)? {
        return Ok(
            validate_lock_matches_tool(&lockfile, tool, requested, config_path)?
                .version
                .clone(),
        );
    }
    if config_schema < PROJECT_CONFIG_SCHEMA {
        return Ok(requested.to_owned());
    }
    load_lockfile(lock_path)?;
    unreachable!("loading a missing schema 3 lock always returns an error")
}

fn latest_stable_selector(tool: &str) -> &'static str {
    match tool {
        "node" => "current",
        "rust" => "stable",
        _ => "latest",
    }
}

fn run_uninstall(
    selection: &str,
    force: bool,
    cwd: Option<PathBuf>,
    dry_run: bool,
    json: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tool, version) = parse_tool_selection(selection, catalog)?;
    validate_exact_tool_version(&tool, &version)?;
    let home = pinset_home()?;
    let cwd = effective_cwd(cwd)?;
    if dry_run {
        let uninstall = plan_uninstall_tool_version(&home, &cwd, &tool, &version, force)?;
        let report = UninstallReport {
            dry_run: true,
            tool,
            version,
            targets: uninstall.targets,
        };
        if json {
            print_json_success("uninstall", report)?;
        } else {
            println!(
                "would uninstall {}@{} [{}]",
                report.tool,
                report.version,
                report.targets.join(",")
            );
        }
        return Ok(());
    }

    let targets = if tool == "node" {
        uninstall_node_version(&home, &cwd, &version, force)?.targets
    } else {
        uninstall_tool_version(&home, &cwd, &tool, &version, force)?.targets
    };
    let report = UninstallReport {
        dry_run: false,
        tool,
        version,
        targets,
    };
    if json {
        print_json_success("uninstall", report)?;
    } else if report.tool == "node" {
        println!(
            "{}",
            catalog.uninstalled_node(&report.version, &report.targets.join(","))
        );
    } else {
        println!(
            "uninstalled {}@{} [{}]",
            report.tool,
            report.version,
            report.targets.join(",")
        );
    }
    Ok(())
}

fn run_prune(
    cwd: Option<PathBuf>,
    additional_projects: &[PathBuf],
    dry_run: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let cwd = effective_cwd(cwd)?;
    if !cwd.is_dir() {
        return Err(format!("prune working directory does not exist: {}", cwd.display()).into());
    }
    let mut project_roots = vec![cwd.clone()];
    for project in additional_projects {
        let project = absolutize(project)?;
        if !project.is_dir() {
            return Err(format!(
                "prune project path is not an existing directory: {}",
                project.display()
            )
            .into());
        }
        if find_optional_project_config(&project)?.is_none() {
            return Err(format!(
                "prune project path has no pinset.toml in its ancestors: {}",
                project.display()
            )
            .into());
        }
        project_roots.push(project);
    }
    let plan = plan_prune_tool_versions(&home, &project_roots)?;
    let mut removed = 0;
    if !dry_run {
        for candidate in &plan.candidates {
            uninstall_tool_version(&home, &cwd, &candidate.tool, &candidate.version, false)?;
            removed += 1;
        }
    }
    let report = PruneExecutionReport {
        dry_run,
        candidates: plan.candidates,
        protected: plan.protected,
        bytes: plan.bytes,
        removed,
    };
    if json {
        print_json_success("prune", report)?;
    } else if report.candidates.is_empty() {
        println!("no unused Pinset-managed runtime versions found");
    } else {
        for candidate in &report.candidates {
            println!(
                "{} {}@{} [{}] {}",
                if dry_run { "would remove" } else { "removed" },
                candidate.tool,
                candidate.version,
                candidate.targets.join(","),
                format_bytes(candidate.bytes)
            );
        }
        println!(
            "{} {} runtime versions ({})",
            if dry_run { "would remove" } else { "removed" },
            report.candidates.len(),
            format_bytes(report.bytes)
        );
    }
    Ok(())
}

fn run_lock_command(
    command: LockCommands,
    catalog: Catalog,
) -> Result<i32, Box<dyn std::error::Error>> {
    match command {
        LockCommands::Audit { global, cwd, json } => {
            let home = pinset_home()?;
            let report = if global {
                audit_global_lock(&home)
            } else {
                audit_project_lock(&home, &effective_cwd(cwd)?)
            };
            let action_required = report.action_required();
            if json {
                print_json_success("lock.audit", report)?;
            } else {
                print_lock_audit_report(&report, catalog);
            }
            Ok(if action_required { 1 } else { 0 })
        }
    }
}

fn print_lock_audit_report(report: &LockAuditReport, catalog: Catalog) {
    let scope = match report.scope {
        LockAuditScope::Project => "project",
        LockAuditScope::Global => "global",
    };
    println!(
        "{}",
        catalog.lock_audit_header(scope, &report.config, &report.lockfile, report.passed)
    );
    for finding in &report.findings {
        let severity = match finding.severity {
            LockAuditSeverity::Error => "error",
            LockAuditSeverity::Warning => "warning",
            LockAuditSeverity::Info => "info",
        };
        println!(
            "{}",
            catalog.lock_audit_finding(
                severity,
                finding.reason_code.as_str(),
                &finding.subject,
                finding.path.as_deref(),
                &finding.message,
            )
        );
        if let Some(repair) = &finding.repair {
            println!(
                "{}",
                catalog.lock_audit_repair(&repair.action, repair.command.as_deref())
            );
        }
    }
    println!("{}", catalog.lock_audit_summary(&report.summary));
}

fn run_cache(command: CacheCommands, catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    match command {
        CacheCommands::List { json } => {
            let entries = list_download_cache(&home)?;
            if json {
                print_json_success("cache.list", serde_json::json!({ "entries": entries }))?;
            } else if entries.is_empty() {
                println!("{}", catalog.cache_empty());
            } else {
                for entry in entries {
                    println!(
                        "{}",
                        catalog.cache_entry(&entry.integrity, entry.size, &entry.path)
                    );
                }
            }
        }
        CacheCommands::Info { json } => {
            let info = download_cache_info(&home)?;
            if json {
                print_json_success("cache.info", info)?;
            } else {
                println!(
                    "archives={} archive_bytes={} partials={} partial_bytes={}",
                    info.archives, info.archive_bytes, info.partial_downloads, info.partial_bytes
                );
            }
        }
        CacheCommands::Verify { json } => {
            let verification = verify_download_cache(&home)?;
            if json {
                if verification.corrupt > 0 {
                    return Err(Error::DownloadCacheCorrupt {
                        entries: verification.corrupt,
                    }
                    .into());
                }
                print_json_success("cache.verify", verification)?;
                return Ok(());
            } else {
                for entry in &verification.entries {
                    println!(
                        "{} integrity={} actual={} bytes={} path={}",
                        if entry.valid { "valid" } else { "corrupt" },
                        entry.integrity,
                        entry.actual,
                        entry.size,
                        entry.path.display()
                    );
                }
                println!(
                    "verified={} corrupt={} bytes={}",
                    verification.valid, verification.corrupt, verification.bytes
                );
            }
            if verification.corrupt > 0 {
                return Err(Error::DownloadCacheCorrupt {
                    entries: verification.corrupt,
                }
                .into());
            }
        }
        CacheCommands::Repair { dry_run, json } => {
            let outcome = if dry_run {
                let verification = verify_download_cache(&home)?;
                pinset_core::DownloadCacheCleanOutcome {
                    entries: verification
                        .entries
                        .iter()
                        .filter(|entry| !entry.valid)
                        .count(),
                    bytes: verification
                        .entries
                        .iter()
                        .filter(|entry| !entry.valid)
                        .map(|entry| entry.size)
                        .sum(),
                }
            } else {
                repair_download_cache(&home)?
            };
            if json {
                print_json_success(
                    "cache.repair",
                    CacheMutationReport {
                        dry_run,
                        entries: outcome.entries,
                        bytes: outcome.bytes,
                    },
                )?;
            } else {
                println!(
                    "{} {} corrupt cache archives ({})",
                    if dry_run { "would remove" } else { "removed" },
                    outcome.entries,
                    format_bytes(outcome.bytes)
                );
            }
        }
        CacheCommands::Clean { dry_run, json } => {
            let outcome = if dry_run {
                let info = download_cache_info(&home)?;
                pinset_core::DownloadCacheCleanOutcome {
                    entries: info.archives + info.partial_downloads,
                    bytes: info.archive_bytes.saturating_add(info.partial_bytes),
                }
            } else {
                clean_download_cache(&home)?
            };
            if json {
                print_json_success(
                    "cache.clean",
                    CacheMutationReport {
                        dry_run,
                        entries: outcome.entries,
                        bytes: outcome.bytes,
                    },
                )?;
            } else if dry_run {
                println!(
                    "would clean {} cached archives ({})",
                    outcome.entries,
                    format_bytes(outcome.bytes)
                );
            } else {
                println!("{}", catalog.cache_cleaned(outcome.entries, outcome.bytes));
            }
        }
        CacheCommands::Import {
            archive,
            sha256,
            integrity,
        } => {
            let archive = absolutize(&archive)?;
            let entry = if let Some(sha256) = sha256 {
                import_download_cache(&home, &archive, &sha256)?
            } else if let Some(integrity) = integrity {
                let integrity = ArtifactIntegrity::parse(&integrity)?;
                import_download_cache_with_integrity(&home, &archive, &integrity)?
            } else {
                return Err("cache import requires --sha256 or --integrity".into());
            };
            println!(
                "{}",
                catalog.cache_imported(&entry.integrity, entry.size, &entry.path)
            );
        }
    }
    Ok(())
}

const COMPLETION_COMMANDS: &str = "init detect import global use unset install which current list outdated update migrate uninstall prune lock cache exec doctor venv shim activate completions source provider";
const COMPLETION_SHELLS: &str = "bash zsh fish powershell";
const COMPLETION_LOCK_COMMANDS: &str = "audit";
const COMPLETION_CACHE_COMMANDS: &str = "list info verify repair clean import";
const COMPLETION_VENV_COMMANDS: &str = "create status recreate";
const COMPLETION_SHIM_COMMANDS: &str = "path install migrate";
const COMPLETION_SOURCE_COMMANDS: &str = "list add use fallback remove test";
const COMPLETION_PROVIDER_COMMANDS: &str = "list verify";

fn print_completions(shell: ActivationShell) {
    println!("{}", completion_script(shell));
}

fn completion_script(shell: ActivationShell) -> String {
    let providers = pinset_core::runtime_providers()
        .iter()
        .map(|provider| provider.tool)
        .collect::<Vec<_>>()
        .join(" ");
    let selections = pinset_core::runtime_providers()
        .iter()
        .map(|provider| format!("{}@", provider.tool))
        .collect::<Vec<_>>()
        .join(" ");
    let source_providers = SUPPORTED_SOURCE_PROVIDERS.join(" ");
    let template = match shell {
        ActivationShell::Bash => {
            r#"_pinset_completion() {
    local current command values
    current="${COMP_WORDS[COMP_CWORD]}"
    command="${COMP_WORDS[1]}"
    if (( COMP_CWORD == 1 )); then
        values="__COMMANDS__ --help --version --lang"
    else
        case "$command" in
            global) values="__SELECTIONS__ --no-install --lang --help" ;;
            detect) values="--cwd --json --lang --help" ;;
            import) values="--cwd --force --no-install --lang --help" ;;
            use) values="__SELECTIONS__ --no-install --global --lang --help" ;;
            install) values="__SELECTIONS__ --locked --global --cwd --lang --help" ;;
            uninstall) values="__SELECTIONS__ --force --cwd --dry-run --json --lang --help" ;;
            unset) values="__PROVIDERS__ --global --cwd --lang --help" ;;
            list) values="__PROVIDERS__ --available --json --lang --help" ;;
            current) values="__PROVIDERS__ --cwd --explain --json --lang --help" ;;
            outdated) values="__PROVIDERS__ --global --cwd --json --lang --help" ;;
            update) values="__PROVIDERS__ --global --cwd --dry-run --json --lang --help" ;;
            migrate) values="--global --cwd --dry-run --json --lang --help" ;;
            prune) values="--cwd --project --dry-run --json --lang --help" ;;
            which) values="--cwd --explain --json --lang --help" ;;
            doctor) values="--cwd --json --lang --help" ;;
            lock) values="__LOCK_COMMANDS__ --global --cwd --json --lang --help" ;;
            cache) values="__CACHE_COMMANDS__ --lang --help" ;;
            venv) values="__VENV_COMMANDS__ --lang --help" ;;
            shim) values="__SHIM_COMMANDS__ __PROVIDERS__ --provider --binary --dir --lang --help" ;;
            activate|completions) values="__SHELLS__ --lang --help" ;;
            source) values="__SOURCE_COMMANDS__ __SOURCE_PROVIDERS__ --lang --help" ;;
            provider) values="__PROVIDER_COMMANDS__ --json --lang --help" ;;
            *) values="--lang --help" ;;
        esac
    fi
    COMPREPLY=( $(compgen -W "$values" -- "$current") )
}
complete -o default -F _pinset_completion pinset"#
        }
        ActivationShell::Zsh => {
            r#"#compdef pinset
_pinset_completion() {
    local command values
    command="$words[2]"
    if (( CURRENT == 2 )); then
        values="__COMMANDS__ --help --version --lang"
    else
        case "$command" in
            global) values="__SELECTIONS__ --no-install --lang --help" ;;
            detect) values="--cwd --json --lang --help" ;;
            import) values="--cwd --force --no-install --lang --help" ;;
            use) values="__SELECTIONS__ --no-install --global --lang --help" ;;
            install) values="__SELECTIONS__ --locked --global --cwd --lang --help" ;;
            uninstall) values="__SELECTIONS__ --force --cwd --dry-run --json --lang --help" ;;
            unset) values="__PROVIDERS__ --global --cwd --lang --help" ;;
            list) values="__PROVIDERS__ --available --json --lang --help" ;;
            current) values="__PROVIDERS__ --cwd --explain --json --lang --help" ;;
            outdated) values="__PROVIDERS__ --global --cwd --json --lang --help" ;;
            update) values="__PROVIDERS__ --global --cwd --dry-run --json --lang --help" ;;
            migrate) values="--global --cwd --dry-run --json --lang --help" ;;
            prune) values="--cwd --project --dry-run --json --lang --help" ;;
            which) values="--cwd --explain --json --lang --help" ;;
            doctor) values="--cwd --json --lang --help" ;;
            lock) values="__LOCK_COMMANDS__ --global --cwd --json --lang --help" ;;
            cache) values="__CACHE_COMMANDS__ --lang --help" ;;
            venv) values="__VENV_COMMANDS__ --lang --help" ;;
            shim) values="__SHIM_COMMANDS__ __PROVIDERS__ --provider --binary --dir --lang --help" ;;
            activate|completions) values="__SHELLS__ --lang --help" ;;
            source) values="__SOURCE_COMMANDS__ __SOURCE_PROVIDERS__ --lang --help" ;;
            provider) values="__PROVIDER_COMMANDS__ --json --lang --help" ;;
            *) values="--lang --help" ;;
        esac
    fi
    compadd -- ${(z)values}
}
compdef _pinset_completion pinset"#
        }
        ActivationShell::Fish => {
            r#"complete -c pinset -f -n '__fish_use_subcommand' -a '__COMMANDS__'
complete -c pinset -f -n '__fish_seen_subcommand_from global use install uninstall' -a '__SELECTIONS__'
complete -c pinset -f -n '__fish_seen_subcommand_from unset list current outdated update' -a '__PROVIDERS__'
complete -c pinset -f -n '__fish_seen_subcommand_from cache' -a '__CACHE_COMMANDS__'
complete -c pinset -f -n '__fish_seen_subcommand_from lock' -a '__LOCK_COMMANDS__'
complete -c pinset -f -n '__fish_seen_subcommand_from venv' -a '__VENV_COMMANDS__'
complete -c pinset -f -n '__fish_seen_subcommand_from shim' -a '__SHIM_COMMANDS__ __PROVIDERS__'
complete -c pinset -f -n '__fish_seen_subcommand_from activate completions' -a '__SHELLS__'
complete -c pinset -f -n '__fish_seen_subcommand_from source' -a '__SOURCE_COMMANDS__ __SOURCE_PROVIDERS__'
complete -c pinset -f -n '__fish_seen_subcommand_from provider' -a '__PROVIDER_COMMANDS__ --json'
complete -c pinset -f -n '__fish_seen_subcommand_from detect which current list outdated update migrate uninstall prune doctor lock cache provider' -a '--json'
complete -c pinset -f -n '__fish_seen_subcommand_from which current' -a '--explain'
complete -c pinset -f -n '__fish_seen_subcommand_from detect import install which current outdated update migrate uninstall prune doctor lock' -a '--cwd'
complete -c pinset -f -n '__fish_seen_subcommand_from import' -a '--force --no-install'
complete -c pinset -f -n '__fish_seen_subcommand_from use unset install outdated update migrate lock' -a '--global'
complete -c pinset -f -n '__fish_seen_subcommand_from update migrate uninstall prune cache' -a '--dry-run'
complete -c pinset -f -a '--help --lang'"#
        }
        ActivationShell::Powershell => {
            r#"Register-ArgumentCompleter -Native -CommandName pinset -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $elements = @($commandAst.CommandElements | ForEach-Object { $_.Extent.Text })
    $command = if ($elements.Count -gt 1) { $elements[1] } else { '' }
    $values = switch ($command) {
        'global' { '__SELECTIONS__ --no-install --lang --help' -split ' ' }
        'detect' { '--cwd --json --lang --help' -split ' ' }
        'import' { '--cwd --force --no-install --lang --help' -split ' ' }
        'use' { '__SELECTIONS__ --no-install --global --lang --help' -split ' ' }
        'install' { '__SELECTIONS__ --locked --global --cwd --lang --help' -split ' ' }
        'uninstall' { '__SELECTIONS__ --force --cwd --dry-run --json --lang --help' -split ' ' }
        'unset' { '__PROVIDERS__ --global --cwd --lang --help' -split ' ' }
        'list' { '__PROVIDERS__ --available --json --lang --help' -split ' ' }
        'current' { '__PROVIDERS__ --cwd --explain --json --lang --help' -split ' ' }
        'outdated' { '__PROVIDERS__ --global --cwd --json --lang --help' -split ' ' }
        'update' { '__PROVIDERS__ --global --cwd --dry-run --json --lang --help' -split ' ' }
        'migrate' { '--global --cwd --dry-run --json --lang --help' -split ' ' }
        'prune' { '--cwd --project --dry-run --json --lang --help' -split ' ' }
        'which' { '--cwd --explain --json --lang --help' -split ' ' }
        'doctor' { '--cwd --json --lang --help' -split ' ' }
        'lock' { '__LOCK_COMMANDS__ --global --cwd --json --lang --help' -split ' ' }
        'cache' { '__CACHE_COMMANDS__ --lang --help' -split ' ' }
        'venv' { '__VENV_COMMANDS__ --lang --help' -split ' ' }
        'shim' { '__SHIM_COMMANDS__ __PROVIDERS__ --provider --binary --dir --lang --help' -split ' ' }
        { $_ -in @('activate', 'completions') } { '__SHELLS__ --lang --help' -split ' ' }
        'source' { '__SOURCE_COMMANDS__ __SOURCE_PROVIDERS__ --lang --help' -split ' ' }
        'provider' { '__PROVIDER_COMMANDS__ --json --lang --help' -split ' ' }
        default { '__COMMANDS__ --help --version --lang' -split ' ' }
    }
    $values |
        Where-Object { $_ -like "$wordToComplete*" } |
        ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}"#
        }
    };
    template
        .replace("__COMMANDS__", COMPLETION_COMMANDS)
        .replace("__PROVIDERS__", &providers)
        .replace("__SELECTIONS__", &selections)
        .replace("__SHELLS__", COMPLETION_SHELLS)
        .replace("__LOCK_COMMANDS__", COMPLETION_LOCK_COMMANDS)
        .replace("__CACHE_COMMANDS__", COMPLETION_CACHE_COMMANDS)
        .replace("__VENV_COMMANDS__", COMPLETION_VENV_COMMANDS)
        .replace("__SHIM_COMMANDS__", COMPLETION_SHIM_COMMANDS)
        .replace("__SOURCE_COMMANDS__", COMPLETION_SOURCE_COMMANDS)
        .replace("__SOURCE_PROVIDERS__", &source_providers)
        .replace("__PROVIDER_COMMANDS__", COMPLETION_PROVIDER_COMMANDS)
}

fn parse_tool_selection(
    selection: &str,
    catalog: Catalog,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let Some((tool, version)) = selection.split_once('@') else {
        return Err(catalog.selection_error().into());
    };
    if version.is_empty() || version.contains('@') {
        return Err(catalog.selection_error().into());
    }
    require_provider(tool)?;
    Ok((tool.to_owned(), version.to_owned()))
}

fn require_provider(tool: &str) -> Result<(), Box<dyn std::error::Error>> {
    runtime_provider(tool)
        .map(|_| ())
        .ok_or_else(|| format!("runtime provider {tool:?} is not available").into())
}

fn validate_exact_tool_version(
    tool: &str,
    version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = runtime_provider(tool).expect("validated provider");
    match provider.capabilities.metadata {
        RuntimeMetadataKind::Node => validate_exact_node_version(version)?,
        RuntimeMetadataKind::Npm => validate_exact_npm_tool_version(tool, version)?,
        RuntimeMetadataKind::Go => {
            validate_exact_go_version(version)?;
        }
        RuntimeMetadataKind::Flutter => {
            validate_exact_flutter_version(version)?;
        }
        RuntimeMetadataKind::Python => {
            validate_exact_python_version(version)?;
        }
        RuntimeMetadataKind::Java => {
            validate_exact_java_version(version)?;
        }
        RuntimeMetadataKind::Rust => {
            validate_exact_rust_version(version)?;
        }
        RuntimeMetadataKind::Dotnet => {
            validate_exact_dotnet_version(version)?;
        }
    }
    Ok(())
}

fn resolve_locked_tool(
    tool: &str,
    selector: &str,
) -> Result<LockedTool, Box<dyn std::error::Error>> {
    let provider = runtime_provider(tool).expect("validated provider");
    let mut locked = match provider.capabilities.metadata {
        RuntimeMetadataKind::Node => {
            let lockfile = node_metadata_client(&pinset_home()?)?
                .resolve_lock(selector, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
            lockfile
                .tool("node")
                .expect("generated Node lock contains node")
                .clone()
        }
        RuntimeMetadataKind::Npm => {
            let client = NpmMetadataClient::official()?;
            let version = client.resolve_version_selector(tool, selector)?;
            client.resolve_tool(tool, &version)?
        }
        RuntimeMetadataKind::Go => go_metadata_client(&pinset_home()?)?.resolve_tool(selector)?,
        RuntimeMetadataKind::Flutter => {
            flutter_metadata_client(&pinset_home()?)?.resolve_tool(selector)?
        }
        RuntimeMetadataKind::Python => PythonMetadataClient::official()?.resolve_tool(selector)?,
        RuntimeMetadataKind::Java => JavaMetadataClient::official()?.resolve_tool(selector)?,
        RuntimeMetadataKind::Rust => RustMetadataClient::official()?.resolve_tool(selector)?,
        RuntimeMetadataKind::Dotnet => DotnetMetadataClient::official()?.resolve_tool(selector)?,
    };
    locked.requested = selector.to_owned();
    Ok(locked)
}

fn new_lockfile() -> Lockfile {
    Lockfile {
        schema: pinset_core::LOCKFILE_SCHEMA,
        generated_by: format!("pinset {}", env!("CARGO_PKG_VERSION")),
        tools: Vec::new(),
    }
}

fn select_tool(
    selection: &str,
    global: bool,
    no_install: bool,
    initialize_project: bool,
    cwd: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tool, selector) = parse_tool_selection(selection, catalog)?;
    let locked_tool = resolve_locked_tool(&tool, &selector)?;
    let version = locked_tool.version.clone();
    if selector != version {
        println!("{tool}@{selector} resolved to {tool}@{version}");
    }
    let (scope, lock_path) = if global {
        let home = pinset_home()?;
        let config_path = global_config_path(&home);
        let mut config = load_optional_global_config(&config_path)?.unwrap_or_default();
        let lock_path = global_lockfile_path(&home);
        let mut lockfile = load_optional_lockfile(&lock_path)?.unwrap_or_else(new_lockfile);
        lockfile.generated_by = format!("pinset {}", env!("CARGO_PKG_VERSION"));
        lockfile.upsert_tool(locked_tool.clone())?;
        config.set_tool(&tool, &selector);
        validate_lock_matches_tools(&lockfile, &config.tools, &config_path)?;
        save_global_state(&home, &config, &lockfile)?;
        ("global", lock_path)
    } else {
        let config_path = match find_optional_project_config(cwd)? {
            Some(path) => path,
            None if initialize_project => create_project_config(cwd)?,
            None => find_project_config(cwd)?,
        };
        let mut project = load_project_config(&config_path)?;
        let lock_path = lockfile_path(&config_path);
        let mut lockfile = load_optional_lockfile(&lock_path)?.unwrap_or_else(new_lockfile);
        lockfile.generated_by = format!("pinset {}", env!("CARGO_PKG_VERSION"));
        lockfile.upsert_tool(locked_tool.clone())?;
        project.set_tool(&tool, &selector);
        save_project_state(&config_path, &project, &lockfile)?;
        ("project", lock_path)
    };
    if tool == "node" {
        println!(
            "{}",
            catalog.selected(scope, &version, locked_tool.artifacts.len(), &lock_path)
        );
    } else {
        println!(
            "selected {tool}@{version} for {scope} ({} targets, lock {})",
            locked_tool.artifacts.len(),
            lock_path.display()
        );
    }
    if !no_install {
        if global {
            install_global(&pinset_home()?, catalog)?;
        } else {
            install_project(cwd, catalog)?;
        }
    } else if let Err(error) = register_provider_commands(&pinset_home()?, &tool, catalog) {
        eprintln!(
            "{}",
            catalog.shim_auto_registration_failed(&error.to_string())
        );
    }
    Ok(())
}

fn print_discovery_report(report: &DiscoveryReport, catalog: Catalog) {
    match catalog.language() {
        Language::English => {
            println!("traditional configuration scan");
            println!("start: {}", report.start.display());
            println!("boundary: {}", report.boundary.display());
            println!("target config: {}", report.target_config.display());
        }
        Language::SimplifiedChinese => {
            println!("传统版本配置扫描");
            println!("起始目录：{}", report.start.display());
            println!("扫描边界：{}", report.boundary.display());
            println!("目标配置：{}", report.target_config.display());
        }
    }
    if report.findings.is_empty() {
        println!(
            "{}",
            match catalog.language() {
                Language::English => "no traditional runtime configuration found",
                Language::SimplifiedChinese => "未发现传统运行时配置",
            }
        );
    }
    for finding in &report.findings {
        let field = finding
            .field
            .as_deref()
            .map(|field| format!("#{field}"))
            .unwrap_or_default();
        let value = finding.normalized.as_deref().unwrap_or(&finding.raw);
        let status = discovery_status_name(finding.status, catalog.language());
        print!(
            "[{status}] {} {} <- {}{}",
            finding.tool, value, finding.source, field
        );
        if let Some(reason) = &finding.reason {
            print!(
                " ({})",
                localized_discovery_reason(reason, catalog.language())
            );
        }
        println!();
    }
    println!(
        "{}: {}",
        match catalog.language() {
            Language::English => "importable",
            Language::SimplifiedChinese => "可导入",
        },
        match (catalog.language(), report.can_import) {
            (Language::English, true) => "yes",
            (Language::English, false) => "no",
            (Language::SimplifiedChinese, true) => "是",
            (Language::SimplifiedChinese, false) => "否",
        }
    );
}

fn localized_discovery_reason(reason: &str, language: Language) -> String {
    if language == Language::English {
        return reason.to_owned();
    }
    let translated = match reason {
        "version constraint is reported but not imported" => "版本范围仅报告，不参与导入",
        "symbolic-link sources are not allowed" => "不允许使用符号链接来源",
        "source is not a regular file" => "来源不是普通文件",
        "source is not valid UTF-8" => "来源不是有效的 UTF-8 文本",
        "source must contain exactly one version selector" => "来源必须只包含一个版本选择器",
        "version selector must not contain whitespace" => "版本选择器不能包含空白",
        ".python-version must contain exactly one CPython selector" => {
            ".python-version 必须只包含一个 CPython 选择器"
        }
        "only one CPython selector can be imported" => "只能导入一个 CPython 选择器",
        "invalid CPython distribution selector" => "CPython 发行版选择器无效",
        "volta.node must be a string" => "volta.node 必须是字符串",
        "packageManager must be a string" => "packageManager 必须是字符串",
        "packageManager must use <name>@<version>" => "packageManager 必须使用 <名称>@<版本>",
        "package manager is not a Pinset Provider" => "该包管理器不是 Pinset Provider",
        "FVM flavors cannot be represented by one Pinset selection" => {
            "FVM flavors 无法表示为一个 Pinset 选择"
        }
        "FVM configuration has no string flutter version" => {
            "FVM 配置中没有字符串类型的 Flutter 版本"
        }
        "invalid .sdkmanrc assignment" => ".sdkmanrc 赋值格式无效",
        "SDKMAN candidate is not imported" => "该 SDKMAN candidate 不参与导入",
        "missing [toolchain] table" => "缺少 [toolchain] 表",
        "unknown Rust toolchain fields cannot be imported safely" => {
            "未知 Rust toolchain 字段无法安全导入"
        }
        "path toolchains are not supported" => "不支持 path toolchain",
        "extra Rust targets are not supported" => "不支持额外 Rust target",
        "only the default Rust profile is supported" => "仅支持 Rust default profile",
        "only rustfmt and clippy components are supported" => "仅支持 rustfmt 和 clippy 组件",
        "Rust channel must be a string" => "Rust channel 必须是字符串",
        "sdk.version must be a string" => "sdk.version 必须是字符串",
        "tool is not a Pinset Provider" => "该工具不是 Pinset Provider",
        "supported tools must have exactly one plain version" => "受支持工具必须只有一个普通版本值",
        "mise value must be one plain string selector" => "mise 值必须是单个普通字符串选择器",
        "only Temurin SDKMAN Java versions can be imported" => {
            "只能导入 SDKMAN 的 Temurin Java 版本"
        }
        "Java selector must be stable numeric, lts, current, or Temurin -tem" => {
            "Java 选择器必须是稳定数字版本、lts、current 或 Temurin -tem"
        }
        "only stable Rust channels can be imported" => "只能导入 Rust stable channel",
        "global.json sdk.version must be one exact stable x.y.z SDK version" => {
            "global.json sdk.version 必须是精确稳定的 x.y.z SDK 版本"
        }
        "selector cannot be mapped safely" => "选择器无法安全映射",
        "invalid JSON" => "JSON 格式无效",
        "invalid JSONC" => "JSONC 格式无效",
        "invalid TOML" => "TOML 格式无效",
        "invalid YAML" => "YAML 格式无效",
        _ if reason.starts_with("multiple traditional sources") => "多个传统来源选择了不同版本",
        _ if reason.starts_with("cannot inspect source:") => "无法检查来源文件",
        _ if reason.starts_with("source exceeds ") => "来源文件超过 1 MiB 限制",
        _ if reason.starts_with("unsupported Pinset Provider ") => "Pinset Provider 不受支持",
        _ => return format!("无法安全导入：{reason}"),
    };
    translated.to_owned()
}

fn discovery_status_name(status: DiscoveryStatus, language: Language) -> &'static str {
    match (language, status) {
        (Language::English, DiscoveryStatus::Ready) => "ready",
        (Language::English, DiscoveryStatus::Informational) => "informational",
        (Language::English, DiscoveryStatus::Ignored) => "ignored",
        (Language::English, DiscoveryStatus::Unsupported) => "unsupported",
        (Language::English, DiscoveryStatus::Conflict) => "conflict",
        (Language::SimplifiedChinese, DiscoveryStatus::Ready) => "可导入",
        (Language::SimplifiedChinese, DiscoveryStatus::Informational) => "仅报告",
        (Language::SimplifiedChinese, DiscoveryStatus::Ignored) => "已忽略",
        (Language::SimplifiedChinese, DiscoveryStatus::Unsupported) => "不支持",
        (Language::SimplifiedChinese, DiscoveryStatus::Conflict) => "冲突",
    }
}

fn run_project_import(
    cwd: &Path,
    force: bool,
    no_install: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = scan_project_sources(cwd)?;
    if !report.can_import {
        print_discovery_report(&report, catalog);
        return Err(match catalog.language() {
            Language::English => {
                "traditional configuration has no safe importable selection or contains blockers"
                    .into()
            }
            Language::SimplifiedChinese => {
                "传统配置中没有可安全导入的版本选择，或存在阻断项".into()
            }
        });
    }

    let config_path = report.target_config.clone();
    let config_exists = match fs::symlink_metadata(&config_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(match catalog.language() {
                Language::English => format!(
                    "refusing to import into unsafe project configuration path {}",
                    config_path.display()
                )
                .into(),
                Language::SimplifiedChinese => {
                    format!("拒绝导入到不安全的项目配置路径 {}", config_path.display()).into()
                }
            });
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let mut project = if config_exists {
        load_project_config(&config_path)?
    } else {
        ProjectConfig {
            schema: PROJECT_CONFIG_SCHEMA,
            policy: Default::default(),
            tools: BTreeMap::new(),
        }
    };
    let lock_path = lockfile_path(&config_path);
    let existing_lockfile = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(match catalog.language() {
                Language::English => format!(
                    "refusing to import with unsafe lock path {}",
                    lock_path.display()
                )
                .into(),
                Language::SimplifiedChinese => {
                    format!("拒绝使用不安全的锁文件路径 {} 导入", lock_path.display()).into()
                }
            });
        }
        Ok(_) => Some(load_lockfile(&lock_path)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    match &existing_lockfile {
        Some(lockfile) => validate_lock_matches_tools(lockfile, &project.tools, &config_path)?,
        None if !project.tools.is_empty() => {
            return Err(match catalog.language() {
                Language::English => format!(
                    "existing project configuration {} has selections but no pinset.lock",
                    config_path.display()
                )
                .into(),
                Language::SimplifiedChinese => format!(
                    "现有项目配置 {} 包含版本选择，但缺少 pinset.lock",
                    config_path.display()
                )
                .into(),
            });
        }
        None => {}
    }

    let selections = report
        .findings
        .iter()
        .filter(|finding| finding.status == DiscoveryStatus::Ready)
        .filter_map(|finding| {
            finding
                .normalized
                .as_ref()
                .map(|selector| (finding.tool.clone(), selector.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut resolved = Vec::with_capacity(selections.len());
    for (tool, selector) in selections {
        let locked_tool = resolve_locked_tool(&tool, &selector)?;
        resolved.push((tool, selector, locked_tool));
    }

    if !force {
        if let Some((tool, existing, imported)) = import_replacement_conflict(&project, &resolved) {
            return Err(match catalog.language() {
                Language::English => format!(
                    "{} already selects {tool}@{existing}; importing {tool}@{imported} requires --force",
                    config_path.display(),
                )
                .into(),
                Language::SimplifiedChinese => format!(
                    "{} 已选择 {tool}@{existing}；导入 {tool}@{imported} 需要 --force",
                    config_path.display(),
                )
                .into(),
            });
        }
    }

    let mut lockfile = existing_lockfile.unwrap_or_else(new_lockfile);
    lockfile.generated_by = format!("pinset {}", env!("CARGO_PKG_VERSION"));
    for (tool, selector, locked_tool) in &resolved {
        if selector != &locked_tool.version {
            println!(
                "{tool}@{selector} resolved to {tool}@{}",
                locked_tool.version
            );
        }
        project.set_tool(tool, selector);
        lockfile.upsert_tool(locked_tool.clone())?;
    }
    save_project_state(&config_path, &project, &lockfile)?;

    match catalog.language() {
        Language::English => println!(
            "imported {} runtime selection(s) into {}; lock {}",
            resolved.len(),
            config_path.display(),
            lock_path.display()
        ),
        Language::SimplifiedChinese => println!(
            "已将 {} 个运行时选择导入 {}；锁文件 {}",
            resolved.len(),
            config_path.display(),
            lock_path.display()
        ),
    }

    if no_install {
        let home = pinset_home()?;
        for (tool, _, _) in &resolved {
            if let Err(error) = register_provider_commands(&home, tool, catalog) {
                eprintln!(
                    "{}",
                    catalog.shim_auto_registration_failed(&error.to_string())
                );
            }
        }
        return Ok(());
    }

    if let Err(error) = install_project(&report.start, catalog) {
        let localized = catalog.command_error(error.as_ref());
        let detail = localized
            .strip_prefix("error: ")
            .or_else(|| localized.strip_prefix("错误："))
            .unwrap_or(&localized);
        return Err(match catalog.language() {
            Language::English => format!(
                "{detail}; project state was saved successfully, retry with `pinset install --locked`"
            )
            .into(),
            Language::SimplifiedChinese => format!(
                "{detail}；项目配置和锁文件已成功保存，请运行 `pinset install --locked` 重试"
            )
            .into(),
        });
    }
    Ok(())
}

fn import_replacement_conflict(
    project: &ProjectConfig,
    resolved: &[(String, String, LockedTool)],
) -> Option<(String, String, String)> {
    resolved.iter().find_map(|(tool, selector, _)| {
        project.tools.get(tool).and_then(|existing| {
            (existing != selector).then(|| (tool.clone(), existing.clone(), selector.clone()))
        })
    })
}

fn run_update(
    tool: Option<&str>,
    global: bool,
    cwd: Option<PathBuf>,
    dry_run: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tool) = tool {
        require_provider(tool)?;
    }
    let home = pinset_home()?;
    let cwd = effective_cwd(cwd)?;
    let (scope, config_path, tools, mut lockfile) = if global {
        let config_path = global_config_path(&home);
        let config = load_global_config(&config_path)?;
        let lockfile = load_lockfile(&global_lockfile_path(&home))?;
        validate_lock_matches_tools(&lockfile, &config.tools, &config_path)?;
        ("global", config_path, config.tools, lockfile)
    } else {
        let config_path = find_project_config(&cwd)?;
        let config = load_project_config(&config_path)?;
        let lockfile = load_lockfile(&lockfile_path(&config_path))?;
        validate_lock_matches_tools(&lockfile, &config.tools, &config_path)?;
        ("project", config_path, config.tools, lockfile)
    };

    let mut reports = Vec::new();
    for (selected_tool, requested) in &tools {
        if tool.is_some_and(|tool| tool != selected_tool) {
            continue;
        }
        let previous = lockfile
            .tool(selected_tool)
            .expect("validated lock contains configured tool")
            .version
            .clone();
        let resolved = resolve_locked_tool(selected_tool, requested)?;
        let report = UpdateReport {
            scope,
            config: config_path.clone(),
            tool: selected_tool.clone(),
            requested: requested.clone(),
            changed: previous != resolved.version,
            previous,
            resolved: resolved.version.clone(),
        };
        lockfile.upsert_tool(resolved)?;
        reports.push(report);
    }
    if let (Some(tool), true) = (tool, reports.is_empty()) {
        return Err(format!(
            "{} does not declare runtime provider {:?}",
            config_path.display(),
            tool
        )
        .into());
    }

    if !global {
        let config = load_project_config(&config_path)?;
        validate_project_lock_policy(&config, &lockfile, std::time::SystemTime::now())?;
    }

    if !dry_run {
        lockfile.generated_by = format!("pinset {}", env!("CARGO_PKG_VERSION"));
        if global {
            let config = load_global_config(&config_path)?;
            save_global_state(&home, &config, &lockfile)?;
        } else {
            let config = load_project_config(&config_path)?;
            save_project_state(&config_path, &config, &lockfile)?;
        }
    }

    if json {
        print_json_success(
            "update",
            serde_json::json!({ "dry_run": dry_run, "runtimes": reports }),
        )?;
    } else if reports.iter().all(|report| !report.changed) {
        println!("all selected runtimes already match their configured selectors");
    } else {
        for report in reports.iter().filter(|report| report.changed) {
            println!(
                "{}@{} -> {} requested={} scope={} lock-only",
                report.tool, report.previous, report.resolved, report.requested, report.scope
            );
        }
        if dry_run {
            println!("dry-run: lockfile was not changed");
        } else {
            println!("lock updated; run `pinset install --locked` to install resolved runtimes");
        }
    }
    Ok(())
}

fn run_migrate(
    global: bool,
    cwd: Option<PathBuf>,
    dry_run: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = effective_cwd(cwd)?;
    let (scope, config_path, lock_path, config_schema, lock_schema) = if global {
        let home = pinset_home()?;
        let config_path = global_config_path(&home);
        let lock_path = global_lockfile_path(&home);
        let config = load_global_config(&config_path)?;
        let lockfile = load_lockfile(&lock_path)?;
        validate_lock_matches_tools(&lockfile, &config.tools, &config_path)?;
        let report = (
            "global",
            config_path.clone(),
            lock_path.clone(),
            config.schema,
            lockfile.schema,
        );
        if !dry_run {
            save_global_state(&home, &config, &lockfile)?;
        }
        report
    } else {
        let config_path = find_project_config(&cwd)?;
        let lock_path = lockfile_path(&config_path);
        let config = load_project_config(&config_path)?;
        let lockfile = load_lockfile(&lock_path)?;
        validate_lock_matches_tools(&lockfile, &config.tools, &config_path)?;
        let report = (
            "project",
            config_path.clone(),
            lock_path.clone(),
            config.schema,
            lockfile.schema,
        );
        if !dry_run {
            save_project_state(&config_path, &config, &lockfile)?;
        }
        report
    };
    let report = MigrationReport {
        scope,
        config: config_path,
        lockfile: lock_path,
        from_config_schema: config_schema,
        from_lock_schema: lock_schema,
        to_schema: PROJECT_CONFIG_SCHEMA,
        changed: config_schema != PROJECT_CONFIG_SCHEMA
            || lock_schema != pinset_core::LOCKFILE_SCHEMA,
        dry_run,
    };
    if json {
        print_json_success("migrate", report)?;
    } else if report.changed {
        println!(
            "{} config-schema={} lock-schema={} -> schema={}{}",
            report.scope,
            report.from_config_schema,
            report.from_lock_schema,
            report.to_schema,
            if dry_run { " dry-run" } else { "" }
        );
    } else {
        println!("{} state already uses schema 3", report.scope);
    }
    Ok(())
}

fn install_tool_selection(
    selection: &str,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tool, selector) = parse_tool_selection(selection, catalog)?;
    let locked_tool = resolve_locked_tool(&tool, &selector)?;
    if selector != locked_tool.version {
        println!(
            "{tool}@{selector} resolved to {tool}@{}",
            locked_tool.version
        );
    }
    let mut lockfile = new_lockfile();
    lockfile.upsert_tool(locked_tool)?;
    install_tool_from_lock(&pinset_home()?, &lockfile, &tool, true, catalog)
}

fn unset_tool(
    tool: &str,
    global: bool,
    cwd: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if global {
        let home = pinset_home()?;
        let config_path = global_config_path(&home);
        let Some(mut config) = load_optional_global_config(&config_path)? else {
            println!(
                "{}",
                catalog.selection_unset("global", tool, &config_path, false)
            );
            return Ok(());
        };
        if config.tools.remove(tool).is_none() {
            println!(
                "{}",
                catalog.selection_unset("global", tool, &config_path, false)
            );
            return Ok(());
        }
        let lock_path = global_lockfile_path(&home);
        if lock_path.is_file() {
            load_lockfile(&lock_path)?;
        }
        save_global_config(&config_path, &config)?;
        remove_tool_from_lock(&lock_path, tool)?;
        println!(
            "{}",
            catalog.selection_unset("global", tool, &config_path, true)
        );
        return Ok(());
    }

    let config_path = find_project_config(cwd)?;
    let mut config = load_project_config(&config_path)?;
    if config.tools.remove(tool).is_none() {
        println!(
            "{}",
            catalog.selection_unset("project", tool, &config_path, false)
        );
        return Ok(());
    }
    let lock_path = lockfile_path(&config_path);
    if lock_path.is_file() {
        load_lockfile(&lock_path)?;
    }
    save_project_config(&config_path, &config)?;
    remove_tool_from_lock(&lock_path, tool)?;
    println!(
        "{}",
        catalog.selection_unset("project", tool, &config_path, true)
    );
    Ok(())
}

fn remove_tool_from_lock(path: &Path, tool: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !path.is_file() {
        return Ok(());
    }
    let mut lockfile = load_lockfile(path)?;
    lockfile.remove_tool(tool);
    if lockfile.tools.is_empty() {
        fs::remove_file(path)?;
    } else {
        save_lockfile(path, &lockfile)?;
    }
    Ok(())
}

fn language_from_arguments(arguments: &[OsString]) -> Result<Option<Language>, String> {
    let mut arguments = arguments.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let Some(argument) = argument.to_str() else {
            continue;
        };
        if argument == "--" {
            break;
        }
        if argument == "--lang" {
            let value = arguments
                .next()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    "--lang requires en or zh-CN / --lang 需要 en 或 zh-CN".to_owned()
                })?;
            return value.parse().map(Some);
        }
        if let Some(value) = argument.strip_prefix("--lang=") {
            return value.parse().map(Some);
        }
    }
    Ok(None)
}

fn language_from_env() -> Result<Option<Language>, String> {
    env::var("PINSET_LANG")
        .ok()
        .map(|value| value.parse())
        .transpose()
}

fn requested_help_command(arguments: &[OsString]) -> Option<Option<&str>> {
    let requested = arguments
        .iter()
        .skip(1)
        .filter_map(|value| value.to_str())
        .take_while(|value| *value != "--")
        .any(|value| value == "--help" || value == "-h");
    requested.then(|| command_from_arguments(arguments))
}

fn command_from_arguments(arguments: &[OsString]) -> Option<&str> {
    const COMMANDS: [&str; 24] = [
        "init",
        "detect",
        "import",
        "global",
        "use",
        "unset",
        "install",
        "which",
        "current",
        "outdated",
        "update",
        "migrate",
        "exec",
        "doctor",
        "shim",
        "activate",
        "completions",
        "source",
        "list",
        "uninstall",
        "prune",
        "lock",
        "cache",
        "venv",
    ];
    arguments
        .iter()
        .skip(1)
        .filter_map(|value| value.to_str())
        .take_while(|value| *value != "--")
        .find(|value| COMMANDS.contains(value))
}

fn resolve_language(requested: Option<Language>) -> Result<Language, Box<dyn std::error::Error>> {
    if let Some(language) = requested {
        return Ok(language);
    }
    let Ok(home) = pinset_home() else {
        return Ok(Language::default());
    };
    let settings = load_user_settings(&user_settings_path(&home))?;
    settings
        .language
        .as_deref()
        .map(str::parse)
        .transpose()
        .map(|language| language.unwrap_or_default())
        .map_err(Into::into)
}

fn install_project(cwd: &Path, catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    install_project_with_venv(cwd, false, catalog)
}

fn install_project_with_venv(
    cwd: &Path,
    recreate_venv: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = find_project_config(cwd)?;
    let project = load_project_config(&config_path)?;
    let lock_path = lockfile_path(&config_path);
    let home = pinset_home()?;
    let policy_lock = load_lockfile(&lock_path)?;
    validate_project_lock_policy(&project, &policy_lock, std::time::SystemTime::now())?;
    install_locked_selection(&home, &project.tools, &config_path, &lock_path, catalog)?;
    if let Some(requested) = project.tools.get("python") {
        let distribution = selected_version_from_lock(
            "python",
            requested,
            project.schema,
            &config_path,
            &lock_path,
        )?;
        ensure_project_python_environment(&home, &config_path, &distribution, recreate_venv)?;
    }
    Ok(())
}

fn run_venv_command(
    command: VenvCommands,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let (cwd, action) = match command {
        VenvCommands::Create { cwd } => (effective_cwd(cwd)?, "create"),
        VenvCommands::Status { cwd } => (effective_cwd(cwd)?, "status"),
        VenvCommands::Recreate { cwd } => (effective_cwd(cwd)?, "recreate"),
    };
    let config_path = find_project_config(&cwd)?;
    let project = load_project_config(&config_path)?;
    let requested =
        project
            .tools
            .get("python")
            .ok_or_else(|| Error::PythonEnvironmentSelectionMissing {
                path: config_path.clone(),
            })?;
    let distribution = selected_version_from_lock(
        "python",
        requested,
        project.schema,
        &config_path,
        &lockfile_path(&config_path),
    )?;
    if action == "status" {
        let target = current_target_for_tool("python");
        let environment = load_project_python_environment(&config_path, &distribution, &target)?;
        println!(
            "python@{} project environment {}",
            environment.distribution,
            environment.root.display()
        );
        return Ok(());
    }
    install_project_with_venv(&cwd, action == "recreate", catalog)
}

fn ensure_project_python_environment(
    home: &Path,
    config_path: &Path,
    distribution: &str,
    recreate: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = current_target_for_tool("python");
    let install_dir = home
        .join("installs")
        .join("python")
        .join(distribution)
        .join(&target);
    let candidates = runtime_command_candidates("python", "python", &install_dir);
    let base_python = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| Error::RuntimeCommandNotFound {
            tool: "python".to_owned(),
            version: distribution.to_owned(),
            command: "python".to_owned(),
            searched: candidates
                .iter()
                .map(|candidate| candidate.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        })?;
    let environment = create_project_python_environment(
        config_path,
        base_python,
        distribution,
        &target,
        recreate,
    )?;
    println!(
        "python@{} project environment ready at {}",
        environment.distribution,
        environment.root.display()
    );
    Ok(())
}

fn install_global(home: &Path, catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = global_config_path(home);
    let config: GlobalConfig = load_global_config(&config_path)?;
    install_locked_selection(
        home,
        &config.tools,
        &config_path,
        &global_lockfile_path(home),
        catalog,
    )
}

fn install_locked_selection(
    home: &Path,
    configured: &std::collections::BTreeMap<String, String>,
    config_path: &Path,
    lock_path: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let lockfile = load_lockfile(lock_path)?;
    validate_lock_matches_tools(&lockfile, configured, config_path)?;
    for provider in pinset_core::selected_provider_order(configured)? {
        install_tool_from_lock(home, &lockfile, provider.tool, true, catalog)?;
    }
    Ok(())
}

fn install_tool_from_lock(
    home: &Path,
    lockfile: &Lockfile,
    tool: &str,
    register_shims: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let locked_tool = lockfile
        .tool(tool)
        .ok_or_else(|| Error::LockedToolMissing {
            tool: tool.to_owned(),
        })?;
    let installer = Installer::new(InstallLimits::for_tool(tool))?
        .with_progress_reporter(download_progress_reporter(catalog));
    let target = current_target_for_tool(tool);
    let provider = runtime_provider(tool).expect("locked tool provider exists");
    let outcome = match provider.capabilities.installer {
        RuntimeInstallKind::Node => {
            let sources = load_source_config(&source_config_path(home))?;
            install_locked_node(&installer, home, &sources, locked_tool, &target)?
        }
        RuntimeInstallKind::Npm => install_locked_npm_tool(&installer, home, locked_tool, &target)?,
        RuntimeInstallKind::Go => {
            let sources = load_source_config(&source_config_path(home))?;
            install_locked_go(&installer, home, &sources, locked_tool, &target)?
        }
        RuntimeInstallKind::Flutter => {
            let sources = load_source_config(&source_config_path(home))?;
            install_locked_flutter(&installer, home, &sources, locked_tool, &target)?
        }
        RuntimeInstallKind::Python => {
            let sources = load_source_config(&source_config_path(home))?;
            install_locked_python(&installer, home, &sources, locked_tool, &target)?
        }
        RuntimeInstallKind::Java => install_locked_java(&installer, home, locked_tool, &target)?,
        RuntimeInstallKind::Rust => install_locked_rust(&installer, home, locked_tool, &target)?,
        RuntimeInstallKind::Dotnet => {
            install_locked_dotnet(&installer, home, locked_tool, &target)?
        }
    };
    if outcome.reused_existing {
        if tool == "node" {
            println!(
                "{}",
                catalog.already_installed(&locked_tool.version, &target, &outcome.install_dir)
            );
        } else {
            println!(
                "{tool}@{} is already installed for {target} at {}",
                locked_tool.version,
                outcome.install_dir.display()
            );
        }
    } else if tool == "node" {
        println!(
            "{}",
            catalog.installed(
                &locked_tool.version,
                &target,
                &outcome.source_id,
                &outcome.install_dir,
            )
        );
    } else {
        println!(
            "installed {tool}@{} for {target} from {} at {}",
            locked_tool.version,
            outcome.source_id,
            outcome.install_dir.display()
        );
    }
    if register_shims {
        if let Err(error) = register_provider_commands(home, tool, catalog) {
            eprintln!(
                "{}",
                catalog.shim_auto_registration_failed(&error.to_string())
            );
        }
    }
    Ok(())
}

#[derive(Debug)]
struct DownloadProgressDisplay {
    interactive: bool,
    active: bool,
    artifact: String,
    last_render: Option<Instant>,
}

fn download_progress_reporter(
    catalog: Catalog,
) -> impl Fn(DownloadProgressEvent) + Send + Sync + 'static {
    let state = Mutex::new(DownloadProgressDisplay {
        interactive: io::stderr().is_terminal(),
        active: false,
        artifact: String::new(),
        last_render: None,
    });
    move |event| {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match event {
            DownloadProgressEvent::Started { url, total_bytes } => {
                state.active = true;
                state.artifact = download_artifact_name(&url);
                state.last_render = Some(Instant::now());
                if state.interactive {
                    render_download_progress(catalog, &state.artifact, 0, total_bytes);
                } else {
                    eprintln!(
                        "{}",
                        catalog.download_started(&state.artifact, total_bytes.map(format_bytes))
                    );
                }
            }
            DownloadProgressEvent::Advanced {
                downloaded_bytes,
                total_bytes,
            } if state.active && state.interactive => {
                let now = Instant::now();
                let complete = total_bytes.is_some_and(|total| downloaded_bytes >= total);
                if complete
                    || state
                        .last_render
                        .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(80))
                {
                    render_download_progress(
                        catalog,
                        &state.artifact,
                        downloaded_bytes,
                        total_bytes,
                    );
                    state.last_render = Some(now);
                }
            }
            DownloadProgressEvent::Advanced { .. } => {}
            DownloadProgressEvent::Finished { downloaded_bytes } if state.active => {
                if state.interactive {
                    clear_progress_line();
                }
                eprintln!(
                    "{}",
                    catalog.download_finished(&state.artifact, &format_bytes(downloaded_bytes))
                );
                state.active = false;
            }
            DownloadProgressEvent::Failed if state.active => {
                if state.interactive {
                    clear_progress_line();
                }
                eprintln!("{}", catalog.download_failed(&state.artifact));
                state.active = false;
            }
            DownloadProgressEvent::Finished { .. } | DownloadProgressEvent::Failed => {}
        }
    }
}

fn download_artifact_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("runtime archive")
        .to_owned()
}

fn render_download_progress(
    catalog: Catalog,
    artifact: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    const FALLBACK_TERMINAL_COLUMNS: usize = 80;
    let terminal_columns = terminal_size_of(io::stderr())
        .map(|(Width(columns), _)| usize::from(columns))
        .unwrap_or(FALLBACK_TERMINAL_COLUMNS);
    let line = download_progress_line(
        catalog,
        artifact,
        downloaded_bytes,
        total_bytes,
        terminal_columns,
    );
    eprint!("\r\x1b[2K{line}");
    let _ = io::stderr().flush();
}

fn download_progress_line(
    catalog: Catalog,
    artifact: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    terminal_columns: usize,
) -> String {
    const MAX_BAR_WIDTH: usize = 24;
    const MIN_BAR_WIDTH: usize = 6;
    const PREFERRED_ARTIFACT_WIDTH: usize = 20;

    // Leave the final terminal column unused. Writing into it can enable
    // automatic line wrapping before the next carriage return is processed.
    let available_columns = terminal_columns.saturating_sub(1);
    if available_columns == 0 {
        return String::new();
    }

    let downloaded = format_bytes(downloaded_bytes);
    let (percent, ratio, total) = match total_bytes.filter(|total| *total > 0) {
        Some(total) => {
            let ratio = (downloaded_bytes as f64 / total as f64).clamp(0.0, 1.0);
            (
                (ratio * 100.0).round() as u8,
                Some(ratio),
                Some(format_bytes(total)),
            )
        }
        None => (0, None, None),
    };

    let fixed = catalog.download_progress("", "", percent, &downloaded, total.clone());
    let fixed_width = UnicodeWidthStr::width(fixed.as_str());
    if fixed_width >= available_columns {
        let compact = match &total {
            Some(total) => format!("{percent:>3}% {downloaded}/{total}"),
            None => downloaded,
        };
        return truncate_end_to_width(&compact, available_columns);
    }

    let variable_width = available_columns - fixed_width;
    let preferred_artifact_width = UnicodeWidthStr::width(artifact).min(PREFERRED_ARTIFACT_WIDTH);
    let remaining_for_bar = variable_width.saturating_sub(preferred_artifact_width);
    let bar_width = if remaining_for_bar >= MIN_BAR_WIDTH {
        remaining_for_bar.min(MAX_BAR_WIDTH)
    } else if variable_width > MIN_BAR_WIDTH {
        MIN_BAR_WIDTH
    } else {
        0
    };
    let artifact_width = variable_width - bar_width;
    let artifact = truncate_middle_to_width(artifact, artifact_width);
    let bar = match ratio {
        Some(ratio) => {
            let filled = (ratio * bar_width as f64).round() as usize;
            format!("{}{}", "=".repeat(filled), " ".repeat(bar_width - filled))
        }
        None => "-".repeat(bar_width),
    };
    let line = catalog.download_progress(&artifact, &bar, percent, &downloaded, total);
    debug_assert!(UnicodeWidthStr::width(line.as_str()) <= available_columns);
    line
}

fn truncate_middle_to_width(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_owned();
    }

    let content_width = max_width - 1;
    let prefix_width = content_width.div_ceil(2);
    let suffix_width = content_width - prefix_width;
    let prefix = take_prefix_to_width(value, prefix_width);
    let suffix = take_suffix_to_width(value, suffix_width);
    format!("{prefix}…{suffix}")
}

fn truncate_end_to_width(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_owned();
    }
    format!("{}…", take_prefix_to_width(value, max_width - 1))
}

fn take_prefix_to_width(value: &str, max_width: usize) -> String {
    let mut width = 0;
    value
        .chars()
        .take_while(|character| {
            let character_width = UnicodeWidthChar::width(*character).unwrap_or(0);
            if width + character_width > max_width {
                return false;
            }
            width += character_width;
            true
        })
        .collect()
}

fn take_suffix_to_width(value: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut suffix = value
        .chars()
        .rev()
        .take_while(|character| {
            let character_width = UnicodeWidthChar::width(*character).unwrap_or(0);
            if width + character_width > max_width {
                return false;
            }
            width += character_width;
            true
        })
        .collect::<Vec<_>>();
    suffix.reverse();
    suffix.into_iter().collect()
}

fn clear_progress_line() {
    eprint!("\r\x1b[2K");
    let _ = io::stderr().flush();
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn print_global_current(catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let config_path = global_config_path(&home);
    let Some(config) = load_optional_global_config(&config_path)? else {
        println!("{}", catalog.global_not_selected(&config_path));
        return Ok(());
    };
    if config.tools.is_empty() {
        println!("{}", catalog.global_not_selected(&config_path));
        return Ok(());
    }
    let lock_path = global_lockfile_path(&home);
    for (tool, requested) in &config.tools {
        let version =
            selected_version_from_lock(tool, requested, config.schema, &config_path, &lock_path)?;
        print_declared_tool(
            &home,
            tool,
            requested,
            &version,
            "global",
            &config_path,
            catalog,
        )?;
    }
    Ok(())
}

fn print_project_override(
    cwd: &Path,
    home: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(config_path) = find_optional_project_config(cwd)? else {
        return Ok(());
    };
    let project = load_project_config(&config_path)?;
    let Some(project_version) = project.tools.get("node") else {
        return Ok(());
    };
    let global_path = global_config_path(home);
    let Some(global) = load_optional_global_config(&global_path)? else {
        return Ok(());
    };
    let Some(global_version) = global.tools.get("node") else {
        return Ok(());
    };
    println!(
        "{}",
        catalog.global_project_override(global_version, project_version, &config_path)
    );
    Ok(())
}

fn print_declared_tool(
    home: &Path,
    tool: &str,
    requested: &str,
    version: &str,
    source: &str,
    config_path: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if requested != version {
        println!("{tool}@{requested} resolved to {tool}@{version}");
    }
    let install_dir = home
        .join("installs")
        .join(tool)
        .join(version)
        .join(current_target_for_tool(tool));
    let command_dir = runtime_command_directory(tool, &install_dir);
    let command = runtime_provider(tool)
        .and_then(|provider| provider.commands.first())
        .ok_or_else(|| Error::UnsupportedCommand {
            command: tool.to_owned(),
        })?;
    let executable = runtime_command_path(&command_dir, command);
    if executable.is_file() {
        println!(
            "{}",
            catalog.current_installed(tool, version, source, &executable, Some(config_path))
        );
    } else {
        println!(
            "{}",
            catalog.current_missing(tool, version, source, &command_dir, Some(config_path))
        );
    }
    Ok(())
}

fn print_current(
    cwd: &Path,
    tool: &str,
    explain: bool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = match current_report(cwd, tool, explain) {
        Ok(report) => report,
        Err(error) => {
            if explain {
                let provider = runtime_provider(tool)
                    .or_else(|| command_tool(tool).and_then(runtime_provider));
                if let Some(provider) = provider {
                    if let Ok(explanation) = resolution_explanation(cwd, provider.tool, "none") {
                        print_resolution_explanation(&explanation);
                    }
                }
            }
            return Err(error);
        }
    };
    if let Some(explanation) = report.explanation.as_ref() {
        print_resolution_explanation(explanation);
    }
    if let Some(requested) = report
        .requested
        .as_deref()
        .filter(|value| *value != report.version.as_str())
    {
        println!(
            "{}@{} resolved to {}@{}",
            report.tool, requested, report.tool, report.version
        );
    }
    if let Some(executable) = report.executable.as_deref() {
        println!(
            "{}",
            catalog.current_installed(
                &report.tool,
                &report.version,
                report.source,
                executable,
                report.config.as_deref(),
            )
        );
    } else {
        println!(
            "{}",
            catalog.current_missing(
                &report.tool,
                &report.version,
                report.source,
                report
                    .expected_directory
                    .as_deref()
                    .expect("missing runtime has an expected directory"),
                report.config.as_deref(),
            )
        );
    }
    Ok(())
}

fn execute_selected(
    cwd: &Path,
    command: &[OsString],
    install_ephemeral: bool,
    catalog: Catalog,
) -> Result<i32, Box<dyn std::error::Error>> {
    let (ephemeral_selection, mut command) = command
        .first()
        .and_then(|value| value.to_str())
        .filter(|value| {
            value.split_once('@').is_some_and(|(tool, selector)| {
                !selector.is_empty() && runtime_provider(tool).is_some()
            })
        })
        .map_or((None, command), |selection| {
            (Some(selection), &command[1..])
        });
    if ephemeral_selection.is_some() && command.first().is_some_and(|value| value == "--") {
        command = &command[1..];
    }
    let command_name = command
        .first()
        .and_then(|value| value.to_str())
        .ok_or_else(|| catalog.utf8_command_error())?;
    let home = pinset_home()?;
    let (executable, tool, version, source, config_path, ephemeral_environment) =
        if let Some(selection) = ephemeral_selection {
            let (tool, selector) = parse_tool_selection(selection, catalog)?;
            if command_tool(command_name) != Some(tool.as_str()) {
                return Err(Error::UnsupportedCommand {
                    command: command_name.to_owned(),
                }
                .into());
            }
            let locked_tool = resolve_locked_tool(&tool, &selector)?;
            let version = locked_tool.version.clone();
            if selector != version {
                println!("{tool}@{selector} resolved to {tool}@{version}");
            }
            if install_ephemeral {
                install_ephemeral_selection(&home, cwd, &tool, locked_tool, catalog)?;
            }
            let install_dir = home
                .join("installs")
                .join(&tool)
                .join(&version)
                .join(current_target_for_tool(&tool));
            let candidates = runtime_command_candidates(&tool, command_name, &install_dir);
            let executable = candidates
                .iter()
                .find(|candidate| candidate.is_file())
                .cloned()
                .ok_or_else(|| Error::RuntimeCommandNotFound {
                    tool: tool.clone(),
                    version: version.clone(),
                    command: command_name.to_owned(),
                    searched: candidates
                        .iter()
                        .map(|candidate| candidate.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                })?;
            let environment = runtime_environment_for_install(&tool, &install_dir);
            (
                executable,
                tool,
                version,
                if install_ephemeral {
                    "one-shot"
                } else {
                    "ephemeral"
                },
                None,
                environment,
            )
        } else {
            let resolution = if command_tool(command_name).is_some() {
                resolve_command(command_name, cwd, &home)?
            } else {
                resolve_project_python_command(command_name, cwd, &home)?
            };
            (
                resolution.executable,
                resolution.tool,
                resolution.version,
                resolution.source.as_str(),
                resolution.selection_path,
                Vec::new(),
            )
        };
    let runtime_path = path_with_selected_tools(&tool, &executable, cwd, &home)?;
    if source != "system" {
        validate_managed_runtime_invocation(&tool, command_name, &command[1..])?;
    }
    let runtime_arguments = if source == "system" {
        command[1..].to_vec()
    } else {
        managed_runtime_arguments(&tool, command_name, &command[1..])
    };
    let mut child = command_for_runtime(&executable);
    child
        .args(runtime_arguments)
        .current_dir(cwd)
        .env("PATH", runtime_path)
        .env("PINSET_SELECTED_TOOL", &tool)
        .env("PINSET_SELECTED_VERSION", &version)
        .env("PINSET_SELECTION_SOURCE", source);
    for variable in selected_runtime_environment(&tool, cwd, &home) {
        child.env(variable.name, variable.value);
    }
    for variable in ephemeral_environment {
        child.env(variable.name, variable.value);
    }
    if tool == "python" {
        child.env_remove("PYTHONHOME");
        if source != "project" {
            child.env_remove("VIRTUAL_ENV");
        }
    }
    if let Some(path) = &config_path {
        child.env("PINSET_CONFIG_PATH", path);
    } else {
        child.env_remove("PINSET_CONFIG_PATH");
    }
    let status = child.status()?;
    // INVARIANT: exec is transparent after launch. Keep the platform's full i32 exit value
    // instead of narrowing it to Pinset's own small exit-code range.
    Ok(status.code().unwrap_or(1))
}

fn install_ephemeral_selection(
    home: &Path,
    cwd: &Path,
    tool: &str,
    selected: LockedTool,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let providers = provider_dependency_order(tool)?;
    let mut lockfile = new_lockfile();
    for provider in &providers {
        let locked_tool = if provider.tool == tool {
            selected.clone()
        } else {
            let selection = match resolve_tool_selection(provider.tool, cwd, home) {
                Ok(selection) => selection,
                Err(
                    Error::ToolSelectionNotFound { .. }
                    | Error::ProjectToolSelectionRequired { .. },
                ) => {
                    return Err(Error::ProviderDependencyMissing {
                        tool: tool.to_owned(),
                        dependency: provider.tool.to_owned(),
                    }
                    .into());
                }
                Err(error) => return Err(error.into()),
            };
            let lock_path = match selection.source {
                pinset_core::SelectionSource::Project => lockfile_path(&selection.config_path),
                pinset_core::SelectionSource::Global => global_lockfile_path(home),
                pinset_core::SelectionSource::System => unreachable!("declared selection"),
            };
            let dependency_lock = load_lockfile(&lock_path)?;
            validate_lock_matches_tool(
                &dependency_lock,
                provider.tool,
                &selection.requested,
                &selection.config_path,
            )?;
            if selection.source == pinset_core::SelectionSource::Project {
                let config = load_project_config(&selection.config_path)?;
                validate_project_lock_policy(
                    &config,
                    &dependency_lock,
                    std::time::SystemTime::now(),
                )?;
            }
            dependency_lock
                .tool(provider.tool)
                .cloned()
                .ok_or_else(|| Error::LockedToolMissing {
                    tool: provider.tool.to_owned(),
                })?
        };
        lockfile.upsert_tool(locked_tool)?;
    }
    for provider in providers {
        install_tool_from_lock(home, &lockfile, provider.tool, false, catalog)?;
    }
    Ok(())
}

fn runtime_command_path(command_dir: &Path, command: &str) -> PathBuf {
    if cfg!(windows) {
        for extension in ["exe", "cmd", "bat"] {
            let candidate = command_dir.join(command).with_extension(extension);
            if candidate.is_file() {
                return candidate;
            }
        }
        return command_dir.join(command).with_extension("exe");
    }
    command_dir.join(command)
}

fn command_for_runtime(executable: &Path) -> Command {
    #[cfg(windows)]
    {
        let extension = executable
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command = Command::new("cmd.exe");
            command.arg("/D").arg("/C").arg(executable);
            return command;
        }
    }
    Command::new(executable)
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    cwd: String,
    pinset_home: String,
    boundary: String,
    project_config: DoctorItem,
    global_config: DoctorItem,
    selection: Option<DoctorSelection>,
    lockfile: DoctorItem,
    runtime: DoctorItem,
    python_environment: DoctorItem,
    shim_path: DoctorItem,
    legacy_shim_path: DoctorItem,
    path_candidates: Vec<DoctorPathCandidate>,
    routing_issues: Vec<DoctorRoutingIssue>,
    traditional_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorItem {
    status: &'static str,
    path: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorSelection {
    tool: String,
    requested: String,
    version: String,
    source: String,
    config_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorPathCandidate {
    command: String,
    path: String,
    owner: String,
    position: usize,
    effective: bool,
    managed: bool,
}

#[derive(Debug, Serialize)]
struct DoctorRoutingIssue {
    code: &'static str,
    command: Option<String>,
    path: Option<String>,
    action: &'static str,
}

fn doctor_report(cwd: &Path) -> Result<DoctorReport, Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let context = find_project_context(cwd)?;
    let project_path = context.config_path.clone();
    let global_path = global_config_path(&home);
    let project_config = DoctorItem {
        status: if project_path.is_some() {
            "ok"
        } else {
            "missing"
        },
        path: project_path.as_ref().map(|path| path.display().to_string()),
        detail: None,
    };
    let global_config = DoctorItem {
        status: if global_path.is_file() {
            "ok"
        } else {
            "missing"
        },
        path: Some(global_path.display().to_string()),
        detail: None,
    };

    let selected = match resolve_tool_selection("node", cwd, &home) {
        Ok(selection) => Some(selection),
        Err(Error::ToolSelectionNotFound { .. } | Error::ProjectToolSelectionRequired { .. }) => {
            None
        }
        Err(error) => return Err(error.into()),
    };
    let selection = selected.as_ref().map(|selection| DoctorSelection {
        tool: selection.tool.clone(),
        requested: selection.requested.clone(),
        version: selection.version.clone(),
        source: selection.source.as_str().to_owned(),
        config_path: Some(selection.config_path.display().to_string()),
    });
    let lockfile = if let Some(selection) = &selected {
        let path = match selection.source {
            pinset_core::SelectionSource::Project => lockfile_path(&selection.config_path),
            pinset_core::SelectionSource::Global => global_lockfile_path(&home),
            pinset_core::SelectionSource::System => unreachable!("declared selection"),
        };
        let lockfile = load_lockfile(&path)?;
        validate_lock_matches_selection(&lockfile, &selection.requested, &selection.config_path)?;
        DoctorItem {
            status: "ok",
            path: Some(path.display().to_string()),
            detail: Some(format!("node@{}", selection.version)),
        }
    } else {
        DoctorItem {
            status: "not-applicable",
            path: None,
            detail: None,
        }
    };
    let runtime = match resolve_command("node", cwd, &home) {
        Ok(resolution) => DoctorItem {
            status: "ok",
            path: Some(resolution.executable.display().to_string()),
            detail: Some(format!(
                "node@{} source={}",
                resolution.version,
                resolution.source.as_str()
            )),
        },
        Err(Error::RuntimeCommandNotFound {
            version, searched, ..
        }) => DoctorItem {
            status: "missing",
            path: None,
            detail: Some(format!("node@{version}; searched={searched}")),
        },
        Err(Error::CommandSelectionNotFound { searched, .. }) => DoctorItem {
            status: "missing",
            path: None,
            detail: Some(format!("searched={searched}")),
        },
        Err(Error::ProjectToolSelectionRequired { config_path, .. }) => DoctorItem {
            status: "blocked",
            path: Some(config_path.display().to_string()),
            detail: Some("strict project does not declare node".to_owned()),
        },
        Err(error) => return Err(error.into()),
    };
    let python_environment = doctor_python_environment(cwd, &home)?;
    let shims = command_routing_directory(&home)?;
    let shim_on_path = directory_on_path(&shims);
    let shim_binary = default_shim_binary()?;
    let commands = pinset_core::runtime_providers()
        .iter()
        .flat_map(|provider| provider.commands.iter().copied())
        .collect::<Vec<_>>();
    let path_candidates = inspect_path_candidates(&commands, &home, &shim_binary);
    let legacy_shims = home.join("shims");
    let legacy_commands = if paths_equal(&legacy_shims, &shims) {
        Vec::new()
    } else {
        existing_shim_commands(&legacy_shims, &commands)
    };
    let mut routing_issues = collect_routing_issues(
        pinset_core::runtime_providers()
            .iter()
            .any(|provider| resolve_tool_selection(provider.tool, cwd, &home).is_ok()),
        &commands,
        &path_candidates,
        &shims,
        &shim_binary,
        &legacy_shims,
        &legacy_commands,
    );
    if let Some(issue) = go_toolchain_routing_issue(cwd, &home) {
        routing_issues.push(issue);
    }
    routing_issues.extend(java_environment_issues(cwd, &home));
    let traditional_sources = scan_project_sources(cwd)?
        .findings
        .into_iter()
        .map(|finding| {
            format!(
                "{}:{}:{}",
                finding.tool,
                finding.source,
                discovery_status_name(finding.status, Language::English)
            )
        })
        .collect();
    Ok(DoctorReport {
        cwd: cwd.display().to_string(),
        pinset_home: home.display().to_string(),
        boundary: context.boundary.display().to_string(),
        project_config,
        global_config,
        selection,
        lockfile,
        runtime,
        python_environment,
        shim_path: DoctorItem {
            status: if shim_on_path {
                "active"
            } else {
                "not-on-path"
            },
            path: Some(shims.display().to_string()),
            detail: None,
        },
        legacy_shim_path: DoctorItem {
            status: if paths_equal(&legacy_shims, &shims) {
                "active-layout"
            } else if legacy_commands.is_empty() {
                "empty"
            } else {
                "legacy-preserved"
            },
            path: Some(legacy_shims.display().to_string()),
            detail: (!legacy_commands.is_empty()).then(|| legacy_commands.join(",")),
        },
        path_candidates,
        routing_issues,
        traditional_sources,
    })
}

fn doctor_python_environment(
    cwd: &Path,
    home: &Path,
) -> Result<DoctorItem, Box<dyn std::error::Error>> {
    let selection = match resolve_tool_selection("python", cwd, home) {
        Ok(selection) => selection,
        Err(Error::ToolSelectionNotFound { .. } | Error::ProjectToolSelectionRequired { .. }) => {
            return Ok(DoctorItem {
                status: "not-applicable",
                path: None,
                detail: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if selection.source != pinset_core::SelectionSource::Project {
        return Ok(DoctorItem {
            status: "not-applicable",
            path: None,
            detail: Some(format!("python@{} source=global", selection.version)),
        });
    }
    let path = project_python_environment_path(&selection.config_path);
    match load_project_python_environment(
        &selection.config_path,
        &selection.version,
        &current_target_for_tool("python"),
    ) {
        Ok(environment) => Ok(DoctorItem {
            status: "ok",
            path: Some(environment.root.display().to_string()),
            detail: Some(format!("python@{}", environment.distribution)),
        }),
        Err(error @ Error::PythonEnvironmentMissing { .. }) => Ok(DoctorItem {
            status: "missing",
            path: Some(path.display().to_string()),
            detail: Some(error.to_string()),
        }),
        Err(
            error @ (Error::PythonEnvironmentNotOwned { .. }
            | Error::PythonEnvironmentMismatch { .. }
            | Error::InvalidPythonEnvironmentMarker { .. }),
        ) => Ok(DoctorItem {
            status: "invalid",
            path: Some(path.display().to_string()),
            detail: Some(error.to_string()),
        }),
        Err(error) => Err(error.into()),
    }
}

fn collect_routing_issues(
    selected: bool,
    commands: &[&str],
    path_candidates: &[DoctorPathCandidate],
    routing_directory: &Path,
    shim_binary: &Path,
    legacy_directory: &Path,
    legacy_commands: &[String],
) -> Vec<DoctorRoutingIssue> {
    let mut issues = Vec::new();
    let routing_has_entries = commands
        .iter()
        .any(|command| command_entry_exists(routing_directory, command));
    if !directory_on_path(routing_directory) && (selected || routing_has_entries) {
        issues.push(DoctorRoutingIssue {
            code: "routing-directory-not-on-path",
            command: None,
            path: Some(routing_directory.display().to_string()),
            action: "pinset activate <shell>",
        });
    }
    if !legacy_commands.is_empty() {
        issues.push(DoctorRoutingIssue {
            code: "legacy-shims-present",
            command: None,
            path: Some(legacy_directory.display().to_string()),
            action: "pinset shim migrate --provider node",
        });
    }
    if !selected {
        return issues;
    }

    for command in commands {
        let candidates = path_candidates
            .iter()
            .filter(|candidate| candidate.command == *command)
            .collect::<Vec<_>>();
        if let Some(effective) = candidates.first().filter(|candidate| !candidate.managed) {
            if candidates.iter().any(|candidate| candidate.managed) {
                issues.push(DoctorRoutingIssue {
                    code: "provider-route-shadowed",
                    command: Some((*command).to_owned()),
                    path: Some(effective.path.clone()),
                    action: "place the Pinset routing directory earlier in PATH",
                });
                continue;
            }
        }
        if candidates.iter().any(|candidate| candidate.managed)
            || managed_command_entry(routing_directory, command, shim_binary).is_some()
        {
            continue;
        }
        if let Some(path) = command_entry_paths(routing_directory, command)
            .into_iter()
            .find(|path| fs::symlink_metadata(path).is_ok())
        {
            issues.push(DoctorRoutingIssue {
                code: "provider-route-conflict",
                command: Some((*command).to_owned()),
                path: Some(path.display().to_string()),
                action: "review the existing command before running pinset shim install",
            });
        } else {
            issues.push(DoctorRoutingIssue {
                code: "provider-route-missing",
                command: Some((*command).to_owned()),
                path: Some(routing_directory.display().to_string()),
                action: "pinset shim install --provider node",
            });
        }
    }
    issues
}

fn go_toolchain_routing_issue(cwd: &Path, home: &Path) -> Option<DoctorRoutingIssue> {
    resolve_tool_selection("go", cwd, home).ok()?;
    let value = env::var_os("GOTOOLCHAIN")?;
    if value.to_string_lossy().eq_ignore_ascii_case("local") {
        return None;
    }
    Some(DoctorRoutingIssue {
        code: "go-toolchain-override",
        command: Some("go".to_owned()),
        path: None,
        action: "unset GOTOOLCHAIN or set it to local to enforce the Pinset lock",
    })
}

fn java_environment_issues(cwd: &Path, home: &Path) -> Vec<DoctorRoutingIssue> {
    if resolve_tool_selection("java", cwd, home).is_err() {
        return Vec::new();
    }
    [
        (
            "CLASSPATH",
            "java-classpath-override",
            "review CLASSPATH if classes or dependencies resolve unexpectedly",
        ),
        (
            "JAVA_TOOL_OPTIONS",
            "java-tool-options-override",
            "review JAVA_TOOL_OPTIONS because the JVM reads it automatically",
        ),
        (
            "JDK_JAVA_OPTIONS",
            "jdk-java-options-override",
            "review JDK_JAVA_OPTIONS because the java launcher reads it automatically",
        ),
        (
            "_JAVA_OPTIONS",
            "java-legacy-options-override",
            "review _JAVA_OPTIONS because some JVMs read it automatically",
        ),
    ]
    .into_iter()
    .filter_map(|(name, code, action)| {
        env::var_os(name).map(|value| DoctorRoutingIssue {
            code,
            command: Some("java".to_owned()),
            path: Some(format!("{name}={}", value.to_string_lossy())),
            action,
        })
    })
    .collect()
}

fn run_doctor(cwd: &Path, catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    println!(
        "{}",
        catalog.doctor_line("pinset_home", home.display(), "ok")
    );
    if let Some(config_path) = find_optional_project_config(cwd)? {
        println!(
            "{}",
            catalog.doctor_line("project_config", config_path.display(), "ok")
        );
    } else {
        println!(
            "{}",
            catalog.doctor_line("project_config", cwd.display(), "missing")
        );
    }
    let global_path = global_config_path(&home);
    println!(
        "{}",
        catalog.doctor_line(
            "global_config",
            global_path.display(),
            if global_path.is_file() {
                "ok"
            } else {
                "missing"
            },
        )
    );
    if let Some(user_home) = user_home_directory() {
        let transitional = user_home.join("pinset.toml");
        if transitional.is_file() && cwd.starts_with(&user_home) {
            println!(
                "{}",
                catalog.transitional_home_config(&transitional, global_path.is_file())
            );
        }
    }

    for provider in pinset_core::runtime_providers() {
        let mut has_declared_selection = false;
        match resolve_tool_selection(provider.tool, cwd, &home) {
            Ok(selection) => {
                has_declared_selection = true;
                if provider.tool == "node" {
                    println!(
                        "{}",
                        catalog.doctor_selection(
                            &selection.version,
                            selection.source.as_str(),
                            Some(&selection.config_path),
                        )
                    );
                } else {
                    println!(
                        "{}",
                        catalog.doctor_line(
                            &format!("{}_selection", provider.tool),
                            format!(
                                "{}@{} source={} config={}",
                                provider.tool,
                                selection.version,
                                selection.source.as_str(),
                                selection.config_path.display()
                            ),
                            "ok",
                        )
                    );
                }
                let lock_path = match selection.source {
                    pinset_core::SelectionSource::Project => lockfile_path(&selection.config_path),
                    pinset_core::SelectionSource::Global => global_lockfile_path(&home),
                    pinset_core::SelectionSource::System => unreachable!("declared selection"),
                };
                let lockfile = load_lockfile(&lock_path)?;
                validate_lock_matches_tool(
                    &lockfile,
                    provider.tool,
                    &selection.requested,
                    &selection.config_path,
                )?;
                println!(
                    "{}",
                    if provider.tool == "node" {
                        catalog.doctor_lock_matches(&lock_path, &selection.version)
                    } else {
                        catalog.doctor_line(
                            &format!("{}_lock", provider.tool),
                            lock_path.display(),
                            "ok",
                        )
                    }
                );
            }
            Err(Error::ToolSelectionNotFound { .. }) => {}
            Err(Error::ProjectToolSelectionRequired { config_path, .. }) => println!(
                "{}",
                catalog.doctor_line(
                    &format!("{}_selection", provider.tool),
                    config_path.display(),
                    "strict-missing",
                )
            ),
            Err(error) => return Err(error.into()),
        }

        if provider.tool != "node" && !has_declared_selection {
            continue;
        }
        let command = provider.commands[0];
        match resolve_command(command, cwd, &home) {
            Ok(resolution) => {
                if provider.tool == "node"
                    && resolution.source == pinset_core::SelectionSource::System
                {
                    println!(
                        "{}",
                        catalog.doctor_selection(
                            &resolution.version,
                            resolution.source.as_str(),
                            None,
                        )
                    );
                }
                println!(
                    "{}",
                    catalog.doctor_line(
                        &format!("{}_runtime", provider.tool),
                        resolution.executable.display(),
                        "ok",
                    )
                );
            }
            Err(Error::RuntimeCommandNotFound { version, .. }) => println!(
                "{}",
                catalog.doctor_line(
                    &format!("{}_runtime", provider.tool),
                    format!("{}@{version}", provider.tool),
                    "missing",
                )
            ),
            Err(error @ Error::PythonEnvironmentMissing { .. }) if provider.tool == "python" => {
                println!(
                    "{}",
                    catalog.doctor_line("python_environment", error, "missing")
                )
            }
            Err(
                error @ (Error::PythonEnvironmentNotOwned { .. }
                | Error::PythonEnvironmentMismatch { .. }
                | Error::InvalidPythonEnvironmentMarker { .. }),
            ) if provider.tool == "python" => println!(
                "{}",
                catalog.doctor_line("python_environment", error, "invalid")
            ),
            Err(Error::CommandSelectionNotFound { .. }) if provider.tool == "node" => {
                println!("{}", catalog.no_selection())
            }
            Err(Error::CommandSelectionNotFound { .. }) => {}
            Err(Error::ProjectToolSelectionRequired { .. }) => {}
            Err(error) => return Err(error.into()),
        }
    }

    let shims = command_routing_directory(&home)?;
    let shim_on_path = directory_on_path(&shims);
    println!(
        "{}",
        catalog.doctor_line(
            "shim_path",
            shims.display(),
            if shim_on_path {
                "active"
            } else {
                "not-on-path"
            },
        )
    );
    let shim_binary = default_shim_binary()?;
    let commands = pinset_core::runtime_providers()
        .iter()
        .flat_map(|provider| provider.commands.iter().copied())
        .collect::<Vec<_>>();
    let path_candidates = inspect_path_candidates(&commands, &home, &shim_binary);
    for candidate in &path_candidates {
        println!(
            "{}",
            catalog.path_candidate(
                &candidate.command,
                Path::new(&candidate.path),
                &candidate.owner,
                candidate.effective,
                candidate.managed,
            )
        );
    }
    let legacy_shims = home.join("shims");
    let legacy_commands = if paths_equal(&legacy_shims, &shims) {
        Vec::new()
    } else {
        existing_shim_commands(&legacy_shims, &commands)
    };
    let selected = pinset_core::runtime_providers()
        .iter()
        .any(|provider| resolve_tool_selection(provider.tool, cwd, &home).is_ok());
    let mut routing_issues = collect_routing_issues(
        selected,
        &commands,
        &path_candidates,
        &shims,
        &shim_binary,
        &legacy_shims,
        &legacy_commands,
    );
    if let Some(issue) = go_toolchain_routing_issue(cwd, &home) {
        routing_issues.push(issue);
    }
    routing_issues.extend(java_environment_issues(cwd, &home));
    for issue in routing_issues {
        println!(
            "{}",
            catalog.doctor_routing_issue(
                issue.code,
                issue.command.as_deref(),
                issue.path.as_deref(),
                issue.action,
            )
        );
    }
    for finding in scan_project_sources(cwd)?.findings {
        println!(
            "traditional_source tool={} source={} status={} action=pinset-detect-or-import",
            finding.tool,
            finding.source,
            discovery_status_name(finding.status, Language::English)
        );
    }
    Ok(())
}

fn path_command_candidates(command: &str) -> Vec<PathBuf> {
    let Some(path) = env::var_os("PATH") else {
        return Vec::new();
    };
    let names = if cfg!(windows) {
        vec![
            format!("{command}.exe"),
            format!("{command}.cmd"),
            format!("{command}.bat"),
            command.to_owned(),
        ]
    } else {
        vec![command.to_owned()]
    };
    let mut seen = std::collections::HashSet::new();
    env::split_paths(&path)
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .filter(|candidate| fs::metadata(candidate).is_ok_and(|metadata| metadata.is_file()))
        .filter(|candidate| {
            let key = if cfg!(windows) {
                candidate.to_string_lossy().to_ascii_lowercase()
            } else {
                candidate.to_string_lossy().into_owned()
            };
            seen.insert(key)
        })
        .collect()
}

fn inspect_path_candidates(
    commands: &[&str],
    pinset_home: &Path,
    shim_binary: &Path,
) -> Vec<DoctorPathCandidate> {
    commands
        .iter()
        .flat_map(|command| {
            path_command_candidates(command)
                .into_iter()
                .enumerate()
                .map(|(position, path)| {
                    let managed = shim_binary.is_file()
                        && is_managed_command_shim(shim_binary, &path, command).unwrap_or(false);
                    DoctorPathCandidate {
                        command: (*command).to_owned(),
                        owner: path_owner(&path, pinset_home, managed),
                        path: path.display().to_string(),
                        position: position + 1,
                        effective: position == 0,
                        managed,
                    }
                })
        })
        .collect()
}

fn existing_shim_commands(directory: &Path, commands: &[&str]) -> Vec<String> {
    commands
        .iter()
        .filter(|command| command_entry_exists(directory, command))
        .map(|command| (*command).to_owned())
        .collect()
}

fn user_home_directory() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        env::var_os("HOME").map(PathBuf::from)
    }
}

fn path_owner(path: &Path, pinset_home: &Path, managed: bool) -> String {
    if managed {
        return "pinset".to_owned();
    }
    if path.starts_with(pinset_home.join("shims")) {
        return "foreign-in-pinset-directory".to_owned();
    }
    "other".to_owned()
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn run_source_command(
    command: SourceCommands,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = source_config_path(&pinset_home()?);
    let mut config = load_source_config(&path)?;
    match command {
        SourceCommands::List { provider } => {
            if let Some(provider) = provider {
                print_sources(&config.list(&provider)?, catalog);
            } else {
                for provider in SUPPORTED_SOURCE_PROVIDERS {
                    print_sources(&config.list(provider)?, catalog);
                }
            }
        }
        SourceCommands::Add {
            provider,
            alias,
            base_url,
            allow_insecure,
            trust_metadata,
        } => {
            config.add(&provider, &alias, &base_url, allow_insecure, trust_metadata)?;
            save_source_config(&path, &config)?;
            println!("{}", catalog.source_changed("added", &provider, &alias));
        }
        SourceCommands::Use { provider, alias } => {
            config.use_source(&provider, &alias)?;
            save_source_config(&path, &config)?;
            println!("{}", catalog.source_changed("active", &provider, &alias));
        }
        SourceCommands::Fallback { provider, aliases } => {
            config.set_fallback(&provider, &aliases)?;
            save_source_config(&path, &config)?;
            if aliases.is_empty() {
                println!(
                    "{}",
                    catalog.source_changed("fallback", &provider, "cleared")
                );
            } else {
                println!(
                    "{}",
                    catalog.source_changed("fallback", &provider, &aliases.join(","))
                );
            }
        }
        SourceCommands::Remove { provider, alias } => {
            config.remove(&provider, &alias)?;
            save_source_config(&path, &config)?;
            println!("{}", catalog.source_changed("removed", &provider, &alias));
        }
        SourceCommands::Test { provider, alias } => {
            let source = config.source(&provider, alias.as_deref())?;
            let releases = match runtime_provider(&provider)
                .map(|provider| provider.capabilities.metadata)
            {
                Some(RuntimeMetadataKind::Node) => {
                    let client = NodeMetadataClient::for_base_url(&source.base_url)?;
                    let releases = client.available_releases()?;
                    let newest = releases.first().ok_or_else(|| Error::InvalidNodeIndex {
                        reason: "source index contains no supported stable releases".to_owned(),
                    })?;
                    client.resolve_exact_lock(&newest.version, "pinset source test")?;
                    releases.len()
                }
                Some(RuntimeMetadataKind::Go) => {
                    let client = GoMetadataClient::for_base_url(&source.base_url)?;
                    let releases = client.available_releases()?;
                    if releases.is_empty() {
                        return Err(Error::InvalidGoIndex {
                            reason: "source index contains no supported stable releases".to_owned(),
                        }
                        .into());
                    }
                    releases.len()
                }
                Some(RuntimeMetadataKind::Flutter) => {
                    let client = FlutterMetadataClient::for_base_url(&source.base_url)?;
                    let releases = client.available_releases()?;
                    if releases.is_empty() {
                        return Err(Error::InvalidFlutterIndex {
                            reason: "source indexes contain no supported stable releases"
                                .to_owned(),
                        }
                        .into());
                    }
                    releases.len()
                }
                Some(RuntimeMetadataKind::Python)
                    if source.kind == pinset_core::SourceKind::Official =>
                {
                    let releases = PythonMetadataClient::official()?.available_releases()?;
                    if releases.is_empty() {
                        return Err(Error::InvalidPythonIndex {
                            reason: "official index contains no supported stable releases"
                                .to_owned(),
                        }
                        .into());
                    }
                    releases.len()
                }
                Some(RuntimeMetadataKind::Python) => {
                    return Err(
                        "custom Python sources mirror locked archives; metadata remains Pinset's official registry"
                            .into(),
                    );
                }
                Some(
                    RuntimeMetadataKind::Java
                    | RuntimeMetadataKind::Rust
                    | RuntimeMetadataKind::Dotnet
                    | RuntimeMetadataKind::Npm,
                )
                | None => {
                    return Err(format!(
                        "source testing is not available for provider {provider:?}"
                    )
                    .into());
                }
            };
            println!(
                "{}",
                catalog.source_test_ok(
                    &provider,
                    &source.alias,
                    &source.base_url,
                    releases,
                    source.base_url.starts_with("https://"),
                )
            );
        }
    }
    Ok(())
}

fn run_provider_command(command: ProviderCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ProviderCommands::List { json } => {
            let verified = pinset_core::embedded_provider_registry()?;
            if json {
                print_json_success("provider.list", verified)?;
            } else {
                println!(
                    "registry={} schema={} signer={}",
                    verified.document.registry,
                    verified.document.schema,
                    verified.signer_fingerprint
                );
                for provider in &verified.document.providers {
                    let dependencies = if provider.dependencies.is_empty() {
                        "none".to_owned()
                    } else {
                        provider.dependencies.join(",")
                    };
                    let methods = provider
                        .capabilities
                        .provenance
                        .methods
                        .iter()
                        .map(|method| method.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    println!(
                        "{} id={} commands={} dependencies={} verification={} activation=built-in-only",
                        provider.tool,
                        provider.id,
                        provider.commands.join(","),
                        dependencies,
                        methods
                    );
                }
            }
        }
        ProviderCommands::Verify { registry, json } => {
            let verified = match registry.as_deref() {
                Some(path) => pinset_core::load_signed_provider_registry(path)?,
                None => pinset_core::embedded_provider_registry()?,
            };
            if json {
                print_json_success("provider.verify", verified)?;
            } else {
                println!(
                    "Provider Registry verified: registry={} schema={} providers={} signer={} activation=none",
                    verified.document.registry,
                    verified.document.schema,
                    verified.document.providers.len(),
                    verified.signer_fingerprint
                );
            }
        }
    }
    Ok(())
}

fn print_sources(sources: &[SourceView], catalog: Catalog) {
    for source in sources {
        if catalog.language() == Language::SimplifiedChinese {
            let state = if source.active {
                "已启用".to_owned()
            } else if let Some(position) = source.fallback_position {
                format!("备用顺序:{position}")
            } else {
                "未启用".to_owned()
            };
            let security = if source.allow_insecure {
                " 允许不安全 HTTP"
            } else {
                ""
            };
            let metadata = if source.trust_metadata {
                " 受信元数据"
            } else {
                ""
            };
            println!(
                "{} {} {} 状态={} {}{}{}",
                source.provider,
                source.alias,
                source.kind.as_str(),
                state,
                source.base_url,
                security,
                metadata
            );
        } else {
            let state = if source.active {
                "active".to_owned()
            } else if let Some(position) = source.fallback_position {
                format!("fallback:{position}")
            } else {
                "-".to_owned()
            };
            let security = if source.allow_insecure {
                " insecure-http"
            } else {
                ""
            };
            let metadata = if source.trust_metadata {
                " trusted-metadata"
            } else {
                ""
            };
            println!(
                "{} {} {} {} {}{}{}",
                source.provider,
                source.alias,
                source.kind.as_str(),
                state,
                source.base_url,
                security,
                metadata
            );
        }
    }
}

fn node_metadata_client(home: &Path) -> Result<NodeMetadataClient, Box<dyn std::error::Error>> {
    let config = load_source_config(&source_config_path(home))?;
    let source = config.metadata_source("node")?;
    if source.kind == pinset_core::SourceKind::Official {
        Ok(NodeMetadataClient::official()?)
    } else {
        Ok(NodeMetadataClient::for_source(
            &source.base_url,
            &source.alias,
        )?)
    }
}

fn go_metadata_client(home: &Path) -> Result<GoMetadataClient, Box<dyn std::error::Error>> {
    let config = load_source_config(&source_config_path(home))?;
    let source = config.metadata_source("go")?;
    if source.kind == pinset_core::SourceKind::Official {
        Ok(GoMetadataClient::official()?)
    } else {
        Ok(GoMetadataClient::for_source(
            &source.base_url,
            &source.alias,
        )?)
    }
}

fn flutter_metadata_client(
    home: &Path,
) -> Result<FlutterMetadataClient, Box<dyn std::error::Error>> {
    let config = load_source_config(&source_config_path(home))?;
    let source = config.metadata_source("flutter")?;
    if source.kind == pinset_core::SourceKind::Official {
        Ok(FlutterMetadataClient::official()?)
    } else {
        Ok(FlutterMetadataClient::for_source(
            &source.base_url,
            &source.alias,
        )?)
    }
}

fn effective_cwd(cwd: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    cwd.map_or_else(env::current_dir, |path| absolutize(&path))
}

fn default_shim_binary() -> Result<PathBuf, std::io::Error> {
    if let Some(path) = env::var_os("PINSET_SHIM_BINARY")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return absolutize(&path);
    }
    let executable = env::current_exe()?;
    let directory = executable.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pinset executable path has no parent directory",
        )
    })?;
    Ok(directory.join(if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    }))
}

fn absolutize(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn manual_shim_commands(
    provider: Option<&str>,
    commands: &[String],
    cwd: &Path,
    home: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !commands.is_empty() {
        return Ok(commands.to_vec());
    }

    let mut tools = std::collections::BTreeSet::new();
    if let Some(provider) = provider {
        tools.insert(provider.to_owned());
    } else {
        if let Some(config_path) = find_optional_project_config(cwd)? {
            tools.extend(load_project_config(&config_path)?.tools.into_keys());
        }
        if let Some(config) = load_optional_global_config(&global_config_path(home))? {
            tools.extend(config.tools.into_keys());
        }
    }
    if tools.is_empty() {
        return Err(
            "no configured runtime provider; pass --provider <tool> or explicit command names"
                .into(),
        );
    }

    let mut resolved = Vec::new();
    for tool in tools {
        let provider = runtime_provider(&tool)
            .ok_or_else(|| format!("runtime provider {tool:?} is not available"))?;
        resolved.extend(
            provider
                .commands
                .iter()
                .map(|command| (*command).to_owned()),
        );
    }
    resolved.sort();
    resolved.dedup();
    Ok(resolved)
}

fn register_provider_commands(
    home: &Path,
    tool: &str,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = runtime_provider(tool)
        .ok_or_else(|| format!("runtime provider {tool:?} is not available"))?;
    let commands = provider
        .commands
        .iter()
        .map(|command| (*command).to_owned())
        .collect::<Vec<_>>();
    let directory = command_routing_directory(home)?;
    let shim_binary = default_shim_binary()?;
    let results = ensure_shims(&shim_binary, &directory, &commands)?;
    let installed = results
        .iter()
        .filter(|result| result.method != ShimInstallMethod::Existing)
        .map(|result| result.command.as_str())
        .collect::<Vec<_>>();
    let preserved = results
        .iter()
        .filter(|result| result.method == ShimInstallMethod::Existing)
        .map(|result| result.command.as_str())
        .collect::<Vec<_>>();
    let (active, shadowed) = provider_command_routing_status(&shim_binary, &commands);
    let activation_command = current_shell_activation_command();
    let routing = (!active).then_some((shadowed.as_slice(), activation_command));
    println!(
        "{}",
        catalog.provider_commands_registered(tool, &directory, &installed, &preserved, routing,)
    );
    Ok(())
}

fn provider_command_routing_status(shim_binary: &Path, commands: &[String]) -> (bool, Vec<String>) {
    let mut active = true;
    let mut shadowed = Vec::new();
    for command in commands {
        let effective = path_command_candidates(command).into_iter().next();
        match effective {
            Some(path) if is_managed_command_shim(shim_binary, &path, command).unwrap_or(false) => {
            }
            Some(path) => {
                active = false;
                shadowed.push(format!("{command}={}", path.display()));
            }
            None => active = false,
        }
    }
    (active, shadowed)
}

fn current_shell_activation_command() -> &'static str {
    activation_command_for_shell(env::var_os("SHELL").as_deref())
}

fn activation_command_for_shell(shell: Option<&std::ffi::OsStr>) -> &'static str {
    let name = shell
        .map(Path::new)
        .and_then(Path::file_stem)
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match name.as_deref() {
        Some("zsh") => "eval \"$(pinset activate zsh)\"",
        Some("fish") => "pinset activate fish | source",
        Some("powershell" | "pwsh") => {
            "pinset activate powershell | Out-String | Invoke-Expression"
        }
        Some("bash") => "eval \"$(pinset activate bash)\"",
        _ if cfg!(windows) => "pinset activate powershell | Out-String | Invoke-Expression",
        _ => "eval \"$(pinset activate bash)\"",
    }
}

fn migrate_provider_shims(
    provider: Option<&str>,
    destination: Option<&Path>,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let cwd = env::current_dir()?;
    let commands = manual_shim_commands(provider, &[], &cwd, &home)?;
    let source = home.join("shims");
    let destination = destination
        .map(absolutize)
        .transpose()?
        .unwrap_or(command_routing_directory(&home)?);
    let binary = default_shim_binary()?;

    let results = ensure_shims(&binary, &destination, &commands)?;
    for result in &results {
        let method = match result.method {
            ShimInstallMethod::Symlink => "symbolic-link",
            ShimInstallMethod::Wrapper => "wrapper",
            ShimInstallMethod::HardLink => "hard-link",
            ShimInstallMethod::Copy => "copy",
            ShimInstallMethod::Existing => "existing",
        };
        println!(
            "{}",
            catalog.shim_installed(&result.command, &result.destination, method)
        );
    }

    if paths_equal(&source, &destination) {
        println!("{}", catalog.shim_migration_not_needed(&destination));
        return Ok(());
    }

    let preserved = commands
        .iter()
        .filter(|command| command_entry_exists(&source, command))
        .count();
    println!(
        "{}",
        catalog.shim_migrated(
            &source,
            &destination,
            commands.len(),
            preserved,
            directory_on_path(&destination),
        )
    );
    Ok(())
}

fn command_entry_exists(directory: &Path, command: &str) -> bool {
    command_entry_paths(directory, command)
        .into_iter()
        .any(|path| fs::symlink_metadata(path).is_ok())
}

fn command_entry_paths(directory: &Path, command: &str) -> Vec<PathBuf> {
    if cfg!(windows) {
        [
            format!("{command}.exe"),
            format!("{command}.cmd"),
            format!("{command}.bat"),
            command.to_owned(),
        ]
        .into_iter()
        .map(|name| directory.join(name))
        .collect()
    } else {
        vec![directory.join(command)]
    }
}

fn managed_command_entry(directory: &Path, command: &str, shim_binary: &Path) -> Option<PathBuf> {
    shim_binary.is_file().then_some(())?;
    command_entry_paths(directory, command)
        .into_iter()
        .find(|path| is_managed_command_shim(shim_binary, path, command).unwrap_or(false))
}

fn command_routing_directory(home: &Path) -> Result<PathBuf, std::io::Error> {
    if let Some(path) = env::var_os("PINSET_SHIM_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }
    let executable = env::current_exe()?;
    if let Some(directory) = executable.parent().filter(|path| directory_on_path(path)) {
        return Ok(directory.to_path_buf());
    }
    Ok(home.join("shims"))
}

fn directory_on_path(directory: &Path) -> bool {
    env::var_os("PATH")
        .map(|value| env::split_paths(&value).any(|entry| paths_equal(&entry, directory)))
        .unwrap_or(false)
}

fn activation_script(shell: ActivationShell, shim_directory: &Path) -> String {
    let path = shim_directory.to_string_lossy();
    match shell {
        ActivationShell::Bash | ActivationShell::Zsh => {
            format!("export PATH='{}':\"$PATH\"", path.replace('\'', "'\\''"))
        }
        ActivationShell::Fish => {
            format!("set -gx PATH '{}' $PATH", path.replace('\'', "\\'"))
        }
        ActivationShell::Powershell => format!(
            "$env:PATH = '{}' + [IO.Path]::PathSeparator + $env:PATH",
            path.replace('\'', "''")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_install_reuses_verified_runtime_without_writing_selection_state() {
        let root = tempfile::tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        let target = current_target_for_tool("node");
        let install_dir = home
            .join("installs")
            .join("node")
            .join("24.0.0")
            .join(&target);
        let command = if cfg!(windows) {
            install_dir.join("node.exe")
        } else {
            install_dir.join("bin").join("node")
        };
        fs::create_dir_all(command.parent().expect("command parent")).expect("install directory");
        fs::write(&command, b"fixture").expect("runtime command");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&command)
                .expect("command metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&command, permissions).expect("command permissions");
        }
        fs::write(
            install_dir.join(".pinset-install.toml"),
            format!(
                "complete = true\ntool = \"node\"\nversion = \"24.0.0\"\ntarget = \"{target}\"\nselected_source = \"fixture\"\nartifact_sha256 = \"{}\"\n",
                "ab".repeat(32)
            ),
        )
        .expect("install receipt");
        fs::create_dir_all(&project).expect("project directory");
        let locked = LockedTool {
            name: "node".to_owned(),
            requested: "24".to_owned(),
            version: "24.0.0".to_owned(),
            provider: "nodejs".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            artifacts: pinset_core::MVP_NODE_TARGETS
                .into_iter()
                .map(|target| {
                    let plan = pinset_core::plan_node_artifact(
                        &pinset_core::SourceConfig::default(),
                        "24.0.0",
                        target,
                    )
                    .expect("Node artifact plan");
                    pinset_core::LockedArtifact {
                        target: target.to_owned(),
                        canonical_url: plan.canonical_url,
                        artifact_path: plan.artifact_path,
                        sha256: "ab".repeat(32),
                        integrity: None,
                        format: match plan.format {
                            pinset_core::NodeArchiveFormat::Zip => {
                                pinset_core::LockedArtifactFormat::Zip
                            }
                            pinset_core::NodeArchiveFormat::TarXz => {
                                pinset_core::LockedArtifactFormat::TarXz
                            }
                        },
                        archive_root: plan.archive_root,
                        verification: "nodejs-openpgp-sha256".to_owned(),
                        overlays: Vec::new(),
                    }
                })
                .collect(),
        };

        install_ephemeral_selection(
            &home,
            &project,
            "node",
            locked,
            Catalog::new(Language::English),
        )
        .expect("one-shot install");

        assert!(!project.join("pinset.toml").exists());
        assert!(!project.join("pinset.lock").exists());
        assert!(!global_config_path(&home).exists());
        assert!(!global_lockfile_path(&home).exists());
        assert!(!home.join("shims").exists());
    }

    #[test]
    fn one_shot_resolves_provider_dependencies_before_installing() {
        let root = tempfile::tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(&project).expect("project directory");
        let selected = LockedTool {
            name: "pnpm".to_owned(),
            requested: "11".to_owned(),
            version: "11.0.0".to_owned(),
            provider: "pnpm-npm".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            artifacts: Vec::new(),
        };

        let error = install_ephemeral_selection(
            &home,
            &project,
            "pnpm",
            selected,
            Catalog::new(Language::English),
        )
        .expect_err("pnpm requires a selected Node.js runtime");

        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::ProviderDependencyMissing { tool, dependency })
                if tool == "pnpm" && dependency == "node"
        ));
        assert!(!home.join("installs").exists());
    }

    #[test]
    fn formats_download_sizes_for_progress_output() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn derives_artifact_name_from_redacted_download_url() {
        assert_eq!(
            download_artifact_name("https://nodejs.org/dist/v24.0.0/node-v24.0.0-linux-x64.tar.xz"),
            "node-v24.0.0-linux-x64.tar.xz"
        );
        assert_eq!(
            download_artifact_name("https://nodejs.org/"),
            "runtime archive"
        );
    }

    #[test]
    fn download_progress_lines_fit_without_terminal_wrapping() {
        let artifact = "node-v24.19.0-linux-x64.tar.xz";
        for language in [Language::English, Language::SimplifiedChinese] {
            for terminal_columns in [24, 40, 60, 80] {
                let line = download_progress_line(
                    Catalog::new(language),
                    artifact,
                    15 * 1024 * 1024,
                    Some(30 * 1024 * 1024),
                    terminal_columns,
                );
                assert!(
                    UnicodeWidthStr::width(line.as_str()) < terminal_columns,
                    "line width {} exceeded {terminal_columns} columns: {line}",
                    UnicodeWidthStr::width(line.as_str())
                );
                assert!(line.contains("50%"));
                assert!(!line.contains(['\r', '\n']));
            }
        }
    }

    #[test]
    fn download_progress_keeps_filename_ends_when_space_is_limited() {
        let line = download_progress_line(
            Catalog::new(Language::SimplifiedChinese),
            "node-v24.19.0-linux-x64.tar.xz",
            5 * 1024 * 1024,
            Some(30 * 1024 * 1024),
            72,
        );
        assert!(line.contains('…'));
        assert!(line.contains("node-"));
        assert!(line.contains("tar.xz"));
        assert!(UnicodeWidthStr::width(line.as_str()) < 72);
    }

    #[test]
    fn middle_truncation_counts_cjk_display_columns() {
        let value = truncate_middle_to_width("正在下载-node-runtime.tar.xz", 16);
        assert!(value.contains('…'));
        assert!(value.ends_with("tar.xz"));
        assert!(UnicodeWidthStr::width(value.as_str()) <= 16);
    }

    #[test]
    fn activation_is_runtime_agnostic_for_supported_shells() {
        let directory = Path::new("/tmp/pinset commands");
        let bash = activation_script(ActivationShell::Bash, directory);
        assert!(bash.contains("export PATH="));
        assert!(bash.contains("$PATH"));
        assert!(!bash.contains("node"));
        assert!(!bash.contains("python"));

        let powershell = activation_script(ActivationShell::Powershell, directory);
        assert!(powershell.contains("$env:PATH"));
        assert!(powershell.contains("PathSeparator"));
        assert!(!powershell.contains("node"));
    }

    #[test]
    fn completions_cover_providers_nested_commands_and_machine_readable_flags() {
        for shell in [
            ActivationShell::Bash,
            ActivationShell::Zsh,
            ActivationShell::Fish,
            ActivationShell::Powershell,
        ] {
            let script = completion_script(shell);
            for expected in [
                "pinset", "detect", "import", "node@", "dotnet", "lock", "audit", "verify",
                "recreate", "--json",
            ] {
                assert!(
                    script.contains(expected),
                    "completion for {shell:?} omitted {expected}"
                );
            }
            assert!(!script.contains("__COMMANDS__"));
            assert!(!script.contains("__PROVIDERS__"));
            for provider in pinset_core::runtime_providers() {
                assert!(script.contains(provider.tool));
                assert!(script.contains(&format!("{}@", provider.tool)));
            }
        }
    }

    #[test]
    fn import_replacement_check_is_limited_to_discovered_tools() {
        let project = ProjectConfig {
            schema: PROJECT_CONFIG_SCHEMA,
            policy: Default::default(),
            tools: BTreeMap::from([
                ("go".to_owned(), "1.24.0".to_owned()),
                ("node".to_owned(), "22.0.0".to_owned()),
            ]),
        };
        let node = LockedTool {
            name: "node".to_owned(),
            requested: "24.0.0".to_owned(),
            version: "24.0.0".to_owned(),
            provider: "nodejs-official".to_owned(),
            released_at: None,
            metadata: BTreeMap::new(),
            artifacts: Vec::new(),
        };

        assert_eq!(
            import_replacement_conflict(
                &project,
                &[("node".to_owned(), "24.0.0".to_owned(), node)]
            ),
            Some(("node".to_owned(), "22.0.0".to_owned(), "24.0.0".to_owned()))
        );
        assert_eq!(project.tools["go"], "1.24.0");
    }

    #[test]
    fn recommends_activation_for_the_current_shell() {
        assert_eq!(
            activation_command_for_shell(Some(std::ffi::OsStr::new("/bin/zsh"))),
            "eval \"$(pinset activate zsh)\""
        );
        assert_eq!(
            activation_command_for_shell(Some(std::ffi::OsStr::new("/usr/bin/fish"))),
            "pinset activate fish | source"
        );
        assert_eq!(
            activation_command_for_shell(Some(std::ffi::OsStr::new("pwsh.exe"))),
            "pinset activate powershell | Out-String | Invoke-Expression"
        );
    }

    #[test]
    fn accepts_direct_node_install_without_selecting_a_scope() {
        let cli = Cli::try_parse_from(["pinset", "install", "node@24"]).expect("direct install");
        assert!(matches!(
            cli.command,
            Some(Commands::Install {
                selection: Some(selection),
                global: false,
                ..
            }) if selection == "node@24"
        ));
    }

    #[test]
    fn parses_v15_explain_update_and_migrate_options() {
        let which = Cli::try_parse_from(["pinset", "which", "node", "--explain", "--json"])
            .expect("which explain");
        assert!(matches!(
            which.command,
            Some(Commands::Which {
                explain: true,
                json: true,
                ..
            })
        ));

        let update = Cli::try_parse_from(["pinset", "update", "node", "--dry-run", "--json"])
            .expect("update");
        assert!(matches!(
            update.command,
            Some(Commands::Update {
                tool: Some(tool),
                dry_run: true,
                json: true,
                ..
            }) if tool == "node"
        ));

        let migrate =
            Cli::try_parse_from(["pinset", "migrate", "--global", "--dry-run"]).expect("migrate");
        assert!(matches!(
            migrate.command,
            Some(Commands::Migrate {
                global: true,
                dry_run: true,
                ..
            })
        ));
    }

    #[test]
    fn parses_v16_read_only_lock_audit_options() {
        let project =
            Cli::try_parse_from(["pinset", "lock", "audit", "--cwd", "project", "--json"])
                .expect("project lock audit");
        assert!(matches!(
            project.command,
            Some(Commands::Lock {
                command: LockCommands::Audit {
                    global: false,
                    json: true,
                    ..
                }
            })
        ));

        let global = Cli::try_parse_from(["pinset", "lock", "audit", "--global"])
            .expect("global lock audit");
        assert!(matches!(
            global.command,
            Some(Commands::Lock {
                command: LockCommands::Audit { global: true, .. }
            })
        ));
    }

    #[test]
    fn maps_each_provider_to_its_latest_stable_selector() {
        for provider in pinset_core::runtime_providers() {
            let expected = match provider.tool {
                "node" => "current",
                "rust" => "stable",
                _ => "latest",
            };
            assert_eq!(latest_stable_selector(provider.tool), expected);
        }
    }

    #[test]
    fn collects_project_and_global_outdated_scopes_without_network_access() {
        let root = tempfile::tempdir().expect("temporary root");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(home.join("state")).expect("global state");
        fs::create_dir_all(&project).expect("project");
        fs::write(
            project.join("pinset.toml"),
            "schema = 2\n[tools]\nnode = \"22.0.0\"\nrust = \"1.90.0\"\n",
        )
        .expect("project config");
        fs::write(
            global_config_path(&home),
            "schema = 2\n[tools]\nbun = \"1.2.0\"\nnode = \"24.0.0\"\n",
        )
        .expect("global config");

        let all =
            selected_runtimes_for_outdated(&home, &project, None, false).expect("all selections");
        assert_eq!(all.len(), 4);
        assert_eq!(
            all.iter()
                .map(|entry| (entry.scope, entry.tool.as_str(), entry.version.as_str()))
                .collect::<Vec<_>>(),
            [
                ("project", "node", "22.0.0"),
                ("project", "rust", "1.90.0"),
                ("global", "bun", "1.2.0"),
                ("global", "node", "24.0.0"),
            ]
        );

        let nodes = selected_runtimes_for_outdated(&home, &project, Some("node"), false)
            .expect("node selections");
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|entry| entry.tool == "node"));

        let globals =
            selected_runtimes_for_outdated(&home, &project, None, true).expect("global selections");
        assert_eq!(globals.len(), 2);
        assert!(globals.iter().all(|entry| entry.scope == "global"));

        fs::write(
            project.join("pinset.toml"),
            "schema = 2\n[tools]\nunknown = \"1.0.0\"\n",
        )
        .expect("unknown project config");
        let error = selected_runtimes_for_outdated(&home, &project, None, false)
            .expect_err("unknown providers must not panic");
        assert!(error.to_string().contains("not available"));
    }
}
