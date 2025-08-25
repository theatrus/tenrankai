pub mod error;
pub mod handlers;
pub mod types;

pub use error::*;
pub use handlers::*;
pub use types::*;

use crate::Config;
use std::sync::Arc;
use webauthn_rs::{Webauthn, WebauthnBuilder};

pub fn create_webauthn(config: &Config) -> Result<Arc<Webauthn>, WebauthnError> {
    let base_url = config.app.base_url.as_ref().ok_or_else(|| {
        WebauthnError::ConfigError("base_url must be set for WebAuthn".to_string())
    })?;

    let url = url::Url::parse(base_url)
        .map_err(|e| WebauthnError::ConfigError(format!("Invalid base_url: {}", e)))?;

    let rp_id = url
        .host_str()
        .ok_or_else(|| WebauthnError::ConfigError("base_url must have a host".to_string()))?;

    let builder = WebauthnBuilder::new(rp_id, &url)?.rp_name(&config.app.name);

    Ok(Arc::new(builder.build()?))
}
