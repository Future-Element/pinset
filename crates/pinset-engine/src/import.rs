//! Reviewable imports of native toolchain declarations. Planning never writes state.
use crate::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

mod selectors;
use selectors::Constraint;

const SOURCES: [&str; 7] = [
    ".tool-versions",
    "package.json",
    ".nvmrc",
    ".node-version",
    "go.mod",
    "rust-toolchain.toml",
    ".python-version",
];

#[derive(Debug, Clone, Serialize)]
struct Declaration {
    tool: String,
    requested: String,
    selector: String,
    source: String,
}

#[derive(Default)]
struct Scan {
    sources: Vec<String>,
    declarations: Vec<Declaration>,
    rust: Option<ToolOptions>,
}

impl Scan {
    fn add(&mut self, name: &str, version: &str, source: impl Into<String>) {
        let tool = match name {
            "nodejs" => "node",
            "golang" => "go",
            "python3" | "cpython" => "python",
            "openjdk" | "temurin" => "java",
            other => other,
        };
        let version = version.trim();
        let selector = match tool {
            "node" => version.strip_prefix('v').unwrap_or(version),
            "go" => version.strip_prefix("go").unwrap_or(version),
            "java" => version.strip_prefix("temurin-").unwrap_or(version),
            _ => version,
        };
        let selector = if tool == "java" && version.starts_with("temurin-") {
            // asdf-java appends a zero distribution suffix and an optional LTS marker.
            let selector = selector.strip_suffix(".LTS").unwrap_or(selector);
            if let Some((release, build)) = selector.split_once('+') {
                format!("{release}+{}", build.strip_suffix(".0").unwrap_or(build))
            } else {
                selector.to_owned()
            }
        } else {
            selector.to_owned()
        };
        self.declarations.push(Declaration {
            tool: tool.to_owned(),
            requested: version.to_owned(),
            selector,
            source: source.into(),
        });
    }
}

/// A resolved plan holds the exact lock that was shown to the user, not selectors to re-resolve.
pub struct ImportPlan {
    context: ProjectContext,
    config: ProjectConfig,
    lock: Lockfile,
    snapshots: Vec<(std::path::PathBuf, Option<Vec<u8>>)>,
    report: Value,
}

impl ImportPlan {
    pub fn report(&self) -> Value {
        self.report.clone()
    }

    pub fn needs_write(&self) -> bool {
        self.report["requires_write"] == true
    }
}

trait Resolver {
    fn releases(&self, name: &str) -> Result<Value>;
    fn lock(&self, name: &str, selector: &str, config: &ProjectConfig) -> Result<LockedTool>;
}

impl Resolver for Services {
    fn releases(&self, name: &str) -> Result<Value> {
        self.remote(name)
    }

    fn lock(&self, name: &str, selector: &str, config: &ProjectConfig) -> Result<LockedTool> {
        self.resolve(name, selector, config)
    }
}

impl Services {
    pub fn plan_import(&self) -> Result<ImportPlan> {
        self.prepare_import(self)
    }

