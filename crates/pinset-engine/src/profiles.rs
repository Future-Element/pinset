use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

fn relative(profile: &str) -> Result<String> {
    validate_id(profile)?;
    Ok(format!(".pinset/env/{profile}.env"))
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AccessRequest {
    pub protocol: String,
    pub id: String,
    pub recipient: String,
    pub created_at: u64,
}
impl Services {
    fn profile_edit(&self) -> Result<(ProjectContext, StateGuard, ProjectConfig, Lockfile)> {
        let c = self.project()?;
        let guard = self.guard(&c.load()?.project_id)?;
        self.require_current_trust(&c)?;
        let (config, lock) = self.load(&c)?;
        Ok((c, guard, config, lock))
    }
    pub fn environment_fingerprint(&self, c: &ProjectContext) -> Result<String> {
        let config = c.load()?;
        let mut bytes = serde_json::to_vec(&config)?;
        for name in config.environment.profiles.keys() {
            let path = c.root.join(relative(name)?);
            bytes.extend(fs::read(path)?);
        }
        Ok(digest(&bytes))
    }
    fn require_current_trust(&self, c: &ProjectContext) -> Result<()> {
        let config = c.load()?;
        if !config.environment.profiles.is_empty() {
            pinset_env::verify_project_trust(
                &self.home,
                &c.root,
                &config.project_id,
                &self.environment_fingerprint(c)?,
            )?;
        }
        Ok(())
    }
    fn refresh_trust(&self, c: &ProjectContext) -> Result<()> {
        let config = c.load()?;
        pinset_env::trust_project(
            &self.home,
            &c.root,
            &config.project_id,
            &self.environment_fingerprint(c)?,
        )?;
        Ok(())
    }
    pub fn trust(&self, action: &str) -> Result<Value> {
        let c = self.project()?;
        let config = c.load()?;
        let fingerprint = self.environment_fingerprint(&c)?;
        match action {
            "add" => {
                self.refresh_trust(&c)?;
                Ok(json!({"protocol":PROTOCOL,"trusted":true,"fingerprint":fingerprint}))
            }
            "revoke" => Ok(
                json!({"protocol":PROTOCOL,"revoked":pinset_env::revoke_project_trust(&self.home,&c.root)?}),
            ),
            "status" => {
                let result = pinset_env::verify_project_trust(
                    &self.home,
                    &c.root,
                    &config.project_id,
                    &fingerprint,
                );
                Ok(
                    json!({"protocol":PROTOCOL,"trusted":result.is_ok(),"detail":result.err().map(|e|e.to_string()),"fingerprint":fingerprint}),
                )
            }
            _ => unreachable!(),
        }
    }
    pub fn profile_init(&self, name: &str) -> Result<Value> {
        let (c, _guard, mut config, lock) = self.profile_edit()?;
        let rel = relative(name)?;
        if config.environment.profiles.contains_key(name) || c.root.join(&rel).exists() {
            return Err(service_error(
                "PINSET_PROFILE_EXISTS",
                "profile already exists",
            ));
        }
        let recipient = pinset_env::public_recipient(&self.home)?;
        config.environment.profiles.insert(
            name.into(),
            ProfileConfig {
                recipients: vec![recipient.clone()],
                grants: BTreeMap::new(),
            },
        );
        let transaction = self.begin_commit(&c, &config, &lock)?;
        pinset_env::write_encrypted_profile(
            &c.root,
            &rel,
            &pinset_env::EnvironmentDocument::default(),
            std::slice::from_ref(&recipient),
        )?;
        self.finish_commit(&c, &transaction)?;
        self.refresh_trust(&c)?;
        Ok(json!({"protocol":PROTOCOL,"profile":name,"path":c.root.join(rel)}))
    }
    pub fn profile_use(&self, name: Option<&str>, project: bool) -> Result<Value> {
        let (c, _guard, mut config, lock) = self.profile_edit()?;
        if let Some(n) = name
            && !config.environment.profiles.contains_key(n)
        {
            return Err(service_error("PINSET_PROFILE_MISSING", n));
        }
        if project {
            config.environment.default = name.map(str::to_string);
            self.commit(&c, &config, &lock)?;
            self.refresh_trust(&c)?;
        } else {
            write_json(
                &c.local.join("profile.json"),
                &json!({"protocol":PROTOCOL,"profile":name}),
            )?;
        }
        Ok(json!({"protocol":PROTOCOL,"profile":name,"scope":if project{"project"}else{"local"}}))
    }
    pub fn profile_remove(&self, name: &str, plan: bool) -> Result<Value> {
        let c = self.project()?;
        self.require_current_trust(&c)?;
        let mut config = c.load()?;
        let rel = relative(name)?;
        if !config.environment.profiles.contains_key(name) {
            return Err(service_error("PINSET_PROFILE_MISSING", name));
        }
        if !plan {
            let _guard = self.guard(&config.project_id)?;
            self.require_current_trust(&c)?;
            let (mut current, lock) = self.load(&c)?;
            std::mem::swap(&mut config, &mut current);
            config.environment.profiles.remove(name);
            if config.environment.default.as_deref() == Some(name) {
                config.environment.default = None;
            }
            let transaction = self.begin_commit(&c, &config, &lock)?;
            fs::remove_file(c.root.join(rel))?;
            let local = c.local.join("profile.json");
            if local.exists() && read_json::<Value>(&local)?["profile"] == name {
                fs::remove_file(local)?;
            }
            self.finish_commit(&c, &transaction)?;
            self.refresh_trust(&c)?;
        }
        Ok(json!({"protocol":PROTOCOL,"profile":name,"removed":!plan,"plan":plan}))
    }
    pub fn profile_list(&self, profile: Option<&str>) -> Result<Value> {
        let c = self.project()?;
        let config = c.load()?;
        if let Some(name) = profile {
            if !config.environment.profiles.contains_key(name) {
                return Err(service_error("PINSET_PROFILE_MISSING", name));
            }
            return Ok(
                json!({"protocol":PROTOCOL,"profile":name,"variables":pinset_env::list_profile_names(&c.root,&relative(name)?)?}),
            );
        }
        Ok(
            json!({"protocol":PROTOCOL,"profiles":config.environment.profiles.keys().collect::<Vec<_>>(),"shared_default":config.environment.default,"local_default":self.local_profile(&c)?}),
        )
    }
    pub fn profile_set(&self, profile: &str, name: &str, mut value: String) -> Result<Value> {
        pinset_env::validate_variable_name(name)?;
        let (c, _guard, config, lock) = self.profile_edit()?;
        let p = config
            .environment
            .profiles
            .get(profile)
            .ok_or_else(|| service_error("PINSET_PROFILE_MISSING", profile))?;
        let transaction = self.begin_commit(&c, &config, &lock)?;
        let values = BTreeMap::from([(name.into(), value.clone())]);
        let result = pinset_env::set_encrypted_profile_values(
            &c.root,
            &relative(profile)?,
            &p.recipients,
            values,
        );
        value.zeroize();
        result?;
        self.finish_commit(&c, &transaction)?;
        self.refresh_trust(&c)?;
        Ok(json!({"protocol":PROTOCOL,"profile":profile,"variable":name,"set":true}))
    }
    pub fn profile_unset(&self, profile: &str, name: &str) -> Result<Value> {
        let (c, _guard, config, lock) = self.profile_edit()?;
        if !config.environment.profiles.contains_key(profile) {
            return Err(service_error("PINSET_PROFILE_MISSING", profile));
        }
        let transaction = self.begin_commit(&c, &config, &lock)?;
        let removed =
            pinset_env::unset_encrypted_profile_value(&c.root, &relative(profile)?, name)?;
        self.finish_commit(&c, &transaction)?;
        self.refresh_trust(&c)?;
        Ok(json!({"protocol":PROTOCOL,"removed":removed}))
    }
    fn local_profile(&self, c: &ProjectContext) -> Result<Option<String>> {
        let file = c.local.join("profile.json");
        if !file.exists() {
            return Ok(None);
        }
        let state: Value = read_json(&file)?;
        if state["protocol"] != PROTOCOL {
            return Err(service_error(
                "PINSET_PROTOCOL_UNSUPPORTED",
                "invalid local profile state",
            ));
        }
        Ok(state["profile"].as_str().map(str::to_string))
    }
    pub fn resolve_profile(
        &self,
        c: &ProjectContext,
        explicit: Option<&str>,
        disabled: bool,
    ) -> Result<BTreeMap<String, String>> {
        if disabled || env::var_os("PINSET_NO_ENV").is_some() {
            return Ok(BTreeMap::new());
        }
        let config = c.load()?;
        let name = explicit
            .map(str::to_string)
            .or_else(|| env::var("PINSET_PROFILE").ok())
            .or(self.local_profile(c)?)
            .or(config.environment.default.clone());
        let Some(name) = name else {
            return Ok(BTreeMap::new());
        };
        if c.global {
            return Err(service_error(
                "PINSET_PROFILE_PROJECT_REQUIRED",
                "encrypted profiles require a project",
            ));
        }
        if !config.environment.profiles.contains_key(&name) {
            return Err(service_error("PINSET_PROFILE_MISSING", name));
        }
        pinset_env::verify_project_trust(
            &self.home,
            &c.root,
            &config.project_id,
            &self.environment_fingerprint(c)?,
        )?;
        let marker = format!("{}:{name}", self.environment_fingerprint(c)?);
        if env::var("PINSET_ENV_RESOLVED").as_deref() == Ok(marker.as_str()) {
            return Ok(BTreeMap::new());
        }
        let identities = pinset_env::load_identity_secrets(&self.home)?;
        let document = pinset_env::read_encrypted_profile(&c.root, &relative(&name)?, &identities)?;
        for name in document.variables.keys() {
            pinset_env::validate_variable_name(name)?;
            if env::vars_os().any(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case(name)) {
                return Err(service_error(
                    "PINSET_ENV_COLLISION",
                    format!("{name} is already set in the process"),
                ));
            }
        }
        let mut values = document.variables;
        values.insert("PINSET_ENV_RESOLVED".into(), marker);
        values.insert("PINSET_PROFILE".into(), name);
        Ok(values)
    }
    pub fn access_request(&self, new: bool) -> Result<Value> {
        self.ensure_home()?;
        let material = pinset_env::request_identity(&self.home, new)?;
        let request = AccessRequest {
            protocol: PROTOCOL.into(),
            id: material.id.clone(),
            recipient: material.recipient,
            created_at: now(),
        };
        let path = self
            .home
            .join("state/access")
            .join(format!("{}.json", request.id));
        write_json(&path, &request)?;
        Ok(json!({"protocol":PROTOCOL,"request":request,"file":path}))
    }
    pub fn save_ci_request(&self, material: &pinset_env::IdentityMaterial) -> Result<PathBuf> {
        let request = AccessRequest {
            protocol: PROTOCOL.into(),
            id: material.record.id.clone(),
            recipient: material.record.recipient.clone(),
            created_at: now(),
        };
        let path = self
            .home
            .join("state/access")
            .join(format!("{}.json", request.id));
        write_json(&path, &request)?;
        Ok(path)
    }
    pub fn ci_access_request(&self) -> Result<(PathBuf, zeroize::Zeroizing<String>)> {
        self.ensure_home()?;
        use secrecy::ExposeSecret;
        let material = pinset_env::generate_identity();
        let path = self.save_ci_request(&material)?;
        Ok((
            path,
            zeroize::Zeroizing::new(material.secret().expose_secret().to_string()),
        ))
    }
    pub fn access_grant(&self, path: &Path, profile: &str) -> Result<Value> {
        let request: AccessRequest = read_json(path)?;
        if request.protocol != PROTOCOL {
            return Err(service_error(
                "PINSET_PROTOCOL_UNSUPPORTED",
                "request is not pinset/3",
            ));
        }
        validate_id(&request.id)?;
        pinset_env::validate_recipient(&request.recipient)?;
        let (c, _guard, mut config, lock) = self.profile_edit()?;
        let p = config
            .environment
            .profiles
            .get_mut(profile)
            .ok_or_else(|| service_error("PINSET_PROFILE_MISSING", profile))?;
        let identities = pinset_env::load_identity_secrets(&self.home)?;
        let document =
            pinset_env::read_encrypted_profile(&c.root, &relative(profile)?, &identities)?;
        p.grants
            .insert(request.id.clone(), request.recipient.clone());
        p.recipients.push(request.recipient);
        p.recipients.sort();
        p.recipients.dedup();
        let recipients = p.recipients.clone();
        let transaction = self.begin_commit(&c, &config, &lock)?;
        pinset_env::write_encrypted_profile(&c.root, &relative(profile)?, &document, &recipients)?;
        self.finish_commit(&c, &transaction)?;
        self.refresh_trust(&c)?;
        Ok(json!({"protocol":PROTOCOL,"granted":request.id}))
    }
    pub fn access_revoke(&self, id: &str, profile: &str) -> Result<Value> {
        let (c, _guard, mut config, lock) = self.profile_edit()?;
        let p = config
            .environment
            .profiles
            .get_mut(profile)
            .ok_or_else(|| service_error("PINSET_PROFILE_MISSING", profile))?;
        let recipient = p
            .grants
            .remove(id)
            .ok_or_else(|| service_error("PINSET_ACCESS_MISSING", id))?;
        p.recipients
            .retain(|r| r != &recipient || p.grants.values().any(|g| g == r));
        let identities = pinset_env::load_identity_secrets(&self.home)?;
        let document =
            pinset_env::read_encrypted_profile(&c.root, &relative(profile)?, &identities)?;
        let recipients = p.recipients.clone();
        let transaction = self.begin_commit(&c, &config, &lock)?;
        pinset_env::write_encrypted_profile(&c.root, &relative(profile)?, &document, &recipients)?;
        self.finish_commit(&c, &transaction)?;
        self.refresh_trust(&c)?;
        Ok(json!({"protocol":PROTOCOL,"revoked":id}))
    }
    pub fn access_list(&self, profile: &str) -> Result<Value> {
        let config = self.project()?.load()?;
        let p = config
            .environment
            .profiles
            .get(profile)
            .ok_or_else(|| service_error("PINSET_PROFILE_MISSING", profile))?;
        Ok(json!({"protocol":PROTOCOL,"grants":p.grants}))
    }
}
