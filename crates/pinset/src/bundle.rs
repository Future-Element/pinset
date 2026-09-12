use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Error as IoError, ErrorKind},
    path::{Component, Path},
};

use atomic_write_file::AtomicWriteFile;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use pinset_core::{
    ArtifactIntegrity, import_download_cache_with_integrity, load_lockfile, verify_download_cache,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, Header};
use tempfile::tempdir;

const BUNDLE_SCHEMA: u32 = 1;
const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleManifest {
    schema: u32,
    target: String,
    lock_sha256: String,
    artifacts: Vec<BundleArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleArtifact {
    tool: String,
    integrity: String,
    path: String,
    bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundleOutcome {
    pub schema: u32,
    pub target: String,
    pub artifacts: usize,
    pub bytes: u64,
}

pub fn export(
    home: &Path,
    lock_path: &Path,
    output: &Path,
    target: &str,
) -> Result<BundleOutcome, Box<dyn std::error::Error>> {
    let lock_bytes = fs::read(lock_path)?;
    let lock = load_lockfile(lock_path)?;
    let verification = verify_download_cache(home)?;
    if verification.corrupt > 0 {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "cannot export bundle with {} corrupt cache artifact(s)",
                verification.corrupt
            ),
        )
        .into());
    }
    let mut artifacts = BTreeMap::<String, BundleArtifact>::new();
    for tool in &lock.tools {
        let selected = pinset_core::artifacts_for_platform(tool, target);
        if selected.is_empty() {
            return Err(format!(
                "bundle is incomplete: {} has no locked artifact for {target}",
                tool.name
            )
            .into());
        }
        for artifact in selected {
            let mut identities = vec![artifact.artifact_integrity()?];
            for overlay in &artifact.overlays {
                identities.push(overlay.artifact_integrity()?);
            }
            for identity in identities {
                let canonical = identity.canonical();
                let cache = home
                    .join("downloads")
                    .join(identity.algorithm().as_str())
                    .join(format!("{}.archive", identity.cache_key()));
                let metadata = fs::symlink_metadata(&cache).map_err(|error| {
                    IoError::new(
                        error.kind(),
                        format!("bundle is missing cached artifact {canonical}"),
                    )
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(IoError::new(
                        ErrorKind::InvalidData,
                        "bundle source must be a regular cache file",
                    )
                    .into());
                }
                artifacts
                    .entry(canonical.clone())
                    .or_insert(BundleArtifact {
                        tool: tool.name.clone(),
                        integrity: canonical,
                        path: format!(
                            "artifacts/{}/{}.archive",
                            identity.algorithm().as_str(),
                            identity.cache_key()
                        ),
                        bytes: metadata.len(),
                    });
            }
        }
    }
    let artifacts = artifacts.into_values().collect::<Vec<_>>();
    if artifacts.is_empty() {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!("lock has no artifacts for target {target}"),
        )
        .into());
    }
    let manifest = BundleManifest {
        schema: BUNDLE_SCHEMA,
        target: target.to_owned(),
        lock_sha256: hex::encode(Sha256::digest(&lock_bytes)),
        artifacts,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let parent = output
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let encoder = GzEncoder::new(temporary.reopen()?, Compression::default());
        let mut archive = Builder::new(encoder);
        append_bytes(&mut archive, "manifest.json", &manifest_bytes)?;
        append_bytes(&mut archive, "pinset.lock", &lock_bytes)?;
        for artifact in &manifest.artifacts {
            let identity = ArtifactIntegrity::parse(&artifact.integrity)?;
            let path = home
                .join("downloads")
                .join(identity.algorithm().as_str())
                .join(format!("{}.archive", identity.cache_key()));
            archive.append_path_with_name(path, &artifact.path)?;
        }
        archive.into_inner()?.finish()?;
    }
    persist_regular(temporary.path(), output)?;
    Ok(BundleOutcome {
        schema: BUNDLE_SCHEMA,
        target: target.to_owned(),
        artifacts: manifest.artifacts.len(),
        bytes: manifest.artifacts.iter().map(|value| value.bytes).sum(),
    })
}

