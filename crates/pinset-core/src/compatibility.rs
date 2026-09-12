//! Local, bounded compatibility checks. Reading project declarations never executes them.
use crate::compatibility_ranges::{
    Version, dotnet_matches, gradle_java_matches, node_matches, python_matches,
};
use crate::{EnvironmentCheck, Lockfile, ReadinessState};
use std::{collections::BTreeMap, fs, path::Path};

const REVISION: &str = "2026-09-12";
const NODE: &str = "https://docs.npmjs.com/cli/v11/configuring-npm/package-json#engines";
const PYTHON: &str = "https://packaging.python.org/en/latest/specifications/version-specifiers/";
const GRADLE: &str = "https://docs.gradle.org/current/userguide/compatibility.html";
const AGP: &str = "https://developer.android.com/build/releases/agp-8-0-0-release-notes";
const GO: &str = "https://go.dev/doc/toolchain";
const RUST: &str = "https://rust-lang.github.io/rustup/overrides.html";
const DOTNET: &str = "https://learn.microsoft.com/en-us/dotnet/core/tools/global-json";

enum Source {
    Missing,
    Text(String),
    Unknown,
}
fn read(root: &Path, relative: &str) -> Source {
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Source::Missing,
        Err(_) => return Source::Unknown,
    };
    // Resolve ancestors too: a directory symlink must not make background checks read another project.
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > 1024 * 1024
        || !fs::canonicalize(&path)
            .ok()
            .zip(fs::canonicalize(root).ok())
            .is_some_and(|(path, root)| path.starts_with(root))
    {
        return Source::Unknown;
    }
    fs::read_to_string(path).map_or(Source::Unknown, Source::Text)
}

fn result(id: &str, declaration: &str, source: &str, matches: Option<bool>) -> EnvironmentCheck {
    EnvironmentCheck {
        id: format!("compatibility.{id}.r1"),
        state: match matches {
            Some(true) => ReadinessState::Pass,
            Some(false) => ReadinessState::Fail,
            None => ReadinessState::Unknown,
        },
        reason: format!(
            "{declaration}: {}; rule revision {REVISION}",
            match matches {
                Some(true) => "locked environment satisfies the declaration",
                Some(false) => "locked environment conflicts with the declaration",
                None => "declaration or compatibility is not statically established",
            }
        ),
        next_step: Some(format!(
            "Review the declared requirement and locked runtime; rule source: {source}"
        )),
    }
}
fn absent(id: &str, declaration: &str, source: &str) -> EnvironmentCheck {
    let mut check = result(id, declaration, source, None);
    check.state = ReadinessState::NotApplicable;
    check.reason = format!("{declaration}: no applicable declaration; rule revision {REVISION}");
    check
}

