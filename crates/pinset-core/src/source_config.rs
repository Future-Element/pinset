use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Error, Result};

pub const SUPPORTED_SOURCE_PROVIDERS: [&str; 3] = ["node", "python", "flutter"];
const SOURCE_CONFIG_SCHEMA: u32 = 1;
const OFFICIAL_ALIAS: &str = "official";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Official,
    Custom,
}

impl SourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceView {
    pub provider: String,
    pub alias: String,
    pub kind: SourceKind,
    pub base_url: String,
    pub active: bool,
    pub fallback_position: Option<usize>,
    pub allow_insecure: bool,
    pub trust_metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtifactSource {
    pub alias: String,
    pub kind: SourceKind,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    schema: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    providers: BTreeMap<String, ProviderSources>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderSources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fallback: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sources: BTreeMap<String, CustomSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomSource {
    base_url: String,
    #[serde(default, skip_serializing_if = "is_false")]
    allow_insecure: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    trust_metadata: bool,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            schema: SOURCE_CONFIG_SCHEMA,
            providers: BTreeMap::new(),
        }
    }
}

impl SourceConfig {
    pub fn list(&self, provider: &str) -> Result<Vec<SourceView>> {
        validate_provider(provider)?;
        let configured = self.providers.get(provider);
        let active = configured
            .and_then(|sources| sources.active.as_deref())
            .unwrap_or(OFFICIAL_ALIAS);
        let fallback = configured
            .map(|sources| sources.fallback.as_slice())
            .unwrap_or_default();
        let mut result = vec![SourceView {
            provider: provider.to_owned(),
            alias: OFFICIAL_ALIAS.to_owned(),
            kind: SourceKind::Official,
            base_url: official_base_url(provider).to_owned(),
            active: active == OFFICIAL_ALIAS,
            fallback_position: fallback
                .iter()
                .position(|alias| alias == OFFICIAL_ALIAS)
                .map(|index| index + 1),
            allow_insecure: false,
            trust_metadata: false,
        }];

        if let Some(configured) = configured {
            result.extend(configured.sources.iter().map(|(alias, source)| {
                SourceView {
                    provider: provider.to_owned(),
                    alias: alias.clone(),
                    kind: SourceKind::Custom,
                    base_url: source.base_url.clone(),
                    active: active == alias,
                    fallback_position: fallback
                        .iter()
                        .position(|fallback_alias| fallback_alias == alias)
                        .map(|index| index + 1),
                    allow_insecure: source.allow_insecure,
                    trust_metadata: source.trust_metadata,
                }
            }));
        }

        Ok(result)
    }

    pub fn source(&self, provider: &str, alias: Option<&str>) -> Result<SourceView> {
        let sources = self.list(provider)?;
        if let Some(alias) = alias {
            return sources
                .into_iter()
                .find(|source| source.alias == alias)
                .ok_or_else(|| Error::SourceNotFound {
                    provider: provider.to_owned(),
                    alias: alias.to_owned(),
                });
        }
        Ok(sources
            .into_iter()
            .find(|source| source.active)
            .expect("every provider has an active source"))
    }

    pub fn resolve_artifact_sources(
        &self,
        provider: &str,
        artifact_path: &str,
    ) -> Result<Vec<ResolvedArtifactSource>> {
        validate_provider(provider)?;
        validate_artifact_path(artifact_path)?;
        let configured = self.providers.get(provider);
        let active = configured
            .and_then(|sources| sources.active.as_deref())
            .unwrap_or(OFFICIAL_ALIAS);
        let fallback = configured
            .map(|sources| sources.fallback.as_slice())
            .unwrap_or_default();

        std::iter::once(active)
            .chain(fallback.iter().map(String::as_str))
            .map(|alias| {
                let (kind, base_url) = self.source_definition(provider, alias);
                let url = join_artifact_url(alias, base_url, artifact_path)?;
                Ok(ResolvedArtifactSource {
                    alias: alias.to_owned(),
                    kind,
                    url,
                })
            })
            .collect()
    }

