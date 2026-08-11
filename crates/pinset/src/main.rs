use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[cfg(windows)]
use std::ffi::OsStr;

use clap::{Parser, Subcommand};
use pinset_core::{
    Error, GlobalConfig, InstallLimits, Installer, NodeMetadataClient, SUPPORTED_SOURCE_PROVIDERS,
    ShimInstallMethod, SourceView, create_project_config, current_target, find_project_config,
    global_config_path, global_lockfile_path, install_locked_node, install_shims,
    load_global_config, load_lockfile, load_optional_global_config, load_project_config,
    load_source_config, lockfile_path, node_command_directory, path_with_selected_runtime,
    pinset_home, resolve_command, save_global_state, save_lockfile, save_project_config,
    save_source_config, source_config_path, validate_lock_matches_project,
    validate_lock_matches_selection,
};

#[derive(Debug, Parser)]
#[command(
    name = "pinset",
    version,
    about = "Predictable runtime version management for multilingual projects"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a minimal pinset.toml in the current directory.
    Init,
    /// Select and lock an exact Node.js version for the current project or globally.
    Use {
        /// Selection in the form node@x.y.z.
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
        #[arg(long)]
        cwd: Option<PathBuf>,
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
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let path = create_project_config(&env::current_dir()?)?;
            println!("created {}", path.display());
        }
        Commands::Use {
            selection,
            no_install,
            global,
        } => {
            let version = parse_node_selection(&selection)?;
            let lockfile = NodeMetadataClient::official()?
                .resolve_exact_lock(&version, &format!("pinset {}", env!("CARGO_PKG_VERSION")))?;
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
                "selected {scope} node@{version}; locked {} targets in {}",
                lockfile
                    .tool("node")
                    .expect("generated lock contains node")
                    .artifacts
                    .len(),
                lock_path.display()
            );
            if !no_install {
                if global {
                    install_global(&pinset_home()?)?;
                } else {
                    install_project(&env::current_dir()?)?;
                }
            }
        }
        Commands::Install {
            locked: _,
            global,
            cwd,
        } => {
            if global {
                install_global(&pinset_home()?)?;
            } else {
                install_project(&effective_cwd(cwd)?)?;
            }
        }
        Commands::Which { command, cwd } => {
            let cwd = effective_cwd(cwd)?;
            let resolution = resolve_command(&command, &cwd, &pinset_home()?)?;
            println!("{}", resolution.executable.display());
        }
        Commands::Current { cwd } => {
            let cwd = effective_cwd(cwd)?;
            print_current(&cwd)?;
        }
        Commands::Exec { cwd, command } => {
            let cwd = effective_cwd(cwd)?;
            return execute_selected(&cwd, &command);
        }
        Commands::Doctor { cwd } => {
            run_doctor(&effective_cwd(cwd)?)?;
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
                    "{} {} {}",
                    result.command,
                    result.destination.display(),
                    method
                );
            }
        }
        Commands::Source { command } => run_source_command(command)?,
    }

    Ok(ExitCode::SUCCESS)
}

fn parse_node_selection(selection: &str) -> Result<String, Box<dyn std::error::Error>> {
    let Some((tool, version)) = selection.split_once('@') else {
        return Err("selection must use node@x.y.z".into());
    };
    if tool != "node" || version.is_empty() || version.contains('@') {
        return Err("the Node-first MVP only accepts node@x.y.z".into());
    }
    Ok(version.to_owned())
}

fn install_project(cwd: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
    install_locked_selection(&home, configured, &config_path, &lock_path)
}

fn install_global(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = global_config_path(home);
    let config: GlobalConfig = load_global_config(&config_path)?;
    let configured = config
        .tools
        .get("node")
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: "node".to_owned(),
            config_path: config_path.clone(),
        })?;
    install_locked_selection(home, configured, &config_path, &global_lockfile_path(home))
}

fn install_locked_selection(
    home: &Path,
    configured: &str,
    config_path: &Path,
    lock_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let lockfile = load_lockfile(lock_path)?;
    let locked_node = validate_lock_matches_selection(&lockfile, configured, config_path)?;
    let sources = load_source_config(&source_config_path(home))?;
    let installer = Installer::new(InstallLimits::default())?;
    let outcome = install_locked_node(&installer, home, &sources, locked_node, &current_target())?;
    if outcome.bytes_downloaded == 0 {
        println!(
            "already installed node@{} for {} at {}",
            locked_node.version,
            current_target(),
            outcome.install_dir.display()
        );
    } else {
        println!(
            "installed node@{} for {} from {} at {}",
            locked_node.version,
            current_target(),
            outcome.source_id,
            outcome.install_dir.display()
        );
    }
    Ok(())
}