/// Overrides are explicit input to keep rule tests independent of process-global environment.
pub fn check_compatibility(
    root: &Path,
    lock: &Lockfile,
    overrides: &BTreeMap<String, String>,
) -> Vec<EnvironmentCheck> {
    let mut checks = Vec::new();
    if let Some(node) = lock.tool("node") {
        let check = match read(root, "package.json") {
            Source::Missing => absent("node.engines", "package.json#engines.node", NODE),
            Source::Unknown => result("node.engines", "package.json#engines.node", NODE, None),
            Source::Text(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => match value.pointer("/engines/node") {
                    None => absent("node.engines", "package.json#engines.node", NODE),
                    Some(range) => {
                        let matched = range
                            .as_str()
                            .and_then(|range| node_matches(range, &node.version));
                        let declaration = if matched.is_some() {
                            format!(
                                "package.json#engines.node {} vs locked {}",
                                range.as_str().unwrap(),
                                node.version
                            )
                        } else {
                            "package.json#engines.node".to_owned()
                        };
                        result("node.engines", &declaration, NODE, matched)
                    }
                },
                Err(_) => result("node.engines", "package.json#engines.node", NODE, None),
            },
        };
        checks.push(check);
    }
    if let Some(python) = lock.tool("python") {
        checks.push(match read(root, "pyproject.toml") {
            Source::Missing => absent(
                "python.requires",
                "pyproject.toml#project.requires-python",
                PYTHON,
            ),
            Source::Unknown => result(
                "python.requires",
                "pyproject.toml#project.requires-python",
                PYTHON,
                None,
            ),
            Source::Text(text) => match toml::from_str::<toml::Value>(&text) {
                Ok(value) => match value
                    .get("project")
                    .and_then(|value| value.get("requires-python"))
                {
                    Some(range) => result(
                        "python.requires",
                        "pyproject.toml#project.requires-python",
                        PYTHON,
                        range
                            .as_str()
                            .and_then(|range| python_matches(range, &python.version)),
                    ),
                    None => absent(
                        "python.requires",
                        "pyproject.toml#project.requires-python",
                        PYTHON,
                    ),
                },
                Err(_) => result(
                    "python.requires",
                    "pyproject.toml#project.requires-python",
                    PYTHON,
                    None,
                ),
            },
        });
    }
    if let Some(go) = lock.tool("go") {
        for filename in ["go.mod", "go.work"] {
            match read(root, filename) {
                Source::Missing => checks.push(absent(&format!("go.{filename}"), filename, GO)),
                Source::Unknown => {
                    checks.push(result(&format!("go.{filename}"), filename, GO, None))
                }
                Source::Text(text) => {
                    let mut found = false;
                    for line in text.lines() {
                        let fields: Vec<_> = line
                            .split("//")
                            .next()
                            .unwrap_or("")
                            .split_whitespace()
                            .collect();
                        if !fields
                            .first()
                            .is_some_and(|field| matches!(*field, "go" | "toolchain"))
                        {
                            continue;
                        }
                        found = true;
                        let matched =
                            fields
                                .get(1)
                                .filter(|_| fields.len() == 2)
                                .and_then(|value| {
                                    if fields[0] == "toolchain" && *value == "default" {
                                        return Some(true);
                                    }
                                    let value = value.trim_start_matches("go");
                                    let expected = if value.split('.').count() == 2 {
                                        format!("{value}.0")
                                    } else {
                                        value.to_owned()
                                    };
                                    Some(Version::parse(&go.version)? >= Version::parse(&expected)?)
                                });
                        checks.push(result(
                            &format!("go.{filename}.{}", fields[0]),
                            &format!("{filename}#{}", fields[0]),
                            GO,
                            matched,
                        ));
                    }
                    if !found {
                        checks.push(result(&format!("go.{filename}"), filename, GO, None));
                    }
                }
            }
        }
        checks.push(result(
            "go.download-override",
            "GOTOOLCHAIN (Pinset defaults to local)",
            GO,
            Some(
                overrides
                    .get("GOTOOLCHAIN")
                    .is_none_or(|value| value == "local"),
            ),
        ));
        checks.push(
            if overrides
                .get("GOWORK")
                .is_some_and(|value| !value.is_empty() && value != "off" && value != "auto")
            {
                result(
                    "go.workspace-override",
                    "external GOWORK selection",
                    GO,
                    None,
                )
            } else {
                absent("go.workspace-override", "external GOWORK selection", GO)
            },
        );
    }
    if let Some(rust) = lock.tool("rust") {
        match read(root, "rust-toolchain.toml") {
            Source::Missing => {
                checks.push(absent("rust.options", "rust-toolchain.toml", RUST));
                checks.push(match read(root, "rust-toolchain") {
                    Source::Missing => absent("rust.channel", "rust-toolchain", RUST),
                    Source::Unknown => result("rust.channel", "rust-toolchain", RUST, None),
                    Source::Text(channel) => result(
                        "rust.channel",
                        "rust-toolchain",
                        RUST,
                        rust_channel_matches(channel.trim(), &rust.version),
                    ),
                });
            }
            Source::Unknown => {
                checks.push(result("rust.options", "rust-toolchain.toml", RUST, None))
            }
            Source::Text(text) => {
                let value = toml::from_str::<toml::Value>(&text).ok();
                let toolchain = value.as_ref().and_then(|value| value.get("toolchain"));
                checks.push(match toolchain.and_then(|value| value.get("channel")) {
                    Some(channel) => result(
                        "rust.channel",
                        "rust-toolchain.toml#toolchain.channel",
                        RUST,
                        channel
                            .as_str()
                            .and_then(|channel| rust_channel_matches(channel, &rust.version)),
                    ),
                    None if toolchain.is_some() => absent(
                        "rust.channel",
                        "rust-toolchain.toml#toolchain.channel",
                        RUST,
                    ),
                    None => result(
                        "rust.channel",
                        "rust-toolchain.toml#toolchain.channel",
                        RUST,
                        None,
                    ),
                });
                for field in ["components", "targets"] {
                    let declaration = format!("rust-toolchain.toml#toolchain.{field}");
                    let matched = toolchain.and_then(|value| value.get(field)).map(|value| {
                        let wanted = value.as_array()?;
                        // The manifest expands default/minimal profiles into actual components.
                        let installed: Vec<_> = if field == "components" {
                            rust.metadata.get(field)
                        } else {
                            rust.options.get(field)
                        }
                        .map_or("", String::as_str)
                        .split(',')
                        .collect();
                        let mut matched = true;
                        for item in wanted {
                            let requested = item.as_str()?;
                            matched &= installed
                                .iter()
                                .any(|value| component_name(value) == component_name(requested));
                        }
                        Some(matched)
                    });
                    checks.push(match matched {
                        Some(matched) => {
                            result(&format!("rust.{field}"), &declaration, RUST, matched)
                        }
                        None if toolchain.is_some() => {
                            absent(&format!("rust.{field}"), &declaration, RUST)
                        }
                        None => result(&format!("rust.{field}"), &declaration, RUST, None),
                    });
                }
                if toolchain.is_some_and(|value| value.get("path").is_some()) {
                    checks.push(result(
                        "rust.path-toolchain",
                        "rust-toolchain.toml#toolchain.path",
                        RUST,
                        None,
                    ));
                }
            }
        }
        for variable in ["RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"] {
            if overrides
                .get(variable)
                .is_some_and(|value| !value.is_empty())
            {
                checks.push(result(
                    &format!("rust.{}", variable.to_ascii_lowercase()),
                    variable,
                    RUST,
                    None,
                ));
            }
        }
    }
    if let Some(dotnet) = lock.tool("dotnet") {
        checks.push(match read(root, "global.json") {
            Source::Missing => absent("dotnet.sdk", "global.json#sdk", DOTNET),
            Source::Unknown => result("dotnet.sdk", "global.json#sdk", DOTNET, None),
            Source::Text(text) => {
                let value = json5::from_str::<serde_json::Value>(&text).ok();
                let matched = value.as_ref().and_then(|value| {
                    let sdk = value.get("sdk")?;
                    let requested = sdk.get("version")?.as_str()?;
                    let policy = match sdk.get("rollForward") {
                        Some(value) => value.as_str()?,
                        None => "patch",
                    };
                    if sdk.get("paths").is_some() {
                        return None;
                    }
                    dotnet_matches(requested, policy, &dotnet.version)
                });
                result(
                    "dotnet.sdk",
                    "global.json#sdk.version/rollForward",
                    DOTNET,
                    matched,
                )
            }
        });
    }
    if let Some(java) = lock.tool("java") {
        let java = java
            .version
            .split('.')
            .next()
            .and_then(|value| value.parse::<u64>().ok());
        for prefix in ["", "android/"] {
            let wrapper = format!("{prefix}gradle/wrapper/gradle-wrapper.properties");
            match read(root, &wrapper) {
                Source::Missing => {}
                Source::Unknown => checks.push(result(
                    &format!("java.{prefix}gradle"),
                    &wrapper,
                    GRADLE,
                    None,
                )),
                Source::Text(text) => {
                    let version = text.lines().find_map(|line| {
                        let line = line.trim();
                        if !line.starts_with("distributionUrl=") {
                            return None;
                        }
                        let filename = line.rsplit('/').next()?;
                        filename
                            .strip_prefix("gradle-")?
                            .strip_suffix("-bin.zip")
                            .or_else(|| filename.strip_prefix("gradle-")?.strip_suffix("-all.zip"))
                    });
                    checks.push(result(
                        &format!("java.{prefix}gradle"),
                        &wrapper,
                        GRADLE,
                        version
                            .zip(java)
                            .and_then(|(gradle, java)| gradle_java_matches(gradle, java)),
                    ));
                }
            }
        }
        // Gradle files are executable programs. Only literal plugin declarations are inspected.
        for filename in [
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "android/build.gradle",
            "android/build.gradle.kts",
            "android/settings.gradle",
            "android/settings.gradle.kts",
        ] {
            let text = match read(root, filename) {
                Source::Missing => continue,
                Source::Unknown => {
                    checks.push(result(&format!("java.agp.{filename}"), filename, AGP, None));
                    continue;
                }
                Source::Text(text) => text,
            };
            let mut block_comment = false;
            for (index, line) in text.lines().enumerate() {
                let line = line.trim();
                if block_comment {
                    if line.contains("*/") {
                        block_comment = false;
                    }
                    continue;
                }
                if line.starts_with("/*") {
                    block_comment = !line.contains("*/");
                    continue;
                }
                if line.starts_with("//")
                    || !(line.contains("com.android.application")
                        || line.contains("com.android.library")
                        || line.contains("com.android.tools.build:gradle:"))
                {
                    continue;
                }
                let version = literal_agp(line);
                let matched = version.as_deref().zip(java).and_then(|(version, java)| {
                    let major = version.split('.').next()?.parse::<u64>().ok()?;
                    match major {
                        8 => Some(java >= 17),
                        _ => None,
                    }
                });
                checks.push(result(
                    &format!("java.agp.{filename}.{index}"),
                    &format!("{filename}#android-gradle-plugin"),
                    AGP,
                    matched,
                ));
            }
        }
    }
    checks
}

