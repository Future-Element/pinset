//! A CLI/shim pair is one update unit. Backups are on the destination volume
//! so rollback remains possible for custom installation directories.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Deserialize)]
pub(crate) struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    state: String,
    size: u64,
}

#[derive(Deserialize)]
struct ReleaseDocument {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

pub(crate) struct UpdateRelease {
    pub version: String,
    pub archive: ReleaseAsset,
    pub checksums: ReleaseAsset,
}

fn release_version(version: &str) -> Result<semver::Version> {
    let parsed = semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
        .map_err(|_| service_error("PINSET_UPDATE_VERSION", "invalid release version"))?;
    if parsed.major != 3 || !parsed.build.is_empty() {
        return Err(service_error(
            "PINSET_UPDATE_VERSION",
            "self update requires a Pinset 3 release version without build metadata",
        ));
    }
    Ok(parsed)
}

impl ReleaseDocument {
    fn asset(&self, name: &str) -> Result<Option<ReleaseAsset>> {
        let mut matches = self.assets.iter().filter(|asset| asset.name == name);
        let Some(asset) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() || asset.state != "uploaded" || asset.size == 0 {
            return Err(service_error(
                "PINSET_UPDATE_ASSET",
                format!("release must contain exactly one complete asset named {name}"),
            ));
        }
        let url = url::Url::parse(&asset.browser_download_url)
            .map_err(|_| service_error("PINSET_UPDATE_ASSET", "invalid release asset URL"))?;
        let parts: Vec<_> = url.path().split('/').collect();
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || parts.len() != 7
            || !parts[1].eq_ignore_ascii_case("Future-Element")
            || !parts[2].eq_ignore_ascii_case("pinset")
            || parts[3] != "releases"
            || parts[4] != "download"
            || parts[5] != self.tag_name
            || parts[6] != name
        {
            return Err(service_error(
                "PINSET_UPDATE_ASSET",
                "asset URL does not belong to the selected official release",
            ));
        }
        Ok(Some(asset.clone()))
    }

