use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{PROJECT_CONFIG_FILENAME, RuntimeDiscoveryKind, runtime_provider, runtime_providers};

const MAX_SOURCE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryKind {
    Selection,
    Constraint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryStatus {
    Ready,
    Informational,
    Ignored,
    Unsupported,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryFinding {
    pub tool: String,
    pub source: String,
    pub field: Option<String>,
    pub raw: String,
    pub normalized: Option<String>,
    pub kind: DiscoveryKind,
    pub status: DiscoveryStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryReport {
    pub start: PathBuf,
    pub boundary: PathBuf,
    pub target_config: PathBuf,
    pub can_import: bool,
    pub findings: Vec<DiscoveryFinding>,
}

pub fn scan_project_sources(start: &Path) -> io::Result<DiscoveryReport> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()?.join(start)
    };
    if !start.is_dir() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("discovery start is not a directory: {}", start.display()),
        ));
    }

    let ancestors = start.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    let mut boundary_index = None;
    for (index, directory) in ancestors.iter().enumerate() {
        if git_marker(directory)? {
            boundary_index = Some(index);
            break;
        }
    }
    let directories = if let Some(index) = boundary_index {
        ancestors[..=index].to_vec()
    } else {
        vec![start.clone()]
    };
    let boundary = directories
        .last()
        .expect("discovery always scans at least the start directory")
        .clone();
    let target_config = directories
        .iter()
        .map(|directory| directory.join(PROJECT_CONFIG_FILENAME))
        .find(|candidate| source_present(candidate))
        .unwrap_or_else(|| start.join(PROJECT_CONFIG_FILENAME));

    let mut scanner = Scanner::new(boundary.clone());
    for directory in &directories {
        scanner.scan_provider_sources(directory);
        scanner.scan_package_json(directory);
        scanner.scan_go_file(directory, "go.mod");
        scanner.scan_go_file(directory, "go.work");
        scanner.scan_tool_versions(directory);
        scanner.scan_mise(directory);
        scanner.scan_pyproject(directory);
        scanner.scan_cargo(directory);
        scanner.scan_pubspec(directory);
    }

    scanner.finish(start, boundary, target_config)
}

