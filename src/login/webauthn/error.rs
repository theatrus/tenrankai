use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebauthnError {
    #[error("WebAuthn error: {0}")]
    WebauthnError(#[from] webauthn_rs::prelude::WebauthnError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Registration not found")]
    RegistrationNotFound,

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("User not found")]
    UserNotFound,

    #[error("Invalid state")]
    InvalidState,

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
