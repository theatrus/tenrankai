use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetadataStorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Storage not available")]
    StorageNotAvailable,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Other error: {0}")]
    Other(String),
}