    pub fn official_artifact_url(&self, provider: &str, artifact_path: &str) -> Result<String> {
        validate_provider(provider)?;
        validate_artifact_path(artifact_path)?;
        join_artifact_url(OFFICIAL_ALIAS, official_base_url(provider), artifact_path)
    }

    pub fn add(
        &mut self,
        provider: &str,
        alias: &str,
        base_url: &str,
        allow_insecure: bool,
        trust_metadata: bool,
    ) -> Result<()> {
        validate_provider(provider)?;
        validate_custom_alias(alias)?;
        let normalized_url = normalize_base_url(base_url, allow_insecure)?;
        if trust_metadata && normalized_url.starts_with("http://") {
            return Err(Error::InvalidSourceBaseUrl {
                url: base_url.to_owned(),
                reason: "trusted metadata sources must use HTTPS".to_owned(),
            });
        }
        let configured = self.providers.entry(provider.to_owned()).or_default();
        if configured.sources.contains_key(alias) {
            return Err(Error::SourceAlreadyExists {
                provider: provider.to_owned(),
                alias: alias.to_owned(),
            });
        }
        let uses_insecure_http = normalized_url.starts_with("http://");
        configured.sources.insert(
            alias.to_owned(),
            CustomSource {
                base_url: normalized_url,
                allow_insecure: uses_insecure_http,
                trust_metadata,
            },
        );
        Ok(())
    }

    pub fn metadata_source(&self, provider: &str) -> Result<SourceView> {
        let active = self.source(provider, None)?;
        if active.kind == SourceKind::Custom && active.trust_metadata {
            return Ok(active);
        }
        self.source(provider, Some(OFFICIAL_ALIAS))
    }

    pub fn use_source(&mut self, provider: &str, alias: &str) -> Result<()> {
        validate_provider(provider)?;
        self.require_source(provider, alias)?;
        let configured = self.providers.entry(provider.to_owned()).or_default();
        configured.active = (alias != OFFICIAL_ALIAS).then(|| alias.to_owned());
        configured.fallback.retain(|item| item != alias);
        self.remove_empty_provider(provider);
        Ok(())
    }

    pub fn set_fallback(&mut self, provider: &str, aliases: &[String]) -> Result<()> {
        validate_provider(provider)?;
        let active = self.active_alias(provider);
        let mut seen = HashSet::with_capacity(aliases.len());
        for alias in aliases {
            self.require_source(provider, alias)?;
            if alias == active {
                return Err(Error::ActiveSourceInFallback {
                    provider: provider.to_owned(),
                    alias: alias.clone(),
                });
            }
            if !seen.insert(alias) {
                return Err(Error::DuplicateSourceFallback {
                    provider: provider.to_owned(),
                    alias: alias.clone(),
                });
            }
        }
        let configured = self.providers.entry(provider.to_owned()).or_default();
        configured.fallback = aliases.to_vec();
        self.remove_empty_provider(provider);
        Ok(())
    }

    pub fn remove(&mut self, provider: &str, alias: &str) -> Result<()> {
        validate_provider(provider)?;
        if alias == OFFICIAL_ALIAS {
            return Err(Error::BuiltinSourceMutation);
        }
        validate_custom_alias(alias)?;
        self.require_source(provider, alias)?;
        let configured = self
            .providers
            .get_mut(provider)
            .expect("custom source requires a provider entry");
        if configured.active.as_deref() == Some(alias) {
            return Err(Error::SourceInUse {
                provider: provider.to_owned(),
                alias: alias.to_owned(),
                usage: "active",
            });
        }
        if configured.fallback.iter().any(|item| item == alias) {
            return Err(Error::SourceInUse {
                provider: provider.to_owned(),
                alias: alias.to_owned(),
                usage: "in the fallback list",
            });
        }
        configured.sources.remove(alias);
        self.remove_empty_provider(provider);
        Ok(())
    }