fn git_marker(directory: &Path) -> io::Result<bool> {
    match fs::metadata(directory.join(".git")) {
        Ok(metadata) => Ok(metadata.is_file() || metadata.is_dir()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

struct Scanner {
    boundary: PathBuf,
    seen: HashSet<&'static str>,
    findings: Vec<DiscoveryFinding>,
}

impl Scanner {
    fn new(boundary: PathBuf) -> Self {
        Self {
            boundary,
            seen: HashSet::new(),
            findings: Vec::new(),
        }
    }

    fn finish(
        mut self,
        start: PathBuf,
        boundary: PathBuf,
        target_config: PathBuf,
    ) -> io::Result<DiscoveryReport> {
        let mut normalized_by_tool = BTreeMap::<String, BTreeSet<String>>::new();
        for finding in &self.findings {
            if finding.status == DiscoveryStatus::Ready {
                if let Some(normalized) = &finding.normalized {
                    normalized_by_tool
                        .entry(finding.tool.clone())
                        .or_default()
                        .insert(normalized.clone());
                }
            }
        }
        let conflicts = normalized_by_tool
            .into_iter()
            .filter_map(|(tool, values)| (values.len() > 1).then_some(tool))
            .collect::<BTreeSet<_>>();
        for finding in &mut self.findings {
            if finding.status == DiscoveryStatus::Ready && conflicts.contains(&finding.tool) {
                finding.status = DiscoveryStatus::Conflict;
                finding.reason = Some(format!(
                    "multiple traditional sources select different {} versions",
                    finding.tool
                ));
            }
        }
        self.findings.sort_by(|left, right| {
            provider_order(&left.tool)
                .cmp(&provider_order(&right.tool))
                .then_with(|| left.tool.cmp(&right.tool))
                .then_with(|| left.source.cmp(&right.source))
                .then_with(|| left.field.cmp(&right.field))
        });
        let has_ready = self
            .findings
            .iter()
            .any(|finding| finding.status == DiscoveryStatus::Ready);
        let blocked = self.findings.iter().any(|finding| {
            matches!(
                finding.status,
                DiscoveryStatus::Unsupported | DiscoveryStatus::Conflict
            )
        });
        Ok(DiscoveryReport {
            start,
            boundary,
            target_config,
            can_import: has_ready && !blocked,
            findings: self.findings,
        })
    }

    fn scan_provider_sources(&mut self, directory: &Path) {
        for provider in runtime_providers() {
            for rule in provider.discovery {
                match rule.kind {
                    RuntimeDiscoveryKind::SimpleFile { filename } => self.scan_simple(
                        rule.source,
                        &directory.join(filename),
                        provider.tool,
                        |raw| normalize_tool(provider.tool, raw),
                    ),
                    RuntimeDiscoveryKind::PythonVersion => self.scan_python_version(directory),
                    RuntimeDiscoveryKind::Fvm => self.scan_fvm(directory),
                    RuntimeDiscoveryKind::Sdkman => self.scan_sdkman(directory),
                    RuntimeDiscoveryKind::RustToolchain => self.scan_rust(directory),
                    RuntimeDiscoveryKind::DotnetGlobalJson => self.scan_global_json(directory),
                }
            }
        }
    }

    fn load(&mut self, group: &'static str, path: &Path, tool: &str) -> Option<String> {
        if self.seen.contains(group) {
            return None;
        }
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => {
                self.seen.insert(group);
                self.unsupported(
                    tool,
                    path,
                    None,
                    "",
                    format!("cannot inspect source: {error}"),
                );
                return None;
            }
            Ok(_) => {
                self.seen.insert(group);
            }
        }
        match read_source(path) {
            Ok(content) => Some(content),
            Err(reason) => {
                self.unsupported(tool, path, None, "", reason);
                None
            }
        }
    }

    fn scan_simple(
        &mut self,
        group: &'static str,
        path: &Path,
        tool: &str,
        normalize: impl FnOnce(&str) -> Result<String, String>,
    ) {
        let Some(content) = self.load(group, path, tool) else {
            return;
        };
        match one_simple_value(&content) {
            Ok(value) => self.selection(tool, path, None, &value, normalize(&value)),
            Err(reason) => self.unsupported(tool, path, None, "", reason),
        }
    }

    fn scan_python_version(&mut self, directory: &Path) {
        let path = directory.join(".python-version");
        let Some(content) = self.load("python-version", &path, "python") else {
            return;
        };
        let values = meaningful_lines(&content)
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>();
        if values.len() != 1 {
            self.unsupported(
                "python",
                &path,
                None,
                "multiple selectors",
                ".python-version must contain exactly one CPython selector",
            );
            return;
        }
        self.selection(
            "python",
            &path,
            None,
            values[0],
            normalize_python(values[0]),
        );
    }

    fn scan_package_json(&mut self, directory: &Path) {
        let path = directory.join("package.json");
        let Some(content) = self.load("package-json", &path, "project") else {
            return;
        };
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("project", &path, None, "", "invalid JSON");
                return;
            }
        };
        if let Some(node) = value.pointer("/volta/node") {
            if let Some(node) = node.as_str() {
                self.selection(
                    "node",
                    &path,
                    Some("volta.node"),
                    node,
                    normalize_node(node),
                );
            } else {
                self.unsupported(
                    "node",
                    &path,
                    Some("volta.node"),
                    json_scalar(node),
                    "volta.node must be a string",
                );
            }
        }
        if let Some(package_manager) = value.get("packageManager") {
            if let Some(package_manager) = package_manager.as_str() {
                self.package_manager(&path, "packageManager", package_manager);
            } else {
                self.unsupported(
                    "project",
                    &path,
                    Some("packageManager"),
                    json_scalar(package_manager),
                    "packageManager must be a string",
                );
            }
        }
        if let Some(engines) = value.get("engines").and_then(serde_json::Value::as_object) {
            for tool in ["node", "pnpm", "bun"] {
                if let Some(raw) = engines.get(tool).and_then(serde_json::Value::as_str) {
                    self.informational(tool, &path, Some(&format!("engines.{tool}")), raw);
                }
            }
        }
        for key in ["runtime", "packageManager"] {
            if let Some(entry) = value.get("devEngines").and_then(|value| value.get(key)) {
                for item in json_entries(entry) {
                    let Some(name) = item.get("name").and_then(serde_json::Value::as_str) else {
                        continue;
                    };
                    let Some(version) = item.get("version").and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    if let Some(tool) = tool_alias(name) {
                        self.informational(
                            tool,
                            &path,
                            Some(&format!("devEngines.{key}.version")),
                            version,
                        );
                    }
                }
            }
        }
    }

    fn package_manager(&mut self, path: &Path, field: &str, raw: &str) {
        let Some((name, version)) = raw.split_once('@') else {
            self.unsupported(
                "project",
                path,
                Some(field),
                raw,
                "packageManager must use <name>@<version>",
            );
            return;
        };
        let version = version.split("+sha").next().unwrap_or(version);
        match name {
            "pnpm" => self.selection("pnpm", path, Some(field), version, normalize_pnpm(version)),
            "bun" => self.selection("bun", path, Some(field), version, normalize_bun(version)),
            other => self.ignored(
                other,
                path,
                Some(field),
                raw,
                "package manager is not a Pinset Provider",
            ),
        }
    }

    fn scan_go_file(&mut self, directory: &Path, filename: &'static str) {
        let path = directory.join(filename);
        let group = if filename == "go.mod" {
            "go-mod"
        } else {
            "go-work"
        };
        let Some(content) = self.load(group, &path, "go") else {
            return;
        };
        for line in content.lines() {
            let line = line.split("//").next().unwrap_or(line).trim();
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next()) {
                (Some("toolchain"), Some(raw)) => {
                    self.selection("go", &path, Some("toolchain"), raw, normalize_go(raw))
                }
                (Some("go"), Some(raw)) => self.informational("go", &path, Some("go"), raw),
                _ => {}
            }
        }
    }

    fn scan_fvm(&mut self, directory: &Path) {
        if self.seen.contains("fvm") {
            return;
        }
        let current = directory.join(".fvmrc");
        let legacy = directory.join(".fvm").join("fvm_config.json");
        let path = if source_present(&current) {
            current
        } else if source_present(&legacy) {
            legacy
        } else {
            return;
        };
        let Some(content) = self.load("fvm", &path, "flutter") else {
            return;
        };
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("flutter", &path, None, "", "invalid JSON");
                return;
            }
        };
        let has_flavors = value.get("flavors").is_some_and(|flavors| match flavors {
            serde_json::Value::Null => false,
            serde_json::Value::Object(flavors) => !flavors.is_empty(),
            _ => true,
        });
        if has_flavors {
            self.unsupported(
                "flutter",
                &path,
                Some("flavors"),
                "configured",
                "FVM flavors cannot be represented by one Pinset selection",
            );
            return;
        }
        let (field, raw) =
            if let Some(raw) = value.get("flutter").and_then(serde_json::Value::as_str) {
                ("flutter", raw)
            } else if let Some(raw) = value
                .get("flutterSdkVersion")
                .and_then(serde_json::Value::as_str)
            {
                ("flutterSdkVersion", raw)
            } else {
                self.unsupported(
                    "flutter",
                    &path,
                    None,
                    "",
                    "FVM configuration has no string flutter version",
                );
                return;
            };
        self.selection("flutter", &path, Some(field), raw, normalize_flutter(raw));
    }

    fn scan_sdkman(&mut self, directory: &Path) {
        let path = directory.join(".sdkmanrc");
        let Some(content) = self.load("sdkmanrc", &path, "java") else {
            return;
        };
        for line in meaningful_lines(&content) {
            let Some((name, raw)) = line.split_once('=') else {
                self.unsupported("java", &path, None, "", "invalid .sdkmanrc assignment");
                continue;
            };
            let name = name.trim();
            let raw = raw.trim();
            if name == "java" {
                self.selection("java", &path, Some("java"), raw, normalize_java(raw));
            } else {
                self.ignored(
                    name,
                    &path,
                    Some(name),
                    raw,
                    "SDKMAN candidate is not imported",
                );
            }
        }
    }

    fn scan_rust(&mut self, directory: &Path) {
        if self.seen.contains("rust-toolchain") {
            return;
        }
        let legacy = directory.join("rust-toolchain");
        let modern = directory.join("rust-toolchain.toml");
        let path = if source_present(&legacy) {
            legacy
        } else {
            modern
        };
        let Some(content) = self.load("rust-toolchain", &path, "rust") else {
            return;
        };
        if !content.trim_start().starts_with('[') {
            match one_simple_value(&content) {
                Ok(value) => self.selection("rust", &path, None, &value, normalize_rust(&value)),
                Err(reason) => self.unsupported("rust", &path, None, "", reason),
            }
            return;
        }
        self.scan_rust_toml(&path, &content);
    }

    fn scan_rust_toml(&mut self, path: &Path, content: &str) {
        let value: toml::Value = match toml::from_str(content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("rust", path, None, "", "invalid TOML");
                return;
            }
        };
        let Some(toolchain) = value.get("toolchain").and_then(toml::Value::as_table) else {
            self.unsupported("rust", path, None, "", "missing [toolchain] table");
            return;
        };
        if let Some(field) = toolchain.keys().find(|field| {
            !matches!(
                field.as_str(),
                "channel" | "components" | "profile" | "targets" | "path"
            )
        }) {
            self.unsupported(
                "rust",
                path,
                Some(&format!("toolchain.{field}")),
                "configured",
                "unknown Rust toolchain fields cannot be imported safely",
            );
            return;
        }
        if toolchain.contains_key("path") {
            self.unsupported(
                "rust",
                path,
                Some("toolchain.path"),
                "configured",
                "path toolchains are not supported",
            );
            return;
        }
        if let Some(targets) = toolchain.get("targets") {
            if !targets.as_array().is_some_and(|targets| targets.is_empty()) {
                self.unsupported(
                    "rust",
                    path,
                    Some("toolchain.targets"),
                    "configured",
                    "extra Rust targets are not supported",
                );
                return;
            }
        }
        if let Some(profile) = toolchain.get("profile") {
            if profile.as_str() != Some("default") {
                self.unsupported(
                    "rust",
                    path,
                    Some("toolchain.profile"),
                    toml_scalar(profile),
                    "only the default Rust profile is supported",
                );
                return;
            }
        }
        if let Some(components) = toolchain.get("components") {
            let valid = components.as_array().is_some_and(|components| {
                components.iter().all(|component| {
                    component
                        .as_str()
                        .is_some_and(|component| matches!(component, "rustfmt" | "clippy"))
                })
            });
            if !valid {
                self.unsupported(
                    "rust",
                    path,
                    Some("toolchain.components"),
                    "configured",
                    "only rustfmt and clippy components are supported",
                );
                return;
            }
        }
        let Some(channel) = toolchain.get("channel").and_then(toml::Value::as_str) else {
            self.unsupported(
                "rust",
                path,
                Some("toolchain.channel"),
                "",
                "Rust channel must be a string",
            );
            return;
        };
        self.selection(
            "rust",
            path,
            Some("toolchain.channel"),
            channel,
            normalize_rust(channel),
        );
    }

    fn scan_global_json(&mut self, directory: &Path) {
        let path = directory.join("global.json");
        let Some(content) = self.load("global-json", &path, "dotnet") else {
            return;
        };
        let value: serde_json::Value = match json5::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("dotnet", &path, None, "", "invalid JSONC");
                return;
            }
        };
        let Some(sdk) = value.get("sdk").and_then(serde_json::Value::as_object) else {
            self.unsupported(
                "dotnet",
                &path,
                Some("sdk"),
                "",
                "global.json has no sdk object",
            );
            return;
        };
        if let Some(version) = sdk.get("version") {
            if let Some(version) = version.as_str() {
                self.selection(
                    "dotnet",
                    &path,
                    Some("sdk.version"),
                    version,
                    normalize_dotnet_exact(version),
                );
            } else {
                self.unsupported(
                    "dotnet",
                    &path,
                    Some("sdk.version"),
                    json_scalar(version),
                    "sdk.version must be a string",
                );
            }
        }
        for field in ["rollForward", "allowPrerelease"] {
            if let Some(raw) = sdk.get(field) {
                self.informational(
                    "dotnet",
                    &path,
                    Some(&format!("sdk.{field}")),
                    &json_scalar(raw),
                );
            }
        }
    }

    fn scan_tool_versions(&mut self, directory: &Path) {
        let path = directory.join(".tool-versions");
        let Some(content) = self.load("tool-versions", &path, "project") else {
            return;
        };
        for line in meaningful_lines(&content) {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.is_empty() {
                continue;
            }
            let Some(tool) = tool_alias(fields[0]) else {
                self.ignored(
                    fields[0],
                    &path,
                    Some(fields[0]),
                    &fields[1..].join(" "),
                    "tool is not a Pinset Provider",
                );
                continue;
            };
            if fields.len() != 2 {
                self.unsupported(
                    tool,
                    &path,
                    Some(fields[0]),
                    fields[1..].join(" "),
                    "supported tools must have exactly one plain version",
                );
                continue;
            }
            self.selection(
                tool,
                &path,
                Some(fields[0]),
                fields[1],
                normalize_tool(tool, fields[1]),
            );
        }
    }

    fn scan_mise(&mut self, directory: &Path) {
        let path = directory.join("mise.toml");
        let Some(content) = self.load("mise-toml", &path, "project") else {
            return;
        };
        let value: toml::Value = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("project", &path, None, "", "invalid TOML");
                return;
            }
        };
        let Some(tools) = value.get("tools").and_then(toml::Value::as_table) else {
            return;
        };
        for (name, value) in tools {
            let Some(tool) = tool_alias(name) else {
                self.ignored(
                    name,
                    &path,
                    Some(&format!("tools.{name}")),
                    &toml_scalar(value),
                    "tool is not a Pinset Provider",
                );
                continue;
            };
            let Some(raw) = value.as_str() else {
                self.unsupported(
                    tool,
                    &path,
                    Some(&format!("tools.{name}")),
                    toml_scalar(value),
                    "mise value must be one plain string selector",
                );
                continue;
            };
            self.selection(
                tool,
                &path,
                Some(&format!("tools.{name}")),
                raw,
                normalize_tool(tool, raw),
            );
        }
    }

    fn scan_pyproject(&mut self, directory: &Path) {
        let path = directory.join("pyproject.toml");
        let Some(content) = self.load("pyproject", &path, "python") else {
            return;
        };
        let value: toml::Value = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("python", &path, None, "", "invalid TOML");
                return;
            }
        };
        if let Some(raw) = value
            .get("project")
            .and_then(|value| value.get("requires-python"))
            .and_then(toml::Value::as_str)
        {
            self.informational("python", &path, Some("project.requires-python"), raw);
        }
    }

    fn scan_cargo(&mut self, directory: &Path) {
        let path = directory.join("Cargo.toml");
        let Some(content) = self.load("cargo-toml", &path, "rust") else {
            return;
        };
        let value: toml::Value = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("rust", &path, None, "", "invalid TOML");
                return;
            }
        };
        for (field, raw) in [
            (
                "package.rust-version",
                value
                    .get("package")
                    .and_then(|value| value.get("rust-version"))
                    .and_then(toml::Value::as_str),
            ),
            (
                "workspace.package.rust-version",
                value
                    .get("workspace")
                    .and_then(|value| value.get("package"))
                    .and_then(|value| value.get("rust-version"))
                    .and_then(toml::Value::as_str),
            ),
        ] {
            if let Some(raw) = raw {
                self.informational("rust", &path, Some(field), raw);
            }
        }
    }

    fn scan_pubspec(&mut self, directory: &Path) {
        let path = directory.join("pubspec.yaml");
        let Some(content) = self.load("pubspec", &path, "flutter") else {
            return;
        };
        let value: serde_yaml::Value = match serde_yaml::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                self.unsupported("flutter", &path, None, "", "invalid YAML");
                return;
            }
        };
        let Some(environment) = yaml_get(&value, "environment") else {
            return;
        };
        for field in ["sdk", "flutter"] {
            if let Some(raw) = yaml_get(environment, field).and_then(yaml_scalar) {
                self.informational(
                    "flutter",
                    &path,
                    Some(&format!("environment.{field}")),
                    &raw,
                );
            }
        }
    }

    fn selection(
        &mut self,
        tool: &str,
        path: &Path,
        field: Option<&str>,
        raw: &str,
        normalized: Result<String, String>,
    ) {
        match normalized {
            Ok(normalized) => self.findings.push(DiscoveryFinding {
                tool: tool.to_owned(),
                source: self.source(path),
                field: field.map(str::to_owned),
                raw: bounded_raw(raw),
                normalized: Some(normalized),
                kind: DiscoveryKind::Selection,
                status: DiscoveryStatus::Ready,
                reason: None,
            }),
            Err(reason) => self.unsupported(tool, path, field, raw, reason),
        }
    }

    fn informational(&mut self, tool: &str, path: &Path, field: Option<&str>, raw: &str) {
        self.findings.push(DiscoveryFinding {
            tool: tool.to_owned(),
            source: self.source(path),
            field: field.map(str::to_owned),
            raw: bounded_raw(raw),
            normalized: None,
            kind: DiscoveryKind::Constraint,
            status: DiscoveryStatus::Informational,
            reason: Some("version constraint is reported but not imported".to_owned()),
        });
    }

    fn ignored(&mut self, tool: &str, path: &Path, field: Option<&str>, raw: &str, reason: &str) {
        self.findings.push(DiscoveryFinding {
            tool: tool.to_owned(),
            source: self.source(path),
            field: field.map(str::to_owned),
            raw: bounded_raw(raw),
            normalized: None,
            kind: DiscoveryKind::Selection,
            status: DiscoveryStatus::Ignored,
            reason: Some(reason.to_owned()),
        });
    }

    fn unsupported(
        &mut self,
        tool: &str,
        path: &Path,
        field: Option<&str>,
        raw: impl AsRef<str>,
        reason: impl Into<String>,
    ) {
        self.findings.push(DiscoveryFinding {
            tool: tool.to_owned(),
            source: self.source(path),
            field: field.map(str::to_owned),
            raw: bounded_raw(raw.as_ref()),
            normalized: None,
            kind: DiscoveryKind::Selection,
            status: DiscoveryStatus::Unsupported,
            reason: Some(reason.into()),
        });
    }

    fn source(&self, path: &Path) -> String {
        path.strip_prefix(&self.boundary)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

fn read_source(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("symbolic-link sources are not allowed".to_owned());
    }
    if !metadata.is_file() {
        return Err("source is not a regular file".to_owned());
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(format!("source exceeds {MAX_SOURCE_BYTES} bytes"));
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let content = String::from_utf8(bytes).map_err(|_| "source is not valid UTF-8".to_owned())?;
    Ok(content
        .strip_prefix('\u{feff}')
        .unwrap_or(&content)
        .to_owned())
}

fn bounded_raw(raw: &str) -> String {
    const MAX_RAW_CHARS: usize = 160;
    let raw = raw.trim();
    let mut characters = raw.chars();
    let value = characters.by_ref().take(MAX_RAW_CHARS).collect::<String>();
    if characters.next().is_some() {
        format!("{value}…")
    } else {
        value
    }
}

fn source_present(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != ErrorKind::NotFound,
    }
}

