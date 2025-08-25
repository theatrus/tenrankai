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
    match config {
        EmailProviderConfig::Ses(ses_config) => Ok(Arc::new(
            providers::ses::SesProvider::new(ses_config).await?,
        )),
    }
}
