use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

use clap::Subcommand;
use pinset_core::{
    EnvironmentCollision, EnvironmentProfile, PROJECT_CONFIG_SCHEMA, ProjectConfig,
    ProjectEnvironment, encode_environment, find_project_config, load_project_config, pinset_home,
    save_project_config,
};
use pinset_env::{
    EnvironmentDocument, backup_identity, generate_identity, import_identity, list_identities,
    load_identity_secret, load_identity_secrets, mutate_encrypted_profile, read_encrypted_profile,
    restore_encrypted_profile, restore_identity, revoke_project_trust, store_identity,
    trust_project, validate_variable_name, verify_project_trust, write_encrypted_profile,
};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

type SelectedProfile = (PathBuf, String, EnvironmentProfile, Vec<SecretString>);

#[derive(Debug, Subcommand)]
pub(crate) enum EnvCommands {
    /// Create a profile, device identity, and optional recovery identity.
    Init {
        #[arg(long)]
        profile: String,
        #[arg(long)]
        auto: bool,
        #[arg(
            long,
            required_unless_present = "no_recovery",
            conflicts_with = "no_recovery"
        )]
        recovery: Option<PathBuf>,
        #[arg(long)]
        no_recovery: bool,
        /// Store the device identity in this passphrase-protected file instead of the system keyring.
        #[arg(long)]
        identity_file: Option<PathBuf>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Set one encrypted variable. The value is hidden unless --stdin is used.
    Set {
        name: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Remove one encrypted variable.
    Unset {
        name: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// List variable names without decrypting values to output.
    List {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Reveal one value on an interactive terminal.
    Reveal {
        name: String,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Import Pinset's non-executable dotenv subset.
    Import {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Explicitly export plaintext dotenv to a new file.
    Export {
        #[arg(long)]
        profile: String,
        #[arg(long, default_value = "dotenv", value_parser = ["dotenv"])]
        format: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_plaintext: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Manage the recipients of one encrypted profile.
    Recipient {
        #[command(subcommand)]
        command: RecipientCommands,
    },
    /// Manage local age identities and recovery files.
    Identity {
        #[command(subcommand)]
        command: IdentityCommands,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RecipientCommands {
    Add {
        recipient: String,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    Remove {
        recipient: String,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    List {
        #[arg(long)]
        profile: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum IdentityCommands {
    Create {
        /// Store the new identity in a passphrase-protected file instead of the system keyring.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Import {
        #[arg(long)]
        from: PathBuf,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Backup {
        id: String,
        #[arg(long)]
        output: PathBuf,
    },
    Export {
        id: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_plaintext: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum TrustCommands {
    Add {
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Refuse to trust unless pinset.toml has this project-id.
        #[arg(long)]
        project_id: Option<String>,
    },
    Status {
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Revoke {
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
}

pub(crate) fn run_env_command(command: EnvCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        EnvCommands::Init {
            profile,
            auto,
            recovery,
            no_recovery,
            identity_file,
            cwd,
        } => {
            init_profile(
                &effective_cwd(cwd)?,
                &profile,
                auto,
                recovery.as_deref(),
                no_recovery,
                identity_file.as_deref(),
            )?;
        }
        EnvCommands::Set {
            name,
            profile,
            stdin,
            cwd,
        } => {
            validate_variable_name(&name)?;
            let value = if stdin {
                let mut value = String::new();
                io::stdin().read_to_string(&mut value)?;
                if value.ends_with('\n') {
                    value.pop();
                }
                if value.ends_with('\r') {
                    value.pop();
                }
                value
            } else {
                SecretString::from(rpassword::prompt_password(format!("Value for {name}: "))?)
                    .expose_secret()
                    .to_owned()
            };
            mutate_profile(&effective_cwd(cwd)?, profile.as_deref(), |document| {
                remove_case_insensitive(&mut document.variables, &name);
                document.variables.insert(name.clone(), value);
                Ok(())
            })?;
            println!("set {name}");
        }
        EnvCommands::Unset { name, profile, cwd } => {
            validate_variable_name(&name)?;
            let removed = mutate_profile(&effective_cwd(cwd)?, profile.as_deref(), |document| {
                Ok(remove_case_insensitive(&mut document.variables, &name))
            })?;
            println!("{} {name}", if removed { "unset" } else { "not set" });
        }
        EnvCommands::List { profile, json, cwd } => {
            let (_, profile_name, _, document) =
                load_profile(&effective_cwd(cwd)?, profile.as_deref())?;
            let names = document.variables.keys().cloned().collect::<Vec<_>>();
            if json {
                print_json(
                    "env.list",
                    serde_json::json!({"profile": profile_name, "names": names}),
                )?;
            } else {
                for name in names {
                    println!("{name}");
                }
            }
        }
        EnvCommands::Reveal { name, profile, cwd } => {
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err("env reveal requires an interactive terminal".into());
            }
            let (_, _, _, document) = load_profile(&effective_cwd(cwd)?, Some(&profile))?;
            let value = find_case_insensitive(&document.variables, &name)
                .ok_or("environment variable is not set")?;
            println!("{value}");
        }
        EnvCommands::Import { from, profile, cwd } => {
            let content = fs::read_to_string(&from)?;
            let imported = parse_dotenv(&content)?;
            let count = imported.len();
            mutate_profile(&effective_cwd(cwd)?, Some(&profile), move |document| {
                for (name, value) in imported {
                    remove_case_insensitive(&mut document.variables, &name);
                    document.variables.insert(name, value);
                }
                Ok(())
            })?;
            println!("imported {count} variable names into {profile}");
        }
        EnvCommands::Export {
            profile,
            format: _,
            output,
            allow_plaintext,
            cwd,
        } => {
            if !allow_plaintext {
                return Err("plaintext export requires --allow-plaintext".into());
            }
            let (_, _, _, document) = load_profile(&effective_cwd(cwd)?, Some(&profile))?;
            write_private_new(&output, render_dotenv(&document).as_bytes())?;
            println!(
                "exported {} variable names to {}",
                document.variables.len(),
                output.display()
            );
        }
        EnvCommands::Recipient { command } => run_recipient(command)?,
        EnvCommands::Identity { command } => run_identity(command)?,
    }
    Ok(())
}

pub(crate) fn run_trust_command(command: TrustCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        TrustCommands::Add { cwd, project_id } => {
            let (root, config, serialized) = project_environment(&effective_cwd(cwd)?)?;
            if let Some(expected) = project_id.as_deref()
                && required_project_id(&config)? != expected
            {
                return Err("project-id does not match --project-id".into());
            }
            trust_project(
                &pinset_home()?,
                &root,
                required_project_id(&config)?,
                &serialized,
            )?;
            println!("trusted project {}", root.display());
        }
        TrustCommands::Status { cwd, json } => {
            let (root, config, serialized) = project_environment(&effective_cwd(cwd)?)?;
            let status = match verify_project_trust(
                &pinset_home()?,
                &root,
                required_project_id(&config)?,
                &serialized,
            ) {
                Ok(()) => (true, "trusted"),
                Err(pinset_env::Error::TrustMissing) => (false, "trust_missing"),
                Err(pinset_env::Error::TrustChanged) => (false, "trust_changed"),
                Err(error) => return Err(error.into()),
            };
            if json {
                print_json(
                    "trust.status",
                    serde_json::json!({"trusted": status.0, "reason": status.1, "root": root}),
                )?;
            } else {
                println!("{} ({})", status.1, root.display());
            }
        }
        TrustCommands::Revoke { cwd } => {
            let config_path = find_project_config(&effective_cwd(cwd)?)?;
            let root = config_path
                .parent()
                .ok_or("project configuration has no parent")?;
            let removed = revoke_project_trust(&pinset_home()?, root)?;
            println!(
                "{}",
                if removed {
                    "trust revoked"
                } else {
                    "project was not trusted"
                }
            );
        }
    }
    Ok(())
}

pub(crate) fn resolve_environment(
    cwd: &Path,
    explicit_profile: Option<&str>,
) -> Result<(EnvironmentCollision, BTreeMap<String, String>), Box<dyn std::error::Error>> {
    if env::var_os("PINSET_ENV_DISABLE").is_some_and(|value| value == "1") {
        return Ok((EnvironmentCollision::Error, BTreeMap::new()));
    }
    let Some(config_path) = pinset_core::find_optional_project_config(cwd)? else {
        return Ok((EnvironmentCollision::Error, BTreeMap::new()));
    };
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?;
    let config = load_project_config(&config_path)?;
    let Some(environment) = config.environment.as_ref() else {
        if explicit_profile.is_some() || env::var_os("PINSET_ENV_PROFILE").is_some() {
            return Err("project has no encrypted environment configuration".into());
        }
        return Ok((EnvironmentCollision::Error, BTreeMap::new()));
    };
    let owned_profile = env::var("PINSET_ENV_PROFILE").ok();
    let Some(profile) = explicit_profile
        .or(owned_profile.as_deref())
        .or(environment.auto_profile.as_deref())
    else {
        return Ok((environment.collision, BTreeMap::new()));
    };
    let serialized = toml::to_string(environment)?;
    verify_project_trust(
        &pinset_home()?,
        root,
        required_project_id(&config)?,
        &serialized,
    )?;
    let selected = environment
        .profiles
        .get(profile)
        .ok_or("selected environment profile is not declared")?;
    let identities = selected_identities(&pinset_home()?)?;
    let document = read_encrypted_profile(root, &selected.file, &identities)?;
    ensure_environment_size(&document.variables)?;
    Ok((environment.collision, document.variables))
}

pub(crate) fn write_internal_environment(
    cwd: &Path,
    explicit_profile: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, mut variables) = resolve_environment(cwd, explicit_profile)?;
    let encoded = encode_environment(&variables);
    for value in variables.values_mut() {
        value.zeroize();
    }
    let mut encoded = encoded?;
    let result = io::stdout().lock().write_all(&encoded);
    encoded.zeroize();
    result?;
    Ok(())
}

fn init_profile(
    cwd: &Path,
    profile: &str,
    auto: bool,
    recovery: Option<&Path>,
    no_recovery: bool,
    identity_file: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = find_project_config(cwd)?;
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?;
    let mut config = load_project_config(&config_path)?;
    if config.schema != PROJECT_CONFIG_SCHEMA {
        return Err("encrypted environments require schema 4; run `pinset migrate` first".into());
    }
    if config
        .environment
        .as_ref()
        .is_some_and(|environment| environment.profiles.contains_key(profile))
    {
        return Err("environment profile already exists".into());
    }
    let device = generate_identity();
    if let Some(path) = identity_file {
        let passphrase = prompt_new_passphrase("Device identity passphrase: ")?;
        backup_identity(path, device.secret(), passphrase)?;
    } else {
        store_identity(&pinset_home()?, &device)?;
    }
    let mut recipients = vec![device.record.recipient.clone()];
    if !no_recovery {
        let recovery_path =
            recovery.ok_or("--recovery is required unless --no-recovery is explicit")?;
        let recovery_identity = generate_identity();
        let passphrase = prompt_new_passphrase("Recovery passphrase: ")?;
        backup_identity(recovery_path, recovery_identity.secret(), passphrase)?;
        recipients.push(recovery_identity.record.recipient);
    }
    let relative = format!("pinset.env/{profile}.age");
    if root.join(&relative).exists() {
        return Err("profile ciphertext already exists".into());
    }
    write_encrypted_profile(
        root,
        &relative,
        &EnvironmentDocument::default(),
        &recipients,
    )?;
    let environment = config
        .environment
        .get_or_insert_with(ProjectEnvironment::default);
    environment.profiles.insert(
        profile.to_owned(),
        EnvironmentProfile {
            file: relative.clone(),
            recipients,
        },
    );
    if auto {
        environment.auto_profile = Some(profile.to_owned());
    }
    if let Err(error) = save_project_config(&config_path, &config) {
        let ciphertext = root.join(&relative);
        if ciphertext.is_file() {
            fs::remove_file(&ciphertext)?;
        }
        return Err(error.into());
    }
    println!("initialized encrypted environment profile {profile}");
    println!("run `pinset trust add` before automatic injection");
    Ok(())
}

fn mutate_profile<T>(
    cwd: &Path,
    profile: Option<&str>,
    mutation: impl FnOnce(&mut EnvironmentDocument) -> pinset_env::Result<T>,
) -> Result<T, Box<dyn std::error::Error>> {
    let (config_path, _, selected, identities) = selected_profile(cwd, profile)?;
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?;
    mutate_encrypted_profile(
        root,
        &selected.file,
        &identities,
        &selected.recipients,
        mutation,
    )
    .map_err(Into::into)
}

fn load_profile(
    cwd: &Path,
    profile: Option<&str>,
) -> Result<(PathBuf, String, EnvironmentProfile, EnvironmentDocument), Box<dyn std::error::Error>>
{
    let (config_path, profile_name, selected, identities) = selected_profile(cwd, profile)?;
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?;
    let document = read_encrypted_profile(root, &selected.file, &identities)?;
    Ok((config_path, profile_name, selected, document))
}

fn selected_profile(
    cwd: &Path,
    profile: Option<&str>,
) -> Result<SelectedProfile, Box<dyn std::error::Error>> {
    let config_path = find_project_config(cwd)?;
    let config = load_project_config(&config_path)?;
    if config.schema != PROJECT_CONFIG_SCHEMA {
        return Err("encrypted environments require schema 4; run `pinset migrate` first".into());
    }
    let environment = config
        .environment
        .as_ref()
        .ok_or("project has no encrypted environment profiles")?;
    let from_env = env::var("PINSET_ENV_PROFILE").ok();
    let profile_name = profile
        .or(from_env.as_deref())
        .or(environment.auto_profile.as_deref())
        .ok_or("specify --profile because no auto-profile is configured")?
        .to_owned();
    let selected = environment
        .profiles
        .get(&profile_name)
        .cloned()
        .ok_or("selected environment profile is not declared")?;
    let identities = selected_identities(&pinset_home()?)?;
    Ok((config_path, profile_name, selected, identities))
}

fn run_recipient(command: RecipientCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        RecipientCommands::List { profile, cwd } => {
            let config_path = find_project_config(&effective_cwd(cwd)?)?;
            let config = load_project_config(&config_path)?;
            let selected = config
                .environment
                .as_ref()
                .and_then(|e| e.profiles.get(&profile))
                .ok_or("selected environment profile is not declared")?;
            for recipient in &selected.recipients {
                println!("{recipient}");
            }
        }
        RecipientCommands::Add {
            recipient,
            profile,
            cwd,
        } => change_recipient(&effective_cwd(cwd)?, &profile, &recipient, true)?,
        RecipientCommands::Remove {
            recipient,
            profile,
            cwd,
        } => change_recipient(&effective_cwd(cwd)?, &profile, &recipient, false)?,
    }
    Ok(())
}

fn change_recipient(
    cwd: &Path,
    profile: &str,
    recipient: &str,
    add: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !recipient.starts_with("age1") {
        return Err("invalid age X25519 recipient".into());
    }
    let (config_path, _, selected, document) = load_profile(cwd, Some(profile))?;
    let mut config = load_project_config(&config_path)?;
    let configured = config
        .environment
        .as_mut()
        .and_then(|e| e.profiles.get_mut(profile))
        .ok_or("selected environment profile is not declared")?;
    if add {
        if !configured.recipients.iter().any(|value| value == recipient) {
            configured.recipients.push(recipient.to_owned());
        }
    } else {
        configured.recipients.retain(|value| value != recipient);
        if configured.recipients.is_empty() {
            return Err("cannot remove the final profile recipient".into());
        }
    }
    configured.recipients.sort();
    configured.recipients.dedup();
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?;
    let ciphertext_path = root.join(&selected.file);
    let original_ciphertext = fs::read(&ciphertext_path)?;
    write_encrypted_profile(root, &selected.file, &document, &configured.recipients)?;
    if let Err(error) = save_project_config(&config_path, &config) {
        restore_encrypted_profile(root, &selected.file, &original_ciphertext)?;
        return Err(error.into());
    }
    println!(
        "{} recipient for {profile}; project trust must be renewed",
        if add { "added" } else { "removed" }
    );
    Ok(())
}

fn run_identity(command: IdentityCommands) -> Result<(), Box<dyn std::error::Error>> {
    let home = pinset_home()?;
    match command {
        IdentityCommands::Create { output } => {
            let material = generate_identity();
            if let Some(path) = output {
                let passphrase = prompt_new_passphrase("Identity passphrase: ")?;
                backup_identity(&path, material.secret(), passphrase)?;
            } else {
                store_identity(&home, &material)?;
            }
            println!("{} {}", material.record.id, material.record.recipient);
        }
        IdentityCommands::Import { from } => {
            let passphrase =
                SecretString::from(rpassword::prompt_password("Recovery passphrase: ")?);
            let secret = restore_identity(&from, passphrase)?;
            let record = import_identity(&home, secret)?;
            println!("{} {}", record.id, record.recipient);
        }
        IdentityCommands::List { json } => {
            let identities = list_identities(&home)?;
            if json {
                print_json("env.identity.list", &identities)?;
            } else {
                for identity in identities {
                    println!(
                        "{} {} {}",
                        identity.id, identity.recipient, identity.backend
                    );
                }
            }
        }
        IdentityCommands::Backup { id, output } => {
            let secret = load_identity_secret(&home, &id)?;
            let passphrase = prompt_new_passphrase("Backup passphrase: ")?;
            backup_identity(&output, &secret, passphrase)?;
            println!("backed up identity {id} to {}", output.display());
        }
        IdentityCommands::Export {
            id,
            output,
            allow_plaintext,
        } => {
            if !allow_plaintext {
                return Err("plaintext identity export requires --allow-plaintext".into());
            }
            let secret = load_identity_secret(&home, &id)?;
            write_private_new(&output, secret.expose_secret().as_bytes())?;
            println!("exported identity {id} to {}", output.display());
        }
    }
    Ok(())
}

fn project_environment(
    cwd: &Path,
) -> Result<(PathBuf, ProjectConfig, String), Box<dyn std::error::Error>> {
    let config_path = find_project_config(cwd)?;
    let root = config_path
        .parent()
        .ok_or("project configuration has no parent")?
        .to_path_buf();
    let config = load_project_config(&config_path)?;
    let serialized = toml::to_string(
        config
            .environment
            .as_ref()
            .ok_or("project has no encrypted environment configuration")?,
    )?;
    Ok((root, config, serialized))
}

fn required_project_id(config: &ProjectConfig) -> Result<&str, Box<dyn std::error::Error>> {
    config
        .project_id
        .as_deref()
        .ok_or_else(|| "schema 4 project-id is missing".into())
}

fn ensure_environment_size(
    variables: &BTreeMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let inherited = env::vars_os()
        .map(|(name, value)| name.len() + value.len() + 2)
        .sum::<usize>();
    let total = inherited
        + variables
            .iter()
            .map(|(name, value)| name.len() + value.len() + 2)
            .sum::<usize>();
    #[cfg(windows)]
    const LIMIT: usize = 32767 * 2;
    #[cfg(not(windows))]
    const LIMIT: usize = 1024 * 1024;
    if total > LIMIT {
        return Err("selected environment exceeds the platform environment block limit".into());
    }
    Ok(())
}

fn selected_identities(home: &Path) -> Result<Vec<SecretString>, Box<dyn std::error::Error>> {
    let mut identities = Vec::new();
    if let Some(path) = env::var_os("PINSET_IDENTITY_FILE").map(PathBuf::from) {
        if !io::stdin().is_terminal() {
            return Err(
                "PINSET_IDENTITY_FILE requires an interactive terminal for its passphrase".into(),
            );
        }
        let passphrase =
            SecretString::from(rpassword::prompt_password("Identity file passphrase: ")?);
        identities.push(restore_identity(&path, passphrase)?);
    }
    match load_identity_secrets(home) {
        Ok(mut system_identities) => identities.append(&mut system_identities),
        Err(_) if !identities.is_empty() => {}
        Err(error) => return Err(error.into()),
    }
    Ok(identities)
}

fn parse_dotenv(content: &str) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let lines = content.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        let raw = lines[index];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            index += 1;
            continue;
        }
        if line.starts_with("export ")
            || line.contains("$('")
            || line.contains("$(")
            || line.contains('`')
        {
            return Err(format!("unsupported dotenv expression on line {}", index + 1).into());
        }
        let (name, raw_value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid dotenv assignment on line {}", index + 1))?;
        let name = name.trim();
        validate_variable_name(name)?;
        let folded = name.to_ascii_uppercase();
        if !seen.insert(folded) {
            return Err(format!("duplicate dotenv variable on line {}", index + 1).into());
        }
        let mut raw_value = raw_value.trim().to_owned();
        if let Some(quote) = raw_value
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''))
        {
            while !quoted_value_complete(&raw_value, quote) {
                index += 1;
                let continuation = lines
                    .get(index)
                    .ok_or_else(|| format!("unterminated quoted dotenv value on line {}", index))?;
                raw_value.push('\n');
                raw_value.push_str(continuation);
            }
        }
        let value = parse_dotenv_value(&raw_value, index + 1)?;
        values.insert(name.to_owned(), value);
        index += 1;
    }
    Ok(values)
}

fn quoted_value_complete(value: &str, quote: char) -> bool {
    let mut escaped = false;
    for character in value.chars().skip(1) {
        if quote == '"' && character == '\\' && !escaped {
            escaped = true;
            continue;
        }
        if character == quote && !escaped {
            return true;
        }
        escaped = false;
    }
    false
}

fn parse_dotenv_value(value: &str, line: usize) -> Result<String, Box<dyn std::error::Error>> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars
                    .next()
                    .ok_or_else(|| format!("invalid escape on line {line}"))?
                {
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    '\\' => output.push('\\'),
                    '"' => output.push('"'),
                    _ => return Err(format!("unsupported escape on line {line}").into()),
                }
            } else {
                output.push(ch);
            }
        }
        return Ok(output);
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Ok(inner.to_owned());
    }
    if value.starts_with(['"', '\''])
        || value.ends_with(['"', '\''])
        || value.contains("${")
        || value.contains("$(")
        || value.contains('`')
    {
        return Err(format!("unsupported dotenv value on line {line}").into());
    }
    let value = value
        .split_once(" #")
        .map_or(value, |(value, _)| value)
        .trim_end();
    Ok(value.to_owned())
}

fn render_dotenv(document: &EnvironmentDocument) -> String {
    let mut output = String::new();
    for (name, value) in &document.variables {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(&escaped);
        output.push_str("\"\n");
    }
    output
}

fn write_private_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(windows)]
    if let Err(error) = restrict_windows_private_file(path) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn restrict_windows_private_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let identity = std::process::Command::new("whoami.exe").output()?;
    if !identity.status.success() {
        return Err("failed to resolve the current Windows account for private export".into());
    }
    let account = String::from_utf8(identity.stdout)?.trim().to_owned();
    if account.is_empty() {
        return Err("the current Windows account name is empty".into());
    }
    let acl = std::process::Command::new("icacls.exe")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{account}:(F)"))
        .output()?;
    if !acl.status.success() {
        return Err("failed to restrict the exported file to the current Windows user".into());
    }
    Ok(())
}

fn prompt_new_passphrase(prompt: &str) -> Result<SecretString, Box<dyn std::error::Error>> {
    let one = rpassword::prompt_password(prompt)?;
    if one.is_empty() {
        return Err("passphrase must not be empty".into());
    }
    let two = rpassword::prompt_password("Confirm passphrase: ")?;
    if one != two {
        return Err("passphrases do not match".into());
    }
    Ok(SecretString::from(one))
}

fn find_case_insensitive<'a>(
    variables: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a str> {
    variables
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn remove_case_insensitive(variables: &mut BTreeMap<String, String>, name: &str) -> bool {
    let key = variables
        .keys()
        .find(|candidate| candidate.eq_ignore_ascii_case(name))
        .cloned();
    key.is_some_and(|key| variables.remove(&key).is_some())
}

fn effective_cwd(cwd: Option<PathBuf>) -> io::Result<PathBuf> {
    cwd.map(Ok).unwrap_or_else(env::current_dir)
}

fn print_json(command: &'static str, data: impl serde::Serialize) -> Result<(), serde_json::Error> {
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"schema": 1, "command": command, "ok": true, "data": data})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_subset_supports_quotes_empty_values_comments_and_multiline() {
        let values = parse_dotenv(
            "# comment\nEMPTY=\nPLAIN=value # note\nQUOTED=\"a\\nline\"\nMULTI=\"first\nsecond\"\nSINGLE='literal'\n",
        )
        .unwrap();
        assert_eq!(values["EMPTY"], "");
        assert_eq!(values["PLAIN"], "value");
        assert_eq!(values["QUOTED"], "a\nline");
        assert_eq!(values["MULTI"], "first\nsecond");
        assert_eq!(values["SINGLE"], "literal");
    }

    #[test]
    fn dotenv_subset_rejects_duplicates_and_shell_expressions() {
        assert!(parse_dotenv("TOKEN=a\ntoken=b\n").is_err());
        assert!(parse_dotenv("export TOKEN=a\n").is_err());
        assert!(parse_dotenv("TOKEN=$(whoami)\n").is_err());
        assert!(parse_dotenv("PATH=/tmp\n").is_err());
    }
}
