use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenAIError {
    #[error("OpenAI API error: {0}")]
    ApiError(String),

    #[error("Rate limit exceeded, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("Image encoding error: {0}")]
    ImageEncodingError(String),

    #[error("Response parsing error: {0}")]
    ResponseParseError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error("Metadata storage error: {0}")]
    MetadataStorageError(String),
}
