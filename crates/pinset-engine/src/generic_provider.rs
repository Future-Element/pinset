//! Exact, user-declared HTTPS binary artifacts; never runs builds or installers.
use crate::*;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn https_client(timeout: Duration) -> Result<Client> {
    crate::http_client_builder()?
        .https_only(true)
        .no_gzip()
        .no_brotli()
        .connect_timeout(Duration::from_secs(15).min(timeout))
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10
                || validate_generic_url(attempt.url().as_str()).is_err()
            {
                attempt.error("generic download redirects must remain public HTTPS")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|source| Error::HttpClient { source })
}

impl Services {
    pub(crate) fn resolve_generic(
        &self,
        name: &str,
        version: &str,
        definition: &GenericTool,
        platforms: &[String],
        cache: bool,
    ) -> Result<LockedTool> {
        definition.validate(name)?;
        validate_generic_version(version)?;
        let targets = if platforms.is_empty() {
            vec![current_target()]
        } else {
            platforms.to_vec()
        };
        let limits = InstallLimits::default();
        let client = https_client(limits.request_timeout)?;
        let mut artifacts = Vec::new();
        let mut options = BTreeMap::new();
        for target in targets {
            let url = definition.artifact_url(version, &target)?;
            let mut response = crate::send_metadata_get(client.get(&url))
                .and_then(reqwest::blocking::Response::error_for_status)
                .map_err(|e| {
                    service_error("PINSET_GENERIC_DOWNLOAD_FAILED", format!("{url}: {e}"))
                })?;
            let length = response.content_length();
            if length.is_some_and(|n| n > limits.max_download_bytes) {
                return Err(Error::DownloadTooLarge {
                    url,
                    limit: limits.max_download_bytes,
                });
            }
            self.progress
                .report(ProgressEvent::Download(DownloadProgressEvent::Started {
                    url: url.clone(),
                    total_bytes: length,
                }));
            let mut temporary = tempfile::NamedTempFile::new()?;
            let mut hasher = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = response.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                if bytes > limits.max_download_bytes {
                    return Err(Error::DownloadTooLarge {
                        url,
                        limit: limits.max_download_bytes,
                    });
                }
                temporary.write_all(&buffer[..count])?;
                hasher.update(&buffer[..count]);
                self.progress
                    .report(ProgressEvent::Download(DownloadProgressEvent::Advanced {
                        downloaded_bytes: bytes,
                        total_bytes: length,
                    }));
            }
            if bytes == 0 || length.is_some_and(|n| n != bytes) {
                return Err(service_error(
                    "PINSET_GENERIC_DOWNLOAD_FAILED",
                    "empty or incomplete generic binary artifact",
                ));
            }
            let sha256 = hex::encode(hasher.finalize());
            let attestation = crate::github_attestation::fetch(&client, &url, &sha256)?;
            if cache {
                temporary.flush()?;
                import_download_cache_with_integrity(
                    &self.home,
                    temporary.path(),
                    &ArtifactIntegrity::parse(&sha256)?,
                )?;
            }
            self.progress
                .report(ProgressEvent::Download(DownloadProgressEvent::Finished {
                    downloaded_bytes: bytes,
                }));
            options.insert(
                format!("executable:{target}"),
                definition.executable_path(version, &target)?,
            );
            artifacts.push(LockedArtifact {
                target,
                canonical_url: url.clone(),
                artifact_path: url::Url::parse(&url)
                    .expect("validated URL")
                    .path()
                    .trim_start_matches('/')
                    .to_owned(),
                sha256,
                integrity: None,
                format: definition.format,
                archive_root: String::new(),
                verification: if attestation.is_some() {
                    GENERIC_GITHUB_VERIFICATION.into()
                } else {
                    GENERIC_VERIFICATION.into()
                },
                attestation,
                overlays: vec![],
            });
        }
        let tool = LockedTool {
            name: name.into(),
            requested: version.into(),
            version: version.into(),
            provider: GENERIC_PROVIDER.into(),
            released_at: None,
            metadata: BTreeMap::new(),
            options,
            artifacts,
        };
        validate_generic_lock(&tool)?;
        Ok(tool)
    }
}

pub(crate) fn install_locked_generic(
    installer: &Installer,
    home: &Path,
    tool: &LockedTool,
    target: &str,
) -> Result<InstallOutcome> {
    validate_generic_lock(tool)?;
    let artifact = tool.artifact(target).ok_or_else(|| {
        service_error(
            "PINSET_PLATFORM_UNAVAILABLE",
            "generic tool has no locked artifact for this platform",
        )
    })?;
    if let Some(evidence) = &artifact.attestation {
        crate::github_attestation::verify(&artifact.canonical_url, &artifact.sha256, evidence)?;
    }
    let entry = generic_executable(tool, target)?;
    let request = InstallRequest {
        pinset_home: home.into(),
        tool: tool.name.clone(),
        version: tool.version.clone(),
        target: target.into(),
        artifact: ArtifactSpec {
            canonical_url: artifact.canonical_url.clone(),
            sources: vec![ArtifactSource {
                id: "generic".into(),
                url: artifact.canonical_url.clone(),
                kind: ArtifactSourceKind::Generic,
            }],
            integrity: artifact.artifact_integrity()?.canonical(),
            format: match artifact.format {
                LockedArtifactFormat::Binary => ArtifactFormat::Binary,
                LockedArtifactFormat::Zip => ArtifactFormat::Zip,
                LockedArtifactFormat::TarGz => ArtifactFormat::TarGz,
                LockedArtifactFormat::TarXz => {
                    return Err(service_error(
                        "PINSET_GENERIC_INVALID",
                        "generic tar.xz is unsupported",
                    ));
                }
            },
        },
        strip_components: 0,
        include_prefixes: vec![],
        required_paths: vec![PathBuf::from(&entry)],
        base_artifacts: vec![],
        executable_paths: vec![PathBuf::from(&entry)],
        aliases: vec![],
    };
    let outcome = installer.install_generic(&request)?;
    let receipt_path = outcome.install_dir.join(".pinset-install.toml");
    let mut receipt: InstallReceipt = toml::from_str(&fs::read_to_string(&receipt_path)?)?;
    receipt.commands.insert(tool.name.clone(), entry);
    write_atomic(&receipt_path, toml::to_string(&receipt)?.as_bytes())?;
    Ok(outcome)
}

