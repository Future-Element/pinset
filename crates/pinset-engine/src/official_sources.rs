use crate::{Error, Result};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Official,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtifactSource {
    pub alias: String,
    pub kind: SourceKind,
    pub url: String,
}
#[derive(Debug, Default)]
pub struct OfficialSources;
impl OfficialSources {
    pub fn official_artifact_url(&self, provider: &str, path: &str) -> Result<String> {
        let base = match provider {
            "node" => "https://nodejs.org/dist/",
            "go" => "https://go.dev/dl/",
            "python" => "https://www.python.org/ftp/python/",
            "flutter" => "https://storage.googleapis.com/",
            _ => {
                return Err(Error::Service {
                    code: "PINSET_TOOL_UNKNOWN",
                    message: provider.into(),
                });
            }
        };
        if path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|p| p == ".." || p == "." || p.is_empty())
        {
            return Err(Error::InvalidLockfile {
                reason: "unsafe artifact path".into(),
            });
        }
        Ok(format!("{base}{path}"))
    }
    pub fn resolve_artifact_sources(
        &self,
        provider: &str,
        path: &str,
    ) -> Result<Vec<ResolvedArtifactSource>> {
        Ok(vec![ResolvedArtifactSource {
            alias: "official".into(),
            kind: SourceKind::Official,
            url: self.official_artifact_url(provider, path)?,
        }])
    }
}
