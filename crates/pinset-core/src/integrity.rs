use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{Error, Result};

#[cfg(feature = "installer")]
use sha2::{Digest, Sha256, Sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityAlgorithm {
    Sha256,
    Sha512,
}

impl IntegrityAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIntegrity {
    algorithm: IntegrityAlgorithm,
    bytes: Vec<u8>,
}

impl ArtifactIntegrity {
    pub fn parse(value: &str) -> Result<Self> {
        if is_hex(value, 64) {
            return Self::from_hex(IntegrityAlgorithm::Sha256, value);
        }
        if let Some(value) = value.strip_prefix("sha256:") {
            return Self::from_hex(IntegrityAlgorithm::Sha256, value);
        }
        if let Some(value) = value.strip_prefix("sha512:") {
            return Self::from_hex(IntegrityAlgorithm::Sha512, value);
        }
        if let Some(value) = value.strip_prefix("sha512-") {
            let bytes = STANDARD
                .decode(value)
                .map_err(|_| Error::InvalidArtifactIntegrity {
                    value: format!("sha512-{value}"),
                })?;
            if bytes.len() != 64 {
                return Err(Error::InvalidArtifactIntegrity {
                    value: format!("sha512-{value}"),
                });
            }
            return Ok(Self {
                algorithm: IntegrityAlgorithm::Sha512,
                bytes,
            });
        }
        Err(Error::InvalidArtifactIntegrity {
            value: value.to_owned(),
        })
    }

    fn from_hex(algorithm: IntegrityAlgorithm, value: &str) -> Result<Self> {
        let expected_length = match algorithm {
            IntegrityAlgorithm::Sha256 => 64,
            IntegrityAlgorithm::Sha512 => 128,
        };
        if !is_hex(value, expected_length) {
            return Err(Error::InvalidArtifactIntegrity {
                value: format!("{}:{value}", algorithm.as_str()),
            });
        }
        let bytes = hex::decode(value).map_err(|_| Error::InvalidArtifactIntegrity {
            value: format!("{}:{value}", algorithm.as_str()),
        })?;
        Ok(Self { algorithm, bytes })
    }

    pub const fn algorithm(&self) -> IntegrityAlgorithm {
        self.algorithm
    }

    pub fn expected_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn cache_key(&self) -> String {
        hex::encode(&self.bytes)
    }

    pub fn canonical(&self) -> String {
        self.canonical_digest(&self.bytes)
    }

    #[cfg(feature = "installer")]
    pub(crate) fn canonical_digest(&self, bytes: &[u8]) -> String {
        match self.algorithm {
            IntegrityAlgorithm::Sha256 => format!("sha256:{}", hex::encode(bytes)),
            IntegrityAlgorithm::Sha512 => format!("sha512-{}", STANDARD.encode(bytes)),
        }
    }

    #[cfg(feature = "installer")]
    pub(crate) fn hasher(&self) -> IntegrityHasher {
        match self.algorithm {
            IntegrityAlgorithm::Sha256 => IntegrityHasher::Sha256(Sha256::new()),
            IntegrityAlgorithm::Sha512 => IntegrityHasher::Sha512(Sha512::new()),
        }
    }
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(feature = "installer")]
#[derive(Clone)]
pub(crate) enum IntegrityHasher {
    Sha256(Sha256),
    Sha512(Sha512),
}

#[cfg(feature = "installer")]
impl IntegrityHasher {
    pub fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha256(hasher) => hasher.update(bytes),
            Self::Sha512(hasher) => hasher.update(bytes),
        }
    }

    pub fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha256(hasher) => hasher.finalize().to_vec(),
            Self::Sha512(hasher) => hasher.finalize().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_sha256_and_npm_sha512() {
        let sha256 = ArtifactIntegrity::parse(&"ab".repeat(32)).expect("legacy sha256");
        assert_eq!(sha256.algorithm(), IntegrityAlgorithm::Sha256);
        assert_eq!(sha256.canonical(), format!("sha256:{}", "ab".repeat(32)));

        let bytes = vec![7_u8; 64];
        let value = format!("sha512-{}", STANDARD.encode(&bytes));
        let sha512 = ArtifactIntegrity::parse(&value).expect("npm integrity");
        assert_eq!(sha512.algorithm(), IntegrityAlgorithm::Sha512);
        assert_eq!(sha512.expected_bytes(), bytes);
        assert_eq!(sha512.canonical(), value);
    }

    #[test]
    fn rejects_malformed_integrity_values() {
        for value in ["", "sha256:no", "sha512-no", "md5:abcd"] {
            assert!(ArtifactIntegrity::parse(value).is_err(), "{value}");
        }
    }
}