    fn resolve(self, requested: Option<&semver::Version>, target: &str) -> Result<UpdateRelease> {
        let version = release_version(&self.tag_name)?;
        if self.draft
            || self.tag_name != format!("v{version}")
            || requested.is_some_and(|requested| *requested != version)
            || (requested.is_none() && (self.prerelease || !version.pre.is_empty()))
        {
            return Err(service_error(
                "PINSET_UPDATE_METADATA",
                "release does not match the requested published version",
            ));
        }
        // The release directory binds the version. Select an asset actually
        // listed there, accepting stable and versioned platform ZIP names.
        let mut archive = None;
        for name in [
            format!("pinset-{target}.zip"),
            format!("pinset-v{version}-{target}.zip"),
        ] {
            if let Some(asset) = self.asset(&name)? {
                archive = Some(asset);
                break;
            }
        }
        let archive = archive.ok_or_else(|| {
            service_error(
                "PINSET_UPDATE_ASSET",
                "release has no ZIP for this platform",
            )
        })?;
        let checksums = self.asset("SHA256SUMS")?.ok_or_else(|| {
            service_error("PINSET_UPDATE_CHECKSUM", "release has no SHA256SUMS asset")
        })?;
        Ok(UpdateRelease {
            version: version.to_string(),
            archive,
            checksums,
        })
    }
}

pub(crate) fn resolve_release(
    client: &reqwest::blocking::Client,
    requested: Option<&str>,
    target: &str,
) -> Result<UpdateRelease> {
    let requested = requested.map(release_version).transpose()?;
    let endpoint = requested
        .as_ref()
        .map_or_else(|| "latest".to_owned(), |version| format!("tags/v{version}"));
    let response = client
        .get(format!(
            "https://api.github.com/repos/Future-Element/pinset/releases/{endpoint}"
        ))
        .header("User-Agent", "pinset/3")
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map_err(|error| service_error("PINSET_UPDATE_FETCH", error.to_string()))?;
    let document: ReleaseDocument = serde_json::from_slice(&response)
        .map_err(|error| service_error("PINSET_UPDATE_METADATA", error.to_string()))?;
    document.resolve(requested.as_ref(), target)
}

pub(crate) fn release_checksum(sums: &str, filename: &str) -> Result<String> {
    let invalid = || {
        service_error(
            "PINSET_UPDATE_CHECKSUM",
            format!("SHA256SUMS must contain exactly one valid SHA-256 entry for {filename}"),
        )
    };
    let mut found = None;
    for line in sums.lines() {
        let mut fields = line.split_whitespace();
        let (Some(hash), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        if name.strip_prefix('*').unwrap_or(name) != filename {
            continue;
        }
        if found.is_some()
            || fields.next().is_some()
            || hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
        found = Some(hash.to_ascii_lowercase());
    }
    found.ok_or_else(invalid)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BinaryJournal {
    protocol: String,
    destination: PathBuf,
    backup: PathBuf,
    phase: String,
    names: Vec<String>,
}
fn rollback(journal: &BinaryJournal) -> Result<()> {
    for name in &journal.names {
        let backup = journal.backup.join(name);
        if backup.is_file() {
            let dst = journal.destination.join(name);
            if dst.exists() {
                if cfg!(windows) {
                    fs::rename(&dst, journal.backup.join(format!("failed-{name}")))?;
                } else {
                    fs::remove_file(&dst)?;
                }
            }
            fs::rename(backup, dst)?;
        }
    }
    Ok(())
}
pub(crate) fn replace_binary_pair(
    source: &Path,
    destination: &Path,
    home: &Path,
    names: &[String],
) -> Result<PathBuf> {
    let backup = destination.join(format!(".pinset-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&backup)?;
    let journal = BinaryJournal {
        protocol: PROTOCOL.into(),
        destination: destination.canonicalize()?,
        backup: backup.clone(),
        phase: "prepared".into(),
        names: names.to_vec(),
    };
    let path = home.join("state/self-update/transaction.json");
    write_json(&path, &journal)?;
    let result = (|| -> Result<()> {
        for name in names {
            let dst = destination.join(name);
            if !dst.is_file() || fs::symlink_metadata(&dst)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_PAIR",
                    "both owned regular binaries must exist",
                ));
            }
            fs::copy(&dst, backup.join(name))?;
        }
        for name in names {
            let staged = backup.join(format!("staged-{name}"));
            let input = source.join(name);
            if !input.is_file() || fs::symlink_metadata(&input)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_PAIR",
                    "replacement binaries must be regular files",
                ));
            }
            fs::copy(input, &staged)?;
            // Windows permits renaming an in-use executable. Keep its inode
            // until the old process exits, then atomically publish the stage.
            let dst = destination.join(name);
            if cfg!(windows) {
                fs::rename(&dst, backup.join(format!("running-{name}")))?;
            }
            fs::rename(staged, &dst)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        rollback(&journal)?;
        fs::remove_file(path)?;
        return Err(error);
    }
    let done = BinaryJournal {
        phase: "committed".into(),
        ..journal
    };
    write_json(&path, &done)?;
    Ok(backup)
}
impl Services {
    pub fn self_repair(&self, plan: bool) -> Result<serde_json::Value> {
        if !plan {
            self.ensure_home()?;
        }
        let transactions = self.recover_transactions(plan)?;
        // Release project guards before acquiring the binary-update guard: a
        // valid project identity may have the same name as this internal lock.
        let _guard = if plan {
            None
        } else {
            Some(self.guard("self-update")?)
        };
        let path = self.home.join("state/self-update/transaction.json");
        let mut binary_update = None;
        if path.exists() {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "binary journal is a symlink",
                ));
            }
            let journal: BinaryJournal = read_json(&path)?;
            let parent = std::env::current_exe()?.parent().unwrap().canonicalize()?;
            if journal.protocol != PROTOCOL
                || journal.destination != parent
                || !journal.backup.starts_with(&parent)
                || journal.backup.parent() != Some(parent.as_path())
                || journal.names
                    != [
                        executable_name("pinset", &current_target()),
                        executable_name("pinset-shim", &current_target()),
                    ]
            {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "invalid binary update journal",
                ));
            }
            if !matches!(journal.phase.as_str(), "prepared" | "committed") {
                return Err(service_error(
                    "PINSET_UPDATE_RECOVERY",
                    "unknown binary update phase",
                ));
            }
            binary_update = Some(journal.phase.clone());
            if !plan {
                if journal.phase == "prepared" {
                    rollback(&journal)?;
                }
                self.install_shims()?;
                fs::remove_file(path)?;
            }
        } else if !plan {
            self.install_shims()?;
        }
        Ok(
            serde_json::json!({"protocol":PROTOCOL,"plan":plan,"repaired":!plan,"transactions":transactions,"binary_update":binary_update}),
        )
    }
    pub(crate) fn validate_new_cli(&self, path: &Path, version: &str) -> Result<()> {
        let output = Command::new(path)
            .arg("--version")
            .env_remove("PINSET_IDENTITY")
            .output()?;
        if !output.status.success()
            || String::from_utf8_lossy(&output.stdout).trim() != format!("pinset {version}")
        {
            return Err(service_error(
                "PINSET_UPDATE_VERSION",
                "downloaded CLI does not match the requested release",
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str, target: &str, versioned: bool) -> ReleaseDocument {
        let name = if versioned {
            format!("pinset-v{version}-{target}.zip")
        } else {
            format!("pinset-{target}.zip")
        };
        let base =
            format!("https://github.com/Future-Element/pinset/releases/download/v{version}/");
        serde_json::from_value(serde_json::json!({
            "tag_name":format!("v{version}"),"draft":false,"prerelease":false,
            "assets":[
                {"name":name,"browser_download_url":format!("{base}{name}"),"state":"uploaded","size":100},
                {"name":"SHA256SUMS","browser_download_url":format!("{base}SHA256SUMS"),"state":"uploaded","size":100}
            ]
        }))
        .unwrap()
    }
    #[test]
    fn future_versions_resolve_the_target_release_on_all_published_platforms() {
        for version in ["3.0.2", "3.1.0", "3.12.7"] {
            for target in [
                "linux-x86_64",
                "linux-aarch64",
                "windows-x86_64",
                "macos-aarch64",
            ] {
                for versioned in [true, false] {
                    let resolved = release(version, target, versioned)
                        .resolve(None, target)
                        .unwrap();
                    assert_eq!(resolved.version, version);
                    assert!(
                        resolved
                            .archive
                            .browser_download_url
                            .contains(&format!("/v{version}/"))
                    );
                    assert!(resolved.archive.name.ends_with(&format!("{target}.zip")));
                }
            }
        }
    }
    #[test]
    fn explicit_versions_must_match_metadata_and_may_select_prereleases() {
        let expected = release_version("3.1.0").unwrap();
        assert!(
            release("3.0.2", "linux-x86_64", true)
                .resolve(Some(&expected), "linux-x86_64")
                .is_err()
        );
        let mut rc = release("3.1.0-rc.1", "linux-x86_64", true);
        rc.prerelease = true;
        let expected = release_version("v3.1.0-rc.1").unwrap();
        assert!(rc.resolve(Some(&expected), "linux-x86_64").is_ok());
        assert!(
            release("3.1.0-rc.1", "linux-x86_64", true)
                .resolve(None, "linux-x86_64")
                .is_err()
        );
        assert!(release_version("2.16.2").is_err());
        assert!(release_version("4.0.0").is_err());
        assert!(release_version("3.0.2+local").is_err());
    }
    #[test]
    fn missing_platform_or_checksums_and_draft_releases_fail_closed() {
        assert!(
            release("3.0.2", "linux-x86_64", true)
                .resolve(None, "windows-x86_64")
                .is_err()
        );
        let mut doc = release("3.0.2", "linux-x86_64", true);
        doc.assets.retain(|asset| asset.name != "SHA256SUMS");
        assert!(doc.resolve(None, "linux-x86_64").is_err());
        let mut doc = release("3.0.2", "linux-x86_64", true);
        doc.draft = true;
        assert!(doc.resolve(None, "linux-x86_64").is_err());
    }
    #[test]
    fn duplicate_incomplete_and_foreign_assets_are_rejected() {
        let mut doc = release("3.0.2", "windows-x86_64", true);
        doc.assets.push(doc.assets[0].clone());
        assert!(doc.resolve(None, "windows-x86_64").is_err());
        for index in [0, 1] {
            let mut doc = release("3.0.2", "windows-x86_64", true);
            doc.assets[index].state = "new".into();
            assert!(doc.resolve(None, "windows-x86_64").is_err());
            let mut doc = release("3.0.2", "windows-x86_64", true);
            doc.assets[index].size = 0;
            assert!(doc.resolve(None, "windows-x86_64").is_err());
            for (from, to) in [
                ("https://github.com/", "http://github.com/"),
                ("github.com", "example.com"),
                ("Future-Element/pinset", "other/project"),
                ("/v3.0.2/", "/v3.0.1/"),
            ] {
                let mut doc = release("3.0.2", "windows-x86_64", true);
                doc.assets[index].browser_download_url =
                    doc.assets[index].browser_download_url.replace(from, to);
                assert!(doc.resolve(None, "windows-x86_64").is_err());
            }
        }
    }
    #[test]
    fn checksum_matches_the_selected_name_once_and_requires_sha256() {
        let name = "pinset-v3.0.2-windows-x86_64.zip";
        let hash = "AB".repeat(32);
        assert_eq!(
            release_checksum(&format!("{hash} *{name}\r\n"), name).unwrap(),
            hash.to_ascii_lowercase()
        );
        for sums in [
            format!("{hash} pinset-v3.0.1-windows-x86_64.zip\n"),
            format!("{hash} {name}\n{hash} {name}\n"),
            format!("{hash} {name} trailing\n"),
            format!("{} {name}\n", "a".repeat(32)),
            format!("{} {name}\n", "z".repeat(64)),
        ] {
            assert!(release_checksum(&sums, name).is_err());
        }
    }
    #[test]
    fn successful_pair_keeps_both_backups_and_committed_journal() {
        let root = tempfile::tempdir().unwrap();
        let dst = root.path().join("bin");
        let source = root.path().join("new");
        fs::create_dir(&dst).unwrap();
        fs::create_dir(&source).unwrap();
        let names = vec!["pinset".into(), "pinset-shim".into()];
        for name in &names {
            fs::write(dst.join(name), format!("old-{name}")).unwrap();
            fs::write(source.join(name), format!("new-{name}")).unwrap();
        }
        let backup = replace_binary_pair(&source, &dst, root.path(), &names).unwrap();
        let journal: BinaryJournal =
            read_json(&root.path().join("state/self-update/transaction.json")).unwrap();
        assert_eq!(journal.phase, "committed");
        for name in &names {
            assert_eq!(
                fs::read_to_string(dst.join(name)).unwrap(),
                format!("new-{name}")
            );
            assert_eq!(
                fs::read_to_string(backup.join(name)).unwrap(),
                format!("old-{name}")
            );
        }
        // A prepared journal can restore the complete pair after an interrupted replacement.
        rollback(&journal).unwrap();
        for name in &names {
            assert_eq!(
                fs::read_to_string(dst.join(name)).unwrap(),
                format!("old-{name}")
            );
        }
    }
    #[test]
    fn partial_pair_replacement_rolls_back_both_original_binaries() {
        let root = tempfile::tempdir().unwrap();
        let dst = root.path().join("bin");
        let source = root.path().join("new");
        fs::create_dir(&dst).unwrap();
        fs::create_dir(&source).unwrap();
        for name in ["pinset", "pinset-shim"] {
            fs::write(dst.join(name), format!("old-{name}")).unwrap();
        }
        fs::write(source.join("pinset"), "new-cli").unwrap();
        assert!(
            replace_binary_pair(
                &source,
                &dst,
                root.path(),
                &["pinset".into(), "pinset-shim".into()]
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(dst.join("pinset")).unwrap(),
            "old-pinset"
        );
        assert_eq!(
            fs::read_to_string(dst.join("pinset-shim")).unwrap(),
            "old-pinset-shim"
        );
    }
}