fn rust_channel_matches(channel: &str, locked: &str) -> Option<bool> {
    // Floating channels and dated prereleases need upstream metadata; never infer
    // that a stable local compiler satisfies them from the channel name alone.
    if !(2..=3).contains(&channel.split('.').count())
        || !channel.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return None;
    }
    let requested = Version::parse(&if channel.split('.').count() == 2 {
        format!("{channel}.0")
    } else {
        channel.to_owned()
    })?;
    let actual = Version::parse(locked)?;
    Some(if channel.split('.').count() == 2 {
        channel.split('.').eq(locked.split('.').take(2))
    } else {
        requested == actual
    })
}

fn component_name(value: &str) -> &str {
    match value {
        "rustfmt-preview" => "rustfmt",
        "clippy-preview" => "clippy",
        other => other,
    }
}

/// Read the npm shipped inside the selected Node installation, never a separate npm selection.
pub fn check_bundled_npm(installation: &Path, node: &str, target: &str) -> EnvironmentCheck {
    let relative = if target.starts_with("windows-") {
        "node_modules/npm/package.json"
    } else {
        "lib/node_modules/npm/package.json"
    };
    let matched = match read(installation, relative) {
        Source::Text(text) => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| node_matches(value.pointer("/engines/node")?.as_str()?, node)),
        _ => None,
    };
    result(
        "node.bundled-npm",
        "bundled npm package.json#engines.node vs selected Node",
        NODE,
        matched,
    )
}

