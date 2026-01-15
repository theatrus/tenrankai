//! Error types for user storage operations.

use thiserror::Error;

/// Errors that can occur during user storage operations.
#[derive(Debug, Error)]
pub enum UserStorageError {
    /// User not found.
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// User already exists.
    #[error("User already exists: {0}")]
    UserAlreadyExists(String),

    /// Passkey not found.
    #[error("Passkey not found: {0}")]
    PasskeyNotFound(String),

    /// Invalid storage URL.
    #[error("Invalid user storage URL: {0}")]
    InvalidUrl(String),

    /// Unsupported backend (feature not enabled).
    #[error("Unsupported user storage backend: {0}")]
    UnsupportedBackend(String),

    /// I/O error (file operations).
    #[error("User storage I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("User storage serialization error: {0}")]
    Serialization(String),

    /// Database error (SQL backends).
    #[error("User storage database error: {0}")]
    Database(String),

    /// AWS error (DynamoDB backend).
    #[error("User storage AWS error: {0}")]
    Aws(String),

    /// Generic error.
    #[error("User storage error: {0}")]
    Other(String),
}

impl UserStorageError {
    /// Check if this is a "not found" error.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::UserNotFound(_) | Self::PasskeyNotFound(_))
    }

    /// Check if this is an "already exists" error.
    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::UserAlreadyExists(_))
    }
}