fn one_simple_value(content: &str) -> Result<String, String> {
    let values = meaningful_lines(content).collect::<Vec<_>>();
    if values.len() != 1 {
        return Err("source must contain exactly one version selector".to_owned());
    }
    let fields = values[0].split_whitespace().collect::<Vec<_>>();
    if fields.len() != 1 {
        return Err("version selector must not contain whitespace".to_owned());
    }
    Ok(fields[0].to_owned())
}

fn meaningful_lines(content: &str) -> impl Iterator<Item = &str> {
    content.lines().filter_map(|line| {
        let line = line.split('#').next().unwrap_or(line).trim();
        (!line.is_empty()).then_some(line)
    })
}

fn normalize_tool(tool: &str, raw: &str) -> Result<String, String> {
    match tool {
        "node" => normalize_node(raw),
        "pnpm" => normalize_pnpm(raw),
        "bun" => normalize_bun(raw),
        "go" => normalize_go(raw),
        "flutter" => normalize_flutter(raw),
        "python" => normalize_python(raw),
        "java" => normalize_java(raw),
        "rust" => normalize_rust(raw),
        "dotnet" => normalize_dotnet(raw),
        _ => Err(format!("unsupported Pinset Provider {tool}")),
    }
}

fn normalize_node(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    let value = match value.as_str() {
        "node" => "current",
        "lts/*" => "lts",
        value if value.starts_with("lts/") => {
            return Err("named Node.js LTS channels cannot be mapped safely".to_owned());
        }
        value => value,
    };
    normalize_numeric_or(
        value.strip_prefix('v').unwrap_or(value),
        &["current", "lts"],
    )
}