    fn prepare_import(&self, resolver: &impl Resolver) -> Result<ImportPlan> {
        let context =
            find_project_context(&self.cwd)?.unwrap_or(ProjectContext::at(&self.cwd, false)?);
        let mut snapshots = Vec::new();
        for path in SOURCES
            .iter()
            .map(|name| context.root.join(name))
            .chain([context.config_path.clone(), context.lock_path.clone()])
        {
            let before = read_optional(&path)?;
            snapshots.push((path, before));
        }
        let scan = scan(&context.root)?;
        if scan.sources.is_empty() {
            return Err(service_error(
                "PINSET_IMPORT_NO_SOURCES",
                format!(
                    "no toolchain source files found in {}; checked: {}",
                    context.root.display(),
                    SOURCES.join(", ")
                ),
            ));
        }
        let existed = context.config_path.exists();
        let mut config = if existed {
            context.load()?
        } else {
            ProjectConfig::new(uuid::Uuid::new_v4().to_string())
        };
        let had_lock = context.lock_path.exists();
        let mut lock = if had_lock {
            let lock = load_lockfile(&context.lock_path)?;
            config.validate_lock(&lock)?;
            lock
        } else {
            Lockfile::empty(config.project_id.clone())
        };
        let mut imports = Vec::new();
        let mut retained = Vec::new();
        let mut unsupported = Vec::new();
        let mut conflicts = Vec::new();
        let mut unresolved = Vec::new();
        let mut grouped: BTreeMap<String, Vec<Declaration>> = BTreeMap::new();
        for declaration in scan.declarations {
            if TOOLS.contains(&declaration.tool.as_str())
                || config.generic.contains_key(&declaration.tool)
            {
                grouped
                    .entry(declaration.tool.clone())
                    .or_default()
                    .push(declaration);
            } else {
                unsupported.push(declaration);
            }
        }

        // Missing locks may be generated, but existing selections are never replaced or upgraded.
        for (name, selector) in &config.tools {
            if lock.tool(name).is_none() {
                let mut tool = resolver.lock(name, selector, &config)?;
                tool.requested = selector.clone();
                lock.upsert_tool(tool)?;
            }
            retained.push(
                json!({"tool":name,"selector":selector,"version":lock.tool(name).unwrap().version}),
            );
        }

        let total = grouped.len();
        for (index, (name, declarations)) in grouped.into_iter().enumerate() {
            let constraints = declarations
                .iter()
                .map(|d| {
                    if config.generic.contains_key(&name) {
                        validate_generic_version(&d.selector)?;
                        Ok(Constraint::literal(&d.selector))
                    } else {
                        Constraint::parse(&name, &d.selector)
                    }
                })
                .collect::<Result<Vec<_>>>();
            let constraints = match constraints {
                Ok(constraints) => constraints,
                Err(error) => {
                    unresolved.push(issue(&name, &declarations, &error.to_string()));
                    continue;
                }
            };
            if let Some(existing) = lock.tool(&name) {
                let compatible = matches_existing(resolver, &name, existing, &constraints);
                match compatible {
                    Ok(true) => {
                        if let Some(item) = retained.iter_mut().find(|item| item["tool"] == name) {
                            item["resolved"] = declarations
                                .iter()
                                .any(|d| d.selector != existing.version)
                                .into();
                            item["declarations"] = serde_json::to_value(&declarations)?;
                        }
                    }
                    Ok(false) => conflicts.push(json!({
                        "tool":name,"reason":"existing_selection","existing":config.tools[&name],
                        "version":existing.version,"declarations":declarations
                    })),
                    Err(error) => unresolved.push(issue(&name, &declarations, &error.to_string())),
                }
                continue;
            }

            let mut proposed = config.clone();
            if name == "rust"
                && let Some(options) = &scan.rust
            {
                if proposed.rust_options != ToolOptions::default()
                    && proposed.rust_options != *options
                {
                    conflicts.push(json!({"tool":name,"reason":"existing_rust_options","declarations":declarations}));
                    continue;
                }
                proposed.rust_options = options.clone();
            }
            self.progress.report(ProgressEvent::Resolving {
                tool: name.clone(),
                selector: declarations
                    .iter()
                    .map(|d| d.requested.as_str())
                    .collect::<Vec<_>>()
                    .join(" & "),
                index: index + 1,
                total,
            });
            let version = select_version(resolver, &name, &constraints);
            let tool = version.and_then(|version| resolver.lock(&name, &version, &proposed));
            match tool {
                Ok(mut tool) => {
                    // A dated nightly is already exact. Its numeric compiler version
                    // alone would resolve to a different (stable) toolchain.
                    let selection = if name == "rust" && tool.requested.starts_with("nightly-") {
                        tool.requested.clone()
                    } else {
                        tool.version.clone()
                    };
                    tool.requested = selection.clone();
                    proposed.tools.insert(name.clone(), selection.clone());
                    let resolved = declarations.iter().any(|d| d.selector != tool.version);
                    imports.push(
                        json!({"tool":name,"selector":selection,"version":tool.version,"provider":tool.provider,
                        "resolved":resolved,"declarations":declarations}),
                    );
                    self.progress.report(ProgressEvent::Resolved {
                        tool: name,
                        version: tool.version.clone(),
                    });
                    lock.upsert_tool(tool)?;
                    config = proposed;
                }
                Err(error) if error.code() == "PINSET_IMPORT_SOURCE_CONFLICT" => {
                    conflicts.push(issue(&name, &declarations, "incompatible_sources"));
                }
                Err(error) => unresolved.push(issue(&name, &declarations, &error.to_string())),
            }
        }
        if config.tools.contains_key("pnpm") && !config.tools.contains_key("node") {
            config.tools.remove("pnpm");
            lock.remove_tool("pnpm");
            if let Some(index) = imports.iter().position(|item| item["tool"] == "pnpm") {
                let mut item = imports.remove(index);
                item["reason"] =
                    "pnpm requires a project Node selection; declare Node and retry".into();
                unresolved.push(item);
            }
        }
        config.validate_lock(&lock)?;
        let requires_write = !imports.is_empty() || (existed && !had_lock);
        let report = json!({
            "protocol":PROTOCOL,"operation":"import","plan":true,"project":context.root,
            "sources":scan.sources,"checked_files":SOURCES,"imports":imports,"retained":retained,
            "unsupported":unsupported,"conflicts":conflicts,"unresolved":unresolved,
            "config":context.config_path,"lock":context.lock_path,"requires_write":requires_write,
            "confirmation_required":requires_write,"install":false
        });
        let plan = ImportPlan {
            context,
            config,
            lock,
            snapshots,
            report,
        };
        // Detect changes even while metadata was resolving, before showing a misleading plan.
        plan.validate_sources()?;
        Ok(plan)
    }

