use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmailError {
    #[error("Email configuration error: {0}")]
    ConfigError(String),

    #[error("Email provider error: {0}")]
    ProviderError(String),

    #[error("Invalid email address: {0}")]
    InvalidEmail(String),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("AWS SDK error: {0}")]
    AwsError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Provider not implemented: {0}")]
    ProviderNotImplemented(String),

    #[error("Feature not supported by provider: {0}")]
    FeatureNotSupported(String),

    #[error("Rate limit exceeded for provider: {0}")]
    RateLimitExceeded(String),

    #[error("Authentication failed for provider: {0}")]
    AuthenticationFailed(String),
}
