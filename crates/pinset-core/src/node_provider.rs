use crate::{Error, ResolvedArtifactSource, Result, SourceConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeArchiveFormat {
    Zip,
    TarXz,
}

impl NodeArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarXz => "tar.xz",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeArtifactPlan {
    pub version: String,
    pub target: String,
    pub artifact_path: String,
    pub archive_root: String,
    pub node_executable: String,
    pub format: NodeArchiveFormat,
    pub canonical_url: String,
    pub sources: Vec<ResolvedArtifactSource>,
}

pub fn plan_node_artifact(
    config: &SourceConfig,
    version: &str,
    target: &str,
) -> Result<NodeArtifactPlan> {
    validate_exact_node_version(version)?;
    let platform = node_platform(target)?;
    let format = if platform.starts_with("win-") {
        NodeArchiveFormat::Zip
    } else {
        NodeArchiveFormat::TarXz
    };
    let extension = format.as_str();
    let archive_root = format!("node-v{version}-{platform}");
    let artifact_path = format!("v{version}/{archive_root}.{extension}");
    let canonical_url = config.official_artifact_url("node", &artifact_path)?;
    let sources = config.resolve_artifact_sources("node", &artifact_path)?;
    let node_executable = if format == NodeArchiveFormat::Zip {
        format!("{archive_root}/node.exe")
    } else {
        format!("{archive_root}/bin/node")
    };

    Ok(NodeArtifactPlan {
        version: version.to_owned(),
        target: target.to_owned(),
        artifact_path,
        archive_root,
        node_executable,
        format,
        canonical_url,
        sources,
    })
}

pub fn validate_exact_node_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u64>().is_err()
        })
    {
        return Err(Error::InvalidNodeVersion {
            version: version.to_owned(),
        });
    }
    Ok(())
}

fn node_platform(target: &str) -> Result<&'static str> {
    match target {
        "windows-x86_64" => Ok("win-x64"),
        "windows-aarch64" => Ok("win-arm64"),
        "linux-x86_64" => Ok("linux-x64"),
        "linux-aarch64" => Ok("linux-arm64"),
        "macos-x86_64" => Ok("darwin-x64"),
        "macos-aarch64" => Ok("darwin-arm64"),
        _ => Err(Error::UnsupportedNodeTarget {
            target: target.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceKind;

    #[test]
    fn plans_windows_zip_from_official_source() {
        let plan =
            plan_node_artifact(&SourceConfig::default(), "24.0.0", "windows-x86_64").expect("plan");

        assert_eq!(plan.format, NodeArchiveFormat::Zip);
        assert_eq!(plan.artifact_path, "v24.0.0/node-v24.0.0-win-x64.zip");
        assert_eq!(
            plan.canonical_url,
            "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-x64.zip"
        );
        assert_eq!(plan.node_executable, "node-v24.0.0-win-x64/node.exe");
        assert_eq!(plan.sources.len(), 1);
        assert_eq!(plan.sources[0].kind, SourceKind::Official);
    }

    #[test]
    fn plans_linux_tar_xz_without_claiming_zip_install_support() {
        let plan =
            plan_node_artifact(&SourceConfig::default(), "24.0.0", "linux-x86_64").expect("plan");

        assert_eq!(plan.format, NodeArchiveFormat::TarXz);
        assert_eq!(plan.artifact_path, "v24.0.0/node-v24.0.0-linux-x64.tar.xz");
        assert_eq!(plan.node_executable, "node-v24.0.0-linux-x64/bin/node");
    }

    #[test]
    fn plans_macos_as_tar_xz_not_windows_zip() {
        for target in ["macos-x86_64", "macos-aarch64"] {
            let plan =
                plan_node_artifact(&SourceConfig::default(), "24.0.0", target).expect("plan");
            assert_eq!(plan.format, NodeArchiveFormat::TarXz);
            assert!(plan.artifact_path.ends_with(".tar.xz"));
            assert!(plan.node_executable.ends_with("/bin/node"));
        }
    }

    #[test]
    fn custom_source_changes_transport_but_not_canonical_identity() {
        let mut config = SourceConfig::default();
        config
            .add(
                "node",
                "mirror",
                "https://mirror.example/node/",
                false,
                false,
            )
            .expect("mirror");
        config.use_source("node", "mirror").expect("active");
        config
            .set_fallback("node", &["official".to_owned()])
            .expect("fallback");

        let plan = plan_node_artifact(&config, "22.11.0", "linux-aarch64").expect("artifact plan");
        assert_eq!(
            plan.canonical_url,
            "https://nodejs.org/dist/v22.11.0/node-v22.11.0-linux-arm64.tar.xz"
        );
        assert_eq!(plan.sources[0].alias, "mirror");
        assert_eq!(
            plan.sources[0].url,
            "https://mirror.example/node/v22.11.0/node-v22.11.0-linux-arm64.tar.xz"
        );
        assert_eq!(plan.sources[1].alias, "official");
    }

    #[test]
    fn rejects_floating_or_unsupported_requests_before_network_access() {
        let config = SourceConfig::default();
        for version in ["24", "24.0", "v24.0.0", "24.x.0", "24.0.0-rc.1"] {
            assert!(matches!(
                plan_node_artifact(&config, version, "linux-x86_64"),
                Err(Error::InvalidNodeVersion { .. })
            ));
        }
        assert!(matches!(
            plan_node_artifact(&config, "24.0.0", "linux-riscv64"),
            Err(Error::UnsupportedNodeTarget { .. })
        ));
    }
}