fn normalize_pnpm(raw: &str) -> Result<String, String> {
    normalize_numeric_or(&raw.trim().to_ascii_lowercase(), &["latest", "current"])
}

fn normalize_bun(raw: &str) -> Result<String, String> {
    normalize_pnpm(raw)
}

fn normalize_go(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    let value = value.strip_prefix("go").unwrap_or(&value);
    normalize_numeric_or(value, &["latest", "current"])
}

fn normalize_flutter(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    let value = if value == "stable" { "current" } else { &value };
    normalize_numeric_or(value, &["latest", "current"])
}

fn normalize_python(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value == "system" || value.contains("pypy") || value.contains('/') || value.contains(':') {
        return Err("only one CPython selector can be imported".to_owned());
    }
    if let Some((version, build)) = value.split_once('+') {
        if numeric_parts(version, 3, 3)
            && build.len() == 8
            && build.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Ok(value);
        }
        return Err("invalid CPython distribution selector".to_owned());
    }
    normalize_numeric_or(&value, &["latest", "current"])
}

fn normalize_java(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    let value = if let Some(value) = value.strip_suffix("-tem") {
        value
    } else if value.rsplit_once('-').is_some() {
        return Err("only Temurin SDKMAN Java versions can be imported".to_owned());
    } else {
        &value
    };
    if matches!(value, "latest" | "current" | "lts") {
        return Ok(value.to_owned());
    }
    let (version, build) = value.split_once('+').unwrap_or((value, ""));
    if numeric_parts(version, 1, 3)
        && (build.is_empty() || build.bytes().all(|byte| byte.is_ascii_digit()))
    {
        Ok(value.to_owned())
    } else {
        Err("Java selector must be stable numeric, lts, current, or Temurin -tem".to_owned())
    }
}

