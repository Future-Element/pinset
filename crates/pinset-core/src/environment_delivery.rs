//! Complete target requirements and offline checks. These functions never make network requests.
use crate::{ArtifactIntegrity, LockedArtifact, LockedTool, Lockfile, ReadinessState};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

/// A portable Bun x64 bundle contains both CPU variants; current-machine installation
/// continues to use current_target_for_tool and selects just its matching variant.
pub fn artifacts_for_platform<'a>(tool: &'a LockedTool, platform: &str) -> Vec<&'a LockedArtifact> {
    if tool.name == "bun" && platform.ends_with("-x86_64") {
        let targets = [format!("{platform}-baseline"), format!("{platform}-avx2")];
        return targets
            .iter()
            .map(|target| tool.artifact(target))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
    }
    tool.artifacts
        .iter()
        .filter(|artifact| {
            artifact.target == platform
                || (tool.name == "bun"
                    && platform.ends_with("-x86_64")
                    && [format!("{platform}-baseline"), format!("{platform}-avx2")]
                        .contains(&artifact.target))
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryArtifact {
    pub tool: String,
    pub target: String,
    /// 0 identifies the base artifact; overlay numbers start at 1.
    pub overlay: usize,
    pub integrity: Option<String>,
    pub state: ReadinessState,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryReport {
    pub schema: u32,
    pub platforms: Vec<String>,
    pub artifacts: Vec<DeliveryArtifact>,
    pub ready: bool,
    pub project_dependencies_verified: bool,
    pub network_requests: u32,
}

pub fn inspect_offline_delivery(
    home: &Path,
    lock: &Lockfile,
    platforms: &[String],
) -> DeliveryReport {
    let mut artifacts = Vec::new();
    let mut verified = BTreeMap::<String, (ReadinessState, &'static str)>::new();
    for platform in platforms {
        for tool in &lock.tools {
            let selected = artifacts_for_platform(tool, platform);
            if selected.is_empty() {
                artifacts.push(DeliveryArtifact {
                    tool: tool.name.clone(),
                    target: platform.clone(),
                    overlay: 0,
                    integrity: None,
                    state: ReadinessState::Fail,
                    reason: "platform_artifact_not_locked",
                });
            }
            for artifact in selected {
                let mut identities = vec![artifact.artifact_integrity()];
                identities.extend(
                    artifact
                        .overlays
                        .iter()
                        .map(|overlay| overlay.artifact_integrity()),
                );
                for (overlay, integrity) in identities.into_iter().enumerate() {
                    let (identity, state, reason) = match integrity {
                        Err(_) => (None, ReadinessState::Fail, "locked_integrity_invalid"),
                        Ok(integrity) => {
                            let identity = integrity.canonical();
                            let (state, reason) = *verified
                                .entry(identity.clone())
                                .or_insert_with(|| inspect_cache(home, &integrity));
                            (Some(identity), state, reason)
                        }
                    };
                    artifacts.push(DeliveryArtifact {
                        tool: tool.name.clone(),
                        target: artifact.target.clone(),
                        overlay,
                        integrity: identity,
                        state,
                        reason,
                    });
                }
            }
        }
    }
    let ready = !platforms.is_empty()
        && !artifacts.is_empty()
        && artifacts
            .iter()
            .all(|artifact| artifact.state == ReadinessState::Pass);
    DeliveryReport {
        schema: 1,
        platforms: platforms.to_vec(),
        artifacts,
        ready,
        project_dependencies_verified: false,
        network_requests: 0,
    }
}

fn inspect_cache(home: &Path, integrity: &ArtifactIntegrity) -> (ReadinessState, &'static str) {
    match crate::download_cache::verify_download_cache_integrity(home, integrity) {
        Ok(Some(entry)) if entry.valid => (ReadinessState::Pass, "cached_artifact_verified"),
        Ok(Some(_)) => (ReadinessState::Fail, "cached_artifact_corrupt"),
        Ok(None) => (ReadinessState::Fail, "cached_artifact_missing"),
        Err(_) => (
            ReadinessState::Unknown,
            "cached_artifact_unreadable_or_unsafe",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LOCKFILE_SCHEMA, LockedArtifactFormat};
    fn lock() -> Lockfile {
        Lockfile {
            schema: LOCKFILE_SCHEMA,
            generated_by: "fixture".to_owned(),
            tools: vec![LockedTool {
                name: "node".to_owned(),
                requested: "24.1.0".to_owned(),
                version: "24.1.0".to_owned(),
                provider: "nodejs-official".to_owned(),
                released_at: None,
                metadata: BTreeMap::new(),
                options: BTreeMap::new(),
                artifacts: vec![LockedArtifact {
                    target: "linux-x86_64".to_owned(),
                    canonical_url: "https://example.invalid/archive".to_owned(),
                    artifact_path: "archive".to_owned(),
                    sha256: "a".repeat(64),
                    integrity: None,
                    format: LockedArtifactFormat::TarGz,
                    archive_root: "node".to_owned(),
                    verification: "fixture".to_owned(),
                    overlays: Vec::new(),
                }],
            }],
        }
    }
    #[test]
    fn offline_report_lists_every_missing_platform_and_archive_without_writes() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("absent-home");
        let report = inspect_offline_delivery(
            &home,
            &lock(),
            &["linux-x86_64".to_owned(), "windows-x86_64".to_owned()],
        );
        assert!(!report.ready);
        assert_eq!(report.artifacts.len(), 2);
        assert_eq!(report.artifacts[0].reason, "cached_artifact_missing");
        assert_eq!(report.artifacts[1].reason, "platform_artifact_not_locked");
        assert_eq!(report.network_requests, 0);
        assert!(!home.exists());
    }
    #[test]
    fn empty_environment_cannot_be_certified_as_offline_ready() {
        let root = tempfile::tempdir().unwrap();
        let mut lock = lock();
        lock.tools.clear();
        assert!(!inspect_offline_delivery(root.path(), &lock, &["linux-x86_64".to_owned()]).ready);
    }
    #[test]
    fn portable_bun_requires_both_cpu_variants() {
        let mut lock = lock();
        let tool = &mut lock.tools[0];
        tool.name = "bun".to_owned();
        tool.artifacts[0].target = "linux-x86_64-baseline".to_owned();
        assert!(artifacts_for_platform(tool, "linux-x86_64").is_empty());
        assert_eq!(
            artifacts_for_platform(tool, "linux-x86_64-baseline").len(),
            1
        );
        let mut avx2 = tool.artifacts[0].clone();
        avx2.target = "linux-x86_64-avx2".to_owned();
        tool.artifacts.push(avx2);
        assert_eq!(artifacts_for_platform(tool, "linux-x86_64").len(), 2);
    }
    #[test]
    fn corrupt_and_valid_cache_content_are_checked_before_installation() {
        use sha2::{Digest, Sha256};
        let root = tempfile::tempdir().unwrap();
        let mut lock = lock();
        let bytes = b"offline fixture";
        let hash = hex::encode(Sha256::digest(bytes));
        lock.tools[0].artifacts[0].sha256 = hash.clone();
        let cache = root
            .path()
            .join("downloads/sha256")
            .join(format!("{hash}.archive"));
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(&cache, bytes).unwrap();
        assert!(inspect_offline_delivery(root.path(), &lock, &["linux-x86_64".to_owned()]).ready);
        std::fs::write(&cache, b"changed archive").unwrap();
        let report = inspect_offline_delivery(root.path(), &lock, &["linux-x86_64".to_owned()]);
        assert!(!report.ready);
        assert_eq!(report.artifacts[0].reason, "cached_artifact_corrupt");
        assert_eq!(report.network_requests, 0);
    }
}
