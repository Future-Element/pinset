use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

mod i18n;

#[cfg(windows)]
use std::ffi::OsStr;

use clap::{Parser, Subcommand, error::ErrorKind};
use pinset_core::{
    Error, GlobalConfig, InstallLimits, Installer, NodeMetadataClient, SUPPORTED_SOURCE_PROVIDERS,
    ShimInstallMethod, SourceView, command_tool, create_project_config, current_target,
    find_optional_project_config, find_project_config, global_config_path, global_lockfile_path,
    install_locked_node, install_shims, list_installed_node_versions, load_global_config,
    load_lockfile, load_optional_global_config, load_project_config, load_source_config,
    load_user_settings, lockfile_path, node_command_directory, path_with_selected_runtime,
    pinset_home, resolve_command, resolve_tool_selection, save_global_state, save_lockfile,
    save_project_config, save_source_config, save_user_settings, source_config_path,
    user_settings_path, validate_lock_matches_selection,
};

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
    /// Install the current target from the project or global lockfile.
    Install {
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
    /// Print the current project tool selections and their exact executable paths.
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
    /// Execute a command through the selected runtime without installing shims.
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
    },
    /// Install multi-call shim links into a user-owned directory.
    Shim {
        #[command(subcommand)]
        command: ShimCommands,
    },
    /// Manage local download sources without changing project lock files.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ShimCommands {
    Install {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        dir: PathBuf,
        #[arg(default_values_t = default_commands())]
        commands: Vec<String>,
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
        Commands::Use {
            selection,
            no_install,
            global,
        } => {
            let selector = parse_node_selection(&selection, catalog)?;
            let lockfile = NodeMetadataClient::official()?
                .resolve_lock(&selector, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
            let version = lockfile
                .tool("node")
                .expect("generated lock contains node")
                .version
                .clone();
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
                let cwd = env::current_dir()?;
                let config_path = find_project_config(&cwd)?;
                let mut project = load_project_config(&config_path)?;
                let lock_path = lockfile_path(&config_path);
                save_lockfile(&lock_path, &lockfile)?;
                project.set_tool("node", &version);
                save_project_config(&config_path, &project)?;
                ("project", lock_path)
            };
            println!(
                "{}",
                catalog.selected(
                    scope,
                    &version,
                    lockfile
                        .tool("node")
                        .expect("generated lock contains node")
                        .artifacts
                        .len(),
                    &lock_path,
                )
            );
            if !no_install {
                if global {
                    install_global(&pinset_home()?, catalog)?;
                } else {
                    install_project(&env::current_dir()?, catalog)?;
                }
            }
        }
        Commands::Install {
            locked: _,
            global,
            cwd,
        } => {
            if global {
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
                let releases = NodeMetadataClient::official()?.available_releases()?;
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
        Commands::Exec { cwd, command } => {
            let cwd = effective_cwd(cwd)?;
            return execute_selected(&cwd, &command, catalog);
        }
        Commands::Doctor { cwd } => {
            run_doctor(&effective_cwd(cwd)?, catalog)?;
        }
        Commands::Shim {
            command:
                ShimCommands::Install {
                    binary,
                    dir,
                    commands,
                },
        } => {
            for result in install_shims(&binary, &dir, &commands)? {
                let method = match result.method {
                    ShimInstallMethod::HardLink => "hard-link",
                    ShimInstallMethod::Copy => "copy",
                };
                println!(
                    "{}",
                    catalog.shim_installed(&result.command, &result.destination, method)
                );
            }
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
    const COMMANDS: [&str; 10] = [
        "init", "use", "install", "which", "current", "exec", "doctor", "shim", "source", "list",
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
    let locked_node = validate_lock_matches_selection(&lockfile, configured, config_path)?;
    let sources = load_source_config(&source_config_path(home))?;
    let installer = Installer::new(InstallLimits::default())?;
    let outcome = install_locked_node(&installer, home, &sources, locked_node, &current_target())?;
    if outcome.bytes_downloaded == 0 {
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
            let install_dir = home
                .join("installs")
                .join(&selection.tool)
                .join(&selection.version)
                .join(current_target());
            println!(
                "{}",
                catalog.current_missing(
                    &selection.version,
                    selection.source.as_str(),
                    &node_command_directory(&install_dir, &current_target())?,
                    Some(&selection.config_path),
                )
            );
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
    let command_name = command
        .first()
        .and_then(|value| value.to_str())
        .ok_or_else(|| catalog.utf8_command_error())?;
    let resolution = resolve_command(command_name, cwd, &pinset_home()?)?;
    let runtime_path = path_with_selected_runtime(&resolution.executable)?;
    let mut child = command_for_runtime(&resolution.executable);
    child
        .args(&command[1..])
        .current_dir(cwd)
        .env("PATH", runtime_path)
        .env("PINSET_SELECTED_TOOL", &resolution.tool)
        .env("PINSET_SELECTED_VERSION", &resolution.version)
        .env("PINSET_SELECTION_SOURCE", resolution.source.as_str());
    if let Some(path) = &resolution.selection_path {
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

    let shims = home.join("shims");
    let shim_on_path = env::var_os("PATH")
        .map(|value| env::split_paths(&value).any(|entry| paths_equal(&entry, &shims)))
        .unwrap_or(false);
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
    for candidate in path_command_candidates("node") {
        println!(
            "{}",
            catalog.path_candidate(&candidate, path_owner(&candidate, &home))
        );
    }
    Ok(())
}

fn path_command_candidates(command: &str) -> Vec<PathBuf> {
    let Some(path) = env::var_os("PATH") else {
        return Vec::new();
    };
    let names: &[&str] = if cfg!(windows) {
        &["node.exe", "node.cmd", "node.bat", "node"]
    } else {
        &[command]
    };
    env::split_paths(&path)
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .filter(|candidate| fs::metadata(candidate).is_ok_and(|metadata| metadata.is_file()))
        .collect()
}

fn user_home_directory() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        env::var_os("HOME").map(PathBuf::from)
    }
}

fn path_owner(path: &Path, pinset_home: &Path) -> &'static str {
    if path.starts_with(pinset_home.join("shims")) {
        return "pinset";
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
            return owner;
        }
    }
    "other"
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
        } => {
            config.add(&provider, &alias, &base_url, allow_insecure)?;
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
            println!(
                "{} {} {} 状态={} {}{}",
                source.provider,
                source.alias,
                source.kind.as_str(),
                state,
                source.base_url,
                security
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
            println!(
                "{} {} {} {} {}{}",
                source.provider,
                source.alias,
                source.kind.as_str(),
                state,
                source.base_url,
                security
            );
        }
    }
}

fn effective_cwd(cwd: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    cwd.map_or_else(env::current_dir, |path| absolutize(&path))
}

fn absolutize(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn default_commands() -> Vec<String> {
    ["node", "npm", "npx", "corepack"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}
