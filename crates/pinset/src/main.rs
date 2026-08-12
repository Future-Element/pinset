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
    DownloadProgressEvent, Error, GlobalConfig, InstallLimits, Installer, Lockfile,
    NodeMetadataClient, SUPPORTED_SOURCE_PROVIDERS, ShimInstallMethod, SourceView,
    clean_download_cache, command_tool, create_project_config, current_target, ensure_shims,
    find_optional_project_config, find_project_config, global_config_path, global_lockfile_path,
    import_download_cache, install_locked_node, is_managed_command_shim, list_download_cache,
    list_installed_node_versions, load_global_config, load_lockfile, load_optional_global_config,
    load_project_config, load_source_config, load_user_settings, lockfile_path,
    node_command_directory, path_with_selected_runtime, pinset_home, resolve_command,
    resolve_tool_selection, runtime_provider, save_global_config, save_global_state, save_lockfile,
    save_project_config, save_source_config, save_user_settings, source_config_path,
    uninstall_node_version, user_settings_path, validate_exact_node_version,
    validate_lock_matches_selection,
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
    /// Show or set the default Node.js version used outside projects.
    Global {
        /// Selection such as node@24.0.0, node@24, node@24.12, node@lts or node@current.
        selection: Option<String>,
        /// Update the global selection and lock without downloading the runtime.
        #[arg(long, requires = "selection")]
        no_install: bool,
    },
    /// Select and lock a Node.js version for the current project or globally.
    Use {
        /// Selection such as node@24.0.0, node@24, node@24.12, node@lts or node@current.
        selection: String,
        /// Update the selection and lock without downloading the runtime.
        #[arg(long)]
        no_install: bool,
        /// Save the selection under PINSET_HOME instead of pinset.toml.
        #[arg(long)]
        global: bool,
    },
    /// Clear the project or global Node.js selection without uninstalling anything.
    Unset {
        /// Tool to clear. The Node-first release accepts node.
        tool: String,
        /// Clear the global default instead of the nearest project selection.
        #[arg(long, conflicts_with = "cwd")]
        global: bool,
        /// Project directory whose nearest Pinset configuration is updated.
        #[arg(long, conflicts_with = "global")]
        cwd: Option<PathBuf>,
    },
    /// Install an explicit Node.js version or the current project/global lockfile target.
    Install {
        /// Install a Node.js version without changing project or global selection.
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
        /// Tool to inspect. Defaults to node in the Node-first release.
        tool: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// List installed or officially available Node.js versions.
    List {
        /// Tool to list. The Node-first release accepts node.
        tool: String,
        /// Query the official Node.js release index instead of local installations.
        #[arg(long)]
        available: bool,
    },
    /// Uninstall an exact Node.js version owned by Pinset.
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
    /// Preview or explicitly import legacy Node.js version files.
    #[command(group(
        clap::ArgGroup::new("import_mode")
            .required(true)
            .args(["dry_run", "apply"])
    ))]
    Import {
        /// Preview detected values; no files are modified.
        #[arg(long)]
        dry_run: bool,
        /// Import one detected value into Pinset without changing the legacy file.
        #[arg(long)]
        apply: bool,
        /// Select a legacy source when detected files disagree (for example nvm or volta).
        #[arg(long = "from", requires = "apply")]
        source: Option<String>,
        /// Import as the global default instead of the current project selection.
        #[arg(long, requires = "apply")]
        global: bool,
        /// Write the Pinset selection and lock without installing Node.js.
        #[arg(long, requires = "apply")]
        no_install: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
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
        #[arg(long)]
        sha256: String,
    },
}

