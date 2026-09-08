//! Verification and validation for the constrained declarative Provider Registry preview.
//!
//! A registry document may describe capabilities and dependencies, but it cannot contain scripts,
//! hooks, environment code, or executable templates. Activation remains a Pinset release decision;
//! verifying a third-party manifest never executes it or changes local state.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use pgp::{
    composed::{CleartextSignedMessage, Deserializable, SignedPublicKey},
    types::KeyDetails,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, VerificationMethod};

const PROVIDER_REGISTRY_SCHEMA: u32 = 2;
const MAX_PROVIDER_REGISTRY_BYTES: u64 = 256 * 1024;
#[cfg(test)]
const OFFICIAL_REGISTRY_JSON: &str = include_str!("../../../registry/providers.json");
const OFFICIAL_REGISTRY: &str = include_str!("../../../registry/providers.json.asc");
const OFFICIAL_REGISTRY_KEY: &str =
    include_str!("../../../registry/pinset-provider-registry-key.asc");
const OFFICIAL_REGISTRY_FINGERPRINT: &str = "344588BBBFCC111E8FA61D82D63D8DE4D3B15A4B";
pub const PROVIDER_REGISTRY_FILENAME: &str = "provider-registry.asc";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProviderRegistryDocument {
    pub schema: u32,
    pub registry: String,
    pub generated_at: String,
    pub providers: Vec<DeclarativeProviderManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeclarativeProviderManifest {
    pub id: String,
    pub tool: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default = "default_revision")]
    pub revision: u32,
    #[serde(default)]
    pub disabled: bool,
    pub capabilities: DeclarativeProviderCapabilities,
    #[serde(default)]
    pub backend: Option<DeclarativeProviderBackend>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", tag = "kind")]
