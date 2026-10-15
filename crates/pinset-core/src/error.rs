#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PINSET_LOCK_INVALID: {reason}")]
    InvalidLockfile { reason: String },
    #[error(
        "verification policy for {tool} requires {required}, but the lock provides only {actual}"
    )]
    VerificationPolicyViolation {
        tool: String,
        required: String,
        actual: String,
    },
    #[error("refusing to downgrade {tool} verification from {previous} to {next}")]
    VerificationDowngrade {
        tool: String,
        previous: String,
        next: String,
    },
    #[error("minimum release age cannot be enforced for {tool}: no release timestamp")]
    ReleaseAgeUnavailable { tool: String },
    #[error("{tool} release {released_at} is newer than minimum release age {required}")]
    ReleaseTooNew {
        tool: String,
        released_at: String,
        required: String,
    },
    #[error("{code}: {message}")]
    Contract { code: &'static str, message: String },
    #[error("PINSET_INTEGRITY_INVALID: invalid artifact integrity {value}")]
    InvalidArtifactIntegrity { value: String },
    #[error("PINSET_IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("PINSET_PROTOCOL_INVALID: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("PINSET_PROTOCOL_INVALID: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn failure(code: &'static str, message: impl Into<String>) -> Error {
    Error::Contract {
        code,
        message: message.into(),
    }
}
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidLockfile { .. } => "PINSET_LOCK_INVALID",
            Self::VerificationPolicyViolation { .. } => "PINSET_POLICY_VIOLATION",
            Self::VerificationDowngrade { .. } => "PINSET_VERIFICATION_DOWNGRADE",
            Self::ReleaseAgeUnavailable { .. } => "PINSET_RELEASE_AGE_UNAVAILABLE",
            Self::ReleaseTooNew { .. } => "PINSET_RELEASE_TOO_NEW",
            Self::Contract { code, .. } => code,
            Self::InvalidArtifactIntegrity { .. } => "PINSET_INTEGRITY_INVALID",
            Self::Io(_) => "PINSET_IO",
            Self::Toml(_) | Self::Json(_) => "PINSET_PROTOCOL_INVALID",
        }
    }
}
