use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::Mutex,
    time::{Duration, Instant},
};

mod i18n;

#[cfg(windows)]
use std::ffi::OsStr;

use clap::{Parser, Subcommand, ValueEnum, error::ErrorKind};
use pinset_core::{
    ArtifactIntegrity, DownloadProgressEvent, Error, FlutterMetadataClient, GlobalConfig,
    GoMetadataClient, InstallLimits, Installer, JavaMetadataClient, LockedTool, Lockfile,
    NodeMetadataClient, NpmMetadataClient, PythonMetadataClient, RuntimeInstallKind,
    RuntimeMetadataKind, SUPPORTED_SOURCE_PROVIDERS, ShimInstallMethod, SourceView,
    clean_download_cache, command_tool, create_project_config, create_project_python_environment,
    current_target_for_tool, ensure_shims, find_optional_project_config, find_project_config,
    global_config_path, global_lockfile_path, import_download_cache,
    import_download_cache_with_integrity, install_locked_flutter, install_locked_go,
    install_locked_java, install_locked_node, install_locked_npm_tool, install_locked_python,
    is_managed_command_shim, list_download_cache, list_installed_tool_versions, load_global_config,
    load_lockfile, load_optional_global_config, load_optional_lockfile, load_project_config,
    load_project_python_environment, load_source_config, load_user_settings, lockfile_path,
    managed_runtime_arguments, path_with_selected_tools, pinset_home,
    project_python_environment_path, resolve_command, resolve_project_python_command,
    resolve_tool_selection, runtime_command_candidates, runtime_command_directory,
    runtime_environment_for_install, runtime_provider, save_global_config, save_global_state,
    save_lockfile, save_project_config, save_source_config, save_user_settings,
    selected_runtime_environment, source_config_path, uninstall_node_version,
    uninstall_tool_version, user_settings_path, validate_exact_flutter_version,
    validate_exact_go_version, validate_exact_java_version, validate_exact_node_version,
    validate_exact_python_version, validate_lock_matches_selection, validate_lock_matches_tool,
    validate_lock_matches_tools, validate_managed_runtime_invocation,
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
    /// Show or set a default runtime version used outside projects.
    Global {
        /// Selection such as node@lts, pnpm@11, bun@1.3, go@1.25, python@3.14, flutter@3.47 or java@21.
        selection: Option<String>,
        /// Update the global selection and lock without downloading the runtime.
        #[arg(long, requires = "selection")]
        no_install: bool,
    },
    /// Select and lock a runtime version for the current project or globally.
    Use {
        /// Selection such as node@24, pnpm@11, bun@1.3, go@1.25, python@3.14, flutter@3.47 or java@lts.
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
        /// Tool to clear: node, pnpm, bun, go, python, flutter or java.
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
    },
    /// Print the effective project, global or system selection and executable path.
    Current {
        /// Tool to inspect. Defaults to node.
        tool: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// List installed or officially available runtime versions.
    List {
        /// Tool to list: node, pnpm, bun, go, python, flutter or java.
        tool: String,
        /// Query the official provider index instead of local installations.
        #[arg(long)]
        available: bool,
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
    /// Manage local download sources without changing project lock files.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
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
    List,
    /// Remove content-addressed archives from the Pinset download cache.
    Clean,
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

fn main() -> ExitCode {
    let arguments = env::args_os().collect::<Vec<_>>();
    let requested_language = match language_from_arguments(&arguments)
        .and_then(|language| language.map_or_else(language_from_env, |language| Ok(Some(language))))
    {
        Ok(language) => language,
        Err(error) => {
            eprintln!("{}", Catalog::new(Language::default()).error(error));
            return ExitCode::from(2);
        }
    };
    let language = match resolve_language(requested_language) {
        Ok(language) => language,
        Err(error) => {
            let catalog = Catalog::new(requested_language.unwrap_or_default());
            eprintln!("{}", catalog.error(error));
            return ExitCode::from(2);
        }
    };
    let catalog = Catalog::new(language);
    let help_command = requested_help_command(&arguments);
    if language == Language::SimplifiedChinese && help_command.is_some() {
        println!("{}", catalog.command_help(help_command.flatten()));
        return ExitCode::SUCCESS;
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
            return ExitCode::SUCCESS;
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
            return ExitCode::from(2);
        }
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(2);
        }
    };
    match run(cli, catalog) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", catalog.command_error(error.as_ref()));
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli, catalog: Catalog) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
        return Ok(ExitCode::SUCCESS);
    };

    match command {
        Commands::Init => {
            let path = create_project_config(&env::current_dir()?)?;
            println!("{}", catalog.created(&path));
        }
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
        Commands::Which { command, cwd } => {
            let cwd = effective_cwd(cwd)?;
            let resolution = resolve_command(&command, &cwd, &pinset_home()?)?;
            println!("{}", resolution.executable.display());
        }
        Commands::Current { tool, cwd } => {
            let cwd = effective_cwd(cwd)?;
            print_current(&cwd, tool.as_deref().unwrap_or("node"), catalog)?;
        }
        Commands::List { tool, available } => {
            require_provider(&tool)?;
            if available {
                let provider = runtime_provider(&tool).expect("required provider exists");
                match provider.metadata {
                    RuntimeMetadataKind::Node => {
                        let releases =
                            node_metadata_client(&pinset_home()?)?.available_releases()?;
                        for release in releases {
                            println!(
                                "{}",
                                catalog.available_node(
                                    &release.version,
                                    &release.date,
                                    release.lts.as_deref(),
                                    release.security,
                                )
                            );
                        }
                    }
                    RuntimeMetadataKind::Npm => {
                        for release in NpmMetadataClient::official()?.available_releases(&tool)? {
                            println!("{}@{}", tool, release.version);
                        }
                    }
                    RuntimeMetadataKind::Go => {
                        for release in go_metadata_client(&pinset_home()?)?.available_releases()? {
                            println!("go@{}", release.version);
                        }
                    }
                    RuntimeMetadataKind::Flutter => {
                        for release in
                            flutter_metadata_client(&pinset_home()?)?.available_releases()?
                        {
                            println!(
                                "flutter@{} dart@{} stable",
                                release.version, release.dart_version
                            );
                        }
                    }
                    RuntimeMetadataKind::Python => {
                        for release in PythonMetadataClient::official()?.available_releases()? {
                            println!(
                                "python@{}+{} ({})",
                                release.version, release.build_id, release.date
                            );
                        }
                    }
                    RuntimeMetadataKind::Java => {
                        for release in JavaMetadataClient::official()?.available_releases()? {
                            println!(
                                "java@{} temurin {} ({})",
                                release.version,
                                if release.lts { "lts" } else { "ga" },
                                release.date
                            );
                        }
                    }
                }
            } else {
                let installed = list_installed_tool_versions(&pinset_home()?, &tool)?;
                if installed.is_empty() {
                    if tool == "node" {
                        println!("{}", catalog.no_installed_node());
                    } else {
                        println!("no Pinset-managed {tool} versions are installed");
                    }
                } else {
                    for entry in installed {
                        if tool == "node" {
                            println!(
                                "{}",
                                catalog.installed_node(&entry.version, &entry.targets.join(","))
                            );
                        } else {
                            println!("{}@{} [{}]", tool, entry.version, entry.targets.join(","));
                        }
                    }
                }
            }
        }
        Commands::Uninstall {
            selection,
            force,
            cwd,
        } => {
            let (tool, version) = parse_tool_selection(&selection, catalog)?;
            validate_exact_tool_version(&tool, &version)?;
            if tool == "node" {
                let outcome =
                    uninstall_node_version(&pinset_home()?, &effective_cwd(cwd)?, &version, force)?;
                println!(
                    "{}",
                    catalog.uninstalled_node(&outcome.version, &outcome.targets.join(","))
                );
            } else {
                let outcome = uninstall_tool_version(
                    &pinset_home()?,
                    &effective_cwd(cwd)?,
                    &tool,
                    &version,
                    force,
                )?;
                println!(
                    "uninstalled {tool}@{} [{}]",
                    outcome.version,
                    outcome.targets.join(",")
                );
            }
        }
        Commands::Cache { command } => match command {
            CacheCommands::List => {
                let entries = list_download_cache(&pinset_home()?)?;
                if entries.is_empty() {
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
            CacheCommands::Clean => {
                let outcome = clean_download_cache(&pinset_home()?)?;
                println!("{}", catalog.cache_cleaned(outcome.entries, outcome.bytes));
            }
            CacheCommands::Import {
                archive,
                sha256,
                integrity,
            } => {
                let archive = absolutize(&archive)?;
                let entry = if let Some(sha256) = sha256 {
                    import_download_cache(&pinset_home()?, &archive, &sha256)?
                } else if let Some(integrity) = integrity {
                    let integrity = ArtifactIntegrity::parse(&integrity)?;
                    import_download_cache_with_integrity(&pinset_home()?, &archive, &integrity)?
                } else {
                    return Err("cache import requires --sha256 or --integrity".into());
                };
                println!(
                    "{}",
                    catalog.cache_imported(&entry.integrity, entry.size, &entry.path)
                );
            }
        },
        Commands::Exec { cwd, command } => {
            let cwd = effective_cwd(cwd)?;
            return execute_selected(&cwd, &command, catalog);
        }
        Commands::Doctor { cwd, json } => {
            let cwd = effective_cwd(cwd)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&doctor_report(&cwd)?)?);
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
        Commands::Source { command } => run_source_command(command, catalog)?,
    }

    Ok(ExitCode::SUCCESS)
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
    match provider.metadata {
        RuntimeMetadataKind::Node => validate_exact_node_version(version)?,
        RuntimeMetadataKind::Npm => {
            let resolved =
                NpmMetadataClient::official()?.resolve_version_selector(tool, version)?;
            if resolved != version {
                return Err(format!("{tool}@{version} is not an exact stable version").into());
            }
        }
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
    }
    Ok(())
}