fn literal_agp(line: &str) -> Option<String> {
    if !["id(", "id '", "id \"", "classpath ", "classpath("]
        .iter()
        .any(|prefix| line.trim_start().starts_with(prefix))
    {
        return None;
    }
    if let Some((_, value)) = line.split_once("com.android.tools.build:gradle:") {
        let version = value.split(['\'', '"']).next()?;
        return Version::parse(version).map(|_| version.to_owned());
    }
    let (_, value) = line.split_once("version")?;
    let value = value.trim().trim_start_matches('(').trim();
    let quote = value
        .chars()
        .next()
        .filter(|value| matches!(value, '\'' | '"'))?;
    let version = value[1..].split(quote).next()?;
    Version::parse(version).map(|_| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lock(tool: &str, version: &str) -> Lockfile {
        Lockfile {
            schema: crate::LOCKFILE_SCHEMA,
            generated_by: "fixture".to_owned(),
            tools: vec![crate::LockedTool {
                name: tool.to_owned(),
                requested: version.to_owned(),
                version: version.to_owned(),
                provider: "fixture".to_owned(),
                released_at: None,
                metadata: BTreeMap::new(),
                options: BTreeMap::new(),
                artifacts: vec![],
            }],
        }
    }
    #[test]
    fn declarations_cover_pass_conflict_unknown_and_not_applicable() {
        let root = tempfile::tempdir().unwrap();
        for (tool, version, file, passing, failing, unknown, rule) in [
            (
                "node",
                "20.1.0",
                "package.json",
                r#"{"engines":{"node":">=18 <21"}}"#,
                r#"{"engines":{"node":">=22"}}"#,
                r#"{"engines":{"node":"lts/*"}}"#,
                "compatibility.node.engines.r1",
            ),
            (
                "python",
                "3.12.0",
                "pyproject.toml",
                "[project]\nrequires-python = '>=3.10,<4'",
                "[project]\nrequires-python = '<3.12'",
                "[project]\nrequires-python = '^3.12'",
                "compatibility.python.requires.r1",
            ),
            (
                "go",
                "1.24.0",
                "go.mod",
                "go 1.23",
                "go 1.25",
                "go dynamic",
                "compatibility.go.go.mod.go.r1",
            ),
            (
                "rust",
                "1.97.1",
                "rust-toolchain.toml",
                "[toolchain]\ncomponents = []",
                "[toolchain]\ncomponents = ['rust-src']",
                "[toolchain]\ncomponents = [true]",
                "compatibility.rust.components.r1",
            ),
            (
                "dotnet",
                "8.0.303",
                "global.json",
                r#"{"sdk":{"version":"8.0.302"}}"#,
                r#"{"sdk":{"version":"9.0.100"}}"#,
                r#"{"sdk":{"version":"8.0.302","rollForward":"custom"}}"#,
                "compatibility.dotnet.sdk.r1",
            ),
        ] {
            let lock = lock(tool, version);
            for (content, state) in [
                (passing, ReadinessState::Pass),
                (failing, ReadinessState::Fail),
                (unknown, ReadinessState::Unknown),
            ] {
                fs::write(root.path().join(file), content).unwrap();
                let checks = check_compatibility(root.path(), &lock, &BTreeMap::new());
                assert_eq!(
                    checks.iter().find(|check| check.id == rule).unwrap().state,
                    state,
                    "{tool}: {content}"
                );
            }
            fs::remove_file(root.path().join(file)).unwrap();
            let checks = check_compatibility(root.path(), &lock, &BTreeMap::new());
            assert!(
                checks
                    .iter()
                    .any(|check| check.state == ReadinessState::NotApplicable),
                "{tool}"
            );
        }
    }
    #[test]
    fn npm_ten_is_rejected_with_node_twelve_without_selecting_a_separate_npm() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("lib/node_modules/npm/package.json");
        fs::create_dir_all(package.parent().unwrap()).unwrap();
        fs::write(
            package,
            r#"{"version":"10.0.0","engines":{"node":"^18.17.0 || >=20.0.0"}}"#,
        )
        .unwrap();
        assert_eq!(
            check_bundled_npm(root.path(), "12.22.12", "linux-x86_64").state,
            ReadinessState::Fail
        );
        assert_eq!(
            check_bundled_npm(root.path(), "20.1.0", "linux-x86_64").state,
            ReadinessState::Pass
        );
    }
    #[test]
    fn literal_agp_does_not_execute_or_guess_gradle_code() {
        assert_eq!(
            literal_agp("id(\"com.android.application\") version \"8.2.1\" apply false").as_deref(),
            Some("8.2.1")
        );
        assert_eq!(
            literal_agp("classpath 'com.android.tools.build:gradle:8.0.0'").as_deref(),
            Some("8.0.0")
        );
        assert_eq!(
            literal_agp("id(\"com.android.application\") version libs.versions.agp.get()"),
            None
        );
    }
    #[test]
    fn rust_channels_and_java_gradle_conflicts_remain_bounded() {
        assert_eq!(rust_channel_matches("1.97", "1.97.1"), Some(true));
        assert_eq!(rust_channel_matches("1.96.0", "1.97.1"), Some(false));
        assert_eq!(rust_channel_matches("nightly-2026-09-01", "1.97.1"), None);
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("gradle/wrapper")).unwrap();
        fs::write(
            root.path().join("gradle/wrapper/gradle-wrapper.properties"),
            "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.4-bin.zip\n",
        )
        .unwrap();
        fs::write(
            root.path().join("build.gradle.kts"),
            "id(\"com.android.application\") version \"8.0.0\"\n",
        )
        .unwrap();
        for (java, gradle, agp) in [
            ("21.0.0", ReadinessState::Fail, ReadinessState::Pass),
            ("11.0.0", ReadinessState::Pass, ReadinessState::Fail),
        ] {
            let checks = check_compatibility(root.path(), &lock("java", java), &BTreeMap::new());
            assert_eq!(
                checks
                    .iter()
                    .find(|check| check.id == "compatibility.java.gradle.r1")
                    .unwrap()
                    .state,
                gradle
            );
            assert_eq!(
                checks
                    .iter()
                    .find(|check| check.id == "compatibility.java.agp.build.gradle.kts.0.r1")
                    .unwrap()
                    .state,
                agp
            );
        }
    }
    #[test]
    fn source_limits_preserve_the_background_read_boundary() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(read(root.path(), "absent"), Source::Missing));
        fs::create_dir(root.path().join("directory")).unwrap();
        assert!(matches!(read(root.path(), "directory"), Source::Unknown));
        fs::write(root.path().join("oversized"), vec![b' '; 1024 * 1024 + 1]).unwrap();
        assert!(matches!(read(root.path(), "oversized"), Source::Unknown));
    }
}
