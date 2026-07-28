use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use pinset_core::{
    SUPPORTED_SOURCE_PROVIDERS, ShimInstallMethod, SourceView, install_shims, load_source_config,
    pinset_home_from_env, resolve_command, save_source_config, source_config_path,
};

#[derive(Debug, Parser)]
#[command(name = "pinset", version, about = "Pinset Phase 0 runtime shim spike")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
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
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Which { command, cwd } => {
            let cwd = effective_cwd(cwd)?;
            let resolution = resolve_command(&command, &cwd, &pinset_home_from_env()?)?;
            println!("{}", resolution.executable.display());
        }
        Commands::Current { cwd } => {
            let cwd = effective_cwd(cwd)?;
            let home = pinset_home_from_env()?;
            for command in ["node", "npm", "npx", "corepack"] {
                match resolve_command(command, &cwd, &home) {
                    Ok(resolution) => println!(
                        "{} {} {} {}",
                        resolution.tool,
                        resolution.version,
                        resolution.command,
                        resolution.executable.display()
                    ),
                    Err(pinset_core::Error::RuntimeCommandNotFound { .. }) => {}
                    Err(error) => return Err(error.into()),
                }
            }
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

    Ok(())
}

fn run_source_command(command: SourceCommands) -> Result<(), Box<dyn std::error::Error>> {
    let path = source_config_path(&pinset_home_from_env()?);
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
