//! Encrypted project environments, local identities, and explicit project trust.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use age::{
    Decryptor, Encryptor,
    secrecy::{ExposeSecret, SecretString},
    x25519,
};
use atomic_write_file::AtomicWriteFile;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const PROFILE_SCHEMA: u32 = 1;
pub const PROFILE_MAX_BYTES: usize = 1024 * 1024;
pub const DOTENV_PROFILE_HEADER: &str = "# pinset-encrypted-env v1";
const DOTENV_VALUE_PREFIX: &str = "encrypted:pinset:v1:";
const IDENTITY_SERVICE: &str = "dev.pinset.identity";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid environment variable {name}: {reason}")]
    InvalidVariable { name: String, reason: String },
    #[error("invalid age recipient")]
    InvalidRecipient,
    #[error("no age recipient was configured")]
    MissingRecipient,
    #[error("no Pinset identity could decrypt this profile")]
    NoMatchingIdentity,
    #[error("environment profile is invalid: {0}")]
    InvalidProfile(String),
    #[error("environment profile exceeds the 1 MiB limit")]
    ProfileTooLarge,
    #[error("unsafe environment profile path: {0}")]
    UnsafePath(PathBuf),
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to access the system credential store: {0}")]
    Keyring(String),
    #[error("identity metadata is invalid")]
    InvalidIdentityMetadata,
    #[error("project trust is missing")]
    TrustMissing,
    #[error("project trust no longer matches the environment policy")]
    TrustChanged,
    #[error("cryptographic operation failed")]
    Crypto,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentDocument {
    pub schema: u32,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

impl Default for EnvironmentDocument {
    fn default() -> Self {
        Self {
            schema: PROFILE_SCHEMA,
            variables: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IdentityRecord {
    pub id: String,
    pub recipient: String,
    pub backend: String,
}

#[derive(Debug, Clone)]
pub struct IdentityMaterial {
    pub record: IdentityRecord,
    secret: SecretString,
}

impl IdentityMaterial {
    pub fn secret(&self) -> &SecretString {
        &self.secret
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityMetadata {
    schema: u32,
    identities: Vec<IdentityRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TrustRecord {
    schema: u32,
    project_id: String,
    root: String,
    environment_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<pinset_core::WorkDirectoryIdentity>,
}

pub fn validate_variable_name(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return Err(invalid_variable(name, "name is empty"));
    };
    if !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(invalid_variable(name, "name is not portable"));
    }
    let upper = name.to_ascii_uppercase();
    if upper == "PATH" || upper.starts_with("PINSET_") {
        return Err(invalid_variable(name, "name is reserved by Pinset"));
    }
    Ok(())
}

pub fn validate_document(document: &EnvironmentDocument) -> Result<()> {
    if document.schema != PROFILE_SCHEMA {
        return Err(Error::InvalidProfile(format!(
            "unsupported schema {}",
            document.schema
        )));
    }
    let mut names = BTreeSet::new();
    for (name, value) in &document.variables {
        validate_variable_name(name)?;
        if !names.insert(name.to_ascii_uppercase()) {
            return Err(invalid_variable(name, "name differs only by ASCII case"));
        }
        if value.contains('\0') {
            return Err(invalid_variable(name, "value contains NUL"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct EncryptedDotenvDocument {
    variables: BTreeMap<String, String>,
}

fn encrypt_value(value: &str, recipient_strings: &[String]) -> Result<String> {
    if value.contains('\0') {
        return Err(Error::InvalidProfile("value contains NUL".to_owned()));
    }
    let recipients = parse_recipients(recipient_strings)?;
    let encryptor = Encryptor::with_recipients(
        recipients
            .iter()
            .map(|recipient| recipient as &dyn age::Recipient),
    )
    .map_err(|_| Error::Crypto)?;
    let mut output = Vec::with_capacity(value.len() + 512);
    let mut writer = encryptor
        .wrap_output(&mut output)
        .map_err(|_| Error::Crypto)?;
    writer
        .write_all(value.as_bytes())
        .and_then(|()| writer.finish())
        .map_err(|_| Error::Crypto)?;
    Ok(format!("{DOTENV_VALUE_PREFIX}{}", BASE64.encode(output)))
}

fn decrypt_value(value: &str, identity_strings: &[SecretString]) -> Result<String> {
    let encoded = value
        .strip_prefix(DOTENV_VALUE_PREFIX)
        .ok_or_else(|| Error::InvalidProfile("dotenv value is not encrypted".to_owned()))?;
    let ciphertext = BASE64
        .decode(encoded)
        .map_err(|_| Error::InvalidProfile("dotenv ciphertext is not valid base64".to_owned()))?;
    let identities = parse_identities(identity_strings)?;
    let decryptor = Decryptor::new_buffered(ciphertext.as_slice()).map_err(|_| Error::Crypto)?;
    let mut reader = decryptor
        .decrypt(
            identities
                .iter()
                .map(|identity| identity as &dyn age::Identity),
        )
        .map_err(|_| Error::NoMatchingIdentity)?;
    let mut plaintext = Vec::new();
    let read_result = std::io::Read::take(&mut reader, (PROFILE_MAX_BYTES + 1) as u64)
        .read_to_end(&mut plaintext)
        .map_err(|_| Error::Crypto);
    if let Err(error) = read_result {
        plaintext.zeroize();
        return Err(error);
    }
    if plaintext.len() > PROFILE_MAX_BYTES {
        plaintext.zeroize();
        return Err(Error::ProfileTooLarge);
    }
    let value = String::from_utf8(plaintext.clone())
        .map_err(|_| Error::InvalidProfile("dotenv plaintext is not UTF-8".to_owned()));
    plaintext.zeroize();
    value
}

fn parse_recipients(recipient_strings: &[String]) -> Result<Vec<x25519::Recipient>> {
    if recipient_strings.is_empty() {
        return Err(Error::MissingRecipient);
    }
    recipient_strings
        .iter()
        .map(|recipient| {
            recipient
                .parse::<x25519::Recipient>()
                .map_err(|_| Error::InvalidRecipient)
        })
        .collect()
}

fn parse_identities(identity_strings: &[SecretString]) -> Result<Vec<x25519::Identity>> {
    let identities = identity_strings
        .iter()
        .filter_map(|secret| {
            secret
                .expose_secret()
                .trim()
                .parse::<x25519::Identity>()
                .ok()
        })
        .collect::<Vec<_>>();
    if identities.is_empty() {
        return Err(Error::NoMatchingIdentity);
    }
    Ok(identities)
}

fn parse_encrypted_dotenv(bytes: &[u8]) -> Result<EncryptedDotenvDocument> {
    if bytes.len() > PROFILE_MAX_BYTES + 64 * 1024 {
        return Err(Error::ProfileTooLarge);
    }
    let content = std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidProfile("encrypted dotenv is not UTF-8".to_owned()))?;
    if content.lines().next() != Some(DOTENV_PROFILE_HEADER) {
        return Err(Error::InvalidProfile(
            "encrypted dotenv header is missing".to_owned(),
        ));
    }
    let mut document = EncryptedDotenvDocument::default();
    let mut seen = BTreeSet::new();
    for (index, raw) in content.lines().enumerate().skip(1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, encoded) = line.split_once('=').ok_or_else(|| {
            Error::InvalidProfile(format!("invalid dotenv assignment on line {}", index + 1))
        })?;
        let name = name.trim();
        validate_variable_name(name)?;
        if !seen.insert(name.to_ascii_uppercase()) {
            return Err(invalid_variable(name, "name differs only by ASCII case"));
        }
        let encoded = encoded.trim();
        let encoded = encoded
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(encoded);
        if !encoded.starts_with(DOTENV_VALUE_PREFIX) {
            return Err(Error::InvalidProfile(format!(
                "{name} is not a Pinset-encrypted value"
            )));
        }
        let ciphertext = BASE64
            .decode(&encoded[DOTENV_VALUE_PREFIX.len()..])
            .map_err(|_| Error::InvalidProfile(format!("{name} ciphertext is not valid base64")))?;
        Decryptor::new_buffered(ciphertext.as_slice()).map_err(|_| {
            Error::InvalidProfile(format!("{name} ciphertext is not a valid age payload"))
        })?;
        document
            .variables
            .insert(name.to_owned(), encoded.to_owned());
    }
    Ok(document)
}

fn render_encrypted_dotenv(document: &EncryptedDotenvDocument) -> Result<Vec<u8>> {
    let mut output = String::from(DOTENV_PROFILE_HEADER);
    output.push_str(
        "\n# Ciphertext is safe to commit. Private identities stay in the OS credential store.\n",
    );
    for (name, value) in &document.variables {
        validate_variable_name(name)?;
        if !value.starts_with(DOTENV_VALUE_PREFIX) {
            return Err(Error::InvalidProfile(format!(
                "{name} is not a Pinset-encrypted value"
            )));
        }
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(value);
        output.push_str("\"\n");
    }
    if output.len() > PROFILE_MAX_BYTES + 64 * 1024 {
        return Err(Error::ProfileTooLarge);
    }
    Ok(output.into_bytes())
}

fn decrypt_dotenv_document(
    encrypted: &EncryptedDotenvDocument,
    identities: &[SecretString],
) -> Result<EnvironmentDocument> {
    let mut variables = BTreeMap::new();
    for (name, value) in &encrypted.variables {
        variables.insert(name.clone(), decrypt_value(value, identities)?);
    }
    let document = EnvironmentDocument {
        schema: PROFILE_SCHEMA,
        variables,
    };
    validate_document(&document)?;
    Ok(document)
}

fn encrypt_dotenv_document(
    document: &EnvironmentDocument,
    recipients: &[String],
) -> Result<EncryptedDotenvDocument> {
    validate_document(document)?;
    let mut variables = BTreeMap::new();
    for (name, value) in &document.variables {
        variables.insert(name.clone(), encrypt_value(value, recipients)?);
    }
    Ok(EncryptedDotenvDocument { variables })
}

pub fn list_profile_names(project_root: &Path, relative: &str) -> Result<Vec<String>> {
    let path = safe_profile_path(project_root, relative, false)?;
    let bytes = fs::read(&path).map_err(|source| Error::Io { path, source })?;
    Ok(parse_encrypted_dotenv(&bytes)?
        .variables
        .into_keys()
        .collect())
}

pub fn set_encrypted_profile_values(
    project_root: &Path,
    relative: &str,
    recipients: &[String],
    values: BTreeMap<String, String>,
) -> Result<()> {
    let path = safe_profile_path(project_root, relative, false)?;
    let lock = lock_profile(&path)?;
    let bytes = read_regular_file(&path)?;
    let mut encrypted = parse_encrypted_dotenv(&bytes)?;
    for (name, value) in values {
        validate_variable_name(&name)?;
        remove_case_insensitive(&mut encrypted.variables, &name);
        encrypted
            .variables
            .insert(name, encrypt_value(&value, recipients)?);
    }
    atomic_write(&path, &render_encrypted_dotenv(&encrypted)?)?;
    let _ = FileExt::unlock(&lock);
    Ok(())
}

pub fn unset_encrypted_profile_value(
    project_root: &Path,
    relative: &str,
    name: &str,
) -> Result<bool> {
    let path = safe_profile_path(project_root, relative, false)?;
    let lock = lock_profile(&path)?;
    let bytes = read_regular_file(&path)?;
    let mut encrypted = parse_encrypted_dotenv(&bytes)?;
    let removed = remove_case_insensitive(&mut encrypted.variables, name);
    atomic_write(&path, &render_encrypted_dotenv(&encrypted)?)?;
    let _ = FileExt::unlock(&lock);
    Ok(removed)
}

pub fn read_encrypted_profile(
    project_root: &Path,
    relative: &str,
    identities: &[SecretString],
) -> Result<EnvironmentDocument> {
    let path = safe_profile_path(project_root, relative, false)?;
    let metadata = fs::symlink_metadata(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafePath(path));
    }
    let bytes = fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    decrypt_dotenv_document(&parse_encrypted_dotenv(&bytes)?, identities)
}

pub fn mutate_encrypted_profile<T>(
    project_root: &Path,
    relative: &str,
    identities: &[SecretString],
    recipients: &[String],
    mutation: impl FnOnce(&mut EnvironmentDocument) -> Result<T>,
) -> Result<T> {
    let path = safe_profile_path(project_root, relative, false)?;
    let lock = lock_profile(&path)?;
    let metadata = fs::symlink_metadata(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafePath(path));
    }
    let ciphertext = fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let mut document = decrypt_dotenv_document(&parse_encrypted_dotenv(&ciphertext)?, identities)?;
    let result = mutation(&mut document)?;
    let encrypted = render_encrypted_dotenv(&encrypt_dotenv_document(&document, recipients)?)?;
    atomic_write(&path, &encrypted)?;
    let _ = FileExt::unlock(&lock);
    Ok(result)
}

pub fn write_encrypted_profile(
    project_root: &Path,
    relative: &str,
    document: &EnvironmentDocument,
    recipients: &[String],
) -> Result<PathBuf> {
    let path = safe_profile_path(project_root, relative, true)?;
    let encrypted = render_encrypted_dotenv(&encrypt_dotenv_document(document, recipients)?)?;
    let lock = lock_profile(&path)?;
    atomic_write(&path, &encrypted)?;
    let _ = FileExt::unlock(&lock);
    Ok(path)
}

fn lock_profile(path: &Path) -> Result<fs::File> {
    let lock_path = path.with_file_name(format!(
        "{}.lock",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::UnsafePath(path.to_path_buf()))?
    ));
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| Error::Io {
            path: lock_path.clone(),
            source,
        })?;
    FileExt::lock(&lock).map_err(|source| Error::Io {
        path: lock_path.clone(),
        source,
    })?;
    Ok(lock)
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafePath(path.to_path_buf()));
    }
    fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn remove_case_insensitive(values: &mut BTreeMap<String, String>, name: &str) -> bool {
    let existing = values
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned();
    existing.and_then(|key| values.remove(&key)).is_some()
}

/// Atomically restores previously validated ciphertext at a project-relative profile path.
pub fn restore_encrypted_profile(
    project_root: &Path,
    relative: &str,
    ciphertext: &[u8],
) -> Result<()> {
    let path = safe_profile_path(project_root, relative, true)?;
    let lock = lock_profile(&path)?;
    atomic_write(&path, ciphertext)?;
    let _ = FileExt::unlock(&lock);
    Ok(())
}

pub fn generate_identity() -> IdentityMaterial {
    let identity = x25519::Identity::generate();
    let id = uuid::Uuid::new_v4().to_string();
    IdentityMaterial {
        record: IdentityRecord {
            id,
            recipient: identity.to_public().to_string(),
            backend: "keyring".to_owned(),
        },
        secret: identity.to_string(),
    }
}

pub fn store_identity(home: &Path, material: &IdentityMaterial) -> Result<()> {
    let entry = keyring::Entry::new(IDENTITY_SERVICE, &material.record.id)
        .map_err(|error| Error::Keyring(error.to_string()))?;
    entry
        .set_secret(material.secret.expose_secret().as_bytes())
        .map_err(|error| Error::Keyring(error.to_string()))?;
    let mut metadata = load_identity_metadata(home)?;
    metadata
        .identities
        .retain(|record| record.id != material.record.id);
    metadata.identities.push(material.record.clone());
    metadata
        .identities
        .sort_by(|left, right| left.id.cmp(&right.id));
    save_identity_metadata(home, &metadata)
}

pub fn list_identities(home: &Path) -> Result<Vec<IdentityRecord>> {
    Ok(load_identity_metadata(home)?.identities)
}

pub fn load_identity_secrets(home: &Path) -> Result<Vec<SecretString>> {
    let mut identities = Vec::new();
    let mut has_explicit_identity = false;
    if let Some(value) = env::var_os("PINSET_IDENTITY") {
        let value = value.to_string_lossy();
        for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
            identities.push(SecretString::from(line.to_owned()));
            has_explicit_identity = true;
        }
    }
    for record in load_identity_metadata(home)?.identities {
        let entry = keyring::Entry::new(IDENTITY_SERVICE, &record.id)
            .map_err(|error| Error::Keyring(error.to_string()))?;
        match entry.get_secret() {
            Ok(secret) => {
                let secret =
                    String::from_utf8(secret).map_err(|_| Error::InvalidIdentityMetadata)?;
                identities.push(SecretString::from(secret));
            }
            Err(keyring::Error::NoEntry) => {}
            Err(_) if has_explicit_identity => {}
            Err(error) => return Err(Error::Keyring(error.to_string())),
        }
    }
    Ok(identities)
}

pub fn load_identity_secret(home: &Path, id: &str) -> Result<SecretString> {
    let record = load_identity_metadata(home)?
        .identities
        .into_iter()
        .find(|record| record.id == id)
        .ok_or(Error::InvalidIdentityMetadata)?;
    let entry = keyring::Entry::new(IDENTITY_SERVICE, &record.id)
        .map_err(|error| Error::Keyring(error.to_string()))?;
    let secret = entry
        .get_secret()
        .map_err(|error| Error::Keyring(error.to_string()))?;
    String::from_utf8(secret)
        .map(SecretString::from)
        .map_err(|_| Error::InvalidIdentityMetadata)
}

pub fn trust_project(
    home: &Path,
    root: &Path,
    project_id: &str,
    environment_toml: &str,
) -> Result<()> {
    let record = TrustRecord {
        schema: 2,
        project_id: project_id.to_owned(),
        root: canonical_root(root)?.to_string_lossy().into_owned(),
        environment_fingerprint: fingerprint(environment_toml),
        directory: Some(directory_identity(root)?),
    };
    let path = trust_path(home, root)?;
    let bytes = toml::to_string_pretty(&record)
        .map_err(|_| Error::InvalidProfile("cannot serialize trust record".to_owned()))?;
    atomic_write(&path, bytes.as_bytes())
}

pub fn verify_project_trust(
    home: &Path,
    root: &Path,
    project_id: &str,
    environment_toml: &str,
) -> Result<()> {
    let path = trust_path(home, root)?;
    let metadata = fs::symlink_metadata(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::TrustMissing
        } else {
            Error::Io {
                path: path.clone(),
                source,
            }
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err(Error::UnsafePath(path));
    }
    let content = fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::TrustMissing
        } else {
            Error::Io {
                path: path.clone(),
                source,
            }
        }
    })?;
    let record: TrustRecord = toml::from_str(&content).map_err(|_| Error::TrustChanged)?;
    let canonical = canonical_root(root)?.to_string_lossy().into_owned();
    if record.schema != 2
        || record.project_id != project_id
        || record.root != canonical
        || record.directory.as_ref() != Some(&directory_identity(root)?)
        || record.environment_fingerprint != fingerprint(environment_toml)
    {
        return Err(Error::TrustChanged);
    }
    Ok(())
}