fn normalize_rust(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.contains("nightly") || value.contains("beta") || value.contains('-') {
        return Err("only stable Rust channels can be imported".to_owned());
    }
    normalize_numeric_or(&value, &["latest", "current", "stable"])
}

fn normalize_dotnet(raw: &str) -> Result<String, String> {
    normalize_numeric_or(
        &raw.trim().to_ascii_lowercase(),
        &["latest", "current", "lts"],
    )
}

fn normalize_dotnet_exact(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if numeric_parts(value, 3, 3) {
        Ok(value.to_owned())
    } else {
        Err("global.json sdk.version must be one exact stable x.y.z SDK version".to_owned())
    }
}

fn normalize_numeric_or(raw: &str, words: &[&str]) -> Result<String, String> {
    if words.contains(&raw) || numeric_parts(raw, 1, 3) {
        Ok(raw.to_owned())
    } else {
        Err("selector cannot be mapped safely".to_owned())
    }
}

fn numeric_parts(value: &str, minimum: usize, maximum: usize) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    (minimum..=maximum).contains(&parts.len())
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
}

fn tool_alias(name: &str) -> Option<&'static str> {
    let tool = match name.trim().to_ascii_lowercase().as_str() {
        "node" | "nodejs" => "node",
        "pnpm" => "pnpm",
        "bun" => "bun",
        "go" | "golang" => "go",
        "flutter" => "flutter",
        "python" => "python",
        "java" => "java",
        "rust" => "rust",
        "dotnet" | "dotnet-core" => "dotnet",
        _ => return None,
    };
    runtime_provider(tool).map(|provider| provider.tool)
}

