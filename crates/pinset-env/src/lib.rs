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
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const PROFILE_SCHEMA: u32 = 1;
pub const PROFILE_MAX_BYTES: usize = 1024 * 1024;
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

pub fn encrypt_document(
    document: &EnvironmentDocument,
    recipient_strings: &[String],
) -> Result<Vec<u8>> {
    validate_document(document)?;
    if recipient_strings.is_empty() {
        return Err(Error::MissingRecipient);
    }
    let recipients = recipient_strings
        .iter()
        .map(|recipient| {
            recipient
                .parse::<x25519::Recipient>()
                .map_err(|_| Error::InvalidRecipient)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut plaintext = toml::to_string(document)
        .map_err(|_| Error::InvalidProfile("cannot serialize profile".to_owned()))?;
    let result = (|| {
        if plaintext.len() > PROFILE_MAX_BYTES {
            return Err(Error::ProfileTooLarge);
        }
        let encryptor = Encryptor::with_recipients(
            recipients
                .iter()
                .map(|recipient| recipient as &dyn age::Recipient),
        )
        .map_err(|_| Error::Crypto)?;
        let mut output = Vec::with_capacity(plaintext.len() + 512);
        let mut writer = encryptor
            .wrap_output(&mut output)
            .map_err(|_| Error::Crypto)?;
        writer
            .write_all(plaintext.as_bytes())
            .and_then(|()| writer.finish())
            .map_err(|_| Error::Crypto)?;
        Ok(output)
    })();
    plaintext.zeroize();
    result
}

pub fn decrypt_document(
    ciphertext: &[u8],
    identity_strings: &[SecretString],
) -> Result<EnvironmentDocument> {
    if ciphertext.len() > PROFILE_MAX_BYTES + 64 * 1024 {
        return Err(Error::ProfileTooLarge);
    }
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
    let decryptor = Decryptor::new_buffered(ciphertext).map_err(|_| Error::Crypto)?;
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
    let document = std::str::from_utf8(&plaintext)
        .map_err(|_| Error::InvalidProfile("plaintext is not UTF-8".to_owned()))
        .and_then(|plaintext_text| {
            toml::from_str::<EnvironmentDocument>(plaintext_text)
                .map_err(|_| Error::InvalidProfile("plaintext is not canonical TOML".to_owned()))
        });
    plaintext.zeroize();
    let document = document?;
    validate_document(&document)?;
    Ok(document)
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
    decrypt_document(&bytes, identities)
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
    let mut document = decrypt_document(&ciphertext, identities)?;
    let result = mutation(&mut document)?;
    let encrypted = encrypt_document(&document, recipients)?;
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
    let encrypted = encrypt_document(document, recipients)?;
    let lock = lock_profile(&path)?;
    atomic_write(&path, &encrypted)?;
    let _ = FileExt::unlock(&lock);
    Ok(path)
}

fn lock_profile(path: &Path) -> Result<fs::File> {
    let lock_path = path.with_extension("age.lock");
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

pub fn import_identity(home: &Path, secret: SecretString) -> Result<IdentityRecord> {
    let identity = secret
        .expose_secret()
        .trim()
        .parse::<x25519::Identity>()
        .map_err(|_| Error::NoMatchingIdentity)?;
    let material = IdentityMaterial {
        record: IdentityRecord {
            id: uuid::Uuid::new_v4().to_string(),
            recipient: identity.to_public().to_string(),
            backend: "keyring".to_owned(),
        },
        secret,
    };
    store_identity(home, &material)?;
    Ok(material.record)
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

pub fn backup_identity(path: &Path, secret: &SecretString, passphrase: SecretString) -> Result<()> {
    if path.exists() {
        return Err(Error::UnsafePath(path.to_path_buf()));
    }
    let encrypted = age::encrypt(
        &age::scrypt::Recipient::new(passphrase),
        secret.expose_secret().as_bytes(),
    )
    .map_err(|_| Error::Crypto)?;
    atomic_write(path, &encrypted)
}

pub fn restore_identity(path: &Path, passphrase: SecretString) -> Result<SecretString> {
    let ciphertext = fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let plaintext = age::decrypt(&age::scrypt::Identity::new(passphrase), &ciphertext)
        .map_err(|_| Error::Crypto)?;
    let value = String::from_utf8(plaintext).map_err(|_| Error::Crypto)?;
    value
        .trim()
        .parse::<x25519::Identity>()
        .map_err(|_| Error::Crypto)?;
    Ok(SecretString::from(value.trim().to_owned()))
}

pub fn trust_project(
    home: &Path,
    root: &Path,
    project_id: &str,
    environment_toml: &str,
) -> Result<()> {
    let record = TrustRecord {
        schema: 1,
        project_id: project_id.to_owned(),
        root: canonical_root(root)?.to_string_lossy().into_owned(),
        environment_fingerprint: fingerprint(environment_toml),
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
    if record.schema != 1
        || record.project_id != project_id
        || record.root != canonical
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
    let canonical = canonical_root(root)?;
    Ok(home.join("state").join("trust").join(format!(
        "{}.toml",
        fingerprint(&canonical.to_string_lossy())
    )))
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
    fn encrypts_for_multiple_recipients_and_rejects_another_identity() {
        let one = x25519::Identity::generate();
        let two = x25519::Identity::generate();
        let other = x25519::Identity::generate();
        let document = EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
        };
        let encrypted = encrypt_document(
            &document,
            &[one.to_public().to_string(), two.to_public().to_string()],
        )
        .unwrap();
        for identity in [one, two] {
            let decrypted = decrypt_document(&encrypted, &[identity.to_string()]).unwrap();
            assert_eq!(decrypted, document);
        }
        assert!(decrypt_document(&encrypted, &[other.to_string()]).is_err());
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
    fn concurrent_profile_mutations_are_serialized_without_lost_updates() {
        let root = tempfile::tempdir().unwrap();
        let identity = x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let identity_text = identity.to_string();
        write_encrypted_profile(
            root.path(),
            "pinset.env/development.age",
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
                    "pinset.env/development.age",
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
            read_encrypted_profile(root.path(), "pinset.env/development.age", &[identity_text])
                .unwrap();
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
