pub mod config;
pub mod error;
pub mod providers;
pub mod types;

pub use config::*;
pub use error::*;
pub use types::*;

use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn send_email(&self, message: EmailMessage) -> Result<(), EmailError>;
    fn name(&self) -> &str;
}

pub type DynEmailProvider = Arc<dyn EmailProvider>;

pub async fn create_provider(config: &EmailProviderConfig) -> Result<DynEmailProvider, EmailError> {
    let provider_type = config.provider_type();

    // Validate configuration before creating provider
    if provider_type.requires_credentials() && !config.has_required_credentials() {
        return Err(EmailError::InvalidConfiguration(format!(
            "Provider {} requires credentials but none were provided",
            provider_type.display_name()
        )));
    }

    match config {
        EmailProviderConfig::Null => Ok(Arc::new(providers::null::NullProvider::new())),
        EmailProviderConfig::Ses(ses_config) => Ok(Arc::new(
            providers::ses::SesProvider::new(ses_config).await?,
        )),
        EmailProviderConfig::Smtp(_) => {
            // TODO: Implement SMTP provider
            Err(EmailError::ProviderNotImplemented(
                "SMTP provider not yet implemented".to_string(),
            ))
        }
        EmailProviderConfig::SendGrid(_) => {
            // TODO: Implement SendGrid provider
            Err(EmailError::ProviderNotImplemented(
                "SendGrid provider not yet implemented".to_string(),
            ))
        }
        EmailProviderConfig::Mailgun(_) => {
            // TODO: Implement Mailgun provider
            Err(EmailError::ProviderNotImplemented(
                "Mailgun provider not yet implemented".to_string(),
            ))
        }
    }
}
