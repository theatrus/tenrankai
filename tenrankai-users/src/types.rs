//! User storage types.
//!
//! This module defines the core user types used by all storage backends.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

/// A WebAuthn passkey credential for a user.
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

/// A user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub email: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passkeys: Vec<UserPasskey>,
}

impl User {
    pub fn add_passkey(&mut self, passkey: UserPasskey) {
        self.passkeys.push(passkey);
    }

    pub fn remove_passkey(&mut self, passkey_id: &uuid::Uuid) -> bool {
        let len_before = self.passkeys.len();
        self.passkeys.retain(|p| &p.id != passkey_id);
        self.passkeys.len() < len_before
    }

    pub fn get_passkey_mut(&mut self, passkey_id: &uuid::Uuid) -> Option<&mut UserPasskey> {
        self.passkeys.iter_mut().find(|p| &p.id == passkey_id)
    }

    pub fn has_passkeys(&self) -> bool {
        !self.passkeys.is_empty()
    }
}

/// A user with its username.
#[derive(Debug, Clone)]
pub struct UserWithUsername<'a> {
    pub username: &'a str,
    pub user: &'a User,
}

impl<'a> UserWithUsername<'a> {
    pub fn new(username: &'a str, user: &'a User) -> Self {
        Self { username, user }
    }

    pub fn email(&self) -> &str {
        &self.user.email
    }

    pub fn has_passkeys(&self) -> bool {
        self.user.has_passkeys()
    }
}

/// A mutable user with its username.
#[derive(Debug)]
pub struct UserWithUsernameMut<'a> {
    pub username: &'a str,
    pub user: &'a mut User,
}

impl<'a> UserWithUsernameMut<'a> {
    pub fn new(username: &'a str, user: &'a mut User) -> Self {
        Self { username, user }
    }
}
