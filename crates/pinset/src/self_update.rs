use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
    process::Command,
};

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const API: &str = "https://api.github.com/repos/Future-Element/pinset";
const RELEASES: &str = "https://github.com/Future-Element/pinset/releases/download";
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
}

pub(crate) fn outdated(prerelease: bool, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let latest = latest_release(prerelease)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let available = parse_tag(&latest.tag_name)?;
    let is_outdated = available > current;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": 1, "command": "self.outdated", "ok": true,
                "data": {"current": current.to_string(), "latest": available.to_string(), "outdated": is_outdated}
            }))?
        );
    } else if is_outdated {
        println!("Pinset {current} -> {available}");
    } else {
        println!("Pinset {current} is current");
    }
    Ok(())
}

pub(crate) fn update(requested: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let version = match requested {
        Some(value) => Version::parse(value.trim_start_matches('v'))?,
        None => parse_tag(&latest_release(false)?.tag_name)?,
    };
    if version < current {
        return Err(format!("refusing to downgrade Pinset from {current} to {version}").into());
    }
    if version == current {
        println!("Pinset {current} is already installed");
        return Ok(());
    }
    let archive = platform_archive()?;
    let base = format!("{RELEASES}/v{version}");
    let client = client()?;
    let checksums = download(&client, &format!("{base}/SHA256SUMS"), 1024 * 1024)?;
    let expected = expected_checksum(&checksums, archive)?;
    let bytes = download(&client, &format!("{base}/{archive}"), MAX_ASSET_BYTES)?;
    if hex_lower(&Sha256::digest(&bytes)) != expected {
        return Err(format!("SHA-256 mismatch for {archive}").into());
    }
    let (cli_bytes, shim_bytes) = extract_pair(archive, &bytes)?;
    publish(version, &cli_bytes, &shim_bytes)
}

fn latest_release(prerelease: bool) -> Result<Release, Box<dyn std::error::Error>> {
    let client = client()?;
    if !prerelease {
        let response = client
            .get(format!("{API}/releases/latest"))
            .send()?
            .error_for_status()?
            .text()?;
        return Ok(serde_json::from_str(&response)?);
    }
    let response = client
        .get(format!("{API}/releases?per_page=20"))
        .send()?
        .error_for_status()?
        .text()?;
    let releases: Vec<Release> = serde_json::from_str(&response)?;
    releases
        .into_iter()
        .find(|release| !release.draft)
        .ok_or_else(|| "no published Pinset release was found".into())
}

fn client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent(format!("pinset/{}", env!("CARGO_PKG_VERSION")))
        .build()
}

fn parse_tag(tag: &str) -> Result<Version, semver::Error> {
    Version::parse(tag.trim_start_matches('v'))
}

fn download(client: &Client, url: &str, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut response = client.get(url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err("release asset exceeds the download limit".into());
    }
    let mut bytes = Vec::new();
    response.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("release asset exceeds the download limit".into());
    }
    Ok(bytes)
}

fn expected_checksum(content: &[u8], archive: &str) -> Result<String, Box<dyn std::error::Error>> {
    let content = std::str::from_utf8(content)?;
    let matches = content
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            let name = fields.next()?;
            (name == archive && fields.next().is_none()).then_some(hash)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1
        || matches[0].len() != 64
        || !matches[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(
            format!("SHA256SUMS must contain exactly one valid entry for {archive}").into(),
        );
    }
    Ok(matches[0].to_ascii_lowercase())
}

fn platform_archive() -> Result<&'static str, Box<dyn std::error::Error>> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("pinset-windows-x86_64.zip"),
        ("linux", "x86_64") => Ok("pinset-linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Ok("pinset-linux-aarch64.tar.gz"),
        ("macos", "aarch64") => Ok("pinset-macos-aarch64.tar.gz"),
        _ => Err("this platform has no official Pinset self-update asset".into()),
    }
}

fn extract_pair(
    archive: &str,
    bytes: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    let cli_name = if cfg!(windows) {
        "pinset.exe"
    } else {
        "pinset"
    };
    let shim_name = if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    };
    let mut files = std::collections::BTreeMap::new();
    if archive.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            if entry.is_dir() || entry.name().contains(['/', '\\']) {
                return Err("release archive has an invalid structure".into());
            }
            let name = entry.name().to_owned();
            let mut content = Vec::new();
            entry
                .by_ref()
                .take(MAX_ASSET_BYTES)
                .read_to_end(&mut content)?;
            if files.insert(name.clone(), content).is_some() {
                return Err(format!("release archive contains duplicate entry {name}").into());
            }
        }
    } else {
        let gzip = flate2::read::GzDecoder::new(bytes);
        let mut tar = tar::Archive::new(gzip);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            if path.components().count() != 1 || !entry.header().entry_type().is_file() {
                return Err("release archive has an invalid structure".into());
            }
            let name = path
                .to_str()
                .ok_or("release archive entry is not UTF-8")?
                .to_owned();
            let mut content = Vec::new();
            entry
                .by_ref()
                .take(MAX_ASSET_BYTES)
                .read_to_end(&mut content)?;
            if files.insert(name.clone(), content).is_some() {
                return Err(format!("release archive contains duplicate entry {name}").into());
            }
        }
    }
    if files.len() != 2 {
        return Err("release archive must contain exactly the CLI and shim".into());
    }
    let cli = files
        .remove(cli_name)
        .ok_or("release archive is missing pinset")?;
    let shim = files
        .remove(shim_name)
        .ok_or("release archive is missing pinset-shim")?;
    Ok((cli, shim))
}

