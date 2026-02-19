use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigStorageError {
    #[error("Config not found: {0}")]
    NotFound(String),

    #[error("Config already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid config data: {0}")]
    InvalidData(String),

    #[error("Concurrent modification conflict: {0}")]
    ConcurrentModification(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Storage error: {0}")]
    Storage(#[from] tenrankai_storage::StorageError),

    #[error("DynamoDB error: {0}")]
    DynamoDb(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Backend not available: {0}")]
    BackendNotAvailable(String),
}

impl ConfigStorageError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::ConcurrentModification(_))
    }
}