pub fn import(
    home: &Path,
    bundle: &Path,
    expected_target: &str,
) -> Result<BundleOutcome, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(bundle)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_BUNDLE_BYTES
    {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "bundle must be a regular file no larger than 16 GiB",
        )
        .into());
    }
    let staging = tempdir()?;
    let mut archive = Archive::new(GzDecoder::new(File::open(bundle)?));
    let mut seen = BTreeSet::new();
    let mut expanded_bytes = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        validate_entry_path(&path)?;
        expanded_bytes = expanded_bytes
            .checked_add(entry.size())
            .ok_or("bundle size overflow")?;
        if expanded_bytes > MAX_BUNDLE_BYTES
            || seen.len() >= 4096
            || ((path == Path::new("manifest.json") || path == Path::new("pinset.lock"))
                && entry.size() > MAX_MANIFEST_BYTES)
        {
            return Err("bundle expanded content exceeds its size or entry limit".into());
        }
        if !seen.insert(path.clone()) {
            return Err(
                IoError::new(ErrorKind::InvalidData, "bundle contains duplicate entries").into(),
            );
        }
        let destination = staging.path().join(&path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "bundle entries must be regular files",
            )
            .into());
        }
        entry.unpack(&destination)?;
    }
    let manifest_path = staging.path().join("manifest.json");
    let metadata = fs::metadata(&manifest_path)?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(IoError::new(ErrorKind::InvalidData, "bundle manifest exceeds 1 MiB").into());
    }
    let manifest: BundleManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    if manifest.schema != BUNDLE_SCHEMA {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("unsupported bundle schema {}", manifest.schema),
        )
        .into());
    }
    if manifest.target != expected_target {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!(
                "bundle target {} does not match {expected_target}",
                manifest.target
            ),
        )
        .into());
    }
    let lock_bytes = fs::read(staging.path().join("pinset.lock"))?;
    let lock = load_lockfile(&staging.path().join("pinset.lock"))?;
    let actual_lock = hex::encode(Sha256::digest(lock_bytes));
    if actual_lock != manifest.lock_sha256 {
        return Err(IoError::new(ErrorKind::InvalidData, "bundle lock identity mismatch").into());
    }
    let mut required = BTreeSet::new();
    for tool in &lock.tools {
        let selected = pinset_core::artifacts_for_platform(tool, expected_target);
        if selected.is_empty() {
            return Err("bundle lock does not cover every tool for its target".into());
        }
        for artifact in selected {
            required.insert(artifact.artifact_integrity()?.canonical());
            for overlay in &artifact.overlays {
                required.insert(overlay.artifact_integrity()?.canonical());
            }
        }
    }
    let supplied = manifest
        .artifacts
        .iter()
        .map(|artifact| {
            ArtifactIntegrity::parse(&artifact.integrity).map(|value| value.canonical())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if required.is_empty() || supplied != required || supplied.len() != manifest.artifacts.len() {
        return Err(
            "bundle manifest must contain exactly the complete locked target artifact set".into(),
        );
    }
    let listed = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.path.as_str())
        .chain(["manifest.json", "pinset.lock"])
        .map(Path::new)
        .collect::<BTreeSet<_>>();
    if listed.len() != manifest.artifacts.len() + 2
        || seen
            .iter()
            .map(|path| path.as_path())
            .collect::<BTreeSet<_>>()
            != listed
    {
        return Err("bundle contains unlisted, duplicate or missing content".into());
    }
    let mut bytes = 0;
    for artifact in &manifest.artifacts {
        validate_entry_path(Path::new(&artifact.path))?;
        if !artifact.path.starts_with("artifacts/") || !seen.contains(Path::new(&artifact.path)) {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "bundle manifest references a missing artifact",
            )
            .into());
        }
        let identity = ArtifactIntegrity::parse(&artifact.integrity)?;
        let source = staging.path().join(&artifact.path);
        let metadata = fs::metadata(&source)?;
        if metadata.len() != artifact.bytes {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                format!("bundle artifact size mismatch for {}", artifact.integrity),
            )
            .into());
        }
        import_download_cache_with_integrity(home, &source, &identity)?;
        bytes += metadata.len();
    }
    Ok(BundleOutcome {
        schema: BUNDLE_SCHEMA,
        target: manifest.target,
        artifacts: manifest.artifacts.len(),
        bytes,
    })
}

fn append_bytes(
    builder: &mut Builder<GzEncoder<File>>,
    path: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes)
}

fn validate_entry_path(path: &Path) -> std::io::Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "unsafe bundle entry path",
        ));
    }
    let text = path.to_string_lossy();
    if text != "manifest.json"
        && text != "pinset.lock"
        && !(text.starts_with("artifacts/") || text.starts_with("artifacts\\"))
    {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "unexpected bundle entry",
        ));
    }
    Ok(())
}

fn persist_regular(source: &Path, destination: &Path) -> std::io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(destination)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "bundle destination must be a regular file",
        ));
    }
    let mut source = File::open(source)?;
    let mut file = AtomicWriteFile::options().open(destination)?;
    std::io::copy(&mut source, &mut file)?;
    file.commit()
}
