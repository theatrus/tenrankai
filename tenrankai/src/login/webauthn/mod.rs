pub mod error;
pub mod handlers;
pub mod types;

pub use error::*;
pub use handlers::*;
pub use types::*;

use std::sync::Arc;
use webauthn_rs::{Webauthn, WebauthnBuilder};

pub fn create_webauthn(base_url: &str, rp_name: &str) -> Result<Arc<Webauthn>, WebauthnError> {
    let url = url::Url::parse(base_url)
        .map_err(|e| WebauthnError::ConfigError(format!("Invalid base_url: {}", e)))?;

    let rp_id = url
        .host_str()
        .ok_or_else(|| WebauthnError::ConfigError("base_url must have a host".to_string()))?;

    let builder = WebauthnBuilder::new(rp_id, &url)?.rp_name(rp_name);

    Ok(Arc::new(builder.build()?))
}
