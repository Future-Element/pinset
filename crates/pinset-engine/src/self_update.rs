//! A CLI/shim pair is one update unit. Backups are on the destination volume
//! so rollback remains possible for custom installation directories.
use crate::*;
use reqwest::{Method, StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const LATEST_RELEASE: &str = "https://github.com/Future-Element/pinset/releases/latest";
const MAX_CHECKSUM_BYTES: u64 = 1024 * 1024;
const REQUEST_ATTEMPTS: u32 = 3;
const MAX_REDIRECTS: usize = 5;
const MAX_RETRY_WAIT: u64 = 5;

pub(crate) struct UpdateRelease {
    pub version: String,
    pub archive_name: String,
    pub archive_url: String,
    pub checksum: String,
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

fn latest_version(location: &str) -> Result<semver::Version> {
    let invalid = || {
        service_error(
            "PINSET_UPDATE_REDIRECT",
            "latest release must redirect to an official stable Pinset 3 tag",
        )
    };
    let url = url::Url::parse(LATEST_RELEASE)
        .expect("constant official release URL")
        .join(location)
        .map_err(|_| invalid())?;
    let parts: Vec<_> = url.path().split('/').collect();
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || parts.len() != 6
        || !parts[1].eq_ignore_ascii_case("Future-Element")
        || !parts[2].eq_ignore_ascii_case("pinset")
        || parts[3] != "releases"
        || parts[4] != "tag"
    {
        return Err(invalid());
    }
    let version = release_version(parts[5]).map_err(|_| invalid())?;
    if parts[5] != format!("v{version}") || !version.pre.is_empty() {
        return Err(invalid());
    }
    Ok(version)
}

fn retry_delay(retry_after: Option<&str>, attempt: u32) -> Option<Duration> {
    // A long delay or HTTP-date is reported to the caller instead of retrying
    // early or holding an interactive command open for an unbounded wait.
    let seconds = match retry_after {
        Some(value) => value.parse::<u64>().ok()?,
        None => 1 << attempt,
    };
    (seconds <= MAX_RETRY_WAIT).then(|| Duration::from_secs(seconds))
}

fn request(
    client: &Client,
    method: Method,
    url: &url::Url,
    resource: &str,
) -> Result<reqwest::blocking::Response> {
    for attempt in 0..REQUEST_ATTEMPTS {
        let response = match client.request(method.clone(), url.clone()).send() {
            Ok(response) => response,
            Err(error) => {
                if attempt + 1 < REQUEST_ATTEMPTS && (error.is_timeout() || error.is_connect()) {
                    std::thread::sleep(Duration::from_secs(1 << attempt));
                    continue;
                }
                return Err(service_error(
                    "PINSET_UPDATE_FETCH",
                    format!("{resource}: {}", error.without_url()),
                ));
            }
        };
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok());
        let exhausted = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0");
        let limited = status == StatusCode::TOO_MANY_REQUESTS
            || (status == StatusCode::FORBIDDEN && (retry_after.is_some() || exhausted));
        if limited || status.is_server_error() {
            // Do not blindly retry a rate limit with no advertised delay.
            let delay = if limited && retry_after.is_none() {
                None
            } else {
                retry_delay(retry_after, attempt)
            };
            if attempt + 1 < REQUEST_ATTEMPTS
                && let Some(delay) = delay
            {
                drop(response);
                std::thread::sleep(delay);
                continue;
            }
            let advice = retry_after
                .map(|value| format!("; retry after {value} (seconds or HTTP-date)"))
                .or_else(|| {
                    response
                        .headers()
                        .get("x-ratelimit-reset")
                        .and_then(|value| value.to_str().ok())
                        .map(|value| format!("; retry after UTC epoch {value}"))
                })
                .unwrap_or_default();
            return Err(service_error(
                if limited {
                    "PINSET_UPDATE_RATE_LIMIT"
                } else {
                    "PINSET_UPDATE_FETCH"
                },
                format!("{resource}: HTTP {}{advice}", status.as_u16()),
            ));
        }
        if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
            return Err(service_error(
                "PINSET_UPDATE_ACCESS",
                format!("{resource}: access denied (HTTP {})", status.as_u16()),
            ));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(service_error(
                "PINSET_UPDATE_NOT_FOUND",
                format!("{resource}: published release resource not found (HTTP 404)"),
            ));
        }
        if !status.is_success() && !status.is_redirection() {
            return Err(service_error(
                "PINSET_UPDATE_FETCH",
                format!("{resource}: HTTP {}", status.as_u16()),
            ));
        }
        return Ok(response);
    }
    unreachable!("bounded update request attempts always return")
}