fn provider_order(tool: &str) -> usize {
    crate::runtime_providers()
        .iter()
        .position(|provider| provider.tool == tool)
        .unwrap_or(usize::MAX)
}

fn json_entries(value: &serde_json::Value) -> Vec<&serde_json::Map<String, serde_json::Value>> {
    match value {
        serde_json::Value::Object(object) => vec![object],
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(serde_json::Value::as_object)
            .collect(),
        _ => Vec::new(),
    }
}

fn json_scalar(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn toml_scalar(value: &toml::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn yaml_get<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String(key.to_owned()))
}

fn yaml_scalar(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(value) => Some(value.clone()),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scans_all_provider_sources_and_reports_constraints() {
        let root = tempdir().expect("root");
        fs::create_dir(root.path().join(".git")).expect("git marker");
        fs::write(
            root.path().join(".nvmrc"),
            "\u{feff}# project Node\nv24.1.0\n",
        )
        .expect("nvmrc");
        fs::write(root.path().join(".bun-version"), "1.2.3\n").expect("bun");
        fs::write(root.path().join(".go-version"), "go1.25.1\n").expect("go");
        fs::write(root.path().join(".python-version"), "3.14\n").expect("python");
        fs::write(root.path().join(".java-version"), "21.0.7-tem\n").expect("java");
        fs::write(root.path().join("rust-toolchain"), "stable\n").expect("rust");
        fs::write(
            root.path().join("global.json"),
            "{ // comment\n sdk: { version: '10.0.100', rollForward: 'latestPatch' } }",
        )
        .expect("global json");
        fs::write(root.path().join(".fvmrc"), r#"{"flutter":"3.35.0"}"#).expect("fvm");
        fs::write(
            root.path().join("package.json"),
            r#"{"volta":{"node":"24.1.0"},"packageManager":"pnpm@10.2.0","engines":{"node":">=22"}}"#,
        )
        .expect("package");
        fs::write(
            root.path().join("go.mod"),
            "module example.test/app\n\ngo 1.25\ntoolchain go1.25.1\n",
        )
        .expect("go mod");
        fs::write(
            root.path().join(".sdkmanrc"),
            "java=21.0.7-tem\nkotlin=2.2.0\n",
        )
        .expect("sdkman");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"example\"\nrequires-python = \">=3.12\"\n",
        )
        .expect("pyproject");
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"example\"\nversion = \"0.1.0\"\nrust-version = \"1.85\"\n",
        )
        .expect("cargo");
        fs::write(
            root.path().join("pubspec.yaml"),
            "name: example\nenvironment:\n  sdk: '>=3.8.0 <4.0.0'\n  flutter: '>=3.35.0'\n",
        )
        .expect("pubspec");

        let report = scan_project_sources(root.path()).expect("scan");
        assert!(report.can_import);
        for tool in [
            "node", "pnpm", "bun", "go", "flutter", "python", "java", "rust", "dotnet",
        ] {
            assert!(report.findings.iter().any(|finding| finding.tool == tool && finding.status == DiscoveryStatus::Ready), "missing {tool}");
        }
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.kind == DiscoveryKind::Constraint)
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.status == DiscoveryStatus::Ignored)
        );
    }

    #[test]
    fn stops_at_repository_root_and_marks_conflicts() {
        let root = tempdir().expect("root");
        let repository = root.path().join("repo");
        let nested = repository.join("packages").join("app");
        fs::create_dir_all(repository.join(".git")).expect("git marker");
        fs::create_dir_all(&nested).expect("nested");
        fs::write(root.path().join(".tool-versions"), "node 18.0.0\n").expect("outside");
        fs::write(repository.join(".node-version"), "22.0.0\n").expect("node version");
        fs::write(nested.join(".nvmrc"), "24.0.0\n").expect("nvmrc");

        let report = scan_project_sources(&nested).expect("scan");
        assert_eq!(report.boundary, repository);
        assert!(!report.can_import);
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.status == DiscoveryStatus::Conflict)
                .count(),
            2
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.raw != "18.0.0")
        );
    }

    #[test]
    fn blocks_unsafe_and_unrepresentable_sources() {
        let root = tempdir().expect("root");
        fs::create_dir(root.path().join(".git")).expect("git marker");
        fs::write(root.path().join(".python-version"), "3.13\n3.14\n").expect("python");
        fs::write(
            root.path().join(".fvmrc"),
            r#"{"flutter":"3.35.0","flavors":{"prod":"3.35.0"}}"#,
        )
        .expect("fvm");
        fs::write(
            root.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"nightly\"\ntargets = [\"wasm32-unknown-unknown\"]\n",
        )
        .expect("rust");

        let report = scan_project_sources(root.path()).expect("scan");
        assert!(!report.can_import);
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.status == DiscoveryStatus::Unsupported)
                .count(),
            3
        );
    }

    #[test]
    fn without_repository_marker_scans_only_start_directory() {
        let root = tempdir().expect("root");
        let nested = root.path().join("nested");
        fs::create_dir(&nested).expect("nested");
        fs::write(root.path().join(".nvmrc"), "22.0.0\n").expect("outside");
        fs::write(nested.join(".bun-version"), "1.2.3\n").expect("inside");

        let report = scan_project_sources(&nested).expect("scan");
        assert_eq!(report.start, report.boundary);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].tool, "bun");
    }

    #[test]
    fn imports_plain_universal_aliases_and_blocks_complex_supported_values() {
        let root = tempdir().expect("root");
        fs::write(
            root.path().join(".tool-versions"),
            "nodejs 24.0.0\ngolang 1.25.1\ndotnet-core 10.0.100\nruby 3.4.0\n",
        )
        .expect("tool versions");
        fs::write(
            root.path().join("mise.toml"),
            "[tools]\npnpm = \"10.2.0\"\npython = [\"3.13\", \"3.14\"]\n",
        )
        .expect("mise");

        let report = scan_project_sources(root.path()).expect("scan");
        assert!(!report.can_import);
        for tool in ["node", "go", "dotnet", "pnpm"] {
            assert!(report.findings.iter().any(|finding| {
                finding.tool == tool && finding.status == DiscoveryStatus::Ready
            }));
        }
        assert!(report.findings.iter().any(|finding| {
            finding.tool == "ruby" && finding.status == DiscoveryStatus::Ignored
        }));
        assert!(report.findings.iter().any(|finding| {
            finding.tool == "python" && finding.status == DiscoveryStatus::Unsupported
        }));
    }

    #[test]
    fn accepts_toml_in_the_legacy_rust_toolchain_filename() {
        let root = tempdir().expect("root");
        fs::write(
            root.path().join("rust-toolchain"),
            "[toolchain]\nchannel = \"stable\"\nprofile = \"default\"\ncomponents = [\"rustfmt\", \"clippy\"]\ntargets = []\n",
        )
        .expect("rust toolchain");

        let report = scan_project_sources(root.path()).expect("scan");
        assert!(report.can_import);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].normalized.as_deref(), Some("stable"));
        assert_eq!(report.findings[0].status, DiscoveryStatus::Ready);
    }

    #[test]
    fn rejects_non_utf8_and_oversized_sources_without_exposing_contents() {
        let root = tempdir().expect("root");
        fs::create_dir(root.path().join(".git")).expect("git marker");
        fs::write(root.path().join(".nvmrc"), [0xff, 0xfe]).expect("non utf8");
        fs::write(
            root.path().join(".bun-version"),
            vec![b'1'; MAX_SOURCE_BYTES as usize + 1],
        )
        .expect("oversized");

        let report = scan_project_sources(root.path()).expect("scan");
        assert!(!report.can_import);
        assert_eq!(report.findings.len(), 2);
        assert!(report.findings.iter().all(|finding| {
            finding.status == DiscoveryStatus::Unsupported && finding.raw.is_empty()
        }));
    }

    #[test]
    fn malformed_structured_sources_do_not_echo_file_contents() {
        let root = tempdir().expect("root");
        fs::write(
            root.path().join("package.json"),
            "{\"private-token\":\"must-not-be-reported\"",
        )
        .expect("malformed package json");

        let report = scan_project_sources(root.path()).expect("scan");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].status, DiscoveryStatus::Unsupported);
        assert!(report.findings[0].raw.is_empty());
        assert_eq!(report.findings[0].reason.as_deref(), Some("invalid JSON"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_link_sources() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("root");
        fs::write(root.path().join("version.txt"), "24.0.0\n").expect("target");
        symlink(root.path().join("version.txt"), root.path().join(".nvmrc")).expect("link");

        let report = scan_project_sources(root.path()).expect("scan");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].status, DiscoveryStatus::Unsupported);
        assert!(
            report.findings[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("symbolic-link"))
        );
    }
}
