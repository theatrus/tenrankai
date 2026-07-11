use thiserror::Error;

#[derive(Debug, Error)]
pub enum PostsError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml_edit::de::Error),

    #[error("Storage error: {0}")]
    StorageError(#[from] crate::storage::StorageError),

    #[error("Date parsing error: {0}")]
    DateParseError(String),

    #[error("Missing metadata field: {0}")]
    MissingMetadata(String),

    #[error("Invalid post format: {0}")]
    InvalidFormat(String),

    #[error("Post not found: {0}")]
    PostNotFound(String),

    #[error("Invalid slug: {0}")]
    InvalidSlug(String),

    #[error("Post already exists: {0}")]
    PostAlreadyExists(String),

    #[error("TOML serialization error: {0}")]
    TomlSerError(#[from] toml_edit::ser::Error),
}