    pub fn apply_import(&self, plan: ImportPlan) -> Result<Value> {
        if !plan.needs_write() {
            return Ok(plan.completed(false));
        }
        let root_id = digest(plan.context.root.to_string_lossy().as_bytes())[..24].to_owned();
        let _root_guard = self.guard(&root_id)?;
        let _project_guard = if plan.config.project_id == root_id {
            None
        } else {
            Some(self.guard(&plan.config.project_id)?)
        };
        let context = ProjectContext::at(&plan.context.root, false)?;
        if context.directory != plan.context.directory {
            return Err(service_error(
                "PINSET_IMPORT_PLAN_STALE",
                "project directory changed; run pinset import again",
            ));
        }
        plan.validate_sources()?;
        plan.config.validate_lock(&plan.lock)?;
        if context.local.join("transaction.json").exists() {
            return Err(service_error(
                "PINSET_TRANSACTION_PENDING",
                "run pinset self repair first",
            ));
        }
        let new_project = !context.config_path.exists();
        if new_project {
            // Establish a valid empty rollback point using the same identity as the reviewed plan.
            // No resolution or SDK installation happens after this first project write.
            let empty = ProjectConfig::new(plan.config.project_id.clone());
            write_atomic(
                &context.config_path,
                toml::to_string_pretty(&empty)?.as_bytes(),
            )?;
        }
        self.commit(&context, &plan.config, &plan.lock)?;
        if new_project {
            self.gitignore(&context)?;
        }
        Ok(plan.completed(true))
    }
}

impl ImportPlan {
    fn validate_sources(&self) -> Result<()> {
        for (path, before) in &self.snapshots {
            if read_optional(path)? != *before {
                return Err(service_error(
                    "PINSET_IMPORT_PLAN_STALE",
                    format!(
                        "{} changed after planning; run pinset import again",
                        path.display()
                    ),
                ));
            }
        }
        Ok(())
    }

    fn completed(mut self, applied: bool) -> Value {
        self.report["plan"] = false.into();
        self.report["confirmation_required"] = false.into();
        self.report["applied"] = applied.into();
        self.report
    }
}

