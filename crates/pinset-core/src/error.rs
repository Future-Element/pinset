#[derive(Debug, thiserror::Error)]
pub enum Error {
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
            Self::Contract { code, .. } => code,
            Self::InvalidArtifactIntegrity { .. } => "PINSET_INTEGRITY_INVALID",
            Self::Io(_) => "PINSET_IO",
            Self::Toml(_) | Self::Json(_) => "PINSET_PROTOCOL_INVALID",
        }
    }
}
