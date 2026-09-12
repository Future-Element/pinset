//! Structured network failure classification. Never return an error chain or credential URL.
use crate::{EnvironmentCheck, ReadinessState};
use serde::Serialize;
use std::{error::Error as _, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFailure {
    Dns,
    Proxy,
    ProxyAuthentication,
    TlsCertificate,
    Authentication,
    Timeout,
    ArtifactNotFound,
    RateLimited,
    Server,
    Transport,
}

pub fn classify_http_error(error: &reqwest::Error) -> NetworkFailure {
    if error.is_timeout() {
        return NetworkFailure::Timeout;
    }
    if let Some(status) = error.status() {
        return classify_status(status.as_u16());
    }
    let mut source = error.source();
    let mut text = String::new();
    // Error text is inspected locally for transport-library variants, and never exported.
    for _ in 0..8 {
        let Some(value) = source else {
            break;
        };
        let value_text = value.to_string();
        text.extend(value_text.chars().take(4096));
        source = value.source();
    }
    classify_transport_detail(&text)
}

fn classify_status(status: u16) -> NetworkFailure {
    match status {
        401 | 403 => NetworkFailure::Authentication,
        407 => NetworkFailure::ProxyAuthentication,
        404 | 410 => NetworkFailure::ArtifactNotFound,
        408 | 504 => NetworkFailure::Timeout,
        429 => NetworkFailure::RateLimited,
        500..=599 => NetworkFailure::Server,
        _ => NetworkFailure::Transport,
    }
}

fn classify_transport_detail(detail: &str) -> NetworkFailure {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("certificate") || detail.contains("tls") || detail.contains("ssl") {
        NetworkFailure::TlsCertificate
    } else if detail.contains("proxy") || detail.contains("tunnel") {
        NetworkFailure::Proxy
    } else if detail.contains("dns")
        || detail.contains("lookup address")
        || detail.contains("resolve host")
    {
        NetworkFailure::Dns
    } else {
        NetworkFailure::Transport
    }
}

impl NetworkFailure {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Dns => "dns_resolution_failed",
            Self::Proxy => "proxy_connection_failed",
            Self::ProxyAuthentication => "proxy_authentication_required",
            Self::TlsCertificate => "tls_or_certificate_verification_failed",
            Self::Authentication => "source_authentication_failed",
            Self::Timeout => "network_request_timed_out",
            Self::ArtifactNotFound => "artifact_not_found_at_source",
            Self::RateLimited => "source_rate_limited",
            Self::Server => "source_server_failed",
            Self::Transport => "network_transport_failed",
        }
    }
    pub fn next_step(self) -> &'static str {
        match self {
            Self::Dns => "Check DNS for the explicitly selected source and configured proxy.",
            Self::Proxy | Self::ProxyAuthentication => {
                "Review this process's HTTPS_PROXY/HTTP_PROXY/NO_PROXY configuration and proxy credentials."
            }
            Self::TlsCertificate => {
                "Install the organization's trusted CA in the supported certificate store or configure PINSET_CA_BUNDLE; keep TLS verification enabled."
            }
            Self::Authentication => {
                "Renew access to the selected source. Do not put credentials in project reports."
            }
            Self::Timeout => "Check connectivity and proxy routing; retry the selected source.",
            Self::ArtifactNotFound => {
                "Check the locked platform artifact and configured mirror path; use the configured official fallback."
            }
            Self::RateLimited => {
                "Wait for the source's retry window or use an explicitly configured fallback."
            }
            Self::Server => "Retry later or review the configured fallback.",
            Self::Transport => {
                "Inspect the selected source connection and proxy configuration locally."
            }
        }
    }
}

/// Explicit, bounded transport probe. A successful HEAD only proves reachability;
/// archive integrity and complete offline readiness are checked separately.
pub fn probe_source(
    client: &reqwest::blocking::Client,
    ordinal: usize,
    url: &str,
) -> EnvironmentCheck {
    let request = client.head(url).timeout(Duration::from_secs(15)).send();
    let (state, reason, next) = match request {
        Ok(response) if response.status().is_success() => (
            ReadinessState::Pass,
            "source_reachable_integrity_not_checked",
            "Run pinset cache prefetch and the offline check to verify complete artifacts.",
        ),
        Ok(response) if response.status().as_u16() == 405 => (
            ReadinessState::Unknown,
            "source_does_not_support_head",
            "Use an explicit verified prefetch to test this source's download path.",
        ),
        Ok(response) => {
            let error = classify_status(response.status().as_u16());
            (ReadinessState::Fail, error.reason(), error.next_step())
        }
        Err(error) => {
            let failure = classify_http_error(&error);
            (ReadinessState::Fail, failure.reason(), failure.next_step())
        }
    };
    // Ordinals reflect existing source order without publishing private hostnames or aliases.
    EnvironmentCheck {
        id: format!("network.source.{ordinal}"),
        state,
        reason: reason.to_owned(),
        next_step: Some(next.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    #[test]
    fn real_http_failures_remain_structured_and_private() {
        for (status, expected) in [
            (401, "source_authentication_failed"),
            (407, "proxy_authentication_required"),
            (404, "artifact_not_found_at_source"),
            (503, "source_server_failed"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                assert!(stream.read(&mut request).unwrap() > 0);
                write!(
                    stream,
                    "HTTP/1.1 {status} Failure\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            });
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap();
            let report = probe_source(
                &client,
                0,
                &format!("http://{address}/private-artifact?token=not-for-report"),
            );
            server.join().unwrap();
            assert_eq!(report.reason, expected);
            let serialized = serde_json::to_string(&report).unwrap();
            assert!(
                !serialized.contains("not-for-report")
                    && !serialized.contains(&address.to_string())
            );
        }
    }
    #[test]
    fn a_real_transport_timeout_is_not_reported_as_authentication() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(150));
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(25))
            .build()
            .unwrap();
        let error = client
            .get(format!("http://{address}/fixture"))
            .send()
            .unwrap_err();
        assert_eq!(classify_http_error(&error), NetworkFailure::Timeout);
        server.join().unwrap();
    }
    #[test]
    fn network_failure_classes_remain_distinct_and_do_not_echo_secrets() {
        for (status, class) in [
            (401, NetworkFailure::Authentication),
            (407, NetworkFailure::ProxyAuthentication),
            (404, NetworkFailure::ArtifactNotFound),
            (429, NetworkFailure::RateLimited),
            (503, NetworkFailure::Server),
            (504, NetworkFailure::Timeout),
        ] {
            assert_eq!(classify_status(status), class);
        }
        for (text, class) in [
            (
                "dns lookup failed for private.internal",
                NetworkFailure::Dns,
            ),
            (
                "proxy https://user:secret@private.internal",
                NetworkFailure::Proxy,
            ),
            (
                "invalid peer certificate private.internal",
                NetworkFailure::TlsCertificate,
            ),
        ] {
            let found = classify_transport_detail(text);
            assert_eq!(found, class);
            assert!(!found.reason().contains("private"));
            assert!(!found.next_step().contains("secret@"));
        }
    }
}