fn resolve_locked_tool(
    tool: &str,
    selector: &str,
) -> Result<LockedTool, Box<dyn std::error::Error>> {
    let provider = runtime_provider(tool).expect("validated provider");
    match provider.metadata {
        RuntimeMetadataKind::Node => {
            let lockfile = node_metadata_client(&pinset_home()?)?
                .resolve_lock(selector, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
            Ok(lockfile
                .tool("node")
                .expect("generated Node lock contains node")
                .clone())
        }
        RuntimeMetadataKind::Npm => {
            let client = NpmMetadataClient::official()?;
            let version = client.resolve_version_selector(tool, selector)?;
            Ok(client.resolve_tool(tool, &version)?)
        }
        RuntimeMetadataKind::Go => Ok(go_metadata_client(&pinset_home()?)?.resolve_tool(selector)?),
        RuntimeMetadataKind::Flutter => {
            Ok(flutter_metadata_client(&pinset_home()?)?.resolve_tool(selector)?)
        }
        RuntimeMetadataKind::Python => {
            Ok(PythonMetadataClient::official()?.resolve_tool(selector)?)
        }
        RuntimeMetadataKind::Java => Ok(JavaMetadataClient::official()?.resolve_tool(selector)?),
    }
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
        lockfile.upsert_tool(locked_tool.clone());
        config.set_tool(&tool, &version);
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
        lockfile.upsert_tool(locked_tool.clone());
        project.set_tool(&tool, &version);
        validate_lock_matches_tools(&lockfile, &project.tools, &config_path)?;
        save_lockfile(&lock_path, &lockfile)?;
        save_project_config(&config_path, &project)?;
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
    lockfile.upsert_tool(locked_tool);
    install_tool_from_lock(&pinset_home()?, &lockfile, &tool, catalog)
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
    const COMMANDS: [&str; 16] = [
        "init",
        "global",
        "use",
        "unset",
        "install",
        "which",
        "current",
        "exec",
        "doctor",
        "shim",
        "activate",
        "source",
        "list",
        "uninstall",
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
    install_locked_selection(&home, &project.tools, &config_path, &lock_path, catalog)?;
    if let Some(distribution) = project.tools.get("python") {
        ensure_project_python_environment(&home, &config_path, distribution, recreate_venv)?;
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
    let distribution =
        project
            .tools
            .get("python")
            .ok_or_else(|| Error::PythonEnvironmentSelectionMissing {
                path: config_path.clone(),
            })?;
    if action == "status" {
        let target = current_target_for_tool("python");
        let environment = load_project_python_environment(&config_path, distribution, &target)?;
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
    for provider in pinset_core::runtime_providers() {
        if configured.contains_key(provider.tool) {
            install_tool_from_lock(home, &lockfile, provider.tool, catalog)?;
        }
    }
    Ok(())
}

fn install_tool_from_lock(
    home: &Path,
    lockfile: &Lockfile,
    tool: &str,
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
    let outcome = match provider.installer {
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
    } else {
        if tool == "node" {
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
    }
    if let Err(error) = register_provider_commands(home, tool, catalog) {
        eprintln!(
            "{}",
            catalog.shim_auto_registration_failed(&error.to_string())
        );
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
    for (tool, version) in &config.tools {
        print_declared_tool(&home, tool, version, "global", &config_path, catalog)?;
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
    version: &str,
    source: &str,
    config_path: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
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
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    let selected_tool = command_tool(tool).ok_or_else(|| Error::UnsupportedCommand {
        command: tool.to_owned(),
    })?;
    match resolve_command(tool, cwd, &home) {
        Ok(resolution) => println!(
            "{}",
            catalog.current_installed(
                selected_tool,
                &resolution.version,
                resolution.source.as_str(),
                &resolution.executable,
                resolution.selection_path.as_deref(),
            )
        ),
        Err(Error::RuntimeCommandNotFound { .. }) => {
            let selection = resolve_tool_selection(selected_tool, cwd, &home)?;
            print_declared_tool(
                &home,
                selected_tool,
                &selection.version,
                selection.source.as_str(),
                &selection.config_path,
                catalog,
            )?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn execute_selected(
    cwd: &Path,
    command: &[OsString],
    catalog: Catalog,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
            let locked_tool = resolve_locked_tool(&tool, &selector)?;
            let version = locked_tool.version;
            if selector != version {
                println!("{tool}@{selector} resolved to {tool}@{version}");
            }
            if command_tool(command_name) != Some(tool.as_str()) {
                return Err(Error::UnsupportedCommand {
                    command: command_name.to_owned(),
                }
                .into());
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
            (executable, tool, version, "ephemeral", None, environment)
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
    let runtime_path = path_with_selected_tools(&executable, cwd, &home)?;
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
    for variable in selected_runtime_environment(cwd, &home) {
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
    Ok(ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
    ))
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
    schema: u32,
    cwd: String,
    pinset_home: String,
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
    let project_path = find_optional_project_config(cwd)?;
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
        Err(Error::ToolSelectionNotFound { .. }) => None,
        Err(error) => return Err(error.into()),
    };
    let selection = selected.as_ref().map(|selection| DoctorSelection {
        tool: selection.tool.clone(),
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
        validate_lock_matches_selection(&lockfile, &selection.version, &selection.config_path)?;
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
    Ok(DoctorReport {
        schema: 2,
        cwd: cwd.display().to_string(),
        pinset_home: home.display().to_string(),
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
    })
}

fn doctor_python_environment(
    cwd: &Path,
    home: &Path,
) -> Result<DoctorItem, Box<dyn std::error::Error>> {
    let selection = match resolve_tool_selection("python", cwd, home) {
        Ok(selection) => selection,
        Err(Error::ToolSelectionNotFound { .. }) => {
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
                    &selection.version,
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
            let releases = match runtime_provider(&provider).map(|provider| provider.metadata) {
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
                Some(RuntimeMetadataKind::Java | RuntimeMetadataKind::Npm) | None => {
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
}