pub enum DeclarativeProviderBackend {
    #[serde(rename = "github-release-binary")]
    GitHubReleaseBinary {
        repository: String,
        #[serde(rename = "tag-prefix")]
        tag_prefix: String,
        #[serde(rename = "checksum-asset")]
        checksum_asset: String,
        assets: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeclarativeProviderCapabilities {
    pub command_layout: String,
    pub metadata: String,
    pub installer: String,
    pub environment: String,
    pub lock_audit: String,
    pub provenance: DeclarativeProvenanceCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeclarativeProvenanceCapabilities {
    pub methods: Vec<VerificationMethod>,
    pub release_time: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifiedProviderRegistry {
    pub signer_fingerprint: String,
    pub document: ProviderRegistryDocument,
}

pub fn embedded_provider_registry() -> Result<VerifiedProviderRegistry> {
    let verified = verify_signed_provider_registry(OFFICIAL_REGISTRY)?;
    validate_embedded_builtin_declarations(&verified.document)?;
    Ok(verified)
}

pub fn provider_registry_path(home: &Path) -> PathBuf {
    home.join("config").join(PROVIDER_REGISTRY_FILENAME)
}

pub fn effective_provider_registry(home: &Path) -> Result<VerifiedProviderRegistry> {
    let path = provider_registry_path(home);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let verified = load_signed_provider_registry(&path)?;
            validate_runtime_provider_declarations(&verified.document)?;
            Ok(verified)
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            embedded_provider_registry()
        }
        Err(source) => Err(Error::ReadProviderRegistry { path, source }),
    }
}

#[cfg(feature = "lockfile")]
pub fn validate_locked_declarative_provider(home: &Path, locked: &crate::LockedTool) -> Result<()> {
    let registry = effective_provider_registry(home)?;
    let manifest = registry
        .document
        .providers
        .iter()
        .find(|manifest| manifest.tool == locked.name)
        .ok_or_else(|| Error::ProviderRegistryInvalid {
            reason: format!("active registry has no Provider for {}", locked.name),
        })?;
    validate_locked_provider_manifest(manifest, &registry.signer_fingerprint, locked)
}

#[cfg(feature = "lockfile")]
pub fn validate_locked_provider_manifest(
    manifest: &DeclarativeProviderManifest,
    registry_fingerprint: &str,
    locked: &crate::LockedTool,
) -> Result<()> {
    if manifest.disabled {
        return invalid(format!(
            "Provider {} revision {} is disabled",
            manifest.id, manifest.revision
        ));
    }
    let revision = manifest.revision.to_string();
    if locked.name != manifest.tool
        || locked.provider != "declarative-github-release"
        || locked.metadata.get("provider-id") != Some(&manifest.id)
        || locked.metadata.get("provider-revision").map(String::as_str) != Some(revision.as_str())
        || locked
            .metadata
            .get("registry-fingerprint")
            .map(String::as_str)
            != Some(registry_fingerprint)
    {
        return invalid(format!(
            "lock does not match active Provider {} revision {}",
            manifest.id, manifest.revision
        ));
    }
    Ok(())
}

pub fn load_signed_provider_registry(path: &Path) -> Result<VerifiedProviderRegistry> {
    let metadata = fs::symlink_metadata(path).map_err(|source| Error::ReadProviderRegistry {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(Error::ProviderRegistryInvalid {
            reason: format!("{} is not a regular file", path.display()),
        });
    }
    if metadata.len() > MAX_PROVIDER_REGISTRY_BYTES {
        return Err(Error::ProviderRegistryInvalid {
            reason: format!(
                "{} exceeds the {MAX_PROVIDER_REGISTRY_BYTES}-byte limit",
                path.display()
            ),
        });
    }
    let content = fs::read_to_string(path).map_err(|source| Error::ReadProviderRegistry {
        path: path.to_path_buf(),
        source,
    })?;
    verify_signed_provider_registry(&content)
}

pub fn validate_provider_registry_json(input: &str) -> Result<ProviderRegistryDocument> {
    let document: ProviderRegistryDocument =
        serde_json::from_str(input).map_err(|source| Error::ProviderRegistryInvalid {
            reason: format!("payload is not a valid registry document: {source}"),
        })?;
    validate_registry_document(&document)?;
    Ok(document)
}

pub fn verify_signed_provider_registry(input: &str) -> Result<VerifiedProviderRegistry> {
    if input.len() as u64 > MAX_PROVIDER_REGISTRY_BYTES {
        return Err(Error::ProviderRegistryInvalid {
            reason: format!("registry exceeds the {MAX_PROVIDER_REGISTRY_BYTES}-byte limit"),
        });
    }
    let (message, _) = CleartextSignedMessage::from_string(input).map_err(|source| {
        Error::ProviderRegistrySignatureInvalid {
            reason: format!("cannot parse clear-signed registry: {source}"),
        }
    })?;
    if message.signatures().len() != 1 {
        return Err(Error::ProviderRegistrySignatureInvalid {
            reason: "registry must contain exactly one signature".to_owned(),
        });
    }

    let (key, _) = SignedPublicKey::from_reader_single(OFFICIAL_REGISTRY_KEY.as_bytes()).map_err(
        |source| Error::ProviderRegistrySignatureInvalid {
            reason: format!("cannot parse embedded Provider Registry key: {source}"),
        },
    )?;
    let fingerprint = hex::encode_upper(key.fingerprint().as_bytes());
    if fingerprint != OFFICIAL_REGISTRY_FINGERPRINT {
        return Err(Error::ProviderRegistrySignatureInvalid {
            reason: "embedded Provider Registry key does not match its pinned fingerprint"
                .to_owned(),
        });
    }
    for subkey in &key.public_subkeys {
        subkey.verify_bindings(&key.primary_key).map_err(|source| {
            Error::ProviderRegistrySignatureInvalid {
                reason: format!("Provider Registry signing subkey is invalid: {source}"),
            }
        })?;
    }
    let valid = message.verify(&key.primary_key).is_ok()
        || key
            .public_subkeys
            .iter()
            .any(|subkey| message.verify(subkey).is_ok());
    if !valid {
        return Err(Error::ProviderRegistrySignatureInvalid {
            reason: "cryptographic verification failed for the pinned registry signer".to_owned(),
        });
    }

    let document: ProviderRegistryDocument =
        serde_json::from_str(&message.signed_text()).map_err(|source| {
            Error::ProviderRegistryInvalid {
                reason: format!("signed payload is not a valid registry document: {source}"),
            }
        })?;
    validate_registry_document(&document)?;
    Ok(VerifiedProviderRegistry {
        signer_fingerprint: fingerprint,
        document,
    })
}

fn validate_registry_document(document: &ProviderRegistryDocument) -> Result<()> {
    if !matches!(document.schema, 1 | PROVIDER_REGISTRY_SCHEMA) {
        return invalid(format!(
            "unsupported registry schema {}; expected {PROVIDER_REGISTRY_SCHEMA}",
            document.schema
        ));
    }
    if document.registry.is_empty() || document.registry.len() > 128 {
        return invalid("registry identity must contain 1 to 128 characters");
    }
    if !crate::valid_release_time(&document.generated_at) {
        return invalid("generated-at must be a valid RFC 3339 timestamp");
    }
    if document.providers.is_empty() || document.providers.len() > 256 {
        return invalid("registry must contain 1 to 256 Provider manifests");
    }

    let mut ids = BTreeSet::new();
    let mut tools = BTreeSet::new();
    let mut commands = BTreeSet::new();
    for provider in &document.providers {
        if !valid_provider_id(&provider.id) || !ids.insert(provider.id.as_str()) {
            return invalid(format!(
                "invalid or duplicate Provider id {:?}",
                provider.id
            ));
        }
        if provider.revision == 0 {
            return invalid(format!(
                "Provider {} revision must be positive",
                provider.id
            ));
        }
        if !valid_name(&provider.tool) || !tools.insert(provider.tool.as_str()) {
            return invalid(format!(
                "invalid or duplicate tool name {:?}",
                provider.tool
            ));
        }
        if provider.commands.is_empty() || provider.commands.len() > 64 {
            return invalid(format!(
                "Provider {} must declare 1 to 64 commands",
                provider.id
            ));
        }
        for command in &provider.commands {
            if !valid_name(command) || !commands.insert(command.as_str()) {
                return invalid(format!("invalid or duplicate command {command:?}"));
            }
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &provider.dependencies {
            if !valid_name(dependency)
                || dependency == &provider.tool
                || !dependencies.insert(dependency.as_str())
            {
                return invalid(format!(
                    "Provider {} has an invalid dependency {dependency:?}",
                    provider.id
                ));
            }
        }
        validate_capabilities(provider)?;
    }

    let by_tool = document
        .providers
        .iter()
        .map(|provider| (provider.tool.as_str(), provider))
        .collect::<BTreeMap<_, _>>();
    for provider in &document.providers {
        for dependency in &provider.dependencies {
            if !by_tool.contains_key(dependency.as_str()) {
                return invalid(format!(
                    "Provider {} depends on unknown tool {dependency:?}",
                    provider.id
                ));
            }
        }
    }
    validate_dependency_graph(&by_tool)
}

fn validate_capabilities(provider: &DeclarativeProviderManifest) -> Result<()> {
    let capabilities = &provider.capabilities;
    if !matches!(
        capabilities.command_layout.as_str(),
        "node-native" | "python" | "java" | "root" | "bin"
    ) {
        return invalid(format!(
            "Provider {} declares unsupported command layout {:?}",
            provider.id, capabilities.command_layout
        ));
    }
    let builtin_backend = matches!(
        capabilities.metadata.as_str(),
        "node" | "npm" | "go" | "flutter" | "java" | "python" | "rust" | "dotnet"
    );
    let declarative_backend = capabilities.metadata == "github-release-binary";
    if (!builtin_backend && !declarative_backend) || capabilities.metadata != capabilities.installer
    {
        return invalid(format!(
            "Provider {} cannot bind unimplemented metadata/installer capabilities",
            provider.id
        ));
    }
    match (&provider.backend, declarative_backend) {
        (None, false) => {}
        (
            Some(DeclarativeProviderBackend::GitHubReleaseBinary {
                repository,
                tag_prefix,
                checksum_asset,
                assets,
            }),
            true,
        ) => {
            validate_github_binary_backend(
                provider,
                repository,
                tag_prefix,
                checksum_asset,
                assets,
            )?;
        }
        _ => {
            return invalid(format!(
                "Provider {} backend does not match its declared capabilities",
                provider.id
            ));
        }
    }
    if !matches!(
        capabilities.environment.as_str(),
        "none" | "go" | "flutter" | "java" | "python" | "dotnet"
    ) || capabilities.lock_audit != "artifact-receipt"
    {
        return invalid(format!(
            "Provider {} cannot bypass the shared environment or lock-audit contract",
            provider.id
        ));
    }
    if capabilities.provenance.methods.is_empty() {
        return invalid(format!(
            "Provider {} must declare at least one verification method",
            provider.id
        ));
    }
    let unique = capabilities
        .provenance
        .methods
        .iter()
        .collect::<BTreeSet<_>>();
    if unique.len() != capabilities.provenance.methods.len() {
        return invalid(format!(
            "Provider {} repeats a verification method",
            provider.id
        ));
    }
    Ok(())
}

fn validate_github_binary_backend(
    provider: &DeclarativeProviderManifest,
    repository: &str,
    tag_prefix: &str,
    checksum_asset: &str,
    assets: &BTreeMap<String, String>,
) -> Result<()> {
    if !valid_repository(repository)
        || tag_prefix.len() > 32
        || !tag_prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !valid_asset_name(checksum_asset)
    {
        return invalid(format!(
            "Provider {} has an invalid GitHub backend",
            provider.id
        ));
    }
    let required = [
        "windows-x86_64",
        "macos-aarch64",
        "macos-x86_64",
        "linux-x86_64",
        "linux-aarch64",
    ];
    if assets.len() != required.len()
        || required.iter().any(|target| {
            assets
                .get(*target)
                .is_none_or(|asset| !valid_asset_name(asset))
        })
    {
        return invalid(format!(
            "Provider {} must map every supported target to one safe binary asset",
            provider.id
        ));
    }
    Ok(())
}

fn validate_dependency_graph(
    providers: &BTreeMap<&str, &DeclarativeProviderManifest>,
) -> Result<()> {
    let mut visiting = Vec::new();
    let mut visited = BTreeSet::new();
    for tool in providers.keys() {
        visit_manifest(tool, providers, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_manifest<'a>(
    tool: &'a str,
    providers: &BTreeMap<&'a str, &'a DeclarativeProviderManifest>,
    visiting: &mut Vec<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if visited.contains(tool) {
        return Ok(());
    }
    if let Some(position) = visiting.iter().position(|candidate| *candidate == tool) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(tool);
        return Err(Error::ProviderDependencyCycle {
            cycle: cycle.join(" -> "),
        });
    }
    visiting.push(tool);
    for dependency in &providers[tool].dependencies {
        visit_manifest(dependency, providers, visiting, visited)?;
    }
    visiting.pop();
    visited.insert(tool);
    Ok(())
}

pub fn validate_runtime_provider_declarations(document: &ProviderRegistryDocument) -> Result<()> {
    if document.providers.len() != crate::runtime_providers().len() {
        return invalid("embedded registry does not declare every built-in Provider");
    }
    for provider in crate::runtime_providers() {
        let manifest = document
            .providers
            .iter()
            .find(|manifest| manifest.tool == provider.tool)
            .ok_or_else(|| Error::ProviderRegistryInvalid {
                reason: format!("embedded registry is missing Provider {}", provider.tool),
            })?;
        let commands = provider.commands.to_vec();
        let dependencies = provider.dependencies.to_vec();
        let methods = provider.capabilities.provenance.methods.to_vec();
        let command_layout = match provider.capabilities.command_layout {
            crate::RuntimeCommandLayout::NodeNative => "node-native",
            crate::RuntimeCommandLayout::Python => "python",
            crate::RuntimeCommandLayout::Java => "java",
            crate::RuntimeCommandLayout::Root => "root",
            crate::RuntimeCommandLayout::Bin => "bin",
        };
        let backend = match provider.capabilities.metadata {
            crate::RuntimeMetadataKind::Node => "node",
            crate::RuntimeMetadataKind::Npm => "npm",
            crate::RuntimeMetadataKind::Go => "go",
            crate::RuntimeMetadataKind::Flutter => "flutter",
            crate::RuntimeMetadataKind::Java => "java",
            crate::RuntimeMetadataKind::Python => "python",
            crate::RuntimeMetadataKind::Rust => "rust",
            crate::RuntimeMetadataKind::Dotnet => "dotnet",
            crate::RuntimeMetadataKind::Declarative => "github-release-binary",
        };
        let installer = match provider.capabilities.installer {
            crate::RuntimeInstallKind::Node => "node",
            crate::RuntimeInstallKind::Npm => "npm",
            crate::RuntimeInstallKind::Go => "go",
            crate::RuntimeInstallKind::Flutter => "flutter",
            crate::RuntimeInstallKind::Java => "java",
            crate::RuntimeInstallKind::Python => "python",
            crate::RuntimeInstallKind::Rust => "rust",
            crate::RuntimeInstallKind::Dotnet => "dotnet",
            crate::RuntimeInstallKind::Declarative => "github-release-binary",
        };
        let environment = match provider.capabilities.environment {
            crate::RuntimeEnvironmentKind::None => "none",
            crate::RuntimeEnvironmentKind::Go => "go",
            crate::RuntimeEnvironmentKind::Flutter => "flutter",
            crate::RuntimeEnvironmentKind::Java => "java",
            crate::RuntimeEnvironmentKind::Python => "python",
            crate::RuntimeEnvironmentKind::Dotnet => "dotnet",
        };
        if manifest
            .commands
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != commands
            || manifest
                .dependencies
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != dependencies
            || manifest.capabilities.command_layout != command_layout
            || manifest.capabilities.metadata != backend
            || manifest.capabilities.installer != installer
            || manifest.capabilities.environment != environment
            || manifest.capabilities.lock_audit != "artifact-receipt"
            || manifest.capabilities.provenance.methods != methods
            || manifest.capabilities.provenance.release_time
                != provider.capabilities.provenance.release_time
        {
            return invalid(format!(
                "embedded registry declaration for {} has drifted from the binary",
                provider.tool
            ));
        }
    }
    Ok(())
}

fn validate_embedded_builtin_declarations(document: &ProviderRegistryDocument) -> Result<()> {
    validate_runtime_provider_declarations(document)
}

fn default_revision() -> u32 {
    1
}

fn valid_provider_id(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(name), None) if valid_name(owner) && valid_name(name))
}

fn valid_repository(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(name), None) if valid_name(owner) && valid_name(name))
}

fn valid_asset_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn invalid<T>(reason: impl Into<String>) -> Result<T> {
    Err(Error::ProviderRegistryInvalid {
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_is_signed_and_matches_builtin_capabilities() {
        let verified = embedded_provider_registry().expect("embedded registry");
        let source: ProviderRegistryDocument =
            serde_json::from_str(OFFICIAL_REGISTRY_JSON).expect("registry source JSON");
        assert_eq!(verified.signer_fingerprint, OFFICIAL_REGISTRY_FINGERPRINT);
        assert_eq!(verified.document, source);
        assert_eq!(verified.document.providers.len(), 10);
        assert_eq!(
            verified
                .document
                .providers
                .iter()
                .find(|provider| provider.tool == "pnpm")
                .expect("pnpm")
                .dependencies,
            ["node"]
        );
    }

    #[test]
    fn registry_signature_rejects_tampering_and_unsigned_json() {
        let tampered = OFFICIAL_REGISTRY.replacen("pinset/node", "pinset/n0de", 1);
        assert!(matches!(
            verify_signed_provider_registry(&tampered),
            Err(Error::ProviderRegistrySignatureInvalid { .. })
        ));
        assert!(matches!(
            verify_signed_provider_registry("{\"schema\":1}"),
            Err(Error::ProviderRegistrySignatureInvalid { .. })
        ));
    }

    #[test]
    fn registry_dependency_cycles_are_rejected() {
        let mut document = embedded_provider_registry()
            .expect("embedded registry")
            .document;
        document
            .providers
            .iter_mut()
            .find(|provider| provider.tool == "node")
            .expect("node")
            .dependencies
            .push("pnpm".to_owned());
        assert!(matches!(
            validate_registry_document(&document),
            Err(Error::ProviderDependencyCycle { .. })
        ));
    }

    #[cfg(feature = "lockfile")]
    #[test]
    fn active_manifest_revision_and_disable_state_bind_existing_locks() {
        let verified = embedded_provider_registry().expect("embedded registry");
        let mut manifest = verified
            .document
            .providers
            .iter()
            .find(|provider| provider.tool == "jq")
            .expect("jq")
            .clone();
        let locked = crate::LockedTool {
            name: "jq".to_owned(),
            requested: "1.8".to_owned(),
            version: "1.8.2".to_owned(),
            provider: "declarative-github-release".to_owned(),
            released_at: Some("2026-06-20T14:11:27Z".to_owned()),
            metadata: BTreeMap::from([
                ("provider-id".to_owned(), manifest.id.clone()),
                (
                    "provider-revision".to_owned(),
                    manifest.revision.to_string(),
                ),
                (
                    "registry-fingerprint".to_owned(),
                    verified.signer_fingerprint.clone(),
                ),
                ("repository".to_owned(), "jqlang/jq".to_owned()),
                ("tag".to_owned(), "jq-1.8.2".to_owned()),
            ]),
            options: BTreeMap::new(),
            artifacts: Vec::new(),
        };
        validate_locked_provider_manifest(&manifest, &verified.signer_fingerprint, &locked)
            .expect("matching manifest");

        manifest.revision += 1;
        assert!(
            validate_locked_provider_manifest(&manifest, &verified.signer_fingerprint, &locked)
                .is_err()
        );
        manifest.revision -= 1;
        manifest.disabled = true;
        assert!(
            validate_locked_provider_manifest(&manifest, &verified.signer_fingerprint, &locked)
                .is_err()
        );
    }
}
