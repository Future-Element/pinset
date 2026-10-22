//! Process-scoped HTTP configuration shared by metadata, archives and diagnostics.
use crate::{Error, Result};
use reqwest::{
    Certificate, StatusCode,
    blocking::{Client, ClientBuilder, RequestBuilder, Response},
};
use std::{fs, path::Path, time::Duration};

const METADATA_ATTEMPTS: u32 = 4;
const MAX_METADATA_RETRY_AFTER: u64 = 2;

/// Retries only metadata GET transmission and transient HTTP responses. Body
/// limits, decoding, parsing and verification remain the provider's responsibility.
/// The original response/error is returned for the provider's existing mapping.
pub(crate) fn send_metadata_get(
    mut request: RequestBuilder,
) -> std::result::Result<Response, reqwest::Error> {
    for attempt in 0..METADATA_ATTEMPTS {
        let next = request.try_clone();
        let result = request.send();
        if attempt + 1 == METADATA_ATTEMPTS {
            return result;
        }
        let delay = match &result {
            Ok(response) => metadata_retry_delay(response, attempt),
            // Request errors originate in send, rather than body/JSON reads,
            // builder validation, redirects or HTTP error_for_status below.
            Err(error) if error.is_connect() || error.is_timeout() || error.is_request() => {
                Some(metadata_backoff(attempt))
            }
            _ => None,
        };
        let (Some(delay), Some(next)) = (delay, next) else {
            return result;
        };
        drop(result);
        std::thread::sleep(delay);
        request = next;
    }
    unreachable!("metadata attempts are bounded and return the last result")
}

fn metadata_backoff(attempt: u32) -> Duration {
    Duration::from_millis(100 << attempt)
}

fn metadata_retry_delay(response: &Response, attempt: u32) -> Option<Duration> {
    let status = response.status();
    if !(status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error())
    {
        return None;
    }
    if let Some(advertised) = response.headers().get(reqwest::header::RETRY_AFTER) {
        // Malformed, HTTP-date and long delays are surfaced without
        // retrying early or making an interactive command wait indefinitely.
        let seconds = advertised.to_str().ok()?.parse::<u64>().ok()?;
        return (seconds <= MAX_METADATA_RETRY_AFTER)
            .then(|| Duration::from_secs(seconds).max(metadata_backoff(attempt)));
    }
    (status != StatusCode::TOO_MANY_REQUESTS).then(|| metadata_backoff(attempt))
}

pub fn http_client_builder() -> Result<ClientBuilder> {
    let ca = std::env::var_os("PINSET_CA_BUNDLE");
    client_builder_with_ca(ca.as_deref().map(Path::new))
}

/// Adds organization roots to the normal trust store; never disables TLS verification.
pub fn client_builder_with_ca(ca: Option<&Path>) -> Result<ClientBuilder> {
    let builder = Client::builder();
    let Some(path) = ca else {
        return Ok(builder);
    };
    let invalid = || {
        Error::InvalidNetworkConfig { reason: "PINSET_CA_BUNDLE must reference a readable regular PEM certificate bundle of at most 1 MiB".to_owned() }
    };
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(invalid());
    }
    let pem = fs::read(path).map_err(|_| invalid())?;
    let certificates = Certificate::from_pem_bundle(&pem).map_err(|_| invalid())?;
    if certificates.is_empty() {
        return Err(invalid());
    }
    Ok(builder.tls_certs_merge(certificates))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpListener},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread::{self, JoinHandle},
        time::Instant,
    };

    enum Reply {
        Disconnect,
        Status(u16, &'static str),
    }

    fn metadata_server(replies: Vec<Reply>) -> (String, Arc<AtomicUsize>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for reply in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "metadata request was not sent");
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("metadata fixture accept failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|value| value == b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).unwrap();
                    assert!(read > 0 && request.len() + read <= 16 * 1024);
                    request.extend_from_slice(&buffer[..read]);
                }
                assert!(request.starts_with(b"GET /metadata HTTP/1.1\r\n"));
                assert!(
                    String::from_utf8(request)
                        .unwrap()
                        .contains("x-pinset-metadata-test: retained\r\n")
                );
                observed.fetch_add(1, Ordering::Relaxed);
                match reply {
                    Reply::Disconnect => stream.shutdown(Shutdown::Both).unwrap(),
                    Reply::Status(status, headers) => {
                        write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: 2\r\nConnection: close\r\n{headers}\r\nok").unwrap();
                    }
                }
            }
        });
        (format!("http://{address}/metadata"), requests, server)
    }

    fn metadata_request(url: &str) -> RequestBuilder {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
            .get(url)
            .header("x-pinset-metadata-test", "retained")
    }

    #[test]
    fn metadata_disconnect_then_success_retries_the_get_with_headers() {
        let (url, requests, server) =
            metadata_server(vec![Reply::Disconnect, Reply::Status(200, "")]);
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().unwrap(), "ok");
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn metadata_service_unavailable_then_success_retries() {
        let (url, requests, server) =
            metadata_server(vec![Reply::Status(503, ""), Reply::Status(200, "")]);
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn metadata_not_found_is_returned_without_retry() {
        let (url, requests, server) = metadata_server(vec![Reply::Status(404, "")]);
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn metadata_persistent_failure_stops_at_four_requests() {
        let replies = (0..METADATA_ATTEMPTS)
            .map(|_| Reply::Status(503, ""))
            .collect();
        let (url, requests, server) = metadata_server(replies);
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), METADATA_ATTEMPTS as usize);
    }

    #[test]
    fn metadata_persistent_disconnect_stops_at_four_requests() {
        let replies = (0..METADATA_ATTEMPTS).map(|_| Reply::Disconnect).collect();
        let (url, requests, server) = metadata_server(replies);
        let error = send_metadata_get(metadata_request(&url)).unwrap_err();
        assert!(error.is_request() || error.is_connect() || error.is_timeout());
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), METADATA_ATTEMPTS as usize);
    }

    #[test]
    fn metadata_short_rate_limit_retries() {
        let (url, requests, server) = metadata_server(vec![
            Reply::Status(429, "Retry-After: 1\r\n"),
            Reply::Status(200, ""),
        ]);
        let started = Instant::now();
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert!(started.elapsed() >= Duration::from_secs(1));
        assert_eq!(response.status(), StatusCode::OK);
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn metadata_unbounded_or_unknown_rate_limit_is_not_retried() {
        for headers in [
            "",
            "Retry-After: 3\r\n",
            "Retry-After: invalid\r\n",
            "Retry-After: Wed, 21 Oct 2026 07:28:00 GMT\r\n",
        ] {
            let (url, requests, server) = metadata_server(vec![Reply::Status(429, headers)]);
            let response = send_metadata_get(metadata_request(&url)).unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            server.join().unwrap();
            assert_eq!(requests.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn metadata_service_unavailable_with_long_retry_after_is_not_retried() {
        let (url, requests, server) =
            metadata_server(vec![Reply::Status(503, "Retry-After: 3\r\n")]);
        let response = send_metadata_get(metadata_request(&url)).unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn invalid_ca_never_falls_back_to_disabled_verification() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private-company-certificate.pem");
        fs::write(&path, "not a certificate").unwrap();
        let error = client_builder_with_ca(Some(&path)).unwrap_err().to_string();
        assert!(!error.contains("private-company"));
        assert!(client_builder_with_ca(Some(root.path())).is_err());
        assert!(client_builder_with_ca(None).is_ok());
    }
}
