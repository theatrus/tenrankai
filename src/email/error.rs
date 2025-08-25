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
}
