use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::*;

// Re-export UserPasskey from tenrankai-users crate
pub use tenrankai_users::UserPasskey;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyRegistrationState {
    pub username: String,
    pub state: PasskeyRegistration,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAuthenticationState {
    pub state: PasskeyAuthentication,
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterPasskeyRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartAuthenticationRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct PasskeyInfo {
    pub id: Uuid,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}