    fn active_alias(&self, provider: &str) -> &str {
        self.providers
            .get(provider)
            .and_then(|sources| sources.active.as_deref())
            .unwrap_or(OFFICIAL_ALIAS)
    }

    fn require_source(&self, provider: &str, alias: &str) -> Result<()> {
        if alias == OFFICIAL_ALIAS
            || self
                .providers
                .get(provider)
                .is_some_and(|sources| sources.sources.contains_key(alias))
        {
            return Ok(());
        }
        Err(Error::SourceNotFound {
            provider: provider.to_owned(),
            alias: alias.to_owned(),
        })
    }

    fn source_definition(&self, provider: &str, alias: &str) -> (SourceKind, &str) {
        if alias == OFFICIAL_ALIAS {
            return (SourceKind::Official, official_base_url(provider));
        }
        let source = self
            .providers
            .get(provider)
            .and_then(|sources| sources.sources.get(alias))
            .expect("validated source configuration only references existing aliases");
        (SourceKind::Custom, &source.base_url)
    }

    fn remove_empty_provider(&mut self, provider: &str) {
        let should_remove = self.providers.get(provider).is_some_and(|sources| {
            sources.active.is_none() && sources.fallback.is_empty() && sources.sources.is_empty()
        });
        if should_remove {
            self.providers.remove(provider);
        }
    }
}

pub fn source_config_path(pinset_home: &Path) -> PathBuf {
    pinset_home.join("sources.toml")
}

