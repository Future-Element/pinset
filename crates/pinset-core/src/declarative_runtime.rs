use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactIntegrity, ArtifactSource, ArtifactSourceKind, ArtifactSpec,
    DeclarativeProviderBackend, DeclarativeProviderManifest, Error, InstallAlias, InstallOutcome,
    InstallRequest, Installer, LockedArtifactFormat, LockedTool, Result,
};

pub fn install_locked_declarative_provider(
    installer: &Installer,
    home: &Path,
    manifest: &DeclarativeProviderManifest,
    registry_fingerprint: &str,
    locked: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    crate::validate_locked_provider_manifest(manifest, registry_fingerprint, locked)?;
    let DeclarativeProviderBackend::GitHubReleaseBinary {
        repository,
        tag_prefix: _,
        checksum_asset: _,
        assets,
    } = manifest
        .backend
        .as_ref()
        .ok_or_else(|| Error::DeclarativeProviderMetadataInvalid {
            reason: format!("Provider {} has no declarative backend", manifest.id),
        })?;
    let expected_asset = assets
        .get(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked.name.clone(),
            target: target.to_owned(),
        })?;
    let artifact = locked
        .artifacts
        .iter()
        .find(|artifact| artifact.target == target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked.name.clone(),
            target: target.to_owned(),
        })?;
    let expected_url = format!(
        "https://github.com/{repository}/releases/download/{}/{}",
        locked
            .metadata
            .get("tag")
            .ok_or_else(|| invalid("lock has no release tag"))?,
        expected_asset
    );
    if artifact.artifact_path != *expected_asset
        || artifact.canonical_url != expected_url
        || artifact.format != LockedArtifactFormat::Binary
        || !artifact.archive_root.is_empty()
        || artifact.verification != "https-checksum"
        || !artifact.overlays.is_empty()
    {
        return Err(invalid(
            "locked binary artifact does not match its Provider manifest",
        ));
    }
    let integrity = artifact
        .integrity
        .as_deref()
        .ok_or_else(|| invalid("locked binary artifact has no integrity identity"))?;
    let parsed = ArtifactIntegrity::parse(integrity)?;
    if parsed.algorithm().as_str() != "sha256" || parsed.cache_key() != artifact.sha256 {
        return Err(invalid(
            "locked binary artifact SHA-256 identity is inconsistent",
        ));
    }
    let command = manifest
        .commands
        .first()
        .ok_or_else(|| invalid("Provider has no command"))?;
    let destination = if cfg!(windows) {
        PathBuf::from(format!("{command}.exe"))
    } else {
        PathBuf::from(command)
    };
    installer.install(&InstallRequest {
        pinset_home: home.to_path_buf(),
        tool: locked.name.clone(),
        version: locked.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "official".to_owned(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: integrity.to_owned(),
            format: ArtifactFormat::Binary,
        },
        strip_components: 0,
        include_prefixes: Vec::new(),
        required_paths: vec![PathBuf::from("binary")],
        base_artifacts: Vec::new(),
        executable_paths: vec![PathBuf::from("binary")],
        aliases: vec![InstallAlias {
            source: PathBuf::from("binary"),
            destination,
        }],
    })
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::DeclarativeProviderMetadataInvalid {
        reason: reason.into(),
    }
}
