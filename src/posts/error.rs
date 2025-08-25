use thiserror::Error;

#[derive(Debug, Error)]
pub enum PostsError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml_edit::de::Error),

    #[error("Date parsing error: {0}")]
    DateParseError(String),

    #[error("Missing metadata field: {0}")]
    MissingMetadata(String),

    #[error("Invalid post format: {0}")]
    InvalidFormat(String),

    #[error("Post not found: {0}")]
    PostNotFound(String),
}
