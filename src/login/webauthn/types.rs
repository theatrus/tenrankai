use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPasskey {
    pub id: Uuid,
    pub name: String,
    pub credential: Passkey,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl UserPasskey {
    pub fn new(name: String, credential: Passkey) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            credential,
            created_at: chrono::Utc::now().timestamp(),
            last_used_at: None,
        }
    }

    pub fn update_last_used(&mut self) {
        self.last_used_at = Some(chrono::Utc::now().timestamp());
    }
}

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
