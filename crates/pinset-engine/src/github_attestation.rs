//! GitHub public Release attestations. The locked bundle is verified offline
//! against Sigstore's embedded public-good roots; API metadata is never proof.
use crate::{
    GitHubAttestation, MAX_GITHUB_ATTESTATION_BYTES, Result,
    github_release_repository as release_repository, service_error,
};
use reqwest::{StatusCode, blocking::Client};
use serde::Deserialize;
use sigstore_verify::{
    SubjectAltName, VerificationPolicy,
    trust_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot},
    types::{Bundle, Sha256Hash, SignatureContent},
};
use std::io::Read;
use url::Url;

const API_ROOT: &str = "https://api.github.com/";
const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";
const MAX_API_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = MAX_GITHUB_ATTESTATION_BYTES as u64;

fn invalid(message: impl Into<String>) -> crate::Error {
    service_error("PINSET_ATTESTATION_INVALID", message)
}

fn fetch_error(message: impl Into<String>) -> crate::Error {
    service_error("PINSET_ATTESTATION_FETCH_FAILED", message)
}

fn valid_repository_part(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn github_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn source_repository(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if !github_url(&url) {
        return None;
    }
    let parts: Vec<_> = url.path_segments()?.collect();
    (parts.len() == 2 && valid_repository_part(parts[0]) && valid_repository_part(parts[1]))
        .then(|| format!("{}/{}", parts[0], parts[1]))
}

fn github_workflow_identity(identity: &SubjectAltName) -> bool {
    let SubjectAltName::Uri(value) = identity else {
        return false;
    };
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !github_url(&url) {
        return false;
    }
    let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
    parts.len() >= 5
        && valid_repository_part(parts[0])
        && valid_repository_part(parts[1])
        && parts[2] == ".github"
        && parts[3] == "workflows"
        && parts[4..]
            .join("/")
            .split_once('@')
            .is_some_and(|(file, reference)| !file.is_empty() && !reference.is_empty())
}

/// Fetch and verify a matching public GitHub Release attestation. Only an
/// explicit absence is `None`; an unavailable API or invalid evidence is fatal.
pub(crate) fn fetch(client: &Client, url: &str, sha256: &str) -> Result<Option<GitHubAttestation>> {
    fetch_from_api(
        client,
        url,
        sha256,
        &Url::parse(API_ROOT).expect("constant URL"),
    )
}

#[derive(Deserialize)]
struct AttestationsResponse {
    attestations: Vec<ApiAttestation>,
}

#[derive(Deserialize)]
struct ApiAttestation {
    #[serde(default)]
    bundle: Option<serde_json::Value>,
    #[serde(default)]
    bundle_url: String,
}

#[derive(Deserialize)]
struct RepositoryResponse {
    private: bool,
}

fn fetch_from_api(
    client: &Client,
    artifact_url: &str,
    sha256: &str,
    api_root: &Url,
) -> Result<Option<GitHubAttestation>> {
    let Some(repository) = release_repository(artifact_url) else {
        return Ok(None);
    };
    let digest = Sha256Hash::from_hex(sha256)
        .map_err(|error| invalid(format!("invalid artifact SHA256: {error}")))?;
    let endpoint = api_root
        .join(&format!(
            "repos/{repository}/attestations/sha256:{}?per_page=100",
            digest.to_hex()
        ))
        .expect("repository and digest are validated URL components");
    let response = api_get(client, &endpoint)?;
    if response.status() == StatusCode::NOT_FOUND {
        // GitHub uses 404 for an absent digest as well as inaccessible repos.
        // Confirm this is an accessible public repository before accepting TOFU.
        let repository_url = api_root
            .join(&format!("repos/{repository}"))
            .expect("validated repository");
        let repository_response = api_get(client, &repository_url)?;
        require_success(&repository_response)?;
        let metadata: RepositoryResponse =
            serde_json::from_slice(&read_limited(repository_response, MAX_API_BYTES)?).map_err(
                |error| fetch_error(format!("invalid GitHub repository response: {error}")),
            )?;
        if metadata.private {
            return Err(fetch_error(
                "attestations require an accessible public GitHub repository",
            ));
        }
        return Ok(None);
    }
    require_success(&response)?;
    let more_pages = response
        .headers()
        .get(reqwest::header::LINK)
        .is_some_and(|value| {
            value
                .to_str()
                .is_ok_and(|links| links.contains("rel=\"next\""))
        });
    let document: AttestationsResponse =
        serde_json::from_slice(&read_limited(response, MAX_API_BYTES)?).map_err(|error| {
            fetch_error(format!("invalid GitHub attestation response: {error}"))
        })?;
    if document.attestations.is_empty() {
        return if more_pages {
            Err(fetch_error(
                "empty attestation page has a next page; cannot establish absence",
            ))
        } else {
            Ok(None)
        };
    }
    // One independently verified attestation suffices. An invalid candidate
    // never causes a downgrade, even if later pages have not been retrieved.
    let mut last_error = None;
    for candidate in document.attestations {
        let bundle = match candidate.bundle {
            Some(bundle) => bundle.to_string(),
            None => match fetch_bundle(client, &candidate.bundle_url) {
                Ok(bundle) => bundle,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            },
        };
        let evidence = GitHubAttestation {
            repository: repository.clone(),
            bundle,
        };
        match verify(artifact_url, sha256, &evidence) {
            Ok(()) => return Ok(Some(evidence)),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| invalid("no valid GitHub attestation")))
}

fn api_get(client: &Client, url: &Url) -> Result<reqwest::blocking::Response> {
    let response = crate::send_metadata_get(
        client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(reqwest::header::USER_AGENT, "pinset-attestation"),
    )
    .map_err(|error| fetch_error(format!("GitHub attestation request failed: {error}")))?;
    if response.url().origin() != url.origin() {
        return Err(fetch_error(
            "GitHub attestation API redirected outside its origin",
        ));
    }
    Ok(response)
}

fn require_success(response: &reqwest::blocking::Response) -> Result<()> {
    // A partial or empty-success response cannot establish evidence absence.
    if response.status() != StatusCode::OK {
        return Err(fetch_error(format!(
            "GitHub attestation request returned HTTP {}",
            response.status().as_u16()
        )));
    }
    Ok(())
}

fn read_limited(response: reqwest::blocking::Response, maximum: u64) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(fetch_error(
            "GitHub attestation response exceeds the size limit",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| fetch_error(format!("reading GitHub attestation failed: {error}")))?;
    if bytes.len() as u64 > maximum {
        return Err(fetch_error(
            "GitHub attestation response exceeds the size limit",
        ));
    }
    Ok(bytes)
}

fn fetch_bundle(client: &Client, value: &str) -> Result<String> {
    let url = Url::parse(value).map_err(|_| fetch_error("invalid GitHub bundle URL"))?;
    // API-controlled URLs must not turn metadata fetching into arbitrary network
    // access. This is GitHub's public artifact-attestation blob service.
    if url.scheme() != "https"
        || url.host_str() != Some("tmaproduction.blob.core.windows.net")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with("/attestations/")
        || !url.path().ends_with(".json.sn")
    {
        return Err(fetch_error("unsupported GitHub bundle storage URL"));
    }
    let response = crate::send_metadata_get(client.get(url.clone()))
        .map_err(|error| fetch_error(format!("GitHub bundle request failed: {error}")))?;
    if response.url().origin() != url.origin() {
        return Err(fetch_error(
            "GitHub bundle redirected outside its storage origin",
        ));
    }
    require_success(&response)?;
    let compressed = read_limited(response, MAX_BUNDLE_BYTES)?;
    decode_bundle(&compressed)
}

fn decode_bundle(compressed: &[u8]) -> Result<String> {
    if compressed.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(fetch_error(
            "compressed GitHub bundle exceeds the size limit",
        ));
    }
    let decompressed_size = snap::raw::decompress_len(compressed)
        .map_err(|error| fetch_error(format!("invalid compressed GitHub bundle: {error}")))?;
    if decompressed_size as u64 > MAX_BUNDLE_BYTES {
        return Err(fetch_error(
            "decompressed GitHub bundle exceeds the size limit",
        ));
    }
    let decoded = snap::raw::Decoder::new()
        .decompress_vec(compressed)
        .map_err(|error| fetch_error(format!("invalid compressed GitHub bundle: {error}")))?;
    String::from_utf8(decoded).map_err(|_| fetch_error("GitHub attestation bundle is not UTF-8"))
}