fn print_current(cwd: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = find_project_config(cwd)?;
    let project = load_project_config(&config_path)?;
    let version = project
        .tools
        .get("node")
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: "node".to_owned(),
            config_path: config_path.clone(),
        })?;
    match resolve_command("node", cwd, &pinset_home()?) {
        Ok(resolution) => println!(
            "node {} installed {} config={}",
            version,
            resolution.executable.display(),
            config_path.display()
        ),
        Err(Error::RuntimeCommandNotFound { .. }) => {
            let install_dir = pinset_home()?
                .join("installs")
                .join("node")
                .join(version)
                .join(current_target());
            println!(
                "node {} missing expected={} config={}",
                version,
                node_command_directory(&install_dir, &current_target())?.display(),
                config_path.display()
            );
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn execute_selected(
    cwd: &Path,
    command: &[OsString],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let command_name = command
        .first()
        .and_then(|value| value.to_str())
        .ok_or("command name must be valid UTF-8")?;
    let resolution = resolve_command(command_name, cwd, &pinset_home()?)?;
    let runtime_path = path_with_selected_runtime(&resolution.executable)?;
    let mut child = command_for_runtime(&resolution.executable);
    child
        .args(&command[1..])
        .current_dir(cwd)
        .env("PATH", runtime_path)
        .env("PINSET_SELECTED_TOOL", &resolution.tool)
        .env("PINSET_SELECTED_VERSION", &resolution.version)
        .env("PINSET_SELECTION_SOURCE", resolution.source.as_str())
        .env("PINSET_CONFIG_PATH", &resolution.config_path);
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

fn run_doctor(cwd: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    println!("pinset_home {} ok", home.display());
    let config_path = find_project_config(cwd)?;
    println!("project_config {} ok", config_path.display());
    let project = load_project_config(&config_path)?;
    let version = project
        .tools
        .get("node")
        .ok_or_else(|| Error::ToolNotConfigured {
            tool: "node".to_owned(),
            config_path: config_path.clone(),
        })?;
    let lock_path = lockfile_path(&config_path);
    let lockfile = load_lockfile(&lock_path)?;
    validate_lock_matches_project(&lockfile, version)?;
    println!("lockfile {} matches node@{}", lock_path.display(), version);

    match resolve_command("node", cwd, &home) {
        Ok(resolution) => println!("runtime {} ok", resolution.executable.display()),
        Err(Error::RuntimeCommandNotFound { .. }) => println!("runtime node@{version} missing"),
        Err(error) => return Err(error.into()),
    }

    let shims = home.join("shims");
    let shim_on_path = env::var_os("PATH")
        .map(|value| env::split_paths(&value).any(|entry| paths_equal(&entry, &shims)))
        .unwrap_or(false);
    println!(
        "shim_path {} {}",
        shims.display(),
        if shim_on_path {
            "active"
        } else {
            "not-on-path"
        }
    );
    for candidate in path_command_candidates("node") {
        println!("path_node {}", candidate.display());
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

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn run_source_command(command: SourceCommands) -> Result<(), Box<dyn std::error::Error>> {
    let path = source_config_path(&pinset_home()?);
    let mut config = load_source_config(&path)?;
    match command {
        SourceCommands::List { provider } => {
            if let Some(provider) = provider {
                print_sources(&config.list(&provider)?);
            } else {
                for provider in SUPPORTED_SOURCE_PROVIDERS {
                    print_sources(&config.list(provider)?);
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
            println!("added {provider} {alias}");
        }
        SourceCommands::Use { provider, alias } => {
            config.use_source(&provider, &alias)?;
            save_source_config(&path, &config)?;
            println!("active {provider} {alias}");
        }
        SourceCommands::Fallback { provider, aliases } => {
            config.set_fallback(&provider, &aliases)?;
            save_source_config(&path, &config)?;
            if aliases.is_empty() {
                println!("fallback {provider} cleared");
            } else {
                println!("fallback {provider} {}", aliases.join(","));
            }
        }
        SourceCommands::Remove { provider, alias } => {
            config.remove(&provider, &alias)?;
            save_source_config(&path, &config)?;
            println!("removed {provider} {alias}");
        }
    }
    Ok(())
}

fn print_sources(sources: &[SourceView]) {
    for source in sources {
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
