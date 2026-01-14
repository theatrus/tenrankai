use thiserror::Error;

#[derive(Debug, Error)]
pub enum SiteBuilderError {
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("Gallery initialization error: {0}")]
    Gallery(String),

    #[error("Posts initialization error: {0}")]
    Posts(String),

    #[error("Login initialization error: {0}")]
    Login(String),

    #[error("Template error: {0}")]
    Template(String),
}
