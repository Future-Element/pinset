//! Process-scoped HTTP configuration shared by metadata, archives and diagnostics.
use crate::{Error, Result};
use reqwest::{
    Certificate,
    blocking::{Client, ClientBuilder},
};
use std::{fs, path::Path};

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