fn issue(name: &str, declarations: &[Declaration], reason: &str) -> Value {
    json!({"tool":name,"declarations":declarations,"reason":reason})
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn matches_existing(
    resolver: &impl Resolver,
    name: &str,
    existing: &LockedTool,
    constraints: &[Constraint],
) -> Result<bool> {
    let release = if constraints.iter().any(Constraint::needs_release_metadata) {
        resolver.releases(name)?["releases"]
            .as_array()
            .and_then(|releases| releases.iter().find(|r| r["version"] == existing.version))
            .cloned()
            .unwrap_or_else(|| json!({"version":existing.version}))
    } else {
        json!({"version":existing.version})
    };
    Ok(constraints.iter().all(|constraint| {
        if name == "rust"
            && let Some(date) = constraint
                .exact()
                .and_then(|selector| selector.strip_prefix("nightly-"))
        {
            existing
                .metadata
                .get("channel")
                .is_some_and(|channel| channel == "nightly")
                && existing
                    .metadata
                    .get("manifest_date")
                    .is_some_and(|locked| locked == date)
        } else {
            constraint.matches(&release)
        }
    }))
}

fn select_version(
    resolver: &impl Resolver,
    name: &str,
    constraints: &[Constraint],
) -> Result<String> {
    let exact = constraints.iter().find_map(Constraint::exact);
    if let Some(exact) = exact
        && !constraints.iter().any(Constraint::needs_release_metadata)
    {
        let release = json!({"version":exact});
        if constraints.iter().all(|c| c.matches(&release)) {
            return Ok(exact.to_owned());
        }
        return Err(service_error(
            "PINSET_IMPORT_SOURCE_CONFLICT",
            "incompatible versions",
        ));
    }
    let releases = resolver.releases(name)?;
    releases["releases"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|r| constraints.iter().all(|c| c.matches(r)))
        })
        .and_then(|r| r["version"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            service_error(
                if constraints.len() > 1 {
                    "PINSET_IMPORT_SOURCE_CONFLICT"
                } else {
                    "PINSET_IMPORT_VERSION_UNRESOLVED"
                },
                format!("no official {name} release matches the declarations"),
            )
        })
}

fn source_error(source: &str, message: impl std::fmt::Display) -> Error {
    service_error(
        "PINSET_IMPORT_SOURCE_INVALID",
        format!("{source}: {message}"),
    )
}

fn scan(root: &Path) -> Result<Scan> {
    let mut scan = Scan::default();
    for source in SOURCES {
        let Some(bytes) = read_optional(&root.join(source))? else {
            continue;
        };
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| source_error(source, e))?
            .trim_start_matches('\u{feff}');
        scan.sources.push(source.into());
        match source {
            ".tool-versions" => {
                for (line, text) in text.lines().enumerate() {
                    let text = text.split('#').next().unwrap().trim();
                    if text.is_empty() {
                        continue;
                    }
                    let (name, version) =
                        text.split_once(char::is_whitespace).ok_or_else(|| {
                            source_error(source, format!("line {} has no version", line + 1))
                        })?;
                    scan.add(name, version, format!("{source}:{}", line + 1));
                }
            }
            "package.json" => parse_package(&mut scan, text)?,
            "go.mod" => parse_go(&mut scan, text)?,
            "rust-toolchain.toml" => parse_rust(&mut scan, text)?,
            _ => {
                let tool = if source == ".python-version" {
                    "python"
                } else {
                    "node"
                };
                for (line, text) in text.lines().enumerate() {
                    let version = text.split('#').next().unwrap().trim();
                    if !version.is_empty() {
                        scan.add(tool, version, format!("{source}:{}", line + 1));
                    }
                }
            }
        }
    }
    Ok(scan)
}

fn parse_package(scan: &mut Scan, text: &str) -> Result<()> {
    let package: Value = serde_json::from_str(text).map_err(|e| source_error("package.json", e))?;
    if !package.is_object() {
        return Err(source_error("package.json", "expected an object"));
    }
    for field in ["engines", "volta"] {
        let Some(values) = package.get(field) else {
            continue;
        };
        let values = values
            .as_object()
            .ok_or_else(|| source_error("package.json", format!("{field} must be an object")))?;
        for (name, version) in values {
            if field == "volta" && name == "extends" {
                continue;
            }
            let version = version.as_str().ok_or_else(|| {
                source_error(
                    "package.json",
                    format!("{field}.{name} must be a version string"),
                )
            })?;
            scan.add(name, version, format!("package.json:{field}.{name}"));
        }
    }
    if let Some(manager) = package.get("packageManager") {
        let manager = manager
            .as_str()
            .ok_or_else(|| source_error("package.json", "packageManager must be a string"))?;
        let (name, version) = manager
            .rsplit_once('@')
            .filter(|(n, v)| !n.is_empty() && !v.is_empty())
            .ok_or_else(|| source_error("package.json", "packageManager must be name@version"))?;
        let version = ["+sha224.", "+sha256.", "+sha384.", "+sha512."]
            .iter()
            .find_map(|marker| version.split_once(marker).map(|(v, _)| v))
            .unwrap_or(version);
        scan.add(name, version, "package.json:packageManager");
    }
    Ok(())
}