fn require_provenance_statement(payload: &[u8]) -> Result<()> {
    let statement: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|error| invalid(format!("invalid in-toto provenance statement: {error}")))?;
    if statement.get("_type").and_then(serde_json::Value::as_str)
        != Some("https://in-toto.io/Statement/v1")
        || statement
            .get("predicateType")
            .and_then(serde_json::Value::as_str)
            != Some("https://slsa.dev/provenance/v1")
    {
        return Err(invalid(
            "attestation must be an in-toto v1 SLSA provenance v1 statement",
        ));
    }
    Ok(())
}

/// Reverify locked evidence without network access. The repository in the lock
/// must match the download URL, and authorization comes from Fulcio certificate
/// extensions, never from the statement's self-asserted predicate or the API.
pub(crate) fn verify(url: &str, sha256: &str, evidence: &GitHubAttestation) -> Result<()> {
    let repository = release_repository(url).ok_or_else(|| {
        invalid("attestation is only supported for public GitHub Release asset URLs")
    })?;
    if !repository.eq_ignore_ascii_case(&evidence.repository) {
        return Err(invalid(
            "locked attestation repository differs from the Release URL",
        ));
    }
    if evidence.bundle.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(invalid(
            "locked GitHub attestation bundle exceeds the size limit",
        ));
    }
    let digest = Sha256Hash::from_hex(sha256)
        .map_err(|error| invalid(format!("invalid artifact SHA256: {error}")))?;
    let bundle = Bundle::from_json(&evidence.bundle)
        .map_err(|error| invalid(format!("invalid Sigstore bundle: {error}")))?;
    if !matches!(bundle.content, SignatureContent::DsseEnvelope(_)) {
        return Err(invalid(
            "GitHub attestation must contain a DSSE in-toto envelope",
        ));
    }
    let root = TrustedRoot::from_json(SIGSTORE_PRODUCTION_TRUSTED_ROOT)
        .map_err(|error| invalid(format!("invalid embedded Sigstore trusted root: {error}")))?;
    // The defaults require the trusted Fulcio chain, historical validity, SCT,
    // signed Rekor checkpoint/inclusion, DSSE signature and subject digest.
    // Never use any skip_* policy or roots supplied by the bundle/API/lock.
    let policy = VerificationPolicy::any_identity().require_issuer(GITHUB_ISSUER);
    let verified = sigstore_verify::verify(digest, &bundle, &policy, &root)
        .map_err(|error| invalid(format!("GitHub attestation verification failed: {error}")))?;
    if !verified.certificate_verified()
        || !verified.sct_verified()
        || !verified.tlog_verified()
        || !verified.identity_policy_checked()
    {
        return Err(invalid(
            "GitHub attestation is missing required verified evidence",
        ));
    }
    if let SignatureContent::DsseEnvelope(envelope) = &bundle.content {
        require_provenance_statement(envelope.payload.as_bytes())?;
    }
    let certificate = verified
        .certificate()
        .ok_or_else(|| invalid("GitHub attestation has no verified signing certificate"))?;
    let identity = verified
        .identity()
        .ok_or_else(|| invalid("GitHub attestation signing certificate has no identity"))?;
    if !github_workflow_identity(identity) {
        return Err(invalid(
            "attestation signer is not a GitHub Actions workflow",
        ));
    }
    let claims = &certificate.ci_claims;
    let certified_repository = if let Some(uri) = &claims.source_repository_uri {
        source_repository(uri)
            .ok_or_else(|| invalid("invalid source repository in signing certificate"))?
    } else if let Some(repository) = &claims.deprecated_github.workflow_repository {
        source_repository(&format!("https://github.com/{repository}"))
            .ok_or_else(|| invalid("invalid legacy source repository in signing certificate"))?
    } else {
        return Err(invalid(
            "signing certificate does not identify its source repository",
        ));
    };
    if !repository.eq_ignore_ascii_case(&certified_repository) {
        return Err(invalid(format!(
            "certificate source repository {certified_repository} does not match {repository}"
        )));
    }
    if claims
        .source_repository_visibility_at_signing
        .as_deref()
        .is_some_and(|visibility| visibility != "public")
    {
        return Err(invalid(
            "attestation source repository was not public at signing time",
        ));
    }
    // A reusable workflow's SAN may belong to another repository. Its caller's
    // source_repository_uri is the authenticated repository being authorized.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::{io::Write, net::TcpListener, thread};

    const DIGEST: &str = "49a3aa6075e0f49f82843e74b5baa614ad2a588e6675612bf108a0a008c5ac25";
    const RELEASE: &str = "https://github.com/malancas/attest-demo/releases/download/v0.0.0/github_provenance_demo-0.0.0-py3-none-any.whl";
    const BUNDLE: &str =
        include_str!("../tests/fixtures/github-attestation/reusable-workflow.sigstore.json");

    fn evidence() -> GitHubAttestation {
        GitHubAttestation {
            repository: "malancas/attest-demo".into(),
            bundle: BUNDLE.into(),
        }
    }

    fn mutate_bundle(mutate: impl FnOnce(&mut serde_json::Value)) -> GitHubAttestation {
        let mut bundle: serde_json::Value = serde_json::from_str(BUNDLE).unwrap();
        mutate(&mut bundle);
        GitHubAttestation {
            bundle: bundle.to_string(),
            ..evidence()
        }
    }

    fn server(replies: Vec<(u16, String)>) -> (Client, Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).unwrap();
                assert!(read > 0, "fixture received an empty HTTP request");
                write!(stream, "HTTP/1.1 {status} Status\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\nRetry-After: 3\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        (
            client,
            Url::parse(&format!("http://{address}/")).unwrap(),
            worker,
        )
    }

    #[test]
    fn verifies_reusable_workflow_with_source_repository_from_certificate_offline() {
        // The SAN is github/artifact-attestations-workflows while the source
        // repository is malancas/attest-demo. No network is involved.
        verify(RELEASE, DIGEST, &evidence()).unwrap();
        verify(
            &RELEASE.replace("malancas/attest-demo", "MALANCAS/ATTEST-DEMO"),
            DIGEST,
            &evidence(),
        )
        .unwrap();
    }

    #[test]
    fn rejects_a_reusable_signer_repository_as_the_source() {
        let mut bundle = evidence();
        bundle.repository = "github/artifact-attestations-workflows".into();
        let url = RELEASE.replace("malancas/attest-demo", &bundle.repository);
        assert!(
            verify(&url, DIGEST, &bundle)
                .unwrap_err()
                .to_string()
                .contains("certificate source repository")
        );
    }

    #[test]
    fn rejects_wrong_digest_and_locked_repository() {
        assert!(verify(RELEASE, &"0".repeat(64), &evidence()).is_err());
        let mut wrong = evidence();
        wrong.repository = "attacker/attest-demo".into();
        assert!(verify(RELEASE, DIGEST, &wrong).is_err());
    }

    #[test]
    fn only_accepts_in_toto_v1_slsa_provenance_statements() {
        require_provenance_statement(br#"{"_type":"https://in-toto.io/Statement/v1","predicateType":"https://slsa.dev/provenance/v1"}"#).unwrap();
        for value in [
            serde_json::json!({"_type":"https://in-toto.io/Statement/v0.1", "predicateType":"https://slsa.dev/provenance/v1"}),
            serde_json::json!({"_type":"https://in-toto.io/Statement/v1", "predicateType":"https://cyclonedx.org/bom"}),
            serde_json::json!({"_type":"https://in-toto.io/Statement/v1"}),
            serde_json::json!({}),
        ] {
            assert!(require_provenance_statement(value.to_string().as_bytes()).is_err());
        }
    }

    #[test]
    fn rejects_tampered_dsse_signature() {
        let altered = mutate_bundle(|bundle| {
            let mut signature = STANDARD
                .decode(
                    bundle["dsseEnvelope"]["signatures"][0]["sig"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
            *signature.last_mut().unwrap() ^= 1;
            bundle["dsseEnvelope"]["signatures"][0]["sig"] = STANDARD.encode(signature).into();
        });
        assert!(verify(RELEASE, DIGEST, &altered).is_err());
    }

    #[test]
    fn rejects_tampered_statement_and_certificate() {
        let statement = mutate_bundle(|bundle| {
            let mut payload: serde_json::Value = serde_json::from_slice(
                &STANDARD
                    .decode(bundle["dsseEnvelope"]["payload"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            payload["predicate"]["buildDefinition"]["externalParameters"]["workflow"]["repository"] =
                "https://github.com/attacker/project".into();
            bundle["dsseEnvelope"]["payload"] = STANDARD.encode(payload.to_string()).into();
        });
        assert!(verify(RELEASE, DIGEST, &statement).is_err());
        let certificate = mutate_bundle(|bundle| {
            let mut bytes = STANDARD
                .decode(
                    bundle["verificationMaterial"]["certificate"]["rawBytes"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
            *bytes.last_mut().unwrap() ^= 1;
            bundle["verificationMaterial"]["certificate"]["rawBytes"] =
                STANDARD.encode(bytes).into();
        });
        assert!(verify(RELEASE, DIGEST, &certificate).is_err());
    }

    #[test]
    fn rejects_tampered_inclusion_proof_checkpoint_and_signing_time() {
        for (field, value) in [
            (
                "rootHash",
                serde_json::Value::String(STANDARD.encode([0u8; 32])),
            ),
            (
                "checkpoint",
                serde_json::json!({"envelope": "forged checkpoint"}),
            ),
        ] {
            let altered = mutate_bundle(|bundle| {
                bundle["verificationMaterial"]["tlogEntries"][0]["inclusionProof"][field] = value
            });
            assert!(
                verify(RELEASE, DIGEST, &altered).is_err(),
                "accepted altered {field}"
            );
        }
        let altered = mutate_bundle(|bundle| {
            bundle["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = "1".into()
        });
        assert!(verify(RELEASE, DIGEST, &altered).is_err());
    }

    #[test]
    fn rejects_missing_transparency_log_and_oversized_bundle() {
        let altered = mutate_bundle(|bundle| {
            bundle["verificationMaterial"]["tlogEntries"] = serde_json::json!([])
        });
        assert!(verify(RELEASE, DIGEST, &altered).is_err());
        let oversized = GitHubAttestation {
            bundle: " ".repeat(MAX_BUNDLE_BYTES as usize + 1),
            ..evidence()
        };
        assert!(verify(RELEASE, DIGEST, &oversized).is_err());
    }

    #[test]
    fn limits_release_urls_to_explicit_public_github_assets() {
        assert_eq!(
            release_repository(RELEASE).as_deref(),
            Some("malancas/attest-demo")
        );
        for url in [
            "http://github.com/a/b/releases/download/v1/tool.zip",
            "https://github.example/a/b/releases/download/v1/tool.zip",
            "https://github.com/a/b/releases/latest/download/tool.zip",
            "https://github.com/a/b/releases/download/v1/",
            "https://user:pass@github.com/a/b/releases/download/v1/tool.zip",
            "https://github.com/a/b/releases/download/v1/tool.zip#fragment",
            "https://github.com/a%2Fb/c/releases/download/v1/tool.zip",
        ] {
            assert!(release_repository(url).is_none(), "accepted {url}");
            assert!(verify(url, DIGEST, &evidence()).is_err());
        }
    }

    #[test]
    fn fetches_and_verifies_inline_evidence() {
        let body = serde_json::json!({"attestations":[{"bundle":serde_json::from_str::<serde_json::Value>(BUNDLE).unwrap()}]}).to_string();
        let (client, root, worker) = server(vec![(200, body)]);
        let result = fetch_from_api(&client, RELEASE, DIGEST, &root)
            .unwrap()
            .unwrap();
        assert_eq!(result.repository, "malancas/attest-demo");
        verify(RELEASE, DIGEST, &result).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn absent_digest_requires_explicit_empty_response_or_confirmed_public_repository() {
        let (client, root, worker) = server(vec![(200, "{\"attestations\":[]}".into())]);
        assert!(
            fetch_from_api(&client, RELEASE, DIGEST, &root)
                .unwrap()
                .is_none()
        );
        worker.join().unwrap();
        let (client, root, worker) = server(vec![
            (404, "{}".into()),
            (200, "{\"private\":false}".into()),
        ]);
        assert!(
            fetch_from_api(&client, RELEASE, DIGEST, &root)
                .unwrap()
                .is_none()
        );
        worker.join().unwrap();
    }

    #[test]
    fn forbidden_rate_limited_and_inaccessible_repository_are_not_absence() {
        for status in [206, 403, 429] {
            let (client, root, worker) = server(vec![(status, "{}".into())]);
            assert!(fetch_from_api(&client, RELEASE, DIGEST, &root).is_err());
            worker.join().unwrap();
        }
        for repository in [(404, "{}"), (200, "{\"private\":true}")] {
            let (client, root, worker) = server(vec![
                (404, "{}".into()),
                (repository.0, repository.1.into()),
            ]);
            assert!(fetch_from_api(&client, RELEASE, DIGEST, &root).is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn malformed_or_invalid_evidence_never_downgrades_to_tofu() {
        for body in [
            "{}",
            "{\"attestations\":[{\"bundle\":{}}]}",
            "{\"attestations\":[{\"bundle_url\":\"http://localhost/private\"}]}",
        ] {
            let (client, root, worker) = server(vec![(200, body.into())]);
            assert!(fetch_from_api(&client, RELEASE, DIGEST, &root).is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn snappy_bundle_decoding_enforces_valid_utf8_and_decompressed_size_limit() {
        let compressed = snap::raw::Encoder::new()
            .compress_vec(BUNDLE.as_bytes())
            .unwrap();
        assert_eq!(decode_bundle(&compressed).unwrap(), BUNDLE);
        assert!(decode_bundle(b"not a snappy bundle").is_err());
        let invalid_utf8 = snap::raw::Encoder::new().compress_vec(&[0xff]).unwrap();
        assert!(decode_bundle(&invalid_utf8).is_err());
        let bomb = snap::raw::Encoder::new()
            .compress_vec(&vec![b' '; MAX_BUNDLE_BYTES as usize + 1])
            .unwrap();
        assert!(bomb.len() < MAX_BUNDLE_BYTES as usize);
        assert!(decode_bundle(&bomb).is_err());
    }

    #[test]
    fn network_failure_is_not_absent_evidence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let client = Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();
        let root = Url::parse(&format!("http://{address}/")).unwrap();
        assert!(fetch_from_api(&client, RELEASE, DIGEST, &root).is_err());
    }
}
