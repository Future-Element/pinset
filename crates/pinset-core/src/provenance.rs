//! Shared supply-chain verification vocabulary and policy enforcement.
//!
//! Providers keep ownership of their protocol-specific cryptography, while this module defines
//! the common result contract used by locks, policy checks, audits, and downgrade protection.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Error, LockedTool, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStrength {
    Checksum,
    SignedChecksum,
    Provenance,
}

impl VerificationStrength {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Checksum => "checksum",
            Self::SignedChecksum => "signed-checksum",
            Self::Provenance => "provenance",
        }
    }
}

impl std::fmt::Display for VerificationStrength {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationMethod {
    HttpsChecksum,
    OpenPgpSignedChecksum,
    NpmRegistrySignature,
    MinisignSignedChecksum,
    SigstoreBundle,
    GitHubAttestation,
    SlsaProvenance,
}

impl VerificationMethod {
    pub const fn strength(self) -> VerificationStrength {
        match self {
            Self::HttpsChecksum => VerificationStrength::Checksum,
            Self::OpenPgpSignedChecksum
            | Self::NpmRegistrySignature
            | Self::MinisignSignedChecksum => VerificationStrength::SignedChecksum,
            Self::SigstoreBundle | Self::GitHubAttestation | Self::SlsaProvenance => {
                VerificationStrength::Provenance
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "node-metadata")]
pub(crate) struct VerifiedPayload {
    pub payload: Vec<u8>,
    pub signer: Option<String>,
}

/// Protocol-specific verifiers implement this interface instead of exposing bespoke success
/// values to Provider metadata code.
#[cfg(feature = "node-metadata")]
pub(crate) trait ProvenanceVerifier {
    fn method(&self) -> VerificationMethod;
    fn verify(&self, input: &[u8]) -> Result<VerifiedPayload>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimumReleaseAge(Duration);

impl MinimumReleaseAge {
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    fn parse(input: &str) -> std::result::Result<Self, String> {
        let input = input.trim();
        let (digits, unit) = input.split_at(
            input
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(input.len()),
        );
        if digits.is_empty() || unit.len() != 1 {
            return Err("expected a duration such as 7d, 24h, 30m, or 60s".to_owned());
        }
        let value = digits
            .parse::<u64>()
            .map_err(|_| "duration value is too large".to_owned())?;
        if value == 0 {
            return Err("duration must be greater than zero".to_owned());
        }
        let seconds = match unit {
            "d" => value.checked_mul(86_400),
            "h" => value.checked_mul(3_600),
            "m" => value.checked_mul(60),
            "s" => Some(value),
            _ => None,
        }
        .ok_or_else(|| "duration is too large or uses an unsupported unit".to_owned())?;
        Ok(Self(Duration::from_secs(seconds)))
    }
}

impl Serialize for MinimumReleaseAge {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let seconds = self.0.as_secs();
        let value = if seconds.is_multiple_of(86_400) {
            format!("{}d", seconds / 86_400)
        } else if seconds.is_multiple_of(3_600) {
            format!("{}h", seconds / 3_600)
        } else if seconds.is_multiple_of(60) {
            format!("{}m", seconds / 60)
        } else {
            format!("{seconds}s")
        };
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for MinimumReleaseAge {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

pub fn verification_method(value: &str) -> Option<VerificationMethod> {
    let base = value.split_once("-source:").map_or(value, |(base, _)| base);
    match base {
        "nodejs-openpgp-sha256" | "openpgp-signed-checksum-sha256" => {
            Some(VerificationMethod::OpenPgpSignedChecksum)
        }
        "npm-registry-signature-sha512" => Some(VerificationMethod::NpmRegistrySignature),
        "minisign-signed-checksum-sha256" => Some(VerificationMethod::MinisignSignedChecksum),
        "sigstore-bundle-sha256" => Some(VerificationMethod::SigstoreBundle),
        "github-attestation-sha256" => Some(VerificationMethod::GitHubAttestation),
        "slsa-provenance-v1-sha256" => Some(VerificationMethod::SlsaProvenance),
        "go-download-json-sha256"
        | "flutter-release-json-sha256"
        | "python-build-standalone-versions-sha256"
        | "adoptium-api-sha256"
        | "rust-v2-manifest-sha256"
        | "dotnet-release-metadata-sha512" => Some(VerificationMethod::HttpsChecksum),
        _ => None,
    }
}

pub fn tool_verification_strength(tool: &LockedTool) -> Result<VerificationStrength> {
    tool.artifacts
        .iter()
        .flat_map(|artifact| {
            std::iter::once(artifact.verification.as_str()).chain(
                artifact
                    .overlays
                    .iter()
                    .map(|overlay| overlay.verification.as_str()),
            )
        })
        .map(|verification| {
            verification_method(verification)
                .map(VerificationMethod::strength)
                .ok_or_else(|| Error::InvalidLockfile {
                    reason: format!("unsupported verification method {verification:?}"),
                })
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min()
        .ok_or_else(|| Error::InvalidLockfile {
            reason: format!("{} has no verification evidence", tool.name),
        })
}

pub fn validate_verification_transition(previous: &LockedTool, next: &LockedTool) -> Result<()> {
    let previous_strength = tool_verification_strength(previous)?;
    let next_strength = tool_verification_strength(next)?;
    if next_strength < previous_strength {
        return Err(Error::VerificationDowngrade {
            tool: next.name.clone(),
            previous: previous_strength.to_string(),
            next: next_strength.to_string(),
        });
    }
    Ok(())
}

pub fn validate_tool_policy(
    tool: &LockedTool,
    minimum_verification: Option<VerificationStrength>,
    minimum_release_age: Option<MinimumReleaseAge>,
    now: SystemTime,
) -> Result<()> {
    if let Some(required) = minimum_verification {
        let actual = tool_verification_strength(tool)?;
        if actual < required {
            return Err(Error::VerificationPolicyViolation {
                tool: tool.name.clone(),
                required: required.to_string(),
                actual: actual.to_string(),
            });
        }
    }
    if let Some(required_age) = minimum_release_age {
        let released_at =
            tool.released_at
                .as_deref()
                .ok_or_else(|| Error::ReleaseAgeUnavailable {
                    tool: tool.name.clone(),
                })?;
        let release_time =
            parse_release_time(released_at).ok_or_else(|| Error::InvalidLockfile {
                reason: format!(
                    "{} has invalid released-at value {released_at:?}",
                    tool.name
                ),
            })?;
        let actual = now
            .duration_since(release_time)
            .map_err(|_| Error::ReleaseTooNew {
                tool: tool.name.clone(),
                released_at: released_at.to_owned(),
                required: format_duration(required_age.as_duration()),
            })?;
        if actual < required_age.as_duration() {
            return Err(Error::ReleaseTooNew {
                tool: tool.name.clone(),
                released_at: released_at.to_owned(),
                required: format_duration(required_age.as_duration()),
            });
        }
    }
    Ok(())
}

pub fn valid_release_time(value: &str) -> bool {
    parse_release_time(value).is_some()
}

fn parse_release_time(value: &str) -> Option<SystemTime> {
    let date = value.get(..10)?;
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if value.len() > 10 && !valid_time_suffix(&value[10..]) {
        return None;
    }
    let year = date[0..4].parse::<i64>().ok()?;
    let month = date[5..7].parse::<u32>().ok()?;
    let day = date[8..10].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let within_day = if value.len() == 10 {
        // A date-only record does not identify the publication instant. Treat it as the end of
        // that UTC day so a minimum-age policy never becomes eligible prematurely.
        86_399_i64
    } else {
        parse_time_suffix(&value[10..])?
    };
    let seconds = days.checked_mul(86_400)?.checked_add(within_day)?;
    if seconds < 0 {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
}

fn valid_time_suffix(value: &str) -> bool {
    parse_time_suffix(value).is_some()
}

fn parse_time_suffix(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 9
        || bytes[0] != b'T'
        || bytes[3] != b':'
        || bytes[6] != b':'
        || !bytes[1..3].iter().all(u8::is_ascii_digit)
        || !bytes[4..6].iter().all(u8::is_ascii_digit)
        || !bytes[7..9].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let hour = value[1..3].parse::<u32>().ok();
    let minute = value[4..6].parse::<u32>().ok();
    let second = value[7..9].parse::<u32>().ok();
    if hour.is_none_or(|value| value > 23)
        || minute.is_none_or(|value| value > 59)
        || second.is_none_or(|value| value > 60)
    {
        return None;
    }
    let local_seconds = i64::from(hour?) * 3_600 + i64::from(minute?) * 60 + i64::from(second?);
    let zone = &value[9..];
    if zone == "Z" {
        return Some(local_seconds);
    }
    if let Some(fraction) = zone.strip_prefix('.') {
        return fraction
            .strip_suffix('Z')
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|_| local_seconds);
    }
    let (sign, offset) = zone.split_at_checked(1)?;
    if !matches!(sign, "+" | "-") || offset.len() != 5 || offset.as_bytes()[2] != b':' {
        return None;
    }
    let hours = offset[0..2]
        .parse::<i64>()
        .ok()
        .filter(|value| *value <= 23)?;
    let minutes = offset[3..5]
        .parse::<i64>()
        .ok()
        .filter(|value| *value <= 59)?;
    let offset_seconds = hours * 3_600 + minutes * 60;
    Some(if sign == "+" {
        local_seconds - offset_seconds
    } else {
        local_seconds + offset_seconds
    })
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

// Days since 1970-01-01, based on Howard Hinnant's civil calendar algorithm.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(86_400) {
        format!("{}d", seconds / 86_400)
    } else if seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{LockedArtifact, LockedArtifactFormat};

    use super::*;

    fn locked_tool(verification: &str, released_at: Option<&str>) -> LockedTool {
        LockedTool {
            name: "example".to_owned(),
            requested: "1.0.0".to_owned(),
            version: "1.0.0".to_owned(),
            provider: "test".to_owned(),
            released_at: released_at.map(str::to_owned),
            metadata: BTreeMap::new(),
            artifacts: vec![LockedArtifact {
                target: "test".to_owned(),
                canonical_url: "https://example.test/archive.tar.gz".to_owned(),
                artifact_path: "archive.tar.gz".to_owned(),
                sha256: "a".repeat(64),
                integrity: None,
                format: LockedArtifactFormat::TarGz,
                archive_root: "example".to_owned(),
                verification: verification.to_owned(),
                overlays: Vec::new(),
            }],
        }
    }

    #[test]
    fn classifies_existing_and_future_verification_methods() {
        assert_eq!(
            verification_method("nodejs-openpgp-sha256-source:mirror")
                .map(VerificationMethod::strength),
            Some(VerificationStrength::SignedChecksum)
        );
        assert_eq!(
            verification_method("github-attestation-sha256").map(VerificationMethod::strength),
            Some(VerificationStrength::Provenance)
        );
        assert_eq!(
            verification_method("go-download-json-sha256-source:mirror")
                .map(VerificationMethod::strength),
            Some(VerificationStrength::Checksum)
        );
    }

    #[test]
    fn rejects_verification_downgrades() {
        let previous = locked_tool("github-attestation-sha256", Some("2026-01-01"));
        let next = locked_tool("nodejs-openpgp-sha256", Some("2026-01-01"));
        assert!(matches!(
            validate_verification_transition(&previous, &next),
            Err(Error::VerificationDowngrade { .. })
        ));
    }

    #[test]
    fn enforces_strength_and_release_age_without_network_access() {
        let tool = locked_tool("nodejs-openpgp-sha256", Some("2026-08-01"));
        let now = UNIX_EPOCH + Duration::from_secs(days_from_civil(2026, 8, 20) as u64 * 86_400);
        assert!(
            validate_tool_policy(
                &tool,
                Some(VerificationStrength::SignedChecksum),
                Some(MinimumReleaseAge::parse("7d").expect("duration")),
                now,
            )
            .is_ok()
        );
        assert!(matches!(
            validate_tool_policy(&tool, Some(VerificationStrength::Provenance), None, now,),
            Err(Error::VerificationPolicyViolation { .. })
        ));

        let unknown_age = locked_tool("go-download-json-sha256", None);
        assert!(matches!(
            validate_tool_policy(
                &unknown_age,
                None,
                Some(MinimumReleaseAge::parse("1d").expect("duration")),
                now,
            ),
            Err(Error::ReleaseAgeUnavailable { .. })
        ));
    }

    #[test]
    fn duration_and_release_time_parsing_are_strict() {
        assert_eq!(
            MinimumReleaseAge::parse("48h")
                .expect("duration")
                .as_duration(),
            Duration::from_secs(172_800)
        );
        assert!(MinimumReleaseAge::parse("0d").is_err());
        assert!(MinimumReleaseAge::parse("1w").is_err());
        assert!(!valid_release_time("2026-02-29"));
        assert!(valid_release_time("2024-02-29T12:00:00Z"));
        assert!(!valid_release_time("2024-02-29T25:00:00Z"));
        let start_of_next_day =
            UNIX_EPOCH + Duration::from_secs(days_from_civil(2026, 8, 21) as u64 * 86_400);
        let dated = locked_tool("go-download-json-sha256", Some("2026-08-20"));
        assert!(matches!(
            validate_tool_policy(
                &dated,
                None,
                Some(MinimumReleaseAge::parse("1d").expect("duration")),
                start_of_next_day,
            ),
            Err(Error::ReleaseTooNew { .. })
        ));
    }
}
