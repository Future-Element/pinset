use std::path::{Path, PathBuf};

use crate::{
    ArtifactFormat, ArtifactInstallSpec, ArtifactSource, ArtifactSourceKind, ArtifactSpec, Error,
    InstallAlias, InstallOutcome, InstallRequest, Installer, LockedArtifactFormat, LockedTool,
    Result, tool_targets,
};

pub fn install_locked_npm_tool(
    installer: &Installer,
    pinset_home: &Path,
    locked_tool: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    if !matches!(locked_tool.name.as_str(), "pnpm" | "bun") {
        return Err(Error::InvalidLockfile {
            reason: format!("{} is not an npm-distributed tool", locked_tool.name),
        });
    }
    let artifact = locked_tool
        .artifact(target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked_tool.name.clone(),
            version: locked_tool.version.clone(),
            target: target.to_owned(),
        })?;
    if locked_tool.name == "pnpm" {
        let entry = crate::pnpm_entry_path(locked_tool, target)?;
        let native = !entry.starts_with("bin");
        if artifact.format != LockedArtifactFormat::TarGz || artifact.archive_root != "package" {
            return Err(Error::InvalidLockfile {
                reason: "pnpm requires an official npm tarball".into(),
            });
        }
        let mut base_artifacts = Vec::new();
        if native {
            let [wrapper] = artifact.overlays.as_slice() else {
                return Err(Error::InvalidLockfile { reason: "native pnpm requires one locked wrapper payload; run use pnpm@<selector> to resolve it".into() });
            };
            if wrapper.format != LockedArtifactFormat::TarGz || wrapper.archive_root != "package" {
                return Err(Error::InvalidLockfile {
                    reason: "invalid native pnpm wrapper payload".into(),
                });
            }
            base_artifacts.push(ArtifactInstallSpec {
                artifact: ArtifactSpec {
                    canonical_url: wrapper.canonical_url.clone(),
                    sources: vec![ArtifactSource {
                        id: "npm-official".into(),
                        url: wrapper.canonical_url.clone(),
                        kind: ArtifactSourceKind::Official,
                    }],
                    integrity: wrapper.artifact_integrity()?.canonical(),
                    format: ArtifactFormat::TarGz,
                },
                strip_components: 1,
                // Keep the shared runtime next to the native binary. The main
                // archive supplies the binary and notices; omit wrapper launchers.
                include_prefixes: [
                    "bin",
                    "dist",
                    "package.json",
                    "native-binary.mjs",
                    "README.md",
                    "CHANGELOG.md",
                ]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
                required_paths: vec![
                    PathBuf::from("package.json"),
                    PathBuf::from("bin/pnpm.mjs"),
                    PathBuf::from("dist/node_modules/node-gyp/bin/node-gyp.js"),
                ],
            });
        } else if !artifact.overlays.is_empty() {
            return Err(Error::InvalidLockfile {
                reason: "JavaScript pnpm cannot contain native overlays".into(),
            });
        }
        let include_prefixes = if native {
            vec![
                entry.clone(),
                PathBuf::from("LICENSE"),
                PathBuf::from("THIRD-PARTY-NOTICES.md"),
            ]
        } else {
            vec![]
        };
        let required_paths = if native {
            vec![
                entry.clone(),
                PathBuf::from("LICENSE"),
                PathBuf::from("THIRD-PARTY-NOTICES.md"),
            ]
        } else {
            vec![entry.clone(), PathBuf::from("package.json")]
        };
        return installer.install(&InstallRequest {
            pinset_home: pinset_home.to_path_buf(),
            tool: "pnpm".into(),
            version: locked_tool.version.clone(),
            target: target.into(),
            artifact: ArtifactSpec {
                canonical_url: artifact.canonical_url.clone(),
                sources: vec![ArtifactSource {
                    id: "npm-official".into(),
                    url: artifact.canonical_url.clone(),
                    kind: ArtifactSourceKind::Official,
                }],
                integrity: artifact.artifact_integrity()?.canonical(),
                format: ArtifactFormat::TarGz,
            },
            strip_components: 1,
            include_prefixes,
            required_paths,
            base_artifacts,
            executable_paths: if native { vec![entry] } else { vec![] },
            aliases: vec![],
        });
    }
    let target_manifest = tool_targets(&locked_tool.name)?
        .iter()
        .find(|candidate| candidate.target == target)
        .ok_or_else(|| Error::LockedArtifactMissing {
            tool: locked_tool.name.clone(),
            version: locked_tool.version.clone(),
            target: target.to_owned(),
        })?;
    let format = match artifact.format {
        LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
        LockedArtifactFormat::Zip => ArtifactFormat::Zip,
        LockedArtifactFormat::TarXz => ArtifactFormat::TarXz,
        LockedArtifactFormat::Binary => {
            return Err(Error::InvalidLockfile {
                reason: format!("{} artifact cannot use binary format", locked_tool.name),
            });
        }
    };
    let request = InstallRequest {
        pinset_home: pinset_home.to_path_buf(),
        tool: locked_tool.name.clone(),
        version: locked_tool.version.clone(),
        target: target.to_owned(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "npm-official".to_owned(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Official,
            }],
            integrity: artifact.artifact_integrity()?.canonical(),
            format,
        },
        strip_components: 1,
        include_prefixes: vec![],
        required_paths: vec![PathBuf::from(target_manifest.required_path)],
        base_artifacts: vec![],
        executable_paths: vec![PathBuf::from(target_manifest.required_path)],
        aliases: npm_install_aliases(&locked_tool.name, target),
    };
    installer.install(&request)
}

