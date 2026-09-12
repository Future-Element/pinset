//! Bounded static compatibility rules. Unrecognized syntax is unknown, never success.
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub [u64; 3]);

impl Version {
    pub fn parse(value: &str) -> Option<Self> {
        let (version, count) = numeric(value)?;
        (count == 3).then_some(version)
    }
}

fn numeric(value: &str) -> Option<(Version, usize)> {
    let mut output = [0; 3];
    let parts: Vec<_> = value.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        output[index] = part.parse().ok()?;
    }
    Some((Version(output), parts.len()))
}

fn upper(version: Version, position: usize) -> Option<Version> {
    let mut output = version.0;
    output[position] = output[position].checked_add(1)?;
    for value in &mut output[position + 1..] {
        *value = 0;
    }
    Some(Version(output))
}

/// Stable Node semver ranges: comparison sets, OR, hyphens, wildcards, tilde and caret.
/// Prereleases, tags and malformed expressions are intentionally unknown.
pub fn node_matches(requirement: &str, actual: &str) -> Option<bool> {
    if requirement.len() > 4096 {
        return None;
    }
    let actual = Version::parse(actual.strip_prefix('v').unwrap_or(actual))?;
    let mut matches = false;
    for alternative in requirement.split("||") {
        matches |= node_set(alternative.trim(), actual)?;
    }
    Some(matches)
}

fn node_set(requirement: &str, actual: Version) -> Option<bool> {
    if requirement.is_empty() {
        return None;
    }
    if let Some((left, right)) = requirement.split_once(" - ") {
        let (start, _) = numeric(left.trim().trim_start_matches('v'))?;
        let (end, count) = numeric(right.trim().trim_start_matches('v'))?;
        return Some(
            actual >= start
                && if count == 3 {
                    actual <= end
                } else {
                    actual < upper(end, count - 1)?
                },
        );
    }
    let tokens: Vec<_> = requirement.split_whitespace().collect();
    if tokens.len() > 64 {
        return None;
    }
    let mut index = 0;
    let mut matches = true;
    while index < tokens.len() {
        let token = tokens[index];
        let joined;
        let predicate = if [">", ">=", "<", "<=", "=", "~", "^"].contains(&token) {
            index += 1;
            joined = format!("{token}{}", tokens.get(index)?);
            joined.as_str()
        } else {
            token
        };
        matches &= node_predicate(predicate, actual)?;
        index += 1;
    }
    Some(matches)
}

fn operator(value: &str) -> (&str, &str) {
    for prefix in [">=", "<=", "!=", "==", "~=", ">", "<", "=", "~", "^"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return (prefix, rest);
        }
    }
    ("", value)
}

fn node_predicate(predicate: &str, actual: Version) -> Option<bool> {
    let (op, value) = operator(predicate);
    let value = value.strip_prefix('v').unwrap_or(value);
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() > 3 {
        return None;
    }
    let wildcard = parts
        .iter()
        .position(|part| matches!(*part, "x" | "X" | "*"));
    if let Some(position) = wildcard
        && parts[position..]
            .iter()
            .any(|part| !matches!(*part, "x" | "X" | "*"))
    {
        return None;
    }
    let count = wildcard.unwrap_or(parts.len());
    if count == 0 {
        return matches!(op, "" | "=" | "~" | "^").then_some(true);
    }
    let (lower, _) = numeric(&parts[..count].join("."))?;
    let partial = count < 3;
    Some(match op {
        "" | "=" if partial => actual >= lower && actual < upper(lower, count - 1)?,
        "" | "=" => actual == lower,
        ">=" => actual >= lower,
        ">" if partial => actual >= upper(lower, count - 1)?,
        ">" => actual > lower,
        "<" => actual < lower,
        "<=" if partial => actual < upper(lower, count - 1)?,
        "<=" => actual <= lower,
        "~" => actual >= lower && actual < upper(lower, if count == 1 { 0 } else { 1 })?,
        "^" => {
            let position = lower
                .0
                .iter()
                .take(count)
                .position(|value| *value != 0)
                .unwrap_or(count - 1);
            actual >= lower && actual < upper(lower, position)?
        }
        _ => return None,
    })
}

/// PEP 440 release-only Requires-Python comparisons. Unsupported qualifiers remain unknown.
pub fn python_matches(requirement: &str, actual: &str) -> Option<bool> {
    if requirement.len() > 4096 {
        return None;
    }
    let actual = Version::parse(actual.split('+').next()?)?;
    let mut matches = true;
    for (index, predicate) in requirement.split(',').enumerate() {
        if index >= 64 {
            return None;
        }
        let (op, value) = operator(predicate.trim());
        let value = value.trim();
        if let Some(prefix) = value.strip_suffix(".*") {
            if !matches!(op, "==" | "!=") {
                return None;
            }
            let (lower, count) = numeric(prefix)?;
            let equal = actual >= lower && actual < upper(lower, count - 1)?;
            matches &= if op == "==" { equal } else { !equal };
            continue;
        }
        let (expected, count) = numeric(value)?;
        matches &= match op {
            "==" => actual == expected,
            "!=" => actual != expected,
            ">=" => actual >= expected,
            ">" => actual > expected,
            "<=" => actual <= expected,
            "<" => actual < expected,
            "~=" if count >= 2 => actual >= expected && actual < upper(expected, count - 2)?,
            _ => return None,
        };
    }
    Some(matches)
}

