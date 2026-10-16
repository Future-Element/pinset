use crate::*;
use atomic_write_file::AtomicWriteFile;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn service_error(code: &'static str, message: impl Into<String>) -> Error {
    Error::Service {
        code,
        message: message.into(),
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(service_error(
            "PINSET_PATH_UNSAFE",
            "state file is a symlink",
        ));
    }
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| service_error("PINSET_PATH_UNSAFE", "state has no parent"))?,
    )?;
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(bytes)?;
    file.commit()?;
    Ok(())
}
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub struct StateGuard(std::fs::File, Option<std::fs::File>);
fn installation_selection<'a>(lock: &'a Lockfile, names: Option<&[String]>) -> Vec<&'a LockedTool> {
    lock.tools
        .iter()
        .filter(|tool| {
            names.is_none_or(|names| {
                names.is_empty()
                    || names.contains(&tool.name)
                    || (tool.name == "node" && names.iter().any(|name| name == "pnpm"))
            })
        })
        .collect()
}
impl Drop for StateGuard {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.0);
        if let Some(global) = &self.1 {
            let _ = fs4::FileExt::unlock(global);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Services {
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub(crate) progress: ProgressReporter,
}
impl Services {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self {
            cwd: cwd.canonicalize()?,
            home: pinset_home()?,
            progress: ProgressReporter::default(),
        })
    }
    pub fn with_progress_reporter(
        mut self,
        reporter: impl Fn(ProgressEvent) + Send + Sync + 'static,
    ) -> Self {
        self.progress = ProgressReporter::new(reporter);
        self
    }
    pub fn ensure_home(&self) -> Result<()> {
        if fs::symlink_metadata(&self.home).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(service_error(
                "PINSET_HOME_UNSAFE",
                "v3 home cannot be a symlink",
            ));
        }
        fs::create_dir_all(&self.home)?;
        for name in [
            "bin",
            "installs",
            "cache",
            "global",
            "state",
            ".pinset-home.json",
            ".bootstrap.lock",
        ] {
            if fs::symlink_metadata(self.home.join(name)).is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(service_error(
                    "PINSET_HOME_UNSAFE",
                    "managed home paths cannot be symbolic links",
                ));
            }
        }
        let bootstrap = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.home.join(".bootstrap.lock"))?;
        fs4::FileExt::lock(&bootstrap)?;
        let _bootstrap = StateGuard(bootstrap, None);
        let marker = self.home.join(".pinset-home.json");
        if marker.exists() {
            let value: Value = read_json(&marker)?;
            if value["protocol"] != PROTOCOL {
                return Err(service_error(
                    "PINSET_HOME_EXTERNAL",
                    "v3 home is not owned by Pinset",
                ));
            }
            return Ok(());
        }
        if self.home.exists()
            && fs::read_dir(&self.home)?.any(|e| {
                e.is_ok_and(|e| e.file_name() != "cache" && e.file_name() != ".bootstrap.lock")
            })
        {
            return Err(service_error(
                "PINSET_HOME_EXTERNAL",
                "refusing to adopt a populated, unmarked v3 home",
            ));
        }
        write_json(
            &marker,
            &json!({"protocol":PROTOCOL,"home_id":uuid::Uuid::new_v4().to_string()}),
        )
    }
    pub fn context(&self, global: bool) -> Result<ProjectContext> {
        Ok(selected_context(&self.cwd, global)?)
    }
    pub fn project(&self) -> Result<ProjectContext> {
        find_project_context(&self.cwd)?.ok_or_else(|| {
            service_error(
                "PINSET_PROJECT_REQUIRED",
                "run pinset init in the intended project directory",
            )
        })
    }
    pub fn load(&self, context: &ProjectContext) -> Result<(ProjectConfig, Lockfile)> {
        let (config, lock) = self.load_for_edit(context)?;
        if !context.lock_path.exists() && !config.tools.is_empty() {
            return Err(service_error(
                "PINSET_LOCK_MISSING",
                "selected tools require an exact lock; run pinset use",
            ));
        }
        config.validate_lock(&lock)?;
        Ok((config, lock))
    }
    fn load_for_edit(&self, context: &ProjectContext) -> Result<(ProjectConfig, Lockfile)> {
        let config = context.load()?;
        let lock = if context.lock_path.exists() {
            load_lockfile(&context.lock_path)?
        } else {
            Lockfile::empty(config.project_id.clone())
        };
        // Explicit selection edits reconcile config and lock together. Reads and
        // installation still require a matching pair through load().
        if lock.project_id != config.project_id {
            return Err(service_error(
                "PINSET_LOCK_MISMATCH",
                "configuration and lock have different project identities",
            ));
        }
        Ok((config, lock))
    }
    pub fn guard(&self, id: &str) -> Result<StateGuard> {
        self.state_guard("projects", id)
    }
    pub(crate) fn resource_guard(&self, id: &str) -> Result<StateGuard> {
        self.state_guard("resources", id)
    }
    pub(crate) fn maintenance_guard(&self) -> Result<StateGuard> {
        self.ensure_home()?;
        let directory = self.home.join("state/locks");
        crate::download_cache::reject_linked_managed_path(&self.home, &directory)?;
        if fs::symlink_metadata(&directory).is_ok_and(|m| !m.is_dir() || m.file_type().is_symlink())
        {
            return Err(service_error(
                "PINSET_PATH_UNSAFE",
                "managed lock directory must be a real directory",
            ));
        }
        fs::create_dir_all(&directory)?;
        crate::download_cache::reject_linked_managed_path(
            &self.home,
            &directory.join("maintenance.lock"),
        )?;
        let global = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.home.join("state/locks/maintenance.lock"))?;
        fs4::FileExt::lock(&global)?;
        Ok(StateGuard(global, None))
    }
    fn state_guard(&self, namespace: &str, id: &str) -> Result<StateGuard> {
        self.ensure_home()?;
        validate_id(id)?;
        let dir = self.home.join("state/locks").join(namespace);
        crate::download_cache::reject_linked_managed_path(&self.home, &dir)?;
        fs::create_dir_all(&dir)?;
        crate::download_cache::reject_linked_managed_path(
            &self.home,
            &self.home.join("state/locks/maintenance.lock"),
        )?;
        crate::download_cache::reject_linked_managed_path(
            &self.home,
            &dir.join(format!("{id}.lock")),
        )?;
        let global = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.home.join("state/locks/maintenance.lock"))?;
        fs4::FileExt::lock_shared(&global)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(format!("{id}.lock")))?;
        fs4::FileExt::lock(&file)?;
        Ok(StateGuard(file, Some(global)))
    }
    pub fn init(&self) -> Result<Value> {
        self.ensure_home()?;
        let _guard = self.guard(&digest(self.cwd.to_string_lossy().as_bytes())[..24])?;
        let root = &self.cwd;
        let context = ProjectContext::at(root, false)?;
        if context.config_path.exists() {
            return Err(service_error(
                "PINSET_PROJECT_EXISTS",
                "this directory already has a Pinset 3 configuration",
            ));
        }
        let config = ProjectConfig::new(uuid::Uuid::new_v4().to_string());
        write_atomic(
            &context.config_path,
            toml::to_string_pretty(&config)?.as_bytes(),
        )?;
        self.register(&context)?;
        self.gitignore(&context)?;
        Ok(
            json!({"protocol":PROTOCOL,"project":root,"project_id":config.project_id,"config":context.config_path}),
        )
    }
    fn edit_context(
        &self,
        global: bool,
        plan: bool,
    ) -> Result<(ProjectContext, ProjectConfig, Lockfile)> {
        if !global {
            let c = self.project()?;
            let (config, lock) = self.load_for_edit(&c)?;
            return Ok((c, config, lock));
        }
        if !plan {
            self.ensure_home()?;
        }
        let root = self.home.join("global");
        if !root.join("config.toml").exists() {
            let config = ProjectConfig::new("global".into());
            let lock = Lockfile::empty(config.project_id.clone());
            // Plan uses an explicit synthetic global context without creating directories.
            let c = if plan {
                ProjectContext {
                    root: root.clone(),
                    config_path: root.join("config.toml"),
                    lock_path: root.join("lock.toml"),
                    local: root.join("local"),
                    directory: work_directory_identity(&self.cwd)?,
                    global: true,
                }
            } else {
                fs::create_dir_all(&root)?;
                ProjectContext::at(&root, true)?
            };
            return Ok((c, config, lock));
        }
        let c = ProjectContext::at(&root, true)?;
        let (config, lock) = self.load_for_edit(&c)?;
        Ok((c, config, lock))
    }
    pub fn use_tools(
        &self,
        specs: &[String],
        global: bool,
        no_install: bool,
        plan: bool,
    ) -> Result<Value> {
        let (context, mut config, mut lock) = self.edit_context(global, plan)?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&config.project_id)?)
        };
        if !plan && context.config_path.exists() {
            (config, lock) = self.load_for_edit(&context)?;
        }
        let mut names = std::collections::BTreeSet::new();
        for spec in specs {
            let (tool, selector) = spec.split_once('@').ok_or_else(|| {
                service_error("PINSET_SELECTOR_REQUIRED", "use requires tool@selector")
            })?;
            validate_tool(tool)?;
            if selector.is_empty() || !names.insert(tool) {
                return Err(service_error(
                    "PINSET_SELECTION_DUPLICATE",
                    "empty selector or duplicate tool",
                ));
            }
            config.tools.insert(tool.into(), selector.into());
        }
        config.validate()?;
        let total = names.len();
        for (index, name) in names.into_iter().enumerate() {
            let selector = &config.tools[name];
            self.progress.report(ProgressEvent::Resolving {
                tool: name.to_owned(),
                selector: selector.clone(),
                index: index + 1,
                total,
            });
            let mut tool = self.resolve(name, selector, &config)?;
            self.progress.report(ProgressEvent::Resolved {
                tool: name.to_owned(),
                version: tool.version.clone(),
            });
            if let Some(old) = lock.tool(name) {
                validate_verification_transition(old, &tool)?;
            }
            tool.requested = selector.clone();
            lock.upsert_tool(tool)?;
        }
        config.validate_lock(&lock)?;
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"config":config,"lock":lock,"install":!no_install}),
            );
        }
        if !no_install {
            self.install_sdk(&lock, None, false, false)?;
        }
        self.progress
            .report(ProgressEvent::Phase(InstallPhase::Committing));
        let id = self.begin_commit(&context, &config, &lock)?;
        let notes = if no_install {
            vec![]
        } else {
            self.progress
                .report(ProgressEvent::Phase(InstallPhase::Binding));
            self.bind_local(&context, &config, &lock, false, true)?
        };
        self.finish_commit(&context, &id)?;
        Ok(json!({"protocol":PROTOCOL,"transaction":id,"selected":config.tools,"notes":notes}))
    }
    pub fn resolve(
        &self,
        name: &str,
        selector: &str,
        config: &ProjectConfig,
    ) -> Result<LockedTool> {
        let mut tool = match name {
            "node" => NodeMetadataClient::official()?.resolve_tool(selector)?,
            "go" => GoMetadataClient::official()?.resolve_tool(selector)?,
            "python" => PythonMetadataClient::official()?.resolve_tool(selector)?,
            "java" => JavaMetadataClient::official()?.resolve_tool(selector)?,
            "rust" => RustMetadataClient::official()?
                .resolve_tool_with_options(selector, Some(&config.rust_options))?,
            "flutter" => FlutterMetadataClient::official()?.resolve_tool(selector)?,
            "pnpm" | "bun" => {
                let c = NpmMetadataClient::official()?;
                let version = c.resolve_version_selector(name, selector)?;
                c.resolve_tool(name, &version)?
            }
            _ => return Err(service_error("PINSET_TOOL_UNKNOWN", name)),
        };
        tool.requested = selector.into();
        let platforms = if config.platforms.is_empty() {
            vec![current_target_for_tool(name)]
        } else {
            config
                .platforms
                .iter()
                .map(|p| {
                    if name == "bun" && matches!(p.as_str(), "linux-x86_64" | "windows-x86_64") {
                        format!("{p}-baseline")
                    } else {
                        p.clone()
                    }
                })
                .collect()
        };
        for p in &platforms {
            if tool.artifact(p).is_none() {
                return Err(service_error(
                    "PINSET_PLATFORM_UNAVAILABLE",
                    format!("{name} {} has no official artifact for {p}", tool.version),
                ));
            }
        }
        tool.artifacts.retain(|a| platforms.contains(&a.target));
        config.policy.validate_tool(&tool, SystemTime::now())?;
        Ok(tool)
    }
    pub fn remove(&self, names: &[String], global: bool, plan: bool) -> Result<Value> {
        let (context, mut config, mut lock) = self.edit_context(global, plan)?;
        let _guard = if plan {
            None
        } else {
            Some(self.guard(&config.project_id)?)
        };
        if !plan && context.config_path.exists() {
            (config, lock) = self.load_for_edit(&context)?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for name in names {
            validate_tool(name)?;
            if !seen.insert(name) {
                return Err(service_error(
                    "PINSET_SELECTION_DUPLICATE",
                    "duplicate tool",
                ));
            }
            config.tools.remove(name);
            lock.remove_tool(name);
        }
        config.validate_lock(&lock)?;
        if !plan {
            self.commit(&context, &config, &lock)?;
        }
        Ok(
            json!({"protocol":PROTOCOL,"plan":plan,"selected":config.tools,"retained_installs":true}),
        )
    }
    pub fn install(
        &self,
        names: &[String],
        global: bool,
        offline: bool,
        repair: bool,
        recreate: bool,
        plan: bool,
    ) -> Result<Value> {
        let c = self.context(global)?;
        let (config, lock) = self.load(&c)?;
        for name in names {
            if lock.tool(name).is_none() {
                return Err(service_error("PINSET_TOOL_NOT_SELECTED", name));
            }
        }
        if recreate
            && (global
                || !config.tools.contains_key("python")
                || (!names.is_empty() && !names.iter().any(|name| name == "python")))
        {
            return Err(service_error(
                "PINSET_VENV_PROJECT_REQUIRED",
                "recreate-venv requires Python in the project installation selection",
            ));
        }
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"tools":installation_selection(&lock, Some(names)).iter().map(|t|&t.name).collect::<Vec<_>>(),"offline":offline,"repair":repair,"recreate_venv":recreate}),
            );
        }
        let _guard = self.guard(&config.project_id)?;
        let (current, locked) = self.load(&c)?;
        for name in names {
            if locked.tool(name).is_none() {
                return Err(service_error("PINSET_TOOL_NOT_SELECTED", name));
            }
        }
        if recreate && !current.tools.contains_key("python") {
            return Err(service_error(
                "PINSET_VENV_PROJECT_REQUIRED",
                "project Python is no longer selected",
            ));
        }
        self.install_sdk(&locked, Some(names), offline, repair)?;
        self.progress
            .report(ProgressEvent::Phase(InstallPhase::Binding));
        let notes =
            self.bind_selected_local(&c, &current, &locked, recreate, false, Some(names))?;
        self.register(&c)?;
        Ok(json!({"protocol":PROTOCOL,"installed":true,"notes":notes}))
    }
    pub fn install_sdk(
        &self,
        lock: &Lockfile,
        names: Option<&[String]>,
        offline: bool,
        repair: bool,
    ) -> Result<()> {
        self.ensure_home()?;
        let selected = installation_selection(lock, names);
        for (index, tool) in selected.iter().enumerate() {
            self.progress.report(ProgressEvent::Installing {
                tool: tool.name.clone(),
                version: tool.version.clone(),
                index: index + 1,
                total: selected.len(),
            });
            let target = locked_target(tool);
            let progress = self.progress.clone();
            let installer = Installer::new(InstallLimits::for_tool(&tool.name))?
                .with_progress_reporter(move |event| progress.report(event))
                .with_offline(offline)
                .with_repair(repair)
                .with_install_identity(tool.installation_version(&target));
            let sources = OfficialSources;
            let result = match tool.name.as_str() {
                "node" => install_locked_node(&installer, &self.home, &sources, tool, &target),
                "go" => install_locked_go(&installer, &self.home, &sources, tool, &target),
                "python" => install_locked_python(&installer, &self.home, &sources, tool, &target),
                "flutter" => {
                    install_locked_flutter(&installer, &self.home, &sources, tool, &target)
                }
                "java" => install_locked_java(&installer, &self.home, tool, &target),
                "rust" => install_locked_rust(&installer, &self.home, tool, &target),
                "pnpm" | "bun" => install_locked_npm_tool(&installer, &self.home, tool, &target),
                _ => unreachable!(),
            };
            let outcome = result?;
            if tool.name == "java" {
                self.inventory_java(tool, &target)?;
            }
            self.progress.report(ProgressEvent::Installed {
                reused: outcome.reused_existing,
            });
        }
        self.progress
            .report(ProgressEvent::Phase(InstallPhase::Binding));
        self.install_shims()?;
        Ok(())
    }
    fn inventory_java(&self, tool: &LockedTool, target: &str) -> Result<()> {
        let root = install_directory(&self.home, tool, target);
        let sdk = sdk_root(&root, "java", target);
        let mut receipt: InstallReceipt =
            toml::from_str(&fs::read_to_string(root.join(".pinset-install.toml"))?)?;
        let release = fs::read_to_string(sdk.join("release"))?;
        if !release.contains("IMPLEMENTOR=\"Eclipse Adoptium\"")
            && !release.contains("IMPLEMENTOR=\"AdoptOpenJDK\"")
        {
            return Err(service_error(
                "PINSET_JDK_INVALID",
                "release metadata is not Temurin OpenJDK",
            ));
        }
        for path in ["include/jni.h", "include/jvmti.h", "legal"] {
            if !sdk.join(path).exists() && !(path == "legal" && tool.version.starts_with("8.")) {
                return Err(service_error(
                    "PINSET_JDK_INCOMPLETE",
                    format!("JDK missing {path}"),
                ));
            }
        }
        if ![sdk.join("lib/src.zip"), sdk.join("src.zip")]
            .iter()
            .any(|p| p.is_file())
        {
            return Err(service_error(
                "PINSET_JDK_INCOMPLETE",
                "JDK is missing standard library sources",
            ));
        }
        let major = tool
            .version
            .split('.')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let linkable_runtime =
            major >= 24 && sdk.join("lib/modules").is_file() && release.contains("jdk.jlink");
        if major >= 9 && !sdk.join("jmods").is_dir() && !linkable_runtime {
            return Err(service_error(
                "PINSET_JDK_INCOMPLETE",
                "modular JDK is missing packaged modules or a supported linkable runtime image",
            ));
        }
        for entry in fs::read_dir(sdk.join("bin"))? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let filename = entry.file_name().to_string_lossy().into_owned();
                let command = filename.trim_end_matches(".exe");
                if !filename.contains('.') || filename.ends_with(".exe") {
                    receipt.commands.insert(
                        command.into(),
                        entry
                            .path()
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        for required in [
            "java",
            "javac",
            "jar",
            "javadoc",
            "javap",
            "jarsigner",
            "keytool",
            "jdb",
            "jcmd",
        ] {
            if !receipt.commands.contains_key(required) {
                return Err(service_error(
                    "PINSET_JDK_INCOMPLETE",
                    format!("full JDK missing {required}"),
                ));
            }
        }
        write_atomic(
            &root.join(".pinset-install.toml"),
            toml::to_string(&receipt)?.as_bytes(),
        )?;
        Ok(())
    }
    pub fn bind_local(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        lock: &Lockfile,
        recreate: bool,
        preserve_mismatch: bool,
    ) -> Result<Vec<String>> {
        self.bind_selected_local(c, config, lock, recreate, preserve_mismatch, None)
    }
    fn bind_selected_local(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        lock: &Lockfile,
        recreate: bool,
        preserve_mismatch: bool,
        names: Option<&[String]>,
    ) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        if !c.global {
            let selected = |name: &str| {
                names.is_none_or(|names| names.is_empty() || names.iter().any(|n| n == name))
            };
            if let Some(tool) = lock.tool("python").filter(|_| selected("python")) {
                if c.root.join(".venv").exists() && !recreate {
                    if let Err(error) = validate_venv(
                        c,
                        config,
                        tool,
                        &current_target_for_tool("python"),
                        &self.home,
                    ) {
                        if preserve_mismatch && error.code() == "PINSET_VENV_MISMATCH" {
                            notes.push(error.to_string());
                        } else {
                            return Err(error.into());
                        }
                    }
                } else {
                    self.create_venv(c, config, tool, recreate)?;
                }
            }
            if let Some(tool) = lock.tool("flutter").filter(|_| selected("flutter")) {
                let target = current_target_for_tool("flutter");
                read_receipt(&self.home, tool, &target)?;
                let link = c.local.join("flutter-sdk");
                fs::create_dir_all(&c.local)?;
                if fs::symlink_metadata(&link).is_ok() {
                    if !fs::symlink_metadata(&link)?.file_type().is_symlink() {
                        return Err(service_error(
                            "PINSET_SDK_EXTERNAL",
                            "Flutter SDK link is not owned by Pinset",
                        ));
                    }
                    fs::remove_file(&link)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(install_directory(&self.home, tool, &target), &link)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(
                    install_directory(&self.home, tool, &target),
                    &link,
                )?;
            }
            self.gitignore(c)?;
        }
        Ok(notes)
    }
    pub fn register(&self, c: &ProjectContext) -> Result<()> {
        if c.global {
            return Ok(());
        }
        let _guard = self.resource_guard("registry")?;
        let file = self.home.join("state/projects.json");
        let mut roots: BTreeMap<String, PathBuf> = if file.exists() {
            read_json(&file)?
        } else {
            BTreeMap::new()
        };
        roots.insert(c.root.display().to_string(), c.config_path.clone());
        write_json(&file, &roots)
    }
    fn gitignore(&self, c: &ProjectContext) -> Result<()> {
        if c.global {
            return Ok(());
        }
        let p = c.root.join(".gitignore");
        let mut s = fs::read_to_string(&p).unwrap_or_default();
        for pattern in ["/.pinset/local/", "/.venv/"] {
            if !s.lines().any(|l| l == pattern) {
                if !s.ends_with('\n') && !s.is_empty() {
                    s.push('\n');
                }
                s.push_str(pattern);
                s.push('\n');
            }
        }
        write_atomic(&p, s.as_bytes())
    }
    pub fn remote(&self, name: &str) -> Result<Value> {
        validate_tool(name)?;
        let releases = match name {
            "node" => serde_json::to_value(NodeMetadataClient::official()?.available_releases()?)?,
            "go" => serde_json::to_value(GoMetadataClient::official()?.available_releases()?)?,
            "python" => {
                serde_json::to_value(PythonMetadataClient::official()?.available_releases()?)?
            }
            "java" => serde_json::to_value(JavaMetadataClient::official()?.available_releases()?)?,
            "rust" => serde_json::to_value(RustMetadataClient::official()?.available_releases()?)?,
            "flutter" => {
                serde_json::to_value(FlutterMetadataClient::official()?.available_releases()?)?
            }
            _ => serde_json::to_value(NpmMetadataClient::official()?.available_releases(name)?)?,
        };
        Ok(json!({"protocol":PROTOCOL,"tool":name,"releases":releases}))
    }
    pub fn list(&self, name: Option<&str>) -> Result<Value> {
        if let Some(n) = name {
            validate_tool(n)?;
        }
        let context = self.context(false).ok();
        let lock = context
            .as_ref()
            .map(|c| self.load(c))
            .transpose()?
            .map(|(_, l)| l);
        let installs = self.home.join("installs");
        let mut entries = vec![];
        if installs.exists() {
            for tool in fs::read_dir(installs)? {
                let tool = tool?;
                let n = tool.file_name().to_string_lossy().into_owned();
                if name.is_some_and(|v| v != n) {
                    continue;
                }
                for identity in fs::read_dir(tool.path())? {
                    let identity = identity?;
                    for platform in fs::read_dir(identity.path())? {
                        let platform = platform?;
                        let p = platform.path().join(".pinset-install.toml");
                        if let Ok(receipt) = fs::read_to_string(&p).and_then(|s| {
                            toml::from_str::<InstallReceipt>(&s).map_err(std::io::Error::other)
                        }) && receipt.schema == 3
                        {
                            entries.push(json!({"tool":n,"version":receipt.version,"identity":receipt.install_identity,"platform":receipt.target,"path":platform.path()}));
                        }
                    }
                }
            }
        }
        Ok(json!({"protocol":PROTOCOL,"selected":lock,"installs":entries}))
    }
    pub fn which(&self, command: Option<&str>, global: bool) -> Result<Value> {
        let c = self.context(global)?;
        if let Some(cmd) = command {
            return Ok(serde_json::to_value(plan_command(
                &self.cwd, &self.home, &c, cmd,
            )?)?);
        }
        let (config, lock) = self.load(&c)?;
        let mut names = lock
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let global_root = self.home.join("global");
        if !c.global
            && !config.policy.require_project_tools
            && global_root.join("config.toml").exists()
        {
            let global = ProjectContext::at(&global_root, true)?;
            let (_, defaults) = global.load_locked()?;
            names.extend(defaults.tools.into_iter().map(|tool| tool.name));
        }
        let mut plans = vec![];
        for name in &names {
            let cmd = if name == "rust" { "rustc" } else { name };
            plans.push(plan_command(&self.cwd, &self.home, &c, cmd)?);
        }
        Ok(json!({"protocol":PROTOCOL,"commands":plans}))
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn maintenance_initializes_a_fresh_home_without_a_project() {
        let directory = tempfile::tempdir().unwrap();
        let service = Services {
            cwd: directory.path().into(),
            home: directory.path().join("home/v3"),
            progress: ProgressReporter::default(),
        };
        let guard = service.maintenance_guard().unwrap();
        assert!(service.home.join("state/locks/maintenance.lock").is_file());
        drop(guard);
        service.maintenance_guard().unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn linked_lock_paths_never_write_to_an_external_directory() {
        let directory = tempfile::tempdir().unwrap();
        let service = Services {
            cwd: directory.path().into(),
            home: directory.path().join("home/v3"),
            progress: ProgressReporter::default(),
        };
        service.ensure_home().unwrap();
        let external = directory.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::create_dir(service.home.join("state")).unwrap();
        std::os::unix::fs::symlink(&external, service.home.join("state/locks")).unwrap();
        for result in [
            service.maintenance_guard(),
            service.guard("project"),
            service.resource_guard("shims"),
        ] {
            assert!(matches!(result, Err(error) if error.code() == "PINSET_PATH_UNSAFE"));
        }
        assert_eq!(fs::read_dir(external).unwrap().count(), 0);
    }
}