/// Recognize native images for the requested platform without executing their contents.
pub(crate) fn validate_native_executable(path: &Path, target: &str) -> Result<()> {
    let invalid = || {
        service_error(
            "PINSET_GENERIC_BINARY_INVALID",
            format!(
                "{} is not a precompiled {target} executable; scripts and source builds are unsupported",
                path.display()
            ),
        )
    };
    let mut file = File::open(path)?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header).map_err(|_| invalid())?;
    let arch = target.split_once('-').map(|(_, a)| a).unwrap_or("");
    let valid = if target.starts_with("linux-") {
        let machine = if arch == "x86_64" { 62 } else { 183 };
        header.starts_with(b"\x7fELF")
            && header[4] == 2
            && header[5] == 1
            && u16::from_le_bytes([header[18], header[19]]) == machine
    } else if target.starts_with("windows-") {
        let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
        let mut pe = [0u8; 6];
        header.starts_with(b"MZ")
            && file.seek(SeekFrom::Start(offset.into())).is_ok()
            && file.read_exact(&mut pe).is_ok()
            && pe.starts_with(b"PE\0\0")
            && u16::from_le_bytes([pe[4], pe[5]]) == 0x8664
    } else if target.starts_with("macos-") {
        let expected = if arch == "x86_64" {
            0x01000007
        } else {
            0x0100000c
        };
        match &header[..4] {
            b"\xcf\xfa\xed\xfe" => u32::from_le_bytes(header[4..8].try_into().unwrap()) == expected,
            b"\xfe\xed\xfa\xcf" => u32::from_be_bytes(header[4..8].try_into().unwrap()) == expected,
            b"\xca\xfe\xba\xbe" | b"\xca\xfe\xba\xbf" | b"\xbe\xba\xfe\xca"
            | b"\xbf\xba\xfe\xca" => {
                let little = matches!(header[0], 0xbe | 0xbf);
                let number = |bytes: [u8; 4]| {
                    if little {
                        u32::from_le_bytes(bytes)
                    } else {
                        u32::from_be_bytes(bytes)
                    }
                };
                let count = number(header[4..8].try_into().unwrap());
                let stride = if matches!(header[3], 0xbf) || header[0] == 0xbf {
                    32
                } else {
                    20
                };
                let mut found = false;
                if count <= 64 {
                    for index in 0..count {
                        file.seek(SeekFrom::Start(8 + u64::from(index) * stride))?;
                        let mut cpu = [0u8; 4];
                        if file.read_exact(&mut cpu).is_ok() && number(cpu) == expected {
                            found = true;
                            break;
                        }
                    }
                }
                found
            }
            _ => false,
        }
    } else {
        false
    };
    if valid { Ok(()) } else { Err(invalid()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_images_must_match_the_selected_operating_system_and_architecture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool");
        for (target, machine) in [("linux-x86_64", 62u16), ("linux-aarch64", 183u16)] {
            let mut header = [0u8; 64];
            header[..6].copy_from_slice(b"\x7fELF\x02\x01");
            header[18..20].copy_from_slice(&machine.to_le_bytes());
            fs::write(&path, header).unwrap();
            validate_native_executable(&path, target).unwrap();
            assert!(validate_native_executable(&path, "windows-x86_64").is_err());
        }
        for (target, cpu) in [
            ("macos-x86_64", 0x01000007u32),
            ("macos-aarch64", 0x0100000cu32),
        ] {
            let mut header = [0u8; 64];
            header[..4].copy_from_slice(b"\xcf\xfa\xed\xfe");
            header[4..8].copy_from_slice(&cpu.to_le_bytes());
            fs::write(&path, header).unwrap();
            validate_native_executable(&path, target).unwrap();
            assert!(validate_native_executable(&path, "linux-x86_64").is_err());
        }
        let mut pe = [0u8; 96];
        pe[..2].copy_from_slice(b"MZ");
        pe[60..64].copy_from_slice(&64u32.to_le_bytes());
        pe[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        fs::write(&path, pe).unwrap();
        validate_native_executable(&path, "windows-x86_64").unwrap();
    }

    #[test]
    fn scripts_and_source_are_never_executed_as_generic_installers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool");
        for body in [
            "#!/bin/sh\necho script\n".repeat(8),
            "fn main() { println!(\"source\"); }".repeat(8),
        ] {
            fs::write(&path, body).unwrap();
            assert_eq!(
                validate_native_executable(&path, "linux-x86_64")
                    .unwrap_err()
                    .code(),
                "PINSET_GENERIC_BINARY_INVALID"
            );
        }
    }
}