fn fetch_checksums(client: &Client, base: &str) -> Result<String> {
    let initial = url::Url::parse(&format!("{base}SHA256SUMS"))
        .map_err(|_| service_error("PINSET_UPDATE_METADATA", "invalid official release URL"))?;
    let mut url = initial.clone();
    for redirects in 0..=MAX_REDIRECTS {
        let response = request(client, Method::GET, &url, "release checksum file")?;
        if response.status().is_redirection() {
            let invalid = || {
                service_error(
                    "PINSET_UPDATE_REDIRECT",
                    "invalid official checksum download redirect",
                )
            };
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(invalid)?;
            let next = url.join(location).map_err(|_| invalid())?;
            let allowed = match next.host_str() {
                Some("github.com") => next == initial,
                Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com") => {
                    true
                }
                _ => false,
            };
            if redirects == MAX_REDIRECTS
                || !allowed
                || next.scheme() != "https"
                || !next.username().is_empty()
                || next.password().is_some()
                || next.port().is_some()
                || next.fragment().is_some()
            {
                return Err(invalid());
            }
            url = next;
            continue;
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_CHECKSUM_BYTES)
        {
            return Err(service_error(
                "PINSET_UPDATE_CHECKSUM",
                "release checksum file exceeds 1 MiB",
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_CHECKSUM_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                service_error(
                    "PINSET_UPDATE_FETCH",
                    format!("read release checksum file: {error}"),
                )
            })?;
        if bytes.len() as u64 > MAX_CHECKSUM_BYTES {
            return Err(service_error(
                "PINSET_UPDATE_CHECKSUM",
                "release checksum file exceeds 1 MiB",
            ));
        }
        return String::from_utf8(bytes).map_err(|_| {
            service_error(
                "PINSET_UPDATE_CHECKSUM",
                "release checksum file is not UTF-8",
            )
        });
    }
    unreachable!("bounded download redirects always return")
}

fn select_archive(version: &semver::Version, target: &str, sums: &str) -> Result<UpdateRelease> {
    let mut selected = None;
    // Validate both recognized names, preferring the versioned package. A
    // malformed fallback must not hide behind a valid preferred entry.
    for name in [
        format!("pinset-v{version}-{target}.zip"),
        format!("pinset-{target}.zip"),
    ] {
        if sums.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|entry| entry.strip_prefix('*').unwrap_or(entry) == name)
        }) {
            let checksum = release_checksum(sums, &name)?;
            if selected.is_none() {
                selected = Some((name, checksum));
            }
        }
    }
    let (archive_name, checksum) = selected.ok_or_else(|| {
        service_error(
            "PINSET_UPDATE_ASSET",
            "published checksum file has no archive for this platform",
        )
    })?;
    Ok(UpdateRelease {
        archive_url: format!(
            "https://github.com/Future-Element/pinset/releases/download/v{version}/{archive_name}"
        ),
        version: version.to_string(),
        archive_name,
        checksum,
    })
}

