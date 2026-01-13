//! Error types for storage operations.

use thiserror::Error;

/// Error type for storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Object not found at the specified path.
    #[error("Object not found: {0}")]
    NotFound(String),

    /// Permission denied for the operation.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid URL format for storage configuration.
    #[error("Invalid storage URL: {0}")]
    InvalidUrl(String),

    /// I/O error from the underlying storage system.
    #[error("Storage I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic storage error for backend-specific issues.
    #[error("Storage error: {0}")]
    Other(String),
}

impl StorageError {
    /// Check if this error is a "not found" error.
    pub fn is_not_found(&self) -> bool {
        matches!(self, StorageError::NotFound(_))
    }
}

impl From<StorageError> for std::io::Error {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(path) => std::io::Error::new(std::io::ErrorKind::NotFound, path),
            StorageError::PermissionDenied(msg) => {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg)
            }
            StorageError::Io(e) => e,
            StorageError::InvalidUrl(msg) | StorageError::Other(msg) => std::io::Error::other(msg),
        }
    }
}
