//! Provider-owned cryptographic verification; policy is shared by core.
use crate::{Result, VerificationMethod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedPayload {
    pub payload: Vec<u8>,
    pub signer: Option<String>,
}

/// Protocol-specific verifiers implement this interface instead of exposing bespoke success
/// values to Provider metadata code.
pub(crate) trait ProvenanceVerifier {
    fn method(&self) -> VerificationMethod;
    fn verify(&self, input: &[u8]) -> Result<VerifiedPayload>;
}
