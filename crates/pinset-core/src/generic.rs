//! Declarative precompiled tools. These models never download or execute payloads.
use crate::{LockedArtifactFormat, LockedTool, Result, SUPPORTED_TARGETS, TOOLS, failure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const GENERIC_PROVIDER: &str = "generic-binary";
pub const GENERIC_VERIFICATION: &str = "generic-https-sha256";
pub const GENERIC_GITHUB_VERIFICATION: &str = "generic-github-attestation-sha256";
pub const MAX_GITHUB_ATTESTATION_BYTES: usize = 2 * 1024 * 1024;

/// Only canonical public GitHub Release paths establish a repository identity.
pub fn github_release_repository(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let parts = url.path_segments()?.collect::<Vec<_>>();
    if parts.len() != 6
        || parts[2] != "releases"
        || parts[3] != "download"
        || parts.iter().any(|part| part.is_empty())
    {
        return None;
    }
    if parts[..2].iter().any(|part| {
        matches!(*part, "." | "..")
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    }) {
        return None;
    }
    Some(format!("{}/{}", parts[0], parts[1]))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericTool {
    pub url: String,
    pub format: LockedArtifactFormat,
    pub executable: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub os: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arch: BTreeMap<String, String>,
}

fn invalid(message: impl Into<String>) -> crate::Error {
    failure("PINSET_GENERIC_INVALID", message)
}

pub fn validate_generic_name(name: &str) -> Result<()> {
    crate::validate_id(name)?;
    let reserved = ["pinset", "pinset-shim", "con", "prn", "aux", "nul"];
    if name.bytes().any(|c| c.is_ascii_uppercase())
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || TOOLS.contains(&name)
        || crate::command_tool(name).is_some()
        || reserved.contains(&name)
        || ["com", "lpt"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|v| v.len() == 1 && v.as_bytes()[0].is_ascii_digit())
        })
    {
        return Err(invalid(format!(
            "{name:?} is reserved or is not a portable lowercase command name"
        )));
    }
    Ok(())
}

pub fn validate_tool_name(name: &str) -> Result<()> {
    if TOOLS.contains(&name) {
        Ok(())
    } else {
        validate_generic_name(name)
    }
}

pub fn validate_generic_version(version: &str) -> Result<()> {
    if version.is_empty()
        || version.len() > 128
        || matches!(
            version,
            "." | ".." | "latest" | "lts" | "stable" | "current" | "nightly"
        )
        || !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b'+'))
    {
        return Err(invalid(
            "generic tools require an explicit version/tag, not a range or floating alias",
        ));
    }
    Ok(())
}

pub fn validate_generic_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value).map_err(|_| invalid("generic download URL is invalid"))?;
    let public = match url.host() {
        Some(url::Host::Domain(host)) => {
            let host = host.trim_end_matches('.');
            host.contains('.') && !host.ends_with(".localhost") && !host.ends_with(".local")
        }
        Some(url::Host::Ipv4(ip)) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_documentation()
        }
        Some(url::Host::Ipv6(ip)) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && ip.to_ipv4_mapped().is_none()
        }
        None => false,
    };
    if url.scheme() != "https"
        || !public
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "generic downloads require public HTTPS URLs without credentials or fragments",
        ));
    }
    Ok(())
}

pub fn validate_generic_path(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains(['\\', ':', '\0'])
        || value.split('/').any(|part| {
            part.is_empty() || matches!(part, "." | "..") || part.chars().any(char::is_control)
        })
    {
        return Err(invalid(
            "generic executable must be a safe relative path using '/' separators",
        ));
    }
    Ok(())
}