fn npm_install_aliases(tool: &str, target: &str) -> Vec<InstallAlias> {
    if tool != "bun" {
        return Vec::new();
    }
    let (source, destination) = if target.starts_with("windows-") {
        (PathBuf::from("bin/bun.exe"), PathBuf::from("bin/bunx.exe"))
    } else {
        (PathBuf::from("bin/bun"), PathBuf::from("bin/bunx"))
    };
    vec![InstallAlias {
        source,
        destination,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LockedArtifact, LockedArtifactOverlay, import_download_cache_with_integrity, sha256_hex,
    };
    use flate2::{Compression, write::GzEncoder};
    use std::fs;

    fn cached_package(home: &Path, name: &str, files: &[(&str, &[u8])]) -> (String, PathBuf) {
        let path = home.join(format!("{name}.tgz"));
        let mut archive = tar::Builder::new(GzEncoder::new(
            fs::File::create(&path).unwrap(),
            Compression::default(),
        ));
        for (name, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive
                .append_data(&mut header, format!("package/{name}"), *content)
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
        let integrity = format!("sha256:{}", sha256_hex(&fs::read(&path).unwrap()));
        import_download_cache_with_integrity(
            home,
            &path,
            &crate::ArtifactIntegrity::parse(&integrity).unwrap(),
        )
        .unwrap();
        (integrity, path)
    }

    #[test]
    fn native_pnpm_installs_locked_wrapper_and_binary_atomically_on_both_layouts() {
        for target in ["linux-x86_64", "windows-x86_64"] {
            let root = tempfile::tempdir().unwrap();
            let entry = crate::executable_name("pnpm", target);
            let (native_integrity, _) = cached_package(
                root.path(),
                "native",
                &[
                    (&entry, b"native pnpm"),
                    ("package.json", b"platform metadata"),
                    ("LICENSE", b"native license"),
                    ("THIRD-PARTY-NOTICES.md", b"native notices"),
                ],
            );
            let (wrapper_integrity, _) = cached_package(
                root.path(),
                "wrapper",
                &[
                    ("pnpm", b"placeholder launcher"),
                    ("package.json", b"wrapper metadata"),
                    ("bin/pnpm.mjs", b"wrapper entry"),
                    ("native-binary.mjs", b"wrapper resolver"),
                    (
                        "dist/node_modules/node-gyp/bin/node-gyp.js",
                        b"node-gyp payload",
                    ),
                ],
            );
            let tool = LockedTool {
                name: "pnpm".into(),
                requested: "latest".into(),
                version: "12.9.1".into(),
                provider: "pnpm-npm".into(),
                released_at: None,
                metadata: [("pnpm-entry".into(), "pnpm".into())].into(),
                options: Default::default(),
                artifacts: vec![LockedArtifact {
                    target: target.into(),
                    canonical_url: if target.starts_with("windows-") {
                        "https://registry.npmjs.org/@pnpm/exe.win32-x64/-/exe.win32-x64-12.9.1.tgz"
                            .into()
                    } else {
                        "https://registry.npmjs.org/@pnpm/exe.linux-x64/-/exe.linux-x64-12.9.1.tgz"
                            .into()
                    },
                    artifact_path: "native.tgz".into(),
                    sha256: String::new(),
                    integrity: Some(native_integrity),
                    format: LockedArtifactFormat::TarGz,
                    archive_root: "package".into(),
                    verification: "npm-registry-signature-sha512".into(),
                    overlays: vec![LockedArtifactOverlay {
                        canonical_url: "https://registry.npmjs.org/pnpm/-/pnpm-12.9.1.tgz".into(),
                        artifact_path: "wrapper.tgz".into(),
                        integrity: wrapper_integrity,
                        format: LockedArtifactFormat::TarGz,
                        archive_root: "package".into(),
                        verification: "npm-registry-signature-sha512".into(),
                    }],
                }],
            };
            let installer = Installer::new(Default::default())
                .unwrap()
                .with_offline(true)
                .with_install_identity(tool.installation_version(target));
            let installed =
                install_locked_npm_tool(&installer, root.path(), &tool, target).unwrap();
            assert_eq!(
                fs::read(installed.install_dir.join(&entry)).unwrap(),
                b"native pnpm"
            );
            assert_eq!(
                fs::read(installed.install_dir.join("package.json")).unwrap(),
                b"wrapper metadata"
            );
            assert!(
                installed
                    .install_dir
                    .join("dist/node_modules/node-gyp/bin/node-gyp.js")
                    .is_file()
            );
            assert!(!installed.install_dir.join("pnpx").exists());
            let receipt = crate::read_receipt(root.path(), &tool, target).unwrap();
            assert_eq!(receipt.base_artifact_integrities.len(), 1);
            assert!(
                install_locked_npm_tool(&installer, root.path(), &tool, target)
                    .unwrap()
                    .reused_existing
            );
            let mut broken = tool.clone();
            broken.artifacts[0].overlays.clear();
            assert!(matches!(
                install_locked_npm_tool(&installer, root.path(), &broken, target),
                Err(Error::InvalidLockfile { .. })
            ));
        }
    }

    #[test]
    fn bun_declares_bunx_as_an_atomic_install_alias() {
        let aliases = npm_install_aliases("bun", "linux-x86_64-avx2");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].source, Path::new("bin/bun"));
        assert_eq!(aliases[0].destination, Path::new("bin/bunx"));
        assert!(npm_install_aliases("pnpm", "linux-x86_64").is_empty());
    }
}