pub(crate) fn resolve_release(requested: Option<&str>, target: &str) -> Result<UpdateRelease> {
    if ![
        "linux-x86_64",
        "linux-aarch64",
        "windows-x86_64",
        "macos-aarch64",
    ]
    .contains(&target)
    {
        return Err(service_error(
            "PINSET_UPDATE_ASSET",
            "no published Pinset archive for this platform",
        ));
    }
    let requested = requested.map(release_version).transpose()?;
    let client = http_client_builder()?
        .user_agent("pinset/3")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| Error::HttpClient { source })?;
    let version = match requested {
        Some(version) => version,
        None => {
            let response = request(
                &client,
                Method::HEAD,
                &url::Url::parse(LATEST_RELEASE).expect("constant official release URL"),
                "latest release",
            )?;
            if !response.status().is_redirection() {
                return Err(service_error(
                    "PINSET_UPDATE_REDIRECT",
                    "latest release did not redirect to a published tag",
                ));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    service_error(
                        "PINSET_UPDATE_REDIRECT",
                        "latest release redirect has no valid Location",
                    )
                })?;
            latest_version(location)?
        }
    };
    let base = format!("https://github.com/Future-Element/pinset/releases/download/v{version}/");
    let sums = fetch_checksums(&client, &base)?;
    select_archive(&version, target, &sums)
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
    fn release(version: &str, target: &str, versioned: bool) -> UpdateRelease {
        let version = release_version(version).unwrap();
        let name = if versioned {
            format!("pinset-v{version}-{target}.zip")
        } else {
            format!("pinset-{target}.zip")
        };
        select_archive(&version, target, &format!("{}  {name}\n", "ab".repeat(32))).unwrap()
    }
    #[test]
    fn future_versions_resolve_all_published_platforms_from_checksums() {
        for version in ["3.0.2", "3.1.0", "3.12.7"] {
            for target in [
                "linux-x86_64",
                "linux-aarch64",
                "windows-x86_64",
                "macos-aarch64",
            ] {
                for versioned in [true, false] {
                    let resolved = release(version, target, versioned);
                    assert_eq!(resolved.version, version);
                    assert!(resolved.archive_url.contains(&format!("/v{version}/")));
                    assert!(resolved.archive_name.ends_with(&format!("{target}.zip")));
                }
            }
        }
    }
    #[test]
    fn latest_redirect_requires_the_exact_official_stable_tag() {
        for location in [
            "https://github.com/Future-Element/pinset/releases/tag/v3.1.0",
            "/Future-Element/pinset/releases/tag/v3.1.0",
        ] {
            assert_eq!(latest_version(location).unwrap().to_string(), "3.1.0");
        }
        for location in [
            "https://example.com/Future-Element/pinset/releases/tag/v3.1.0",
            "http://github.com/Future-Element/pinset/releases/tag/v3.1.0",
            "https://github.com/other/pinset/releases/tag/v3.1.0",
            "https://user@github.com/Future-Element/pinset/releases/tag/v3.1.0",
            "https://github.com:444/Future-Element/pinset/releases/tag/v3.1.0",
            "/Future-Element/pinset/releases/tag/v3.1.0?source=other",
            "/Future-Element/pinset/releases/tag/v3.1.0#fragment",
            "/Future-Element/pinset/releases/tag/v3.1.0/extra",
            "/Future-Element/pinset/releases/tag/3.1.0",
            "/Future-Element/pinset/releases/tag/v3.1.0-rc.1",
            "/Future-Element/pinset/releases/tag/v3.1.0+local",
            "/Future-Element/pinset/releases/tag/v2.16.2",
            "/Future-Element/pinset/releases/tag/v4.0.0",
        ] {
            assert!(latest_version(location).is_err(), "{location}");
        }
        assert!(release_version("3.1.0-rc.1").is_ok());
        for version in ["2.16.2", "4.0.0", "3.0.2+local", "latest", "3.1"] {
            assert!(release_version(version).is_err());
        }
    }
    #[test]
    fn versioned_package_is_preferred_and_both_entries_are_validated() {
        let version = release_version("3.1.0").unwrap();
        let preferred = "pinset-v3.1.0-windows-x86_64.zip";
        let stable = "pinset-windows-x86_64.zip";
        let sums = format!(
            "{} {preferred}\n{} {stable}\n",
            "ab".repeat(32),
            "cd".repeat(32)
        );
        assert_eq!(
            select_archive(&version, "windows-x86_64", &sums)
                .unwrap()
                .archive_name,
            preferred
        );
        for invalid in [
            format!("{sums}{} {stable}\n", "cd".repeat(32)),
            format!("{} {preferred}\ninvalid {stable}\n", "ab".repeat(32)),
        ] {
            assert!(select_archive(&version, "windows-x86_64", &invalid).is_err());
        }
        assert!(select_archive(&version, "macos-aarch64", &sums).is_err());
        assert!(select_archive(&version, "windows-x86_64", "").is_err());
        assert!(resolve_release(Some("3.1.0"), "linux-s390x").is_err());
    }
    #[test]
    fn retries_are_bounded_and_never_retry_before_an_advertised_delay() {
        assert_eq!(retry_delay(None, 0), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(None, 1), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(Some("0"), 0), Some(Duration::ZERO));
        assert_eq!(retry_delay(Some("5"), 0), Some(Duration::from_secs(5)));
        for value in ["6", "3600", "Wed, 07 Oct 2026 12:00:00 GMT", "invalid"] {
            assert_eq!(retry_delay(Some(value), 0), None);
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
