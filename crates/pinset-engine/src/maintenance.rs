use crate::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
impl Services {
    fn references(&self) -> Result<(BTreeSet<String>, BTreeSet<String>, bool)> {
        let mut installs = BTreeSet::new();
        let mut cache = BTreeSet::new();
        let mut uncertain = false;
        let mut locks = vec![];
        let global = self.home.join("global/lock.toml");
        if global.exists() {
            match load_lockfile(&global) {
                Ok(l) => locks.push(l),
                Err(_) => uncertain = true,
            }
        }
        let registry = self.home.join("state/projects.json");
        if registry.exists() {
            let roots: BTreeMap<String, PathBuf> = read_json(&registry)?;
            for (_, path) in roots {
                if let Some(state) = path.parent() {
                    let lock = state.join("lock.toml");
                    if lock.exists() {
                        match load_lockfile(&lock) {
                            Ok(l) => locks.push(l),
                            Err(_) => uncertain = true,
                        }
                    } else {
                        uncertain = true;
                    }
                    let root = state.parent().unwrap_or(state);
                    let mut markers = vec![root.join(".venv").join(VENV_MARKER)];
                    let backups = state.join("local/venv-backups");
                    if backups.is_dir() {
                        for entry in fs::read_dir(backups)? {
                            markers.push(entry?.path().join(VENV_MARKER));
                        }
                    }
                    for marker in markers {
                        if marker.exists() {
                            match fs::read_to_string(marker)
                                .ok()
                                .and_then(|s| toml::from_str::<VenvOwner>(&s).ok())
                            {
                                Some(o) if o.protocol == PROTOCOL => {
                                    installs.insert(o.interpreter_identity);
                                }
                                _ => uncertain = true,
                            }
                        }
                    }
                }
            }
        }
        let journal = self.home.join("state/transactions");
        if journal.exists() {
            for e in fs::read_dir(journal)? {
                let j: TransactionJournal = match read_json(&e?.path()) {
                    Ok(j) => j,
                    Err(_) => {
                        uncertain = true;
                        continue;
                    }
                };
                if j.phase == "prepared" {
                    for text in [j.old_lock.as_deref(), Some(j.new_lock.as_str())]
                        .into_iter()
                        .flatten()
                    {
                        match toml::from_str::<Lockfile>(text) {
                            Ok(l) => locks.push(l),
                            Err(_) => uncertain = true,
                        }
                    }
                }
            }
        }
        for lock in locks {
            for tool in lock.tools {
                for a in &tool.artifacts {
                    installs.insert(tool.installation_version(&a.target));
                    cache.insert(a.artifact_integrity()?.cache_key());
                    for o in &a.overlays {
                        cache.insert(o.artifact_integrity()?.cache_key());
                    }
                }
            }
        }
        Ok((installs, cache, uncertain))
    }
    pub fn clean(&self, kind: &str, specs: &[String], plan: bool) -> Result<Value> {
        if !matches!(kind, "cache" | "installs") {
            return Err(service_error(
                "PINSET_ARGUMENT_INVALID",
                "unknown cleanup kind",
            ));
        }
        let _guard = if plan {
            None
        } else {
            Some(self.guard("maintenance")?)
        };
        let (protected, cache, uncertain) = self.references()?;
        let mut removals = Vec::new();
        let mut retained = Vec::new();
        if kind == "cache" {
            let dir = self.home.join("cache/downloads");
            if dir.exists() {
                for algorithm in fs::read_dir(&dir)? {
                    let algorithm = algorithm?;
                    if !algorithm.file_type()?.is_dir() {
                        continue;
                    }
                    for entry in fs::read_dir(algorithm.path())? {
                        let entry = entry?;
                        let filename = entry.file_name().to_string_lossy().into_owned();
                        let hash = filename.trim_end_matches(".archive");
                        if entry.file_type()?.is_file()
                            && filename.ends_with(".archive")
                            && !cache.contains(hash)
                            && !uncertain
                        {
                            removals.push(entry.path());
                        } else {
                            retained.push(entry.path());
                        }
                    }
                }
            }
        } else if kind == "installs" {
            let mut filters = BTreeSet::new();
            for spec in specs {
                let (name, version) = spec.split_once('@').ok_or_else(|| {
                    service_error("PINSET_EXACT_REQUIRED", "clean installs accepts tool@exact")
                })?;
                validate_tool(name)?;
                if version.is_empty()
                    || ["latest", "lts", "stable", "current", "nightly"].contains(&version)
                {
                    return Err(service_error(
                        "PINSET_EXACT_REQUIRED",
                        "clean requires an exact version",
                    ));
                }
                filters.insert((name.to_string(), version.to_string()));
            }
            let dir = self.home.join("installs");
            if dir.exists() {
                for tool in fs::read_dir(dir)? {
                    let tool = tool?;
                    if !tool.file_type()?.is_dir() {
                        retained.push(tool.path());
                        continue;
                    }
                    for identity in fs::read_dir(tool.path())? {
                        let identity = identity?;
                        if !identity.file_type()?.is_dir() {
                            retained.push(identity.path());
                            continue;
                        }
                        for platform in fs::read_dir(identity.path())? {
                            let platform = platform?;
                            let receipt =
                                fs::read_to_string(platform.path().join(".pinset-install.toml"))
                                    .ok()
                                    .and_then(|s| toml::from_str::<InstallReceipt>(&s).ok());
                            if let Some(r) = receipt {
                                let matches = filters.is_empty()
                                    || filters.contains(&(r.tool.clone(), r.version.clone()));
                                if r.schema == 3
                                    && r.complete
                                    && platform.file_type()?.is_dir()
                                    && matches
                                    && !protected.contains(&r.install_identity)
                                    && !uncertain
                                {
                                    removals.push(platform.path());
                                } else {
                                    retained.push(platform.path());
                                }
                            } else {
                                retained.push(platform.path());
                            }
                        }
                    }
                }
            }
        }
        if !plan {
            for path in &removals {
                let canonical = path.canonicalize()?;
                if !canonical.starts_with(self.home.canonicalize()?)
                    || fs::symlink_metadata(path)?.file_type().is_symlink()
                {
                    return Err(service_error(
                        "PINSET_CLEAN_UNSAFE",
                        "cleanup object escaped Pinset v3 home",
                    ));
                }
                if canonical.is_dir() {
                    fs::remove_dir_all(&canonical)?;
                } else {
                    fs::remove_file(&canonical)?;
                }
            }
        }
        Ok(
            json!({"protocol":PROTOCOL,"plan":plan,"removed":removals,"retained":retained,"uncertain_references":uncertain}),
        )
    }
    pub fn self_info(&self) -> Value {
        json!({"protocol":PROTOCOL,"version":pinset_version(),"platform":current_target(),"home":self.home,"cli":std::env::current_exe().ok(),"providers":TOOLS})
    }
    pub fn shell(&self, shell: &str) -> String {
        let bin = self.home.join("bin").display().to_string();
        match shell {
            "powershell" => format!(
                "$env:PATH = '{}' + [IO.Path]::PathSeparator + $env:PATH\n",
                bin.replace('\'', "''")
            ),
            "fish" => format!("fish_add_path --prepend '{}'\n", bin.replace('\'', "\\'")),
            _ => format!("export PATH='{}':\"$PATH\"\n", bin.replace('\'', "'\\''")),
        }
    }
    pub fn self_update(&self, version: Option<&str>, plan: bool) -> Result<Value> {
        let _guard = if plan {
            None
        } else {
            Some(self.guard("maintenance")?)
        };
        let client = http_client_builder()?
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        let release = crate::self_update::resolve_release(&client, version, &current_target())?;
        let version = release.version;
        let filename = release.archive.name;
        let url = release.archive.browser_download_url;
        let sums = client
            .get(release.checksums.browser_download_url)
            .send()
            .map_err(|e| service_error("PINSET_UPDATE_FETCH", e.to_string()))?
            .error_for_status()
            .map_err(|e| service_error("PINSET_UPDATE_FETCH", e.to_string()))?
            .text()
            .map_err(|e| service_error("PINSET_UPDATE_FETCH", e.to_string()))?;
        let hash = crate::self_update::release_checksum(&sums, &filename)?;
        if plan {
            return Ok(
                json!({"protocol":PROTOCOL,"plan":true,"version":version,"artifact":filename,"checksum":hash}),
            );
        }
        let updatehome = self.home.join("state/self-update");
        let installer = Installer::new(InstallLimits::default())?
            .with_install_identity(format!("{version}--{}", &hash[..24]));
        let result = installer.install(&InstallRequest {
            pinset_home: updatehome,
            tool: "pinset".into(),
            version: version.clone(),
            target: current_target(),
            artifact: ArtifactSpec {
                canonical_url: url.clone(),
                sources: vec![ArtifactSource {
                    id: "github-official".into(),
                    url,
                    kind: ArtifactSourceKind::Official,
                }],
                integrity: hash,
                format: ArtifactFormat::Zip,
            },
            strip_components: 0,
            include_prefixes: vec![],
            required_paths: vec![
                PathBuf::from(executable_name("pinset", &current_target())),
                PathBuf::from(executable_name("pinset-shim", &current_target())),
            ],
            base_artifacts: vec![],
            executable_paths: vec![
                PathBuf::from(executable_name("pinset", &current_target())),
                PathBuf::from(executable_name("pinset-shim", &current_target())),
            ],
            aliases: vec![],
        })?;
        let current = std::env::current_exe()?;
        let destination = current.parent().unwrap();
        self.validate_new_cli(
            &result
                .install_dir
                .join(executable_name("pinset", &current_target())),
            &version,
        )?;
        let names = vec![
            executable_name("pinset", &current_target()),
            executable_name("pinset-shim", &current_target()),
        ];
        let backup = crate::self_update::replace_binary_pair(
            &result.install_dir,
            destination,
            &self.home,
            &names,
        )?;
        drop(_guard);
        let repaired = std::process::Command::new(destination.join(&names[0]))
            .args(["self", "repair"])
            .env_remove("PINSET_IDENTITY")
            .output()?;
        if !repaired.status.success() {
            return Err(service_error(
                "PINSET_UPDATE_REPAIR",
                "paired binaries were installed; run self repair to finish shim recovery",
            ));
        }
        Ok(json!({"protocol":PROTOCOL,"updated":version,"backup":backup,"restart_required":true}))
    }
}
