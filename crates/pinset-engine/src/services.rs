use crate::*;
use atomic_write_file::AtomicWriteFile;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
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
}
impl Services {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self {
            cwd: cwd.canonicalize()?,
            home: pinset_home()?,
        })
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
        let config = context.load()?;
        let lock = if context.lock_path.exists() {
            load_lockfile(&context.lock_path)?
        } else if config.tools.is_empty() {
            Lockfile::empty(config.project_id.clone())
        } else {
            return Err(service_error(
                "PINSET_LOCK_MISSING",
                "selected tools require an exact lock; run pinset use",
            ));
        };
        config.validate_lock(&lock)?;
        Ok((config, lock))
    }
    pub fn guard(&self, id: &str) -> Result<StateGuard> {
        self.ensure_home()?;
        validate_id(id)?;
        let dir = self.home.join("state/locks/projects");
        fs::create_dir_all(&dir)?;
        let global = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.home.join("state/locks/maintenance.lock"))?;
        if id == "maintenance" {
            fs4::FileExt::lock(&global)?;
        } else {
            fs4::FileExt::lock_shared(&global)?;
        }
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
            let (config, lock) = self.load(&c)?;
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
        let (config, lock) = self.load(&c)?;
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
            (config, lock) = self.load(&context)?;
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
        for name in names {
            let selector = &config.tools[name];
            let mut tool = self.resolve(name, selector, &config)?;
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
        let id = self.begin_commit(&context, &config, &lock)?;
        let notes = if no_install {
            vec![]
        } else {
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
        let strength = config
            .policy
            .minimum_verification
            .as_ref()
            .map(|s| serde_json::from_value::<VerificationStrength>(json!(s)))
            .transpose()?;
        let age = config
            .policy
            .minimum_release_age
            .as_ref()
            .map(|s| serde_json::from_value::<MinimumReleaseAge>(json!(s)))
            .transpose()?;
        validate_tool_policy(&tool, strength, age, SystemTime::now())?;
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
            (config, lock) = self.load(&context)?;
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
        if recreate && (global || !config.tools.contains_key("python")) {
            return Err(service_error(
                "PINSET_VENV_PROJECT_REQUIRED",
                "recreate-venv requires a selected project Python",
            ));
        }
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"tools":if names.is_empty(){config.tools.keys().cloned().collect::<Vec<_>>()}else{names.to_vec()},"offline":offline,"repair":repair,"recreate_venv":recreate}),
            );
        }
        let _guard = self.guard(&config.project_id)?;
        let (current, locked) = self.load(&c)?;
        self.install_sdk(&locked, Some(names), offline, repair)?;
        let notes = self.bind_local(&c, &current, &locked, recreate, false)?;
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
        for tool in &lock.tools {
            if names.is_some_and(|n| !n.is_empty() && !n.contains(&tool.name)) {
                continue;
            }
            let target = locked_target(tool);
            let install = install_directory(&self.home, tool, &target);
            let mut quarantine = None;
            let damaged = if repair && install.exists() {
                match read_receipt(&self.home, tool, &target) {
                    Ok(receipt) => {
                        install_payload_fingerprint(&install, &tool.name)? != receipt.payload_digest
                    }
                    Err(_) => true,
                }
            } else {
                false
            };
            if repair && install.exists() && damaged {
                let receipt: InstallReceipt =
                    toml::from_str(&fs::read_to_string(install.join(".pinset-install.toml"))?)?;
                if receipt.schema != 3
                    || receipt.install_identity != tool.installation_version(&target)
                    || receipt.tool != tool.name
                {
                    return Err(service_error(
                        "PINSET_INSTALL_EXTERNAL",
                        "repair cannot adopt an external installation",
                    ));
                }
                let q = self
                    .home
                    .join("state/quarantine")
                    .join(uuid::Uuid::new_v4().to_string());
                fs::create_dir_all(q.parent().unwrap())?;
                fs::rename(&install, &q)?;
                quarantine = Some(q);
            }
            let installer = Installer::new(InstallLimits::for_tool(&tool.name))?
                .with_offline(offline)
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
            if let Err(error) = result {
                if let Some(q) = quarantine
                    && !install.exists()
                {
                    fs::rename(q, &install)?;
                }
                return Err(error);
            }
            if tool.name == "java" {
                self.inventory_java(tool, &target)?;
            }
            if let Some(quarantine) = quarantine {
                fs::remove_dir_all(quarantine)?;
            }
        }
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
        let mut notes = Vec::new();
        if !c.global {
            if let Some(tool) = lock.tool("python") {
                if c.root.join(".venv").exists() && !recreate {
                    if let Err(error) =
                        validate_venv(c, config, tool, &current_target_for_tool("python"))
                    {
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
            if let Some(tool) = lock.tool("flutter") {
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
    pub fn create_venv(
        &self,
        c: &ProjectContext,
        config: &ProjectConfig,
        tool: &LockedTool,
        recreate: bool,
    ) -> Result<()> {
        let target = current_target_for_tool("python");
        let path = c.root.join(".venv");
        let mut old = None;
        if fs::symlink_metadata(&path).is_ok() {
            let owner: VenvOwner =
                toml::from_str(&fs::read_to_string(path.join(VENV_MARKER)).map_err(|_| {
                    service_error("PINSET_VENV_EXTERNAL", "external venv cannot be replaced")
                })?)?;
            if !recreate
                || fs::symlink_metadata(&path)?.file_type().is_symlink()
                || owner.protocol != PROTOCOL
                || owner.project_id != config.project_id
                || owner.directory != c.directory
            {
                return Err(service_error(
                    "PINSET_VENV_EXTERNAL",
                    "only a valid Pinset 3 venv in this directory can be recreated",
                ));
            }
            let backup = c
                .local
                .join("venv-backups")
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir_all(backup.parent().unwrap())?;
            fs::rename(&path, &backup)?;
            old = Some(backup);
        }
        let interpreter = interpreter_path(&install_directory(&self.home, tool, &target), &target);
        let status = Command::new(&interpreter)
            .args(["-I", "-m", "venv"])
            .arg(&path)
            .env_remove("PINSET_IDENTITY")
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .status();
        if !status.as_ref().is_ok_and(|s| s.success()) {
            if path.exists() {
                fs::remove_dir_all(&path)?;
            }
            if let Some(backup) = old {
                fs::rename(backup, &path)?;
            }
            return Err(service_error(
                "PINSET_VENV_CREATE",
                "standard library venv creation failed",
            ));
        }
        let owner = VenvOwner {
            protocol: PROTOCOL.into(),
            project_id: config.project_id.clone(),
            directory: c.directory.clone(),
            interpreter_identity: tool.installation_version(&target),
            platform: target.clone(),
        };
        write_atomic(&path.join(VENV_MARKER), toml::to_string(&owner)?.as_bytes())?;
        validate_venv(c, config, tool, &target)?;
        Ok(())
    }
    pub fn register(&self, c: &ProjectContext) -> Result<()> {
        if c.global {
            return Ok(());
        }
        let _guard = self.guard("registry")?;
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
    pub fn install_shims(&self) -> Result<()> {
        let _guard = self.guard("shims")?;
        let current = std::env::current_exe()?;
        let adjacent = current.parent().unwrap();
        let shim = adjacent.join(if cfg!(windows) {
            "pinset-shim.exe"
        } else {
            "pinset-shim"
        });
        if !shim.is_file() {
            return Err(service_error(
                "PINSET_SHIM_MISSING",
                "pinset-shim must be distributed beside pinset",
            ));
        }
        let bin = self.home.join("bin");
        fs::create_dir_all(&bin)?;
        let state = self.home.join("state/shims.json");
        let prior: Value = if state.exists() {
            read_json(&state)?
        } else {
            json!({})
        };
        let previous = prior["digest"].as_str();
        let current_digest = digest(&fs::read(&shim)?);
        for (src, dst) in [
            (
                &current,
                bin.join(if cfg!(windows) {
                    "pinset.exe"
                } else {
                    "pinset"
                }),
            ),
            (
                &shim,
                bin.join(if cfg!(windows) {
                    "pinset-shim.exe"
                } else {
                    "pinset-shim"
                }),
            ),
        ] {
            if src.canonicalize()? != dst.canonicalize().unwrap_or_default() {
                let staged = bin.join(format!(".binary-{}", uuid::Uuid::new_v4()));
                fs::copy(src, &staged)?;
                fs::rename(&staged, &dst)?;
            }
        }
        let mut commands = public_commands();
        // Keep ownership of previously generated JDK entries after an SDK is cleaned.
        // Missing capabilities must fail against the selected JDK, never search PATH.
        if let Some(entries) = prior["commands"].as_object() {
            commands.extend(
                entries
                    .keys()
                    .filter(|name| {
                        validate_id(name).is_ok()
                            && !matches!(name.as_str(), "pinset" | "pinset-shim")
                    })
                    .cloned(),
            );
        }
        let java = self.home.join("installs/java");
        if java.is_dir() {
            for identity in fs::read_dir(java)? {
                for target in fs::read_dir(identity?.path())? {
                    let file = target?.path().join(".pinset-install.toml");
                    if let Ok(r) = fs::read_to_string(file).and_then(|s| {
                        toml::from_str::<InstallReceipt>(&s).map_err(std::io::Error::other)
                    }) && r.schema == 3
                        && r.tool == "java"
                    {
                        commands.extend(
                            r.commands
                                .keys()
                                .filter(|name| validate_id(name).is_ok())
                                .cloned(),
                        );
                    }
                }
            }
        }
        commands.sort();
        commands.dedup();
        let owners = commands
            .iter()
            .map(|command| (command.clone(), command_tool(command).unwrap_or("java")))
            .collect::<BTreeMap<_, _>>();
        for command in commands {
            let dst = bin.join(executable_name(&command, &current_target()));
            if dst.exists() {
                let found = digest(&fs::read(&dst)?);
                if found != current_digest && Some(found.as_str()) != previous {
                    return Err(service_error(
                        "PINSET_SHIM_EXTERNAL",
                        format!("refusing to replace external command {command}"),
                    ));
                }
                fs::remove_file(&dst)?;
            }
            fs::hard_link(
                bin.join(if cfg!(windows) {
                    "pinset-shim.exe"
                } else {
                    "pinset-shim"
                }),
                dst,
            )?;
        }
        write_json(
            &state,
            &json!({"protocol":PROTOCOL,"digest":current_digest,"commands":owners}),
        )?;
        Ok(())
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
        let (_, lock) = self.load(&c)?;
        let mut plans = vec![];
        for tool in &lock.tools {
            let cmd = if tool.name == "rust" {
                "rustc"
            } else {
                &tool.name
            };
            plans.push(plan_command(&self.cwd, &self.home, &c, cmd)?);
        }
        Ok(json!({"protocol":PROTOCOL,"commands":plans}))
    }
}