pub fn load_source_config(path: &Path) -> Result<SourceConfig> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(SourceConfig::default()),
        Err(source) => {
            return Err(Error::ReadSourceConfig {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut config: SourceConfig =
        toml::from_str(&content).map_err(|source| Error::ParseSourceConfig {
            path: path.to_path_buf(),
            source,
        })?;
    if config.schema != SOURCE_CONFIG_SCHEMA {
        return Err(Error::UnsupportedSourceSchema {
            actual: config.schema,
        });
    }
    validate_loaded_config(&mut config)?;
    Ok(config)
}

pub fn save_source_config(path: &Path, config: &SourceConfig) -> Result<()> {
    let serialized =
        toml::to_string_pretty(config).map_err(|source| Error::SerializeSourceConfig { source })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::CreateSourceConfigDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut file =
        AtomicWriteFile::options()
            .open(path)
            .map_err(|source| Error::WriteSourceConfig {
                path: path.to_path_buf(),
                source,
            })?;
    file.write_all(serialized.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::WriteSourceConfig {
            path: path.to_path_buf(),
            source,
        })
}

fn validate_loaded_config(config: &mut SourceConfig) -> Result<()> {
    for (provider, sources) in &mut config.providers {
        validate_provider(provider)?;
        if sources.active.as_deref() == Some(OFFICIAL_ALIAS) {
            sources.active = None;
        } else if let Some(active) = &sources.active {
            validate_custom_alias(active)?;
        }
        for (alias, source) in &mut sources.sources {
            validate_custom_alias(alias)?;
            source.base_url = normalize_base_url(&source.base_url, source.allow_insecure)?;
            if source.trust_metadata && source.base_url.starts_with("http://") {
                return Err(Error::InvalidSourceBaseUrl {
                    url: source.base_url.clone(),
                    reason: "trusted metadata sources must use HTTPS".to_owned(),
                });
            }
        }
        let active = sources.active.as_deref().unwrap_or(OFFICIAL_ALIAS);
        if active != OFFICIAL_ALIAS && !sources.sources.contains_key(active) {
            return Err(Error::SourceNotFound {
                provider: provider.clone(),
                alias: active.to_owned(),
            });
        }
        let mut seen = HashSet::with_capacity(sources.fallback.len());
        for alias in &sources.fallback {
            if alias == active {
                return Err(Error::ActiveSourceInFallback {
                    provider: provider.clone(),
                    alias: alias.clone(),
                });
            }
            if alias != OFFICIAL_ALIAS && !sources.sources.contains_key(alias) {
                return Err(Error::SourceNotFound {
                    provider: provider.clone(),
                    alias: alias.clone(),
                });
            }
            if !seen.insert(alias) {
                return Err(Error::DuplicateSourceFallback {
                    provider: provider.clone(),
                    alias: alias.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_provider(provider: &str) -> Result<()> {
    if SUPPORTED_SOURCE_PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(Error::UnsupportedSourceProvider {
            provider: provider.to_owned(),
        })
    }
}

fn validate_custom_alias(alias: &str) -> Result<()> {
    if alias == OFFICIAL_ALIAS {
        return Err(Error::BuiltinSourceMutation);
    }
    if alias.is_empty()
        || alias == "."
        || alias == ".."
        || !alias.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(Error::InvalidSourceAlias {
            alias: alias.to_owned(),
        });
    }
    Ok(())
}

fn normalize_base_url(value: &str, allow_insecure: bool) -> Result<String> {
    let mut url = Url::parse(value).map_err(|error| Error::InvalidSourceBaseUrl {
        url: value.to_owned(),
        reason: error.to_string(),
    })?;
    let scheme_allowed = url.scheme() == "https" || (allow_insecure && url.scheme() == "http");
    if !scheme_allowed {
        return Err(Error::InvalidSourceBaseUrl {
            url: value.to_owned(),
            reason: "HTTPS is required unless --allow-insecure is explicitly set for HTTP"
                .to_owned(),
        });
    }
    if url.host_str().is_none() || url.cannot_be_a_base() {
        return Err(Error::InvalidSourceBaseUrl {
            url: value.to_owned(),
            reason: "an absolute hierarchical URL with a host is required".to_owned(),
        });
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::InvalidSourceBaseUrl {
            url: value.to_owned(),
            reason: "embedded credentials are not allowed".to_owned(),
        });
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::InvalidSourceBaseUrl {
            url: value.to_owned(),
            reason: "query strings and fragments are not allowed".to_owned(),
        });
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url.to_string())
}

fn validate_artifact_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+@".contains(&byte))
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::InvalidSourceArtifactPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn join_artifact_url(alias: &str, base_url: &str, artifact_path: &str) -> Result<String> {
    let base = Url::parse(base_url).expect("stored and built-in source URLs are validated");
    base.join(artifact_path)
        .map(|url| url.to_string())
        .map_err(|source| Error::JoinSourceArtifactUrl {
            alias: alias.to_owned(),
            path: artifact_path.to_owned(),
            source,
        })
}

fn official_base_url(provider: &str) -> &'static str {
    match provider {
        "node" => "https://nodejs.org/dist/",
        "python" => "https://github.com/astral-sh/python-build-standalone/releases/download/",
        "flutter" => "https://storage.googleapis.com/",
        _ => unreachable!(),
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_uses_official_defaults_without_writing() {
        let root = tempdir().expect("temp root");
        let path = root.path().join("sources.toml");

        let config = load_source_config(&path).expect("default config");
        let sources = config.list("node").expect("node sources");

        assert!(!path.exists());
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].alias, "official");
        assert!(sources[0].active);
    }

    #[test]
    fn add_use_fallback_save_and_reload_round_trip() {
        let root = tempdir().expect("temp root");
        let path = root.path().join("config").join("sources.toml");
        let mut config = SourceConfig::default();
        config
            .add(
                "node",
                "mirror-a",
                "https://mirror.example/node",
                false,
                true,
            )
            .expect("add source");
        config
            .add(
                "node",
                "mirror-b",
                "https://backup.example/node/",
                false,
                false,
            )
            .expect("add backup");
        config.use_source("node", "mirror-a").expect("use source");
        config
            .set_fallback("node", &["mirror-b".to_owned(), "official".to_owned()])
            .expect("fallback");
        save_source_config(&path, &config).expect("save");

        let loaded = load_source_config(&path).expect("reload");
        let sources = loaded.list("node").expect("list");
        assert_eq!(sources.len(), 3);
        assert!(sources.iter().any(|source| {
            source.alias == "mirror-a"
                && source.active
                && source.base_url == "https://mirror.example/node/"
        }));
        assert!(
            sources.iter().any(|source| {
                source.alias == "mirror-b" && source.fallback_position == Some(1)
            })
        );
        assert!(
            sources.iter().any(|source| {
                source.alias == "official" && source.fallback_position == Some(2)
            })
        );
    }

    #[test]
    fn resolves_only_active_and_fallback_sources_in_declared_order() {
        let mut config = SourceConfig::default();
        config
            .add(
                "node",
                "primary",
                "https://primary.example/node/",
                false,
                false,
            )
            .expect("primary");
        config
            .add(
                "node",
                "unused",
                "https://unused.example/node/",
                false,
                false,
            )
            .expect("unused");
        config.use_source("node", "primary").expect("active");
        config
            .set_fallback("node", &["official".to_owned()])
            .expect("fallback");

        let sources = config
            .resolve_artifact_sources("node", "v24.0.0/node-v24.0.0-win-x64.zip")
            .expect("resolve");
        assert_eq!(
            sources,
            vec![
                ResolvedArtifactSource {
                    alias: "primary".to_owned(),
                    kind: SourceKind::Custom,
                    url: "https://primary.example/node/v24.0.0/node-v24.0.0-win-x64.zip".to_owned(),
                },
                ResolvedArtifactSource {
                    alias: "official".to_owned(),
                    kind: SourceKind::Official,
                    url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-x64.zip".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn rejects_artifact_paths_that_can_escape_or_rewrite_the_base_url() {
        let config = SourceConfig::default();
        for path in [
            "",
            "/v24.0.0/node.zip",
            "../node.zip",
            "v24.0.0/../node.zip",
            r"v24.0.0\node.zip",
            "v24.0.0/node.zip?token=x",
            "v24.0.0/node.zip#fragment",
            "%2e%2e/node.zip",
            "data:node.zip",
            "v24.0.0/node zip",
        ] {
            assert!(
                matches!(
                    config.resolve_artifact_sources("node", path),
                    Err(Error::InvalidSourceArtifactPath { .. })
                ),
                "path should be rejected: {path}"
            );
        }
    }

    #[test]
    fn repeated_atomic_save_replaces_config_without_leaking_temporary_files() {
        let root = tempdir().expect("temp root");
        let path = root.path().join("sources.toml");
        let mut config = SourceConfig::default();
        save_source_config(&path, &config).expect("initial save");

        config
            .add(
                "flutter",
                "mirror",
                "https://mirror.example/flutter/",
                false,
                false,
            )
            .expect("add source");
        save_source_config(&path, &config).expect("replacement save");

        let loaded = load_source_config(&path).expect("reload");
        assert_eq!(loaded.list("flutter").expect("list").len(), 2);
        let entries = fs::read_dir(root.path())
            .expect("config directory")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("sources.toml")]);
    }

    #[test]
    fn official_source_cannot_be_added_or_removed() {
        let mut config = SourceConfig::default();
        assert!(matches!(
            config.add("node", "official", "https://example.test/", false, false,),
            Err(Error::BuiltinSourceMutation)
        ));
        assert!(matches!(
            config.remove("node", "official"),
            Err(Error::BuiltinSourceMutation)
        ));
    }

    #[test]
    fn rejects_insecure_or_credentialed_urls_by_default() {
        let mut config = SourceConfig::default();
        assert!(matches!(
            config.add("node", "http", "http://mirror.example/", false, false,),
            Err(Error::InvalidSourceBaseUrl { .. })
        ));
        assert!(matches!(
            config.add(
                "node",
                "secret",
                "https://user:password@mirror.example/",
                false,
                false,
            ),
            Err(Error::InvalidSourceBaseUrl { .. })
        ));
        assert!(matches!(
            config.add(
                "node",
                "query",
                "https://mirror.example/?token=x",
                false,
                false,
            ),
            Err(Error::InvalidSourceBaseUrl { .. })
        ));
    }

    #[test]
    fn insecure_http_requires_explicit_opt_in() {
        let mut config = SourceConfig::default();
        config
            .add("node", "lan", "http://127.0.0.1:8080/node", true, false)
            .expect("explicit insecure source");

        let source = config
            .list("node")
            .expect("list")
            .into_iter()
            .find(|source| source.alias == "lan")
            .expect("lan source");
        assert_eq!(source.base_url, "http://127.0.0.1:8080/node/");
        assert!(source.allow_insecure);
    }

    #[test]
    fn fallback_must_exist_be_unique_and_exclude_active() {
        let mut config = SourceConfig::default();
        config
            .add("node", "backup", "https://backup.example/", false, false)
            .expect("add backup");
        assert!(matches!(
            config.set_fallback("node", &["missing".to_owned()]),
            Err(Error::SourceNotFound { .. })
        ));
        assert!(matches!(
            config.set_fallback("node", &["official".to_owned()]),
            Err(Error::ActiveSourceInFallback { .. })
        ));
        assert!(matches!(
            config.set_fallback("node", &["backup".to_owned(), "backup".to_owned()]),
            Err(Error::DuplicateSourceFallback { .. })
        ));
    }

    #[test]
    fn source_in_use_cannot_be_removed() {
        let mut config = SourceConfig::default();
        config
            .add("node", "mirror", "https://mirror.example/", false, false)
            .expect("add");
        config.use_source("node", "mirror").expect("use");
        assert!(matches!(
            config.remove("node", "mirror"),
            Err(Error::SourceInUse {
                usage: "active",
                ..
            })
        ));
        config.use_source("node", "official").expect("use official");
        config
            .set_fallback("node", &["mirror".to_owned()])
            .expect("fallback");
        assert!(matches!(
            config.remove("node", "mirror"),
            Err(Error::SourceInUse {
                usage: "in the fallback list",
                ..
            })
        ));
    }

    #[test]
    fn trusted_https_metadata_is_explicit_and_insecure_metadata_is_rejected() {
        let mut config = SourceConfig::default();
        config
            .add(
                "node",
                "trusted",
                "https://mirror.example/node/",
                false,
                true,
            )
            .expect("trusted source");
        config.use_source("node", "trusted").expect("active");
        let metadata = config.metadata_source("node").expect("metadata source");
        assert_eq!(metadata.alias, "trusted");
        assert!(metadata.trust_metadata);

        let mut insecure = SourceConfig::default();
        assert!(matches!(
            insecure.add("node", "lan", "http://127.0.0.1:8080/node/", true, true,),
            Err(Error::InvalidSourceBaseUrl { .. })
        ));
    }

    #[test]
    fn rejects_unknown_schema_fields_and_dangling_source_references() {
        let root = tempdir().expect("temp root");
        let path = root.path().join("sources.toml");

        fs::write(&path, "schema = 2\n").expect("schema");
        assert!(matches!(
            load_source_config(&path),
            Err(Error::UnsupportedSourceSchema { actual: 2 })
        ));

        fs::write(&path, "schema = 1\nunknown = true\n").expect("unknown");
        assert!(matches!(
            load_source_config(&path),
            Err(Error::ParseSourceConfig { .. })
        ));

        fs::write(
            &path,
            "schema = 1\n[providers.node]\nactive = \"missing\"\n",
        )
        .expect("dangling");
        assert!(matches!(
            load_source_config(&path),
            Err(Error::SourceNotFound { .. })
        ));
    }
}
