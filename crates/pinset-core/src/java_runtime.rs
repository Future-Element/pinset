use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error, InstallOutcome,
    InstallRequest, Installer, JavaArchiveFormat, LockedArtifactFormat, LockedTool, Result,
    plan_java_artifact,
};

pub fn install_locked_java(
    installer: &Installer,
    pinset_home: &Path,
    locked_java: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if locked_java.name != "java" || locked_java.provider != "adoptium-temurin" {
        return Err(Error::InvalidLockfile {
            reason: format!(
                "{}/{} is not the built-in Eclipse Temurin provider",
                locked_java.name, locked_java.provider
            ),
        });
    }
    let artifact = locked_java
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: "java".to_owned(),
            target: target.to_owned(),
        })?;
    let release_name =
        locked_java
            .metadata
            .get("release_name")
            .ok_or_else(|| Error::InvalidLockfile {
                reason: "Java lock has no release_name metadata".to_owned(),
            })?;
    let package_name = artifact
        .canonical_url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::InvalidLockfile {
            reason: format!("Java artifact {target} has no package name"),
        })?;
    let plan = plan_java_artifact(
        &locked_java.version,
        release_name,
        target,
        package_name,
        &artifact.canonical_url,
    )?;
    let required_paths = required_java_paths(target)?;
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: "java".to_owned(),
        version: locked_java.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "official".to_owned(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: artifact.artifact_integrity()?.canonical(),
            format: match artifact.format {
                LockedArtifactFormat::Zip => ArtifactFormat::Zip,
                LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
                LockedArtifactFormat::TarXz => {
                    return Err(Error::InvalidLockfile {
                        reason: format!("Java artifact {target} cannot use tar.xz"),
                    });
                }
            },
        },
        strip_components: 1,
        include_prefixes: Vec::new(),
        required_paths: required_paths.clone(),
        base_artifacts: Vec::new(),
        executable_paths: if plan.format == JavaArchiveFormat::TarGz {
            required_paths
        } else {
            Vec::new()
        },
        aliases: Vec::new(),
    };
    installer.install(&request)
}

fn required_java_paths(target: &str) -> Result<Vec<PathBuf>> {
    let home = if target.starts_with("macos-") {
        PathBuf::from("Contents/Home")
    } else if target.starts_with("windows-") || target.starts_with("linux-") {
        PathBuf::new()
    } else {
        return Err(Error::UnsupportedJavaTarget {
            target: target.to_owned(),
        });
    };
    let extension = if target.starts_with("windows-") {
        ".exe"
    } else {
        ""
    };
    Ok(["java", "javac", "jar", "javadoc", "javap", "keytool"]
        .into_iter()
        .map(|command| home.join("bin").join(format!("{command}{extension}")))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JavaVersion;

    #[test]
    fn requires_jdk_commands_under_the_platform_java_home() {
        assert_eq!(
            required_java_paths("linux-x86_64").expect("Linux paths")[0],
            PathBuf::from("bin/java")
        );
        assert_eq!(
            required_java_paths("windows-x86_64").expect("Windows paths")[1],
            PathBuf::from("bin/javac.exe")
        );
        assert_eq!(
            required_java_paths("macos-aarch64").expect("macOS paths")[0],
            PathBuf::from("Contents/Home/bin/java")
        );
    }

    #[test]
    fn java_nine_and_newer_identity_supports_jshell_without_requiring_java_eight_to_have_it() {
        assert!(JavaVersion::parse("8.0.462+8").is_ok());
        assert!(JavaVersion::parse("21.0.8+9").is_ok());
        assert!(
            !required_java_paths("linux-x86_64")
                .expect("paths")
                .iter()
                .any(|path| path.ends_with("jshell"))
        );
    }
}