#[derive(Debug, Subcommand)]
enum SourceCommands {
    /// List built-in and custom sources.
    List {
        /// Limit output to node, python or flutter.
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
        /// Trust this HTTPS source for Node index and SHASUMS metadata as well as archives.
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
    /// Read-only connectivity and Node index validation for one source.
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
                select_node(&selection, true, no_install, false, &cwd, catalog)?;
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
            select_node(&selection, global, no_install, false, &cwd, catalog)?
        }
        Commands::Unset { tool, global, cwd } => {
            if tool != "node" {
                return Err(catalog.node_only_error().into());
            }
            unset_node(global, &effective_cwd(cwd)?, catalog)?;
        }
        Commands::Install {
            selection,
            locked: _,
            global,
            cwd,
        } => {
            if let Some(selection) = selection {
                install_node_selection(&selection, catalog)?;
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
            if tool != "node" {
                return Err(catalog.node_only_error().into());
            }
            if available {
                let releases = node_metadata_client(&pinset_home()?)?.available_releases()?;
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
            } else {
                let installed = list_installed_node_versions(&pinset_home()?)?;
                if installed.is_empty() {
                    println!("{}", catalog.no_installed_node());
                } else {
                    for entry in installed {
                        println!(
                            "{}",
                            catalog.installed_node(&entry.version, &entry.targets.join(","))
                        );
                    }
                }
            }
        }
        Commands::Uninstall {
            selection,
            force,
            cwd,
        } => {
            let version = parse_node_selection(&selection, catalog)?;
            validate_exact_node_version(&version)?;
            let outcome =
                uninstall_node_version(&pinset_home()?, &effective_cwd(cwd)?, &version, force)?;
            println!(
                "{}",
                catalog.uninstalled_node(&outcome.version, &outcome.targets.join(","))
            );
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
                            catalog.cache_entry(&entry.sha256, entry.size, &entry.path)
                        );
                    }
                }
            }
            CacheCommands::Clean => {
                let outcome = clean_download_cache(&pinset_home()?)?;
                println!("{}", catalog.cache_cleaned(outcome.entries, outcome.bytes));
            }
            CacheCommands::Import { archive, sha256 } => {
                let archive = absolutize(&archive)?;
                let entry = import_download_cache(&pinset_home()?, &archive, &sha256)?;
                println!(
                    "{}",
                    catalog.cache_imported(&entry.sha256, entry.size, &entry.path)
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
        Commands::Import {
            dry_run,
            apply: _,
            source,
            global,
            no_install,
            cwd,
        } => {
            let cwd = effective_cwd(cwd)?;
            let candidates = detect_legacy_node_configs(&cwd)?;
            if candidates.is_empty() {
                println!("{}", catalog.import_none(&cwd));
            } else if dry_run {
                print_import_candidates(&candidates, catalog);
            } else {
                let candidate = select_legacy_candidate(&candidates, source.as_deref(), catalog)?;
                let selector = normalize_legacy_node_selector(&candidate.version)?;
                select_node(
                    &format!("node@{selector}"),
                    global,
                    no_install,
                    true,
                    &cwd,
                    catalog,
                )?;
                println!(
                    "{}",
                    catalog.import_applied(
                        &candidate.kind,
                        &candidate.version,
                        &candidate.path,
                        if global { "global" } else { "project" },
                    )
                );
            }
        }
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

fn parse_node_selection(
    selection: &str,
    catalog: Catalog,
) -> Result<String, Box<dyn std::error::Error>> {
    let Some((tool, version)) = selection.split_once('@') else {
        return Err(catalog.selection_error().into());
    };
    if tool != "node" || version.is_empty() || version.contains('@') {
        return Err(catalog.node_only_error().into());
    }
    Ok(version.to_owned())
}

fn select_node(
    selection: &str,
    global: bool,
    no_install: bool,
    initialize_project: bool,
    cwd: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let selector = parse_node_selection(selection, catalog)?;
    let lockfile = node_metadata_client(&pinset_home()?)?
        .resolve_lock(&selector, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
    let locked_node = lockfile.tool("node").expect("generated lock contains node");
    let version = locked_node.version.clone();
    if selector != version {
        println!("{}", catalog.selector_resolved(&selector, &version));
    }
    let (scope, lock_path) = if global {
        let home = pinset_home()?;
        let config_path = global_config_path(&home);
        let mut config = load_optional_global_config(&config_path)?.unwrap_or_default();
        config.set_tool("node", &version);
        save_global_state(&home, &config, &lockfile)?;
        ("global", global_lockfile_path(&home))
    } else {
        let config_path = match find_optional_project_config(cwd)? {
            Some(path) => path,
            None if initialize_project => create_project_config(cwd)?,
            None => find_project_config(cwd)?,
        };
        let mut project = load_project_config(&config_path)?;
        let lock_path = lockfile_path(&config_path);
        save_lockfile(&lock_path, &lockfile)?;
        project.set_tool("node", &version);
        save_project_config(&config_path, &project)?;
        ("project", lock_path)
    };
    println!(
        "{}",
        catalog.selected(scope, &version, locked_node.artifacts.len(), &lock_path)
    );
    if !no_install {
        if global {
            install_global(&pinset_home()?, catalog)?;
        } else {
            install_project(cwd, catalog)?;
        }
    } else if let Err(error) = register_provider_commands(&pinset_home()?, "node", catalog) {
        eprintln!(
            "{}",
            catalog.shim_auto_registration_failed(&error.to_string())
        );
    }
    Ok(())
}

fn install_node_selection(
    selection: &str,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let selector = parse_node_selection(selection, catalog)?;
    let lockfile = node_metadata_client(&pinset_home()?)?
        .resolve_lock(&selector, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
    let locked_node = lockfile.tool("node").expect("generated lock contains node");
    if selector != locked_node.version {
        println!(
            "{}",
            catalog.selector_resolved(&selector, &locked_node.version)
        );
    }
    install_node_from_lock(&pinset_home()?, &lockfile, catalog)
}

fn unset_node(
    global: bool,
    cwd: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if global {
        let home = pinset_home()?;
        let config_path = global_config_path(&home);
        let Some(mut config) = load_optional_global_config(&config_path)? else {
            println!("{}", catalog.selection_unset("global", &config_path, false));
            return Ok(());
        };
        if config.tools.remove("node").is_none() {
            println!("{}", catalog.selection_unset("global", &config_path, false));
            return Ok(());
        }
        let lock_path = global_lockfile_path(&home);
        if lock_path.is_file() {
            load_lockfile(&lock_path)?;
        }
        save_global_config(&config_path, &config)?;
        remove_tool_from_lock(&lock_path, "node")?;
        println!("{}", catalog.selection_unset("global", &config_path, true));
        return Ok(());
    }

    let config_path = find_project_config(cwd)?;
    let mut config = load_project_config(&config_path)?;
    if config.tools.remove("node").is_none() {
        println!(
            "{}",
            catalog.selection_unset("project", &config_path, false)
        );
        return Ok(());
    }
    let lock_path = lockfile_path(&config_path);
    if lock_path.is_file() {
        load_lockfile(&lock_path)?;
    }
    save_project_config(&config_path, &config)?;
    remove_tool_from_lock(&lock_path, "node")?;
    println!("{}", catalog.selection_unset("project", &config_path, true));
    Ok(())
}

fn remove_tool_from_lock(path: &Path, tool: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !path.is_file() {
        return Ok(());
    }
    let mut lockfile = load_lockfile(path)?;
    lockfile.tools.retain(|locked| locked.name != tool);
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
        "import",
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
    let config_path = find_project_config(cwd)?;
    let project = load_project_config(&config_path)?;
    let configured = project
        .tools
        .get("node")
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: "node".to_owned(),
            config_path: config_path.clone(),
        })?;
    let lock_path = lockfile_path(&config_path);
    let home = pinset_home()?;
    install_locked_selection(&home, configured, &config_path, &lock_path, catalog)
}

fn install_global(home: &Path, catalog: Catalog) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = global_config_path(home);
    let config: GlobalConfig = load_global_config(&config_path)?;
    let configured = config
        .tools
        .get("node")
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: "node".to_owned(),
            config_path: config_path.clone(),
        })?;
    install_locked_selection(
        home,
        configured,
        &config_path,
        &global_lockfile_path(home),
        catalog,
    )
}