/// Official Gradle JVM runtime matrix, revision 2026-09-12 (through Gradle 9.7 / JDK 26).
pub fn gradle_java_matches(gradle: &str, java: u64) -> Option<bool> {
    let (gradle, _) = numeric(gradle)?;
    if gradle < Version([2, 0, 0]) || gradle >= Version([9, 8, 0]) {
        return None;
    }
    let minimum = match java {
        8 => [2, 0, 0],
        9 => [4, 3, 0],
        10 => [4, 7, 0],
        11 => [5, 0, 0],
        12 => [5, 4, 0],
        13 => [6, 0, 0],
        14 => [6, 3, 0],
        15 => [6, 7, 0],
        16 => [7, 0, 0],
        17 => [7, 3, 0],
        18 => [7, 5, 0],
        19 => [7, 6, 0],
        20 => [8, 3, 0],
        21 => [8, 5, 0],
        22 => [8, 8, 0],
        23 => [8, 10, 0],
        24 => [8, 14, 0],
        25 => [9, 1, 0],
        26 => [9, 4, 0],
        _ => return None,
    };
    Some(gradle >= Version(minimum) && (java >= 17 || gradle < Version([9, 0, 0])))
}

/// Whether the one pinned SDK can satisfy global.json without another installed SDK.
pub fn dotnet_matches(requested: &str, roll_forward: &str, actual: &str) -> Option<bool> {
    let requested = Version::parse(requested)?;
    let actual = Version::parse(actual)?;
    if roll_forward == "disable" {
        return Some(actual == requested);
    }
    let ordering = actual.cmp(&requested);
    let same_major = actual.0[0] == requested.0[0];
    let same_minor = same_major && actual.0[1] == requested.0[1];
    let same_feature = same_minor && actual.0[2] / 100 == requested.0[2] / 100;
    Some(
        ordering != Ordering::Less
            && match roll_forward {
                "patch" | "latestPatch" => same_feature,
                "feature" | "latestFeature" => same_minor,
                "minor" | "latestMinor" => same_major,
                "major" | "latestMajor" => true,
                _ => return None,
            },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_ranges_match_package_manager_semantics() {
        for (range, yes, no) in [
            (">=18 <21", "20.1.0", "21.0.0"),
            ("^0.2.3", "0.2.9", "0.3.0"),
            ("^0.0.3", "0.0.3", "0.0.4"),
            ("^0.0", "0.0.9", "0.1.0"),
            ("~18.2", "18.2.9", "18.3.0"),
            ("18.x || >=20.1.0", "20.1.1", "19.9.0"),
            ("18 - 20", "20.9.0", "21.0.0"),
            ("> 18", "19.0.0", "18.9.0"),
            ("<=18.2", "18.2.9", "18.3.0"),
            ("18.2.3", "18.2.3", "18.2.4"),
        ] {
            assert_eq!(node_matches(range, yes), Some(true), "{range}: {yes}");
            assert_eq!(node_matches(range, no), Some(false), "{range}: {no}");
        }
        for range in ["lts/*", ">=cat", "1.x.2", ">=20.0.0-rc.1", "", "==20"] {
            assert_eq!(node_matches(range, "20.0.0"), None, "{range}");
        }
    }

    #[test]
    fn python_compatible_release_and_exclusion_are_not_node_semver() {
        assert_eq!(python_matches("~=3.10", "3.12.2+build"), Some(true));
        assert_eq!(python_matches("~=3.10.2", "3.11.0"), Some(false));
        assert_eq!(python_matches(">=3.10,!=3.11.*,<4", "3.11.7"), Some(false));
        assert_eq!(python_matches(">=3.10,!=3.11.*,<4", "3.12.7"), Some(true));
        assert_eq!(python_matches("==3.10", "3.10.0"), Some(true));
        for range in ["^3.10", "~=3", ">=3.10rc1", "==3.x", ""] {
            assert_eq!(python_matches(range, "3.10.0"), None);
        }
    }

    #[test]
    fn gradle_requires_the_matching_jvm_generation() {
        assert_eq!(gradle_java_matches("8.4", 21), Some(false));
        assert_eq!(gradle_java_matches("8.5", 21), Some(true));
        assert_eq!(gradle_java_matches("9.0", 11), Some(false));
        assert_eq!(gradle_java_matches("9.8", 26), None);
        assert_eq!(gradle_java_matches("8.5-SNAPSHOT", 21), None);
        assert_eq!(gradle_java_matches("9.7.1", 27), None);
    }

    #[test]
    fn dotnet_feature_bands_do_not_follow_minor_versions() {
        assert_eq!(dotnet_matches("8.0.302", "patch", "8.0.303"), Some(true));
        assert_eq!(dotnet_matches("8.0.302", "patch", "8.0.400"), Some(false));
        assert_eq!(dotnet_matches("8.0.302", "feature", "8.0.400"), Some(true));
        assert_eq!(
            dotnet_matches("8.0.302", "latestMajor", "9.0.100"),
            Some(true)
        );
        assert_eq!(
            dotnet_matches("8.0.302", "latestMajor", "8.0.301"),
            Some(false)
        );
        assert_eq!(dotnet_matches("8.0.302", "disable", "8.0.303"), Some(false));
        assert_eq!(dotnet_matches("8.0.302", "custom", "8.0.303"), None);
    }
}