impl GenericTool {
    pub fn validate(&self, name: &str) -> Result<()> {
        validate_generic_name(name)?;
        if self.format == LockedArtifactFormat::TarXz {
            return Err(invalid("generic format must be tar.gz, zip or binary"));
        }
        for (map, keys) in [
            (&self.os, ["linux", "macos", "windows"].as_slice()),
            (&self.arch, ["x86_64", "aarch64"].as_slice()),
        ] {
            if map.iter().any(|(key, value)| {
                !keys.contains(&key.as_str())
                    || value.is_empty()
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
            }) {
                return Err(invalid("invalid generic OS/architecture mapping"));
            }
        }
        for target in SUPPORTED_TARGETS {
            self.artifact_url("1.0.0", target)?;
            self.executable_path("1.0.0", target)?;
        }
        Ok(())
    }

    fn render(&self, template: &str, version: &str, target: &str) -> Result<String> {
        validate_generic_version(version)?;
        if !SUPPORTED_TARGETS.contains(&target) {
            return Err(invalid("unsupported generic target"));
        }
        let (os, arch) = target.split_once('-').expect("validated platform");
        let os = self.os.get(os).map(String::as_str).unwrap_or(os);
        let arch = self.arch.get(arch).map(String::as_str).unwrap_or(arch);
        let value = template
            .replace("{version}", version)
            .replace("{os}", os)
            .replace("{arch}", arch)
            .replace("{target}", target)
            .replace(
                "{exe}",
                if target.starts_with("windows-") {
                    ".exe"
                } else {
                    ""
                },
            );
        if value.contains(['{', '}']) {
            return Err(invalid("unknown generic template variable"));
        }
        Ok(value)
    }

    pub fn artifact_url(&self, version: &str, target: &str) -> Result<String> {
        let url = self.render(&self.url, version, target)?;
        validate_generic_url(&url)?;
        Ok(url)
    }

    pub fn executable_path(&self, version: &str, target: &str) -> Result<String> {
        let path = self.render(&self.executable, version, target)?;
        validate_generic_path(&path)?;
        Ok(path)
    }
}

pub fn generic_executable(tool: &LockedTool, target: &str) -> Result<String> {
    let entry = tool
        .options
        .get(&format!("executable:{target}"))
        .ok_or_else(|| invalid("generic lock is missing its executable path"))?;
    validate_generic_path(entry)?;
    Ok(entry.clone())
}

pub fn validate_generic_lock(tool: &LockedTool) -> Result<()> {
    validate_generic_name(&tool.name)?;
    validate_generic_version(&tool.version)?;
    if !tool.is_generic()
        || tool.requested != tool.version
        || tool.options.len() != tool.artifacts.len()
    {
        return Err(invalid(
            "generic lock must contain an exact version and one executable per artifact",
        ));
    }
    for artifact in &tool.artifacts {
        validate_generic_url(&artifact.canonical_url)?;
        generic_executable(tool, &artifact.target)?;
        match (&artifact.attestation, artifact.verification.as_str()) {
            (None, GENERIC_VERIFICATION) => {}
            (Some(evidence), GENERIC_GITHUB_VERIFICATION)
                if !evidence.bundle.is_empty()
                    && evidence.bundle.len() <= MAX_GITHUB_ATTESTATION_BYTES
                    && github_release_repository(&artifact.canonical_url)
                        .is_some_and(|repo| repo.eq_ignore_ascii_case(&evidence.repository)) => {}
            _ => {
                return Err(invalid(
                    "generic verification method and GitHub attestation evidence disagree",
                ));
            }
        }
        if !SUPPORTED_TARGETS.contains(&artifact.target.as_str())
            || artifact.format == LockedArtifactFormat::TarXz
            || !artifact.overlays.is_empty()
            || !artifact.archive_root.is_empty()
            || artifact.integrity.is_some()
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid(
                "generic lock contains an invalid platform artifact or SHA-256 digest",
            ));
        }
    }
    Ok(())
}

/// Registry aliases for generic tools must route only their own command name.
pub fn validate_registered_command(command: &str, tool: &str) -> Result<()> {
    crate::validate_managed_command_name(command)?;
    if !TOOLS.contains(&tool) {
        validate_generic_name(tool)?;
        if command != tool {
            return Err(invalid(
                "generic command registry entry must match its tool name",
            ));
        }
    }
    Ok(())
}