fn install_locked_selection(
    home: &Path,
    configured: &str,
    config_path: &Path,
    lock_path: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let lockfile = load_lockfile(lock_path)?;
    validate_lock_matches_selection(&lockfile, configured, config_path)?;
    install_node_from_lock(home, &lockfile, catalog)
}

fn install_node_from_lock(
    home: &Path,
    lockfile: &Lockfile,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let locked_node = lockfile
        .tool("node")
        .expect("validated or generated Node lock contains node");
    let sources = load_source_config(&source_config_path(home))?;
    let installer = Installer::new(InstallLimits::default())?
        .with_progress_reporter(download_progress_reporter(catalog));
    let outcome = install_locked_node(&installer, home, &sources, locked_node, &current_target())?;
    if outcome.reused_existing {
        println!(
            "{}",
            catalog.already_installed(
                &locked_node.version,
                &current_target(),
                &outcome.install_dir,
            )
        );
    } else {
        println!(
            "{}",
            catalog.installed(
                &locked_node.version,
                &current_target(),
                &outcome.source_id,
                &outcome.install_dir,
            )
        );
    }
    if let Err(error) = register_provider_commands(home, "node", catalog) {
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
    let Some(version) = config.tools.get("node") else {
        println!("{}", catalog.global_not_selected(&config_path));
        return Ok(());
    };
    print_declared_node(&home, version, "global", &config_path, catalog)
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

fn print_declared_node(
    home: &Path,
    version: &str,
    source: &str,
    config_path: &Path,
    catalog: Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let install_dir = home
        .join("installs")
        .join("node")
        .join(version)
        .join(current_target());
    let command_dir = node_command_directory(&install_dir, &current_target())?;
    let executable = node_runtime_command_path(&command_dir, "node");
    if executable.is_file() {
        println!(
            "{}",
            catalog.current_installed(version, source, &executable, Some(config_path))
        );
    } else {
        println!(
            "{}",
            catalog.current_missing(version, source, &command_dir, Some(config_path))
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
                &resolution.version,
                resolution.source.as_str(),
                &resolution.executable,
                resolution.selection_path.as_deref(),
            )
        ),
        Err(Error::RuntimeCommandNotFound { .. }) => {
            let selection = resolve_tool_selection(selected_tool, cwd, &home)?;
            print_declared_node(
                &home,
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
    let (ephemeral_selector, mut command) = command
        .first()
        .and_then(|value| value.to_str())
        .filter(|value| value.starts_with("node@"))
        .map_or((None, command), |selection| {
            (Some(selection), &command[1..])
        });
    if ephemeral_selector.is_some() && command.first().is_some_and(|value| value == "--") {
        command = &command[1..];
    }
    let command_name = command
        .first()
        .and_then(|value| value.to_str())
        .ok_or_else(|| catalog.utf8_command_error())?;
    let home = pinset_home()?;
    let (executable, tool, version, source, config_path) =
        if let Some(selection) = ephemeral_selector {
            let selector = parse_node_selection(selection, catalog)?;
            let version = node_metadata_client(&home)?.resolve_version_selector(&selector)?;
            if selector != version {
                println!("{}", catalog.selector_resolved(&selector, &version));
            }
            if command_tool(command_name) != Some("node") {
                return Err(Error::UnsupportedCommand {
                    command: command_name.to_owned(),
                }
                .into());
            }
            let install_dir = home
                .join("installs")
                .join("node")
                .join(&version)
                .join(current_target());
            let command_dir = node_command_directory(&install_dir, &current_target())?;
            let executable = node_runtime_command_path(&command_dir, command_name);
            if !executable.is_file() {
                return Err(Error::RuntimeCommandNotFound {
                    tool: "node".to_owned(),
                    version,
                    command: command_name.to_owned(),
                    searched: executable.display().to_string(),
                }
                .into());
            }
            (executable, "node".to_owned(), version, "ephemeral", None)
        } else {
            let resolution = resolve_command(command_name, cwd, &home)?;
            (
                resolution.executable,
                resolution.tool,
                resolution.version,
                resolution.source.as_str(),
                resolution.selection_path,
            )
        };
    let runtime_path = path_with_selected_runtime(&executable)?;
    let mut child = command_for_runtime(&executable);
    child
        .args(&command[1..])
        .current_dir(cwd)
        .env("PATH", runtime_path)
        .env("PINSET_SELECTED_TOOL", &tool)
        .env("PINSET_SELECTED_VERSION", &version)
        .env("PINSET_SELECTION_SOURCE", source);
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

fn node_runtime_command_path(command_dir: &Path, command: &str) -> PathBuf {
    if cfg!(windows) {
        let native = command_dir.join(if command == "node" {
            "node.exe".to_owned()
        } else {
            format!("{command}.cmd")
        });
        if native.is_file() || command != "node" {
            return native;
        }
        return command_dir.join("node.cmd");
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
    shim_path: DoctorItem,
    legacy_shim_path: DoctorItem,
    path_candidates: Vec<DoctorPathCandidate>,
    routing_issues: Vec<DoctorRoutingIssue>,
    legacy_node_configs: Vec<LegacyNodeConfig>,
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

#[derive(Debug, Clone, Serialize)]
struct LegacyNodeConfig {
    kind: String,
    version: String,
    path: PathBuf,
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
    let shims = command_routing_directory(&home)?;
    let shim_on_path = directory_on_path(&shims);
    let shim_binary = default_shim_binary()?;
    let commands = runtime_provider("node")
        .expect("Node provider is built in")
        .commands;
    let path_candidates = inspect_path_candidates(commands, &home, &shim_binary);
    let legacy_shims = home.join("shims");
    let legacy_commands = if paths_equal(&legacy_shims, &shims) {
        Vec::new()
    } else {
        existing_shim_commands(&legacy_shims, commands)
    };
    let routing_issues = collect_routing_issues(
        selected.is_some(),
        commands,
        &path_candidates,
        &shims,
        &shim_binary,
        &legacy_shims,
        &legacy_commands,
    );
    Ok(DoctorReport {
        schema: 1,
        cwd: cwd.display().to_string(),
        pinset_home: home.display().to_string(),
        project_config,
        global_config,
        selection,
        lockfile,
        runtime,
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
        legacy_node_configs: detect_legacy_node_configs(cwd)?,
    })
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

fn detect_legacy_node_configs(
    cwd: &Path,
) -> Result<Vec<LegacyNodeConfig>, Box<dyn std::error::Error>> {
    let mut candidates = Vec::new();
    for (name, kind) in [(".nvmrc", "nvm"), (".node-version", "node-version")] {
        let path = cwd.join(name);
        if path.is_file() {
            let version = fs::read_to_string(&path)?.trim().to_owned();
            if !version.is_empty() {
                candidates.push(LegacyNodeConfig {
                    kind: kind.to_owned(),
                    version,
                    path,
                });
            }
        }
    }

    let path = cwd.join("package.json");
    if path.is_file() {
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)?;
        if let Some(version) = value
            .get("volta")
            .and_then(|volta| volta.get("node"))
            .and_then(serde_json::Value::as_str)
        {
            candidates.push(LegacyNodeConfig {
                kind: "volta".to_owned(),
                version: version.to_owned(),
                path,
            });
        }
    }

    let path = cwd.join(".tool-versions");
    if path.is_file() {
        let content = fs::read_to_string(&path)?;
        if let Some(version) = content.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            matches!(parts.next(), Some("node") | Some("nodejs"))
                .then(|| parts.next())
                .flatten()
        }) {
            candidates.push(LegacyNodeConfig {
                kind: "asdf".to_owned(),
                version: version.to_owned(),
                path,
            });
        }
    }

    for name in ["mise.toml", ".mise.toml"] {
        let path = cwd.join(name);
        if path.is_file() {
            let value = toml::from_str::<toml::Value>(&fs::read_to_string(&path)?)?;
            if let Some(version) = value.get("tools").and_then(|tools| {
                tools
                    .get("node")
                    .or_else(|| tools.get("nodejs"))
                    .and_then(toml::Value::as_str)
            }) {
                candidates.push(LegacyNodeConfig {
                    kind: "mise".to_owned(),
                    version: version.to_owned(),
                    path,
                });
            }
        }
    }
    Ok(candidates)
}

fn print_import_candidates(candidates: &[LegacyNodeConfig], catalog: Catalog) {
    let distinct = candidates
        .iter()
        .map(|candidate| candidate.version.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for candidate in candidates {
        println!(
            "{}",
            catalog.import_candidate(&candidate.kind, &candidate.version, &candidate.path,)
        );
    }
    if distinct.len() > 1 {
        println!("{}", catalog.import_conflict(distinct.len()));
    }
}

fn select_legacy_candidate<'a>(
    candidates: &'a [LegacyNodeConfig],
    source: Option<&str>,
    catalog: Catalog,
) -> Result<&'a LegacyNodeConfig, Box<dyn std::error::Error>> {
    if let Some(source) = source {
        return candidates
            .iter()
            .find(|candidate| candidate.kind.eq_ignore_ascii_case(source))
            .ok_or_else(|| {
                let available = candidates
                    .iter()
                    .map(|candidate| candidate.kind.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(",");
                catalog.import_source_not_found(source, &available).into()
            });
    }

    let normalized = candidates
        .iter()
        .map(|candidate| normalize_legacy_node_selector(&candidate.version))
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if normalized.len() > 1 {
        return Err(catalog.import_apply_conflict(normalized.len()).into());
    }
    Ok(&candidates[0])
}

fn normalize_legacy_node_selector(value: &str) -> Result<String, Box<dyn std::error::Error>> {
    let trimmed = value.trim();
    let normalized = trimmed.to_ascii_lowercase();
    if normalized == "lts" || normalized.starts_with("lts/") {
        return Ok("lts".to_owned());
    }
    if matches!(
        normalized.as_str(),
        "node" | "stable" | "current" | "latest"
    ) {
        return Ok("current".to_owned());
    }
    let numeric = trimmed
        .strip_prefix('v')
        .filter(|value| value.as_bytes().first().is_some_and(u8::is_ascii_digit))
        .unwrap_or(trimmed);
    let parts = numeric.split('.').collect::<Vec<_>>();
    if (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Ok(numeric.to_owned());
    }
    Err(format!("unsupported legacy Node.js selector {value:?}").into())
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

    match resolve_tool_selection("node", cwd, &home) {
        Ok(selection) => {
            println!(
                "{}",
                catalog.doctor_selection(
                    &selection.version,
                    selection.source.as_str(),
                    Some(&selection.config_path),
                )
            );
            let lock_path = match selection.source {
                pinset_core::SelectionSource::Project => lockfile_path(&selection.config_path),
                pinset_core::SelectionSource::Global => global_lockfile_path(&home),
                pinset_core::SelectionSource::System => unreachable!("declared selection"),
            };
            let lockfile = load_lockfile(&lock_path)?;
            validate_lock_matches_selection(&lockfile, &selection.version, &selection.config_path)?;
            println!(
                "{}",
                catalog.doctor_lock_matches(&lock_path, &selection.version)
            );
        }
        Err(Error::ToolSelectionNotFound { .. }) => {}
        Err(error) => return Err(error.into()),
    }

    match resolve_command("node", cwd, &home) {
        Ok(resolution) => {
            if resolution.source == pinset_core::SelectionSource::System {
                println!(
                    "{}",
                    catalog
                        .doctor_selection(&resolution.version, resolution.source.as_str(), None,)
                );
            }
            println!(
                "{}",
                catalog.doctor_line("runtime", resolution.executable.display(), "ok")
            );
        }
        Err(Error::RuntimeCommandNotFound { version, .. }) => println!(
            "{}",
            catalog.doctor_line("runtime", format!("node@{version}"), "missing")
        ),
        Err(Error::CommandSelectionNotFound { .. }) => println!("{}", catalog.no_selection()),
        Err(error) => return Err(error.into()),
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
    let commands = runtime_provider("node")
        .expect("Node provider is built in")
        .commands;
    let path_candidates = inspect_path_candidates(commands, &home, &shim_binary);
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
        existing_shim_commands(&legacy_shims, commands)
    };
    let selected = resolve_tool_selection("node", cwd, &home).is_ok();
    for issue in collect_routing_issues(
        selected,
        commands,
        &path_candidates,
        &shims,
        &shim_binary,
        &legacy_shims,
        &legacy_commands,
    ) {
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
    let normalized = path
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('\\', "/");
    for (pattern, owner) in [
        ("/.nvm/", "nvm"),
        ("/.fnm/", "fnm"),
        ("/.asdf/", "asdf"),
        ("/mise/", "mise"),
        ("/.volta/", "volta"),
    ] {
        if normalized.contains(pattern) {
            return owner.to_owned();
        }
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
            if provider != "node" {
                return Err(catalog.node_only_error().into());
            }
            let source = config.source(&provider, alias.as_deref())?;
            let client = NodeMetadataClient::for_base_url(&source.base_url)?;
            let releases = client.available_releases()?;
            let newest = releases.first().ok_or_else(|| Error::InvalidNodeIndex {
                reason: "source index contains no supported stable releases".to_owned(),
            })?;
            client.resolve_exact_lock(&newest.version, "pinset source test")?;
            println!(
                "{}",
                catalog.source_test_ok(
                    &provider,
                    &source.alias,
                    &source.base_url,
                    releases.len(),
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
    let results = ensure_shims(&default_shim_binary()?, &directory, &commands)?;
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
    let active = directory_on_path(&directory);
    println!(
        "{}",
        catalog.provider_commands_registered(tool, &directory, &installed, &preserved, active)
    );
    Ok(())
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
    fn normalizes_common_legacy_node_selectors() {
        assert_eq!(normalize_legacy_node_selector("v24.1.0").unwrap(), "24.1.0");
        assert_eq!(normalize_legacy_node_selector("lts/*").unwrap(), "lts");
        assert_eq!(normalize_legacy_node_selector("stable").unwrap(), "current");
        assert!(normalize_legacy_node_selector("nightly/latest").is_err());
    }
}
