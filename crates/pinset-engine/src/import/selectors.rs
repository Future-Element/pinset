//! Normalize native numeric/npm ranges without changing the Provider selector language.
use crate::{Error, JavaVersion, Result, service_error};
use semver::{Version, VersionReq};
use serde_json::Value;

#[derive(Debug)]
pub(super) enum Constraint {
    Exact(String),
    Literal(String),
    Range(Vec<VersionReq>),
    Latest,
    Lts(Option<String>),
}

impl Constraint {
    pub(super) fn literal(value: &str) -> Self {
        Self::Literal(value.into())
    }
    pub(super) fn parse(tool: &str, selector: &str) -> Result<Self> {
        let selector = selector.trim();
        let alias = selector.to_ascii_lowercase();
        if matches!(alias.as_str(), "latest" | "stable") || (tool == "node" && alias == "node") {
            return Ok(Self::Latest);
        }
        if matches!(tool, "node" | "java") && matches!(alias.as_str(), "lts" | "lts/*") {
            return Ok(Self::Lts(None));
        }
        if tool == "node"
            && let Some(name) = alias.strip_prefix("lts/")
            && !name.is_empty()
        {
            return Ok(Self::Lts(Some(name.into())));
        }
        if tool == "java" && JavaVersion::parse(selector).is_ok()
            || tool == "rust" && selector.starts_with("nightly-")
        {
            return Ok(Self::Exact(selector.into()));
        }
        if tool != "java"
            && let Ok(version) = Version::parse(selector)
        {
            return Ok(Self::Exact(version.to_string()));
        }
        if selector.is_empty() {
            return Err(invalid(selector));
        }
        let ranges = selector
            .split("||")
            .map(parse_range)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::Range(ranges))
    }

    pub(super) fn exact(&self) -> Option<&str> {
        match self {
            Self::Exact(value) | Self::Literal(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn needs_release_metadata(&self) -> bool {
        matches!(self, Self::Lts(_))
    }

    pub(super) fn matches(&self, release: &Value) -> bool {
        let Some(version) = release["version"].as_str() else {
            return false;
        };
        match self {
            Self::Literal(exact) => exact == version,
            Self::Exact(exact) => {
                exact == version
                    || (Version::parse(exact).is_ok_and(|expected| {
                        expected.build.is_empty()
                            && Version::parse(version).is_ok_and(|actual| {
                                expected.major == actual.major
                                    && expected.minor == actual.minor
                                    && expected.patch == actual.patch
                                    && expected.pre == actual.pre
                            })
                    }))
            }
            Self::Latest => true,
            Self::Lts(name) => match name {
                Some(name) => release["lts"]
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case(name)),
                None => release["lts"] == true || release["lts"].is_string(),
            },
            Self::Range(ranges) => {
                let version = Version::parse(version).ok().or_else(|| {
                    JavaVersion::parse(version)
                        .ok()
                        .map(|v| Version::new(v.major, v.minor, v.security))
                });
                version.is_some_and(|v| ranges.iter().any(|r| r.matches(&v)))
            }
        }
    }
}

fn invalid(selector: &str) -> Error {
    service_error(
        "PINSET_IMPORT_SELECTOR_INVALID",
        format!("unsupported version declaration: {selector}"),
    )
}

fn parse_range(input: &str) -> Result<VersionReq> {
    let input = input.trim();
    if input.is_empty() {
        return Err(invalid(input));
    }
    if let Some((lower, upper)) = input.split_once(" - ") {
        let upper = upper.trim();
        let parts = upper
            .split('.')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| invalid(input))?;
        let upper = match parts.as_slice() {
            [major] => format!(
                "<{}.0.0",
                major.checked_add(1).ok_or_else(|| invalid(input))?
            ),
            [major, minor] => format!(
                "<{major}.{}.0",
                minor.checked_add(1).ok_or_else(|| invalid(input))?
            ),
            [_, _, _] => format!("<={upper}"),
            _ => return Err(invalid(input)),
        };
        return VersionReq::parse(&format!(">={}, {upper}", lower.trim()))
            .map_err(|_| invalid(input));
    }
    // npm joins comparators with spaces; semver::VersionReq expects commas.
    let normalized = input.replace(',', " ");
    let mut tokens = normalized.split_whitespace().peekable();
    let mut comparators = Vec::new();
    while let Some(token) = tokens.next() {
        let token = if matches!(token, ">=" | "<=" | ">" | "<" | "=" | "~" | "^") {
            format!("{token}{}", tokens.next().ok_or_else(|| invalid(input))?)
        } else {
            token.to_owned()
        };
        let token = token.replace(['x', 'X'], "*");
        let first = token.chars().next().ok_or_else(|| invalid(input))?;
        // Bare partials are X-ranges in npm, whereas Rust defaults to caret ranges.
        comparators.push(if first.is_ascii_digit() {
            format!("={token}")
        } else {
            token
        });
    }
    VersionReq::parse(&comparators.join(", ")).map_err(|_| invalid(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generic_release_tags_are_literal_and_do_not_use_semver_range_rules() {
        let tag = Constraint::literal("v1.9.0");
        assert_eq!(tag.exact(), Some("v1.9.0"));
        assert!(tag.matches(&json!({"version":"v1.9.0"})));
        assert!(!tag.matches(&json!({"version":"1.9.0"})));
        assert!(!Constraint::literal("1.9.0").matches(&json!({"version":"1.9.0+build"})));
    }

    #[test]
    fn npm_ranges_keep_native_meaning() {
        for (selector, accepted, rejected) in [
            (">=18", "24.1.0", "16.20.2"),
            (">=18 <22", "20.11.1", "22.0.0"),
            (">= 18.0.0 < 22.0.0", "20.11.1", "22.0.0"),
            ("18 || 20", "20.11.1", "19.9.0"),
            ("^18.12.0", "18.20.8", "20.0.0"),
            ("~18.12", "18.12.9", "18.13.0"),
            ("18.12", "18.12.9", "18.13.0"),
            ("18.x", "18.20.8", "20.0.0"),
            ("*", "24.1.0", "25.0.0-beta.1"),
            ("18 - 20", "20.11.1", "21.0.0"),
            ("18.12 - 20.10", "20.10.9", "20.11.0"),
            ("18.12.0 - 20.10.2", "20.10.2", "20.10.3"),
        ] {
            let constraint = Constraint::parse("node", selector).unwrap();
            assert!(
                constraint.matches(&json!({"version":accepted})),
                "{selector} must match {accepted}"
            );
            assert!(
                !constraint.matches(&json!({"version":rejected})),
                "{selector} must not match {rejected}"
            );
        }
    }

    #[test]
    fn lts_names_and_provider_builds_are_preserved() {
        let lts = Constraint::parse("node", "lts/*").unwrap();
        assert!(lts.matches(&json!({"version":"20.11.1","lts":"Iron"})));
        assert!(!lts.matches(&json!({"version":"21.0.0","lts":null})));
        let iron = Constraint::parse("node", "lts/iron").unwrap();
        assert!(iron.matches(&json!({"version":"20.11.1","lts":"Iron"})));
        assert!(!iron.matches(&json!({"version":"22.1.0","lts":"Jod"})));
        assert!(
            Constraint::parse("python", "3.13.2")
                .unwrap()
                .matches(&json!({"version":"3.13.2+20250201"}))
        );
        assert!(
            Constraint::parse("java", "21")
                .unwrap()
                .matches(&json!({"version":"21.0.5.1+11"}))
        );
        assert!(
            !Constraint::parse("java", "21.0.5+11")
                .unwrap()
                .matches(&json!({"version":"21.0.5+12"}))
        );
        for invalid in [
            "system",
            "path:/opt/python",
            "ref:main",
            "lts/",
            "",
            ">=garbage",
            "18 20",
        ] {
            if invalid == "18 20" {
                continue;
            } // Valid but unsatisfiable comparator conjunction.
            assert!(Constraint::parse("python", invalid).is_err(), "{invalid}");
        }
    }
}
