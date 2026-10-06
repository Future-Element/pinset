use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
#[derive(Parser, Debug)]
#[command(name="pinset",version=pinset_core::pinset_version(),disable_help_subcommand=true,about="Lock project toolchains, inspect execution, and verify upgrades")]
pub struct Cli {
    #[arg(short = 'C', long = "cwd", global = true, default_value = ".")]
    pub cwd: PathBuf,
    #[arg(long,global=true,value_parser=["en","zh-CN","auto"],default_value="auto")]
    pub lang: String,
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Commands,
}
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new project identity in the specified directory.
    Init,
    /// Select tools and resolve their official artifacts.
    Use {
        #[arg(required=true,num_args=1..)]
        tools: Vec<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        no_install: bool,
        #[arg(long)]
        plan: bool,
    },
    /// Remove selections while retaining installations.
    Remove {
        #[arg(required=true,num_args=1..)]
        tools: Vec<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        plan: bool,
    },
    /// Install exactly the artifacts already recorded in the lock.
    Install {
        tools: Vec<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        repair: bool,
        #[arg(long)]
        recreate_venv: bool,
        #[arg(long)]
        plan: bool,
    },
    /// List selections and installations, or official remote versions.
    List {
        tool: Option<String>,
        #[arg(long, requires = "tool")]
        remote: bool,
    },
    /// Show command routes and SDK roots.
    Which {
        command: Option<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        explain: bool,
    },
    /// Inspect configuration and installations; probes require explicit opt-in.
    Check {
        #[arg(long)]
        global: bool,
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        probe: bool,
        #[arg(long, value_enum)]
        target: Option<BuildTarget>,
    },
    /// Execute a raw command using the exact selected environment.
    Exec {
        #[command(flatten)]
        environment: EnvironmentArgs,
        #[arg(last=true,required=true,num_args=1..)]
        command: Vec<String>,
    },
    /// Prepare, test, apply or recover a single-project upgrade.
    Upgrade {
        #[command(subcommand)]
        command: Upgrade,
    },
    /// Manage encrypted profiles, access and project trust.
    Env {
        #[command(subcommand)]
        command: Environment,
    },
    /// Clean only unreferenced Pinset-owned data.
    Clean {
        #[command(subcommand)]
        command: Clean,
    },
    /// Inspect and maintain this Pinset installation.
    #[command(name = "self")]
    SelfCommand {
        #[command(subcommand)]
        command: SelfCommand,
    },
}
#[derive(ValueEnum, Debug, Clone)]
pub enum BuildTarget {
    Android,
    Ios,
    Windows,
    Macos,
    Linux,
    Web,
}
#[derive(Args, Debug, Default)]
pub struct EnvironmentArgs {
    #[arg(long, conflicts_with = "no_env")]
    pub profile: Option<String>,
    #[arg(long)]
    pub no_env: bool,
}
#[derive(Subcommand, Debug)]
pub enum Upgrade {
    Prepare {
        tools: Vec<String>,
        #[arg(long)]
        plan: bool,
    },
    Test {
        id: String,
        #[arg(long)]
        compare: bool,
        #[command(flatten)]
        environment: EnvironmentArgs,
        #[arg(last=true,required=true,num_args=1..)]
        command: Vec<String>,
    },
    Status {
        #[arg(conflicts_with = "history")]
        id: Option<String>,
        #[arg(long)]
        history: bool,
    },
    Apply {
        id: String,
        #[arg(long)]
        allow_limited: bool,
        #[arg(long)]
        plan: bool,
    },
    Restore {
        history_id: String,
        #[arg(long)]
        plan: bool,
    },
    Recover {
        #[arg(long)]
        plan: bool,
    },
}
#[derive(Subcommand, Debug)]
pub enum Environment {
    Init {
        profile: String,
    },
    Use {
        #[arg(required_unless_present = "reset", conflicts_with = "reset")]
        profile: Option<String>,
        #[arg(long)]
        reset: bool,
        #[arg(long)]
        project: bool,
    },
    Remove {
        profile: String,
        #[arg(long)]
        plan: bool,
    },
    List {
        #[arg(long)]
        profile: Option<String>,
    },
    Set {
        name: String,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        stdin: bool,
    },
    Unset {
        name: String,
        #[arg(long)]
        profile: String,
    },
    Access {
        #[command(subcommand)]
        command: Access,
    },
    Trust {
        #[command(subcommand)]
        command: Trust,
    },
}
#[derive(Subcommand, Debug)]
pub enum Access {
    Request {
        #[arg(long, conflicts_with = "ci")]
        new: bool,
        #[arg(long)]
        ci: bool,
    },
    Grant {
        request_file: PathBuf,
        #[arg(long)]
        profile: String,
    },
    Revoke {
        request_id: String,
        #[arg(long)]
        profile: String,
    },
    List {
        #[arg(long)]
        profile: String,
    },
}
#[derive(Subcommand, Debug)]
pub enum Trust {
    Add,
    Status,
    Revoke,
}
#[derive(Subcommand, Debug)]
pub enum Clean {
    Cache {
        #[arg(long)]
        plan: bool,
    },
    Installs {
        tools: Vec<String>,
        #[arg(long)]
        plan: bool,
    },
    History {
        #[arg(long)]
        older_than: String,
        #[arg(long)]
        plan: bool,
    },
}
#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}
#[derive(Subcommand, Debug)]
pub enum SelfCommand {
    Info,
    Shell {
        #[arg(value_enum)]
        shell: Shell,
    },
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    Repair,
    Update {
        version: Option<String>,
        #[arg(long)]
        plan: bool,
    },
}
