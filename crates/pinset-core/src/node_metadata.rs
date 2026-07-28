use std::{collections::HashMap, io::Read, time::Duration};

use reqwest::{Url, blocking::Client};

use crate::{
    Error, LockedArtifact, LockedArtifactFormat, Lockfile, MVP_NODE_TARGETS, NodeArchiveFormat,
    Result, SourceConfig, plan_node_artifact,
};

const OFFICIAL_NODE_DIST_URL: &str = "https://nodejs.org/dist/";
const MAX_SHASUMS_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct NodeMetadataClient {
    client: Client,
    metadata_base_url: Url,
}

impl NodeMetadataClient {
    pub fn official() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|source| Error::HttpClient { source })?;
        Ok(Self {
            client,
            metadata_base_url: Url::parse(OFFICIAL_NODE_DIST_URL)
                .expect("built-in Node distribution URL is valid"),
        })
    }

    pub fn resolve_exact_lock(&self, version: &str, generated_by: &str) -> Result<Lockfile> {
        let plans = MVP_NODE_TARGETS
            .into_iter()
            .map(|target| plan_node_artifact(&SourceConfig::default(), version, target))
            .collect::<Result<Vec<_>>>()?;
        let manifest_url = self
            .metadata_base_url
            .join(&format!("v{version}/SHASUMS256.txt"))
            .expect("validated exact version produces a safe relative URL");
        let manifest = self.download_shasums(manifest_url)?;
        let checksums = parse_shasums(&manifest)?;
        let artifacts =
            plans
                .into_iter()
                .map(|plan| {
                    let filename = plan
                        .artifact_path
                        .rsplit('/')
                        .next()
                        .expect("artifact path contains a filename");
                    let sha256 = checksums.get(filename).cloned().ok_or_else(|| {
                        Error::NodeChecksumMissing {
                            version: version.to_owned(),
                            filename: filename.to_owned(),
                        }
                    })?;
                    Ok(LockedArtifact {
                        target: plan.target,
                        canonical_url: plan.canonical_url,
                        artifact_path: plan.artifact_path,
                        sha256,
                        format: match plan.format {
                            NodeArchiveFormat::Zip => LockedArtifactFormat::Zip,
                            NodeArchiveFormat::TarXz => LockedArtifactFormat::TarXz,
                        },
                        archive_root: plan.archive_root,
                        verification: "nodejs-shasums-https".to_owned(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;

        Ok(Lockfile::new_node(
            generated_by.to_owned(),
            version.to_owned(),
            artifacts,
        ))
    }

    fn download_shasums(&self, url: Url) -> Result<String> {
        let display_url = url.to_string();
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| Error::NodeMetadataRequest {
                url: display_url.clone(),
                source,
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_SHASUMS_BYTES)
        {
            return Err(Error::NodeMetadataTooLarge {
                limit: MAX_SHASUMS_BYTES,
            });
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(MAX_SHASUMS_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| Error::NodeMetadataRead {
                url: display_url,
                source,
            })?;
        if bytes.len() as u64 > MAX_SHASUMS_BYTES {
            return Err(Error::NodeMetadataTooLarge {
                limit: MAX_SHASUMS_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| Error::InvalidNodeShasums {
            reason: "manifest is not UTF-8".to_owned(),
        })
    }
}

fn parse_shasums(manifest: &str) -> Result<HashMap<String, String>> {
    let mut checksums = HashMap::new();
    for (index, line) in manifest.lines().enumerate() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2
            || parts[0].len() != 64
            || !parts[0].bytes().all(|byte| byte.is_ascii_hexdigit())
            || parts[1].contains(['/', '\\'])
        {
            return Err(Error::InvalidNodeShasums {
                reason: format!("invalid line {}", index + 1),
            });
        }
        if checksums
            .insert(parts[1].to_owned(), parts[0].to_ascii_lowercase())
            .is_some()
        {
            return Err(Error::InvalidNodeShasums {
                reason: format!("duplicate filename {}", parts[1]),
            });
        }
    }
    if checksums.is_empty() {
        return Err(Error::InvalidNodeShasums {
            reason: "manifest is empty".to_owned(),
        });
    }
    Ok(checksums)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[test]
    fn resolves_all_mvp_targets_from_official_style_shasums() {
        let manifest = MVP_NODE_TARGETS
            .into_iter()
            .enumerate()
            .map(|(index, target)| {
                let plan =
                    plan_node_artifact(&SourceConfig::default(), "24.0.0", target).expect("plan");
                let filename = plan.artifact_path.rsplit('/').next().expect("filename");
                format!("{}  {filename}", format!("{:x}", index + 1).repeat(64))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (base_url, server) = serve_once(manifest);
        let client = NodeMetadataClient {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            metadata_base_url: Url::parse(&base_url).expect("base URL"),
        };

        let lockfile = client
            .resolve_exact_lock("24.0.0", "pinset test")
            .expect("resolve lock");
        server.join().expect("server");

        let node = lockfile.tool("node").expect("node lock");
        assert_eq!(node.artifacts.len(), MVP_NODE_TARGETS.len());
        assert!(node.artifacts.iter().all(|artifact| {
            artifact
                .canonical_url
                .starts_with("https://nodejs.org/dist/v24.0.0/")
        }));
    }

    #[test]
    fn rejects_invalid_or_incomplete_shasums() {
        assert!(matches!(
            parse_shasums("not-a-hash  node.zip"),
            Err(Error::InvalidNodeShasums { .. })
        ));
        assert!(matches!(
            parse_shasums(&format!("{}  ../node.zip", "a".repeat(64))),
            Err(Error::InvalidNodeShasums { .. })
        ));

        let (base_url, server) = serve_once(format!("{}  unrelated.zip", "a".repeat(64)));
        let client = NodeMetadataClient {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            metadata_base_url: Url::parse(&base_url).expect("base URL"),
        };
        let error = client
            .resolve_exact_lock("24.0.0", "pinset test")
            .expect_err("missing target checksum");
        server.join().expect("server");
        assert!(matches!(error, Error::NodeChecksumMissing { .. }));
    }

    fn serve_once(body: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("response");
        });
        (format!("http://{address}/"), handle)
    }
}