fn publish(
    version: Version,
    cli_bytes: &[u8],
    shim_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let current = std::env::current_exe()?;
    let directory = current
        .parent()
        .ok_or("Pinset executable has no parent directory")?;
    let cli_name = if cfg!(windows) {
        "pinset.exe"
    } else {
        "pinset"
    };
    let shim_name = if cfg!(windows) {
        "pinset-shim.exe"
    } else {
        "pinset-shim"
    };
    let cli = directory.join(cli_name);
    let shim = directory.join(shim_name);
    if !same_path(&current, &cli) || !shim.is_file() {
        return Err(
            "self update requires pinset and pinset-shim in the same installation directory".into(),
        );
    }
    let new_cli = directory.join(format!(".{cli_name}.{version}.new"));
    let new_shim = directory.join(format!(".{shim_name}.{version}.new"));
    write_new(&new_cli, cli_bytes)?;
    write_new(&new_shim, shim_bytes)?;
    let output = Command::new(&new_cli).arg("--version").output()?;
    let expected_version = format!("pinset {version}");
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != expected_version
    {
        let _ = fs::remove_file(&new_cli);
        let _ = fs::remove_file(&new_shim);
        return Err("downloaded Pinset binary failed its version handshake".into());
    }
    #[cfg(windows)]
    return publish_windows(&version, &cli, &shim, &new_cli, &new_shim);
    #[cfg(not(windows))]
    publish_unix(&version, &cli, &shim, &new_cli, &new_shim)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn publish_unix(
    version: &Version,
    cli: &Path,
    shim: &Path,
    new_cli: &Path,
    new_shim: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli_backup = cli.with_extension("bak");
    let shim_backup = shim.with_extension("bak");
    if cli_backup.exists() {
        fs::remove_file(&cli_backup)?;
    }
    if shim_backup.exists() {
        fs::remove_file(&shim_backup)?;
    }
    fs::rename(cli, &cli_backup)?;
    if let Err(error) = fs::rename(new_cli, cli)
        .and_then(|_| fs::rename(shim, &shim_backup))
        .and_then(|_| fs::rename(new_shim, shim))
    {
        let _ = fs::remove_file(cli);
        let _ = fs::rename(&cli_backup, cli);
        let _ = fs::rename(&shim_backup, shim);
        return Err(error.into());
    }
    if !Command::new(cli).arg("--version").status()?.success() {
        let _ = fs::remove_file(cli);
        let _ = fs::remove_file(shim);
        fs::rename(cli_backup, cli)?;
        fs::rename(shim_backup, shim)?;
        return Err("updated Pinset failed validation and was rolled back".into());
    }
    println!("updated Pinset to {version}");
    Ok(())
}

#[cfg(windows)]
fn publish_windows(
    version: &Version,
    cli: &Path,
    shim: &Path,
    new_cli: &Path,
    new_shim: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::process::CommandExt;
    let helper = cli
        .parent()
        .unwrap()
        .join(format!(".pinset-update-{}.ps1", std::process::id()));
    let script = r#"param([int]$OldPid,[string]$Cli,[string]$Shim,[string]$NewCli,[string]$NewShim)
$ErrorActionPreference='Stop'; Wait-Process -Id $OldPid -ErrorAction SilentlyContinue
$cliBak="$Cli.bak"; $shimBak="$Shim.bak"
try { Move-Item -LiteralPath $Cli -Destination $cliBak -Force; Move-Item -LiteralPath $NewCli -Destination $Cli -Force; Move-Item -LiteralPath $Shim -Destination $shimBak -Force; Move-Item -LiteralPath $NewShim -Destination $Shim -Force; & $Cli --version; if ($LASTEXITCODE -ne 0) { throw 'version validation failed' } }
catch { Remove-Item -LiteralPath $Cli,$Shim -Force -ErrorAction SilentlyContinue; Move-Item -LiteralPath $cliBak -Destination $Cli -Force -ErrorAction SilentlyContinue; Move-Item -LiteralPath $shimBak -Destination $Shim -Force -ErrorAction SilentlyContinue }
finally { Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue }
"#;
    write_new(&helper, script.as_bytes())?;
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&helper)
        .arg("-OldPid")
        .arg(std::process::id().to_string())
        .arg("-Cli")
        .arg(cli)
        .arg("-Shim")
        .arg(shim)
        .arg("-NewCli")
        .arg(new_cli)
        .arg("-NewShim")
        .arg(new_shim)
        .creation_flags(0x08000000)
        .spawn()?;
    println!("Pinset {version} was verified; replacement will finish after this process exits");
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_manifest_requires_one_exact_archive_entry() {
        let hash = "ab".repeat(32);
        assert_eq!(
            expected_checksum(
                format!("{hash}  pinset-windows-x86_64.zip\n").as_bytes(),
                "pinset-windows-x86_64.zip"
            )
            .unwrap(),
            hash
        );
        assert!(
            expected_checksum(
                format!("{hash}  other.zip\n").as_bytes(),
                "pinset-windows-x86_64.zip"
            )
            .is_err()
        );
        assert!(
            expected_checksum(
                format!("{hash}  pinset-windows-x86_64.zip\n{hash}  pinset-windows-x86_64.zip\n")
                    .as_bytes(),
                "pinset-windows-x86_64.zip"
            )
            .is_err()
        );
    }

    #[test]
    fn self_update_rejects_unknown_platform_shapes_and_downgrades_by_semver() {
        assert!(Version::parse("1.9.0").unwrap() < Version::parse("2.0.0-rc.1").unwrap());
        assert_eq!(parse_tag("v2.0.0-rc.1").unwrap().to_string(), "2.0.0-rc.1");
    }
}
