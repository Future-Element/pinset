//! Verification and validation for the constrained declarative Provider Registry preview.
//!
//! A registry document may describe capabilities and dependencies, but it cannot contain scripts,
//! hooks, environment code, or executable templates. Activation remains a Pinset release decision;
//! verifying a third-party manifest never executes it or changes local state.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use pgp::{
    composed::{CleartextSignedMessage, Deserializable, SignedPublicKey},
    types::KeyDetails,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, VerificationMethod};

const PROVIDER_REGISTRY_SCHEMA: u32 = 1;
const MAX_PROVIDER_REGISTRY_BYTES: u64 = 256 * 1024;
#[cfg(test)]
const OFFICIAL_REGISTRY_JSON: &str = include_str!("../../../registry/providers.json");
const OFFICIAL_REGISTRY: &str = include_str!("../../../registry/providers.json.asc");
const OFFICIAL_REGISTRY_KEY: &str =
    include_str!("../../../registry/pinset-provider-registry-key.asc");
const OFFICIAL_REGISTRY_FINGERPRINT: &str = "344588BBBFCC111E8FA61D82D63D8DE4D3B15A4B";

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
    pub capabilities: DeclarativeProviderCapabilities,
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
    if document.schema != PROVIDER_REGISTRY_SCHEMA {
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
    if !matches!(
        capabilities.metadata.as_str(),
        "node" | "npm" | "go" | "flutter" | "java" | "python" | "rust" | "dotnet"
    ) || capabilities.metadata != capabilities.installer
        && !(capabilities.metadata == "npm" && capabilities.installer == "npm")
    {
        return invalid(format!(
            "Provider {} cannot bind unimplemented metadata/installer capabilities",
            provider.id
        ));
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

fn validate_embedded_builtin_declarations(document: &ProviderRegistryDocument) -> Result<()> {
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

fn valid_provider_id(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(name), None) if valid_name(owner) && valid_name(name))
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
        assert_eq!(verified.document.providers.len(), 9);
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
}