fn parse_go(scan: &mut Scan, text: &str) -> Result<()> {
    let mut go = None;
    let mut toolchain = None;
    for line in text.lines() {
        let line = line.split("//").next().unwrap().trim();
        let words = line.split_whitespace().collect::<Vec<_>>();
        if let [directive @ ("go" | "toolchain"), value] = words.as_slice() {
            let slot = if *directive == "go" {
                &mut go
            } else {
                &mut toolchain
            };
            if slot.replace(*value).is_some() {
                return Err(source_error("go.mod", "duplicate toolchain directive"));
            }
        }
    }
    if let Some(toolchain) = toolchain.filter(|v| *v != "default") {
        scan.add("go", toolchain, "go.mod:toolchain");
        if let Some(version) = go {
            scan.add("go", &format!(">={version}"), "go.mod:go");
        }
    } else if let Some(version) = go {
        scan.add("go", version, "go.mod:go");
    }
    Ok(())
}

fn parse_rust(scan: &mut Scan, text: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct File {
        toolchain: Toolchain,
    }
    #[derive(serde::Deserialize)]
    struct Toolchain {
        channel: Option<String>,
        path: Option<String>,
        profile: Option<String>,
        #[serde(default)]
        components: Vec<String>,
        #[serde(default)]
        targets: Vec<String>,
    }
    let file: File = toml::from_str(text).map_err(|e| source_error("rust-toolchain.toml", e))?;
    let tc = file.toolchain;
    if let Some(path) = tc.path {
        scan.add(
            "rust",
            &format!("path:{path}"),
            "rust-toolchain.toml:toolchain.path",
        );
    } else {
        // A rustup default is machine-local state, so a missing channel cannot be inferred.
        scan.add(
            "rust",
            tc.channel.as_deref().unwrap_or("default"),
            "rust-toolchain.toml:toolchain.channel",
        );
        scan.rust = Some(ToolOptions {
            profile: tc.profile,
            components: tc.components,
            targets: tc.targets,
            date: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FixtureResolver {
        calls: RefCell<Vec<String>>,
    }

    impl Resolver for FixtureResolver {
        fn releases(&self, name: &str) -> Result<Value> {
            self.calls.borrow_mut().push(format!("releases:{name}"));
            Ok(json!({"releases":[
                {"version":"24.1.0","lts":null},
                {"version":"22.2.0","lts":"Jod"},
                {"version":"20.11.1","lts":"Iron"},
                {"version":"18.20.8","lts":"Hydrogen"},
            ]}))
        }

        fn lock(&self, name: &str, selector: &str, config: &ProjectConfig) -> Result<LockedTool> {
            self.calls
                .borrow_mut()
                .push(format!("lock:{name}@{selector}"));
            let nightly = name == "rust" && selector.starts_with("nightly-");
            let version = if nightly {
                "1.97.0"
            } else if selector == "20" {
                "20.11.1"
            } else {
                selector
            };
            let url = match name {
                "node" => format!("https://nodejs.org/dist/v{version}/node.tar.xz"),
                "go" => format!("https://go.dev/dl/go{version}.tar.gz"),
                "python" => format!("https://www.python.org/ftp/python/{version}/python.tar.xz"),
                "rust" => format!("https://static.rust-lang.org/dist/rust-{version}.tar.xz"),
                _ => format!("https://registry.npmjs.org/{name}/-/{name}-{version}.tgz"),
            };
            Ok(LockedTool {
                name: name.into(),
                requested: selector.into(),
                version: version.into(),
                provider: format!("{name}-fixture"),
                released_at: None,
                metadata: if nightly {
                    BTreeMap::from([
                        ("channel".into(), "nightly".into()),
                        (
                            "manifest_date".into(),
                            selector.trim_start_matches("nightly-").into(),
                        ),
                    ])
                } else {
                    BTreeMap::new()
                },
                options: if name == "rust" {
                    config.rust_options.lock_options()
                } else {
                    BTreeMap::new()
                },
                artifacts: vec![LockedArtifact {
                    target: "linux-x86_64".into(),
                    canonical_url: url,
                    artifact_path: "archive.tar.xz".into(),
                    sha256: "a".repeat(64),
                    integrity: None,
                    format: LockedArtifactFormat::TarXz,
                    archive_root: name.into(),
                    verification: "fixture-sha256".into(),
                    attestation: None,
                    overlays: vec![],
                }],
            })
        }
    }

    fn fixture() -> (tempfile::TempDir, Services, FixtureResolver) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        fs::create_dir(&root).unwrap();
        let service = Services {
            cwd: root,
            home: directory.path().join("home/v3"),
            progress: ProgressReporter::default(),
        };
        (directory, service, FixtureResolver::default())
    }

    fn write(service: &Services, path: &str, text: &str) {
        fs::write(service.cwd.join(path), text).unwrap();
    }

    #[test]
    fn nightly_import_preserves_the_dated_selection_and_detects_another_date() {
        let (_directory, service, resolver) = fixture();
        write(
            &service,
            "rust-toolchain.toml",
            "[toolchain]\nchannel = 'nightly-2026-07-16'\nprofile = 'minimal'\n",
        );
        let plan = service.prepare_import(&resolver).unwrap();
        assert_eq!(
            plan.report()["imports"][0]["selector"],
            "nightly-2026-07-16"
        );
        service.apply_import(plan).unwrap();
        let context = ProjectContext::at(&service.cwd, false).unwrap();
        let (config, lock) = context.load_locked().unwrap();
        assert_eq!(config.tools["rust"], "nightly-2026-07-16");
        assert_eq!(lock.tool("rust").unwrap().requested, "nightly-2026-07-16");
        assert_eq!(lock.tool("rust").unwrap().version, "1.97.0");
        let repeated = service.prepare_import(&resolver).unwrap();
        assert!(!repeated.needs_write());
        assert!(
            repeated.report()["conflicts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        write(
            &service,
            "rust-toolchain.toml",
            "[toolchain]\nchannel = 'nightly-2026-07-17'\nprofile = 'minimal'\n",
        );
        let changed = service.prepare_import(&resolver).unwrap();
        assert_eq!(
            changed.report()["conflicts"][0]["reason"],
            "existing_selection"
        );
        assert_eq!(changed.config.tools["rust"], "nightly-2026-07-16");
    }

    #[test]
    fn scans_all_sources_and_maps_the_eight_providers() {
        let (_directory, service, _) = fixture();
        write(
            &service,
            ".tool-versions",
            "# versions\nnodejs 20.11.1 # note\ngolang 1.22.1\njava temurin-21.0.5+11\nbun 1.2.0\nflutter 3.24.0\nruby 3.3.0\n",
        );
        write(
            &service,
            "package.json",
            r#"{"engines":{"node":">=18","npm":"10"},"volta":{"node":"20.11.1","yarn":"1.22.22"},"packageManager":"pnpm@10.2.0+sha512.deadbeef"}"#,
        );
        write(&service, ".nvmrc", "v20.11.1\n");
        write(&service, ".node-version", "20.11.1\n");
        write(
            &service,
            "go.mod",
            "module example.com/app\ngo 1.22\ntoolchain go1.22.1 // pinned\n",
        );
        write(
            &service,
            "rust-toolchain.toml",
            "[toolchain]\nchannel = '1.85.0'\nprofile = 'minimal'\ncomponents = ['clippy']\ntargets = ['wasm32-unknown-unknown']\n",
        );
        write(&service, ".python-version", "3.13.2\n");
        let scanned = scan(&service.cwd).unwrap();
        assert_eq!(scanned.sources.len(), SOURCES.len());
        for tool in TOOLS {
            assert!(
                scanned.declarations.iter().any(|d| d.tool == tool),
                "{tool}"
            );
        }
        assert!(
            scanned
                .declarations
                .iter()
                .any(|d| d.tool == "pnpm" && d.selector == "10.2.0")
        );
        assert!(
            scanned
                .declarations
                .iter()
                .any(|d| d.tool == "java" && d.selector == "21.0.5+11")
        );
        assert_eq!(scanned.rust.unwrap().components, ["clippy"]);
        let mut asdf_java = Scan::default();
        asdf_java.add("java", "temurin-21.0.5+11.0.LTS", ".tool-versions");
        assert_eq!(asdf_java.declarations[0].selector, "21.0.5+11");
    }

    #[test]
    fn preview_is_read_only_and_apply_uses_the_reviewed_exact_lock() {
        let (_directory, service, resolver) = fixture();
        write(&service, ".nvmrc", "lts/*\n");
        write(
            &service,
            "package.json",
            r#"{"engines":{"node":">=18 <23"},"packageManager":"pnpm@10.2.0"}"#,
        );
        let before = fs::read(service.cwd.join("package.json")).unwrap();
        let plan = service.prepare_import(&resolver).unwrap();
        assert!(plan.needs_write());
        assert_eq!(plan.report()["imports"][0]["version"], "22.2.0");
        assert_eq!(plan.report()["imports"][0]["resolved"], true);
        assert!(!service.cwd.join(".pinset").exists());
        assert!(!service.home.exists());
        let calls = resolver.calls.borrow().clone();
        let result = service.apply_import(plan).unwrap();
        assert_eq!(result["applied"], true);
        assert_eq!(*resolver.calls.borrow(), calls, "apply must not re-resolve");
        let context = ProjectContext::at(&service.cwd, false).unwrap();
        let (config, lock) = context.load_locked().unwrap();
        assert_eq!(config.tools["node"], "22.2.0");
        assert_eq!(lock.tool("node").unwrap().requested, "22.2.0");
        assert_eq!(config.tools["pnpm"], "10.2.0");
        assert!(
            fs::read_to_string(service.cwd.join(".gitignore"))
                .unwrap()
                .contains("/.pinset/local/")
        );
        assert_eq!(fs::read(service.cwd.join("package.json")).unwrap(), before);
        assert_eq!(fs::read(service.cwd.join(".nvmrc")).unwrap(), b"lts/*\n");
        assert!(!service.home.join("installs").exists());
    }

    #[test]
    fn existing_selections_and_locks_are_kept_while_missing_tools_are_added() {
        let (_directory, service, resolver) = fixture();
        write(&service, ".nvmrc", "20.11.1");
        let plan = service.prepare_import(&resolver).unwrap();
        service.apply_import(plan).unwrap();
        let context = ProjectContext::at(&service.cwd, false).unwrap();
        let (_, old_lock) = context.load_locked().unwrap();
        write(&service, ".nvmrc", "18.20.8");
        write(&service, ".tool-versions", "bun 1.2.0\nyarn 1.22.22\n");
        let plan = service.prepare_import(&resolver).unwrap();
        assert_eq!(
            plan.report()["conflicts"][0]["reason"],
            "existing_selection"
        );
        assert_eq!(plan.report()["unsupported"][0]["tool"], "yarn");
        service.apply_import(plan).unwrap();
        let (config, lock) = context.load_locked().unwrap();
        assert_eq!(config.tools["node"], "20.11.1");
        assert_eq!(config.tools["bun"], "1.2.0");
        assert_eq!(lock.tool("node"), old_lock.tool("node"));
    }

    #[test]
    fn compatible_existing_range_is_a_noop_and_does_not_write_files() {
        let (_directory, service, resolver) = fixture();
        service.init().unwrap();
        let context = ProjectContext::at(&service.cwd, false).unwrap();
        let mut config = context.load().unwrap();
        config.tools.insert("node".into(), "20".into());
        let mut lock = Lockfile::empty(config.project_id.clone());
        lock.upsert_tool(resolver.lock("node", "20", &config).unwrap())
            .unwrap();
        service.commit(&context, &config, &lock).unwrap();
        write(&service, "package.json", r#"{"engines":{"node":">=18"}}"#);
        let before = fs::read(&context.config_path).unwrap();
        let lock_before = fs::read(&context.lock_path).unwrap();
        let plan = service.prepare_import(&resolver).unwrap();
        assert!(!plan.needs_write());
        assert_eq!(plan.report()["conflicts"], json!([]));
        service.apply_import(plan).unwrap();
        assert_eq!(fs::read(context.config_path).unwrap(), before);
        assert_eq!(fs::read(context.lock_path).unwrap(), lock_before);
    }

    #[test]
    fn conflicting_sources_and_missing_node_dependency_are_skipped() {
        let (_directory, service, resolver) = fixture();
        write(&service, ".nvmrc", "20.11.1");
        write(&service, ".node-version", "18.20.8");
        write(
            &service,
            "package.json",
            r#"{"packageManager":"pnpm@10.2.0","engines":{"ruby":"3.3"}}"#,
        );
        let plan = service.prepare_import(&resolver).unwrap();
        assert!(!plan.needs_write());
        assert_eq!(plan.report()["conflicts"][0]["tool"], "node");
        assert_eq!(plan.report()["unresolved"][0]["tool"], "pnpm");
        assert_eq!(plan.report()["unsupported"][0]["tool"], "ruby");
        service.apply_import(plan).unwrap();
        assert!(!service.cwd.join(".pinset").exists());
        assert!(!service.home.exists());
    }

    #[test]
    fn changed_sources_or_config_invalidate_confirmation_before_writing() {
        for changed in [
            ".nvmrc",
            ".node-version",
            ".pinset/config.toml",
            ".pinset/lock.toml",
        ] {
            let (_directory, service, resolver) = fixture();
            write(&service, ".nvmrc", "20.11.1");
            let plan = service.prepare_import(&resolver).unwrap();
            let path = service.cwd.join(changed);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "changed").unwrap();
            assert_eq!(
                service.apply_import(plan).unwrap_err().code(),
                "PINSET_IMPORT_PLAN_STALE",
                "{changed}"
            );
            assert_eq!(fs::read(&path).unwrap(), b"changed");
            if changed != ".pinset/config.toml" {
                assert!(!service.cwd.join(".pinset/config.toml").exists());
            }
        }
    }

    #[test]
    fn missing_sources_list_every_checked_file_and_malformed_input_is_not_adopted() {
        let (_directory, service, resolver) = fixture();
        let error = match service.prepare_import(&resolver) {
            Err(error) => error,
            Ok(_) => panic!("expected no sources"),
        };
        assert_eq!(error.code(), "PINSET_IMPORT_NO_SOURCES");
        for source in SOURCES {
            assert!(error.to_string().contains(source));
        }
        for (source, text) in [
            ("package.json", "{broken"),
            ("rust-toolchain.toml", "[toolchain"),
            (".tool-versions", "nodejs"),
        ] {
            write(&service, source, text);
            let error = match service.prepare_import(&resolver) {
                Err(error) => error,
                Ok(_) => panic!("expected invalid input"),
            };
            assert_eq!(error.code(), "PINSET_IMPORT_SOURCE_INVALID");
            fs::remove_file(service.cwd.join(source)).unwrap();
        }
        assert!(!service.home.exists());
    }

    #[test]
    fn missing_lock_is_generated_without_replacing_existing_selector() {
        let (_directory, service, resolver) = fixture();
        service.init().unwrap();
        let context = ProjectContext::at(&service.cwd, false).unwrap();
        let mut config = context.load().unwrap();
        config.tools.insert("node".into(), "20".into());
        write_atomic(
            &context.config_path,
            toml::to_string_pretty(&config).unwrap().as_bytes(),
        )
        .unwrap();
        write(&service, ".nvmrc", "20.11.1");
        let plan = service.prepare_import(&resolver).unwrap();
        assert!(plan.needs_write());
        assert_eq!(plan.report()["imports"], json!([]));
        service.apply_import(plan).unwrap();
        let (current, lock) = context.load_locked().unwrap();
        assert_eq!(current, config);
        assert_eq!(lock.tool("node").unwrap().requested, "20");
    }

    #[test]
    fn rust_options_and_go_toolchain_are_imported_with_the_selection() {
        let (_directory, service, resolver) = fixture();
        write(
            &service,
            "go.mod",
            "module example.com/app\ngo 1.22\ntoolchain go1.23.1\n",
        );
        write(
            &service,
            "rust-toolchain.toml",
            "[toolchain]\nchannel='1.85.0'\nprofile='minimal'\ncomponents=['clippy']\ntargets=['wasm32-unknown-unknown']\n",
        );
        let plan = service.prepare_import(&resolver).unwrap();
        assert_eq!(plan.config.tools["go"], "1.23.1");
        assert_eq!(plan.config.rust_options.profile.as_deref(), Some("minimal"));
        service.apply_import(plan).unwrap();
        let (config, lock) = ProjectContext::at(&service.cwd, false)
            .unwrap()
            .load_locked()
            .unwrap();
        assert_eq!(config.rust_options.components, ["clippy"]);
        assert_eq!(lock.tool("rust").unwrap().options["components"], "clippy");
    }
}