pub fn revoke_project_trust(home: &Path, root: &Path) -> Result<bool> {
    let path = trust_path(home, root)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::Io { path, source }),
    }
}

fn invalid_variable(name: &str, reason: &str) -> Error {
    Error::InvalidVariable {
        name: name.to_owned(),
        reason: reason.to_owned(),
    }
}

fn safe_profile_path(root: &Path, relative: &str, create_parent: bool) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::UnsafePath(relative.to_path_buf()));
    }
    let root = canonical_root(root)?;
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| Error::UnsafePath(path.clone()))?;
    if create_parent {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let canonical_parent = parent.canonicalize().map_err(|source| Error::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    if !canonical_parent.starts_with(&root) {
        return Err(Error::UnsafePath(path));
    }
    if path.exists() {
        let metadata = fs::symlink_metadata(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(Error::UnsafePath(path));
        }
    }
    Ok(path)
}

fn identity_metadata_path(home: &Path) -> PathBuf {
    home.join("state").join("identities.toml")
}

fn load_identity_metadata(home: &Path) -> Result<IdentityMetadata> {
    let path = identity_metadata_path(home);
    match fs::read_to_string(&path) {
        Ok(content) => {
            let metadata: IdentityMetadata =
                toml::from_str(&content).map_err(|_| Error::InvalidIdentityMetadata)?;
            if metadata.schema != 1 {
                return Err(Error::InvalidIdentityMetadata);
            }
            Ok(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(IdentityMetadata {
            schema: 1,
            identities: Vec::new(),
        }),
        Err(source) => Err(Error::Io { path, source }),
    }
}

fn save_identity_metadata(home: &Path, metadata: &IdentityMetadata) -> Result<()> {
    let path = identity_metadata_path(home);
    let content = toml::to_string_pretty(metadata).map_err(|_| Error::InvalidIdentityMetadata)?;
    atomic_write(&path, content.as_bytes())
}

fn trust_path(home: &Path, root: &Path) -> Result<PathBuf> {
    Ok(home
        .join("state/trust/v2")
        .join(format!("{}.toml", directory_identity(root)?.namespace)))
}

fn directory_identity(root: &Path) -> Result<pinset_core::WorkDirectoryIdentity> {
    pinset_core::work_directory_identity(root).map_err(|source| Error::Io {
        path: root.to_path_buf(),
        source,
    })
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize().map_err(|source| Error::Io {
        path: root.to_path_buf(),
        source,
    })
}

fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::UnsafePath(path.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(|source| Error::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.commit())
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_dotenv_keeps_names_visible_and_values_independently_encrypted() {
        let root = tempfile::tempdir().unwrap();
        let identity = x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let identity_text = identity.to_string();
        write_encrypted_profile(
            root.path(),
            ".env.development",
            &EnvironmentDocument {
                schema: PROFILE_SCHEMA,
                variables: BTreeMap::from([
                    ("DATABASE_URL".to_owned(), "postgres://secret".to_owned()),
                    ("TOKEN".to_owned(), "hidden".to_owned()),
                ]),
            },
            std::slice::from_ref(&recipient),
        )
        .unwrap();

        let ciphertext = fs::read_to_string(root.path().join(".env.development")).unwrap();
        assert!(ciphertext.starts_with(DOTENV_PROFILE_HEADER));
        assert!(ciphertext.contains("DATABASE_URL=\"encrypted:pinset:v1:"));
        assert!(ciphertext.contains("TOKEN=\"encrypted:pinset:v1:"));
        assert!(!ciphertext.contains("postgres://secret"));
        assert!(!ciphertext.contains("hidden"));
        assert_eq!(
            list_profile_names(root.path(), ".env.development").unwrap(),
            vec!["DATABASE_URL", "TOKEN"]
        );

        let before = parse_encrypted_dotenv(ciphertext.as_bytes()).unwrap();
        set_encrypted_profile_values(
            root.path(),
            ".env.development",
            std::slice::from_ref(&recipient),
            BTreeMap::from([("TOKEN".to_owned(), "changed".to_owned())]),
        )
        .unwrap();
        let after_bytes = fs::read(root.path().join(".env.development")).unwrap();
        let after = parse_encrypted_dotenv(&after_bytes).unwrap();
        assert_eq!(
            before.variables.get("DATABASE_URL"),
            after.variables.get("DATABASE_URL"),
            "an unrelated ciphertext must remain stable"
        );
        assert_ne!(before.variables.get("TOKEN"), after.variables.get("TOKEN"));

        let document = read_encrypted_profile(
            root.path(),
            ".env.development",
            std::slice::from_ref(&identity_text),
        )
        .unwrap();
        assert_eq!(document.variables["TOKEN"], "changed");
        assert_eq!(document.variables["DATABASE_URL"], "postgres://secret");
    }

    #[test]
    fn encrypted_dotenv_writes_need_only_public_recipients() {
        let root = tempfile::tempdir().unwrap();
        let identity = x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        write_encrypted_profile(
            root.path(),
            ".env.dev",
            &EnvironmentDocument::default(),
            std::slice::from_ref(&recipient),
        )
        .unwrap();

        set_encrypted_profile_values(
            root.path(),
            ".env.dev",
            std::slice::from_ref(&recipient),
            BTreeMap::from([("WRITE_ONLY".to_owned(), "secret".to_owned())]),
        )
        .unwrap();
        assert!(unset_encrypted_profile_value(root.path(), ".env.dev", "WRITE_ONLY").unwrap());
        assert!(
            list_profile_names(root.path(), ".env.dev")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn whole_file_age_profiles_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let identity = x25519::Identity::generate();
        let recipient = identity.to_public();
        let encryptor =
            Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient)).unwrap();
        let mut ciphertext = Vec::new();
        let mut writer = encryptor.wrap_output(&mut ciphertext).unwrap();
        writer
            .write_all(b"schema = 1\n[variables]\nTOKEN = \"secret\"\n")
            .unwrap();
        writer.finish().unwrap();
        fs::write(root.path().join(".env.dev"), ciphertext).unwrap();

        assert!(matches!(
            read_encrypted_profile(root.path(), ".env.dev", &[identity.to_string()]),
            Err(Error::InvalidProfile(_))
        ));
    }

    #[test]
    fn validates_portable_case_insensitive_names() {
        assert!(validate_variable_name("DATABASE_URL").is_ok());
        assert!(validate_variable_name("9BAD").is_err());
        assert!(validate_variable_name("PATH").is_err());
        assert!(validate_variable_name("PINSET_IDENTITY").is_err());
        let document = EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([
                ("Token".to_owned(), "a".to_owned()),
                ("TOKEN".to_owned(), "b".to_owned()),
            ]),
        };
        assert!(validate_document(&document).is_err());
    }

    #[test]
    fn trust_binds_root_project_and_policy_but_not_ciphertext() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        trust_project(home.path(), root.path(), "project", "policy-a").unwrap();
        assert!(verify_project_trust(home.path(), root.path(), "project", "policy-a").is_ok());
        assert!(matches!(
            verify_project_trust(home.path(), root.path(), "project", "policy-b"),
            Err(Error::TrustChanged)
        ));
    }

    #[test]
    fn rebuilt_directory_and_foreign_host_do_not_inherit_trust() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        trust_project(&home, &root, "project", "policy").unwrap();
        let path = trust_path(&home, &root).unwrap();
        let mut record: TrustRecord = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        record.directory.as_mut().unwrap().host = "another-host".into();
        fs::write(&path, toml::to_string(&record).unwrap()).unwrap();
        assert!(matches!(
            verify_project_trust(&home, &root, "project", "policy"),
            Err(Error::TrustChanged)
        ));
        trust_project(&home, &root, "project", "policy").unwrap();
        fs::rename(&root, temp.path().join("previous")).unwrap();
        fs::create_dir(&root).unwrap();
        assert!(matches!(
            verify_project_trust(&home, &root, "project", "policy"),
            Err(Error::TrustMissing)
        ));
        assert!(path.is_file(), "prior state is retained for review");
    }

    #[test]
    fn concurrent_profile_mutations_are_serialized_without_lost_updates() {
        let root = tempfile::tempdir().unwrap();
        let identity = x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let identity_text = identity.to_string();
        write_encrypted_profile(
            root.path(),
            ".env.development",
            &EnvironmentDocument::default(),
            std::slice::from_ref(&recipient),
        )
        .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for (name, value) in [("FIRST", "one"), ("SECOND", "two")] {
            let project = root.path().to_path_buf();
            let recipient = recipient.clone();
            let identity_text = identity_text.expose_secret().to_owned();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                mutate_encrypted_profile(
                    &project,
                    ".env.development",
                    &[SecretString::from(identity_text)],
                    &[recipient],
                    |document| {
                        document.variables.insert(name.to_owned(), value.to_owned());
                        Ok(())
                    },
                )
            }));
        }
        barrier.wait();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }

        let document =
            read_encrypted_profile(root.path(), ".env.development", &[identity_text]).unwrap();
        assert_eq!(
            document.variables.get("FIRST").map(String::as_str),
            Some("one")
        );
        assert_eq!(
            document.variables.get("SECOND").map(String::as_str),
            Some("two")
        );
    }
}
