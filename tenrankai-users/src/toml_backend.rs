//! TOML file-based user storage backend.
//!
//! This backend stores users in a TOML file with automatic
//! file watching and reload support.

use super::UserStorage;
use super::error::UserStorageError;
use super::types::{User, UserPasskey};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, trace};
use uuid::Uuid;

/// TOML file structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlData {
    #[serde(default)]
    users: HashMap<String, User>,
}

/// TOML file-based user storage.
///
/// Provides automatic file watching and reload support.
/// Changes are persisted to disk immediately.
#[derive(Debug)]
pub struct TomlUserStorage {
    file_path: PathBuf,
    /// Site ID for multi-tenant isolation (used by SQL/DynamoDB backends,
    /// stored here for consistency but not used in TOML which is file-per-site)
    #[allow(dead_code)]
    site_id: String,
    data: Arc<RwLock<TomlData>>,
    last_modified: Arc<RwLock<Option<SystemTime>>>,
}

impl TomlUserStorage {
    /// Create a new TOML user storage.
    ///
    /// If the file doesn't exist, it will be created on first write.
    pub async fn new(path: impl Into<PathBuf>, site_id: String) -> Result<Self, UserStorageError> {
        let file_path = path.into();

        let (data, last_modified) = if file_path.exists() {
            let contents = fs::read_to_string(&file_path).await?;
            let doc = contents
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| UserStorageError::Serialization(e.to_string()))?;
            let data: TomlData = toml_edit::de::from_document(doc)
                .map_err(|e| UserStorageError::Serialization(e.to_string()))?;
            let modified = fs::metadata(&file_path).await?.modified().ok();
            (data, modified)
        } else {
            (TomlData::default(), None)
        };

        debug!(
            "Loaded TOML user storage from {:?} with {} users for site {}",
            file_path,
            data.users.len(),
            site_id
        );

        Ok(Self {
            file_path,
            site_id,
            data: Arc::new(RwLock::new(data)),
            last_modified: Arc::new(RwLock::new(last_modified)),
        })
    }

    /// Save the current data to disk.
    async fn save(&self) -> Result<(), UserStorageError> {
        let data = self.data.read().await;
        let toml_string = toml_edit::ser::to_string_pretty(&*data)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;
        drop(data);

        fs::write(&self.file_path, &toml_string).await?;

        // Update last modified time
        if let Ok(metadata) = fs::metadata(&self.file_path).await
            && let Ok(modified) = metadata.modified()
        {
            *self.last_modified.write().await = Some(modified);
        }

        trace!("Saved TOML user storage to {:?}", self.file_path);
        Ok(())
    }

    /// Check if the file has been modified externally and reload if needed.
    async fn check_reload(&self) -> Result<(), UserStorageError> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let current_modified = fs::metadata(&self.file_path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

        let last_modified = *self.last_modified.read().await;

        let needs_reload = match (current_modified, last_modified) {
            (Some(current), Some(last)) => current > last,
            (Some(_), None) => true,
            _ => false,
        };

        if needs_reload {
            debug!(
                "TOML user storage file {:?} modified externally, reloading",
                self.file_path
            );

            let contents = fs::read_to_string(&self.file_path).await?;
            let doc = contents
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| UserStorageError::Serialization(e.to_string()))?;
            let new_data: TomlData = toml_edit::de::from_document(doc)
                .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

            *self.data.write().await = new_data;
            *self.last_modified.write().await = current_modified;
        }

        Ok(())
    }
}

#[async_trait]
impl UserStorage for TomlUserStorage {
    async fn get_user(&self, username: &str) -> Result<Option<User>, UserStorageError> {
        self.check_reload().await?;
        let data = self.data.read().await;
        Ok(data.users.get(username).cloned())
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(String, User)>, UserStorageError> {
        self.check_reload().await?;
        let data = self.data.read().await;

        // Case-insensitive email search
        for (username, user) in &data.users {
            if user.email.eq_ignore_ascii_case(email) {
                return Ok(Some((username.clone(), user.clone())));
            }
        }

        Ok(None)
    }

    async fn list_users(&self) -> Result<Vec<(String, User)>, UserStorageError> {
        self.check_reload().await?;
        let data = self.data.read().await;
        Ok(data
            .users
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    async fn add_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        self.check_reload().await?;

        {
            let mut data = self.data.write().await;
            if data.users.contains_key(username) {
                return Err(UserStorageError::UserAlreadyExists(username.to_string()));
            }
            data.users.insert(username.to_string(), user.clone());
        }

        self.save().await
    }

    async fn update_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        self.check_reload().await?;

        {
            let mut data = self.data.write().await;
            if !data.users.contains_key(username) {
                return Err(UserStorageError::UserNotFound(username.to_string()));
            }
            data.users.insert(username.to_string(), user.clone());
        }

        self.save().await
    }

    async fn remove_user(&self, username: &str) -> Result<bool, UserStorageError> {
        self.check_reload().await?;

        let removed = {
            let mut data = self.data.write().await;
            data.users.remove(username).is_some()
        };

        if removed {
            self.save().await?;
        }

        Ok(removed)
    }

    fn backend_name(&self) -> &str {
        "toml"
    }

    async fn add_passkey(
        &self,
        username: &str,
        passkey: UserPasskey,
    ) -> Result<(), UserStorageError> {
        self.check_reload().await?;

        {
            let mut data = self.data.write().await;
            let user = data
                .users
                .get_mut(username)
                .ok_or_else(|| UserStorageError::UserNotFound(username.to_string()))?;
            user.add_passkey(passkey);
        }

        self.save().await
    }

    async fn remove_passkey(
        &self,
        username: &str,
        passkey_id: &Uuid,
    ) -> Result<bool, UserStorageError> {
        self.check_reload().await?;

        let removed = {
            let mut data = self.data.write().await;
            let user = data
                .users
                .get_mut(username)
                .ok_or_else(|| UserStorageError::UserNotFound(username.to_string()))?;
            user.remove_passkey(passkey_id)
        };

        if removed {
            self.save().await?;
        }

        Ok(removed)
    }

    async fn update_passkey_after_auth(
        &self,
        username: &str,
        passkey_id: &Uuid,
        auth_result: &webauthn_rs::prelude::AuthenticationResult,
    ) -> Result<(), UserStorageError> {
        self.check_reload().await?;

        {
            let mut data = self.data.write().await;
            let user = data
                .users
                .get_mut(username)
                .ok_or_else(|| UserStorageError::UserNotFound(username.to_string()))?;

            let passkey = user
                .get_passkey_mut(passkey_id)
                .ok_or_else(|| UserStorageError::PasskeyNotFound(passkey_id.to_string()))?;

            // Update the credential with counter from auth result
            passkey.credential.update_credential(auth_result);
            // Update last_used_at timestamp
            passkey.update_last_used();
        }

        self.save().await
    }

    async fn get_passkey_by_credential_id(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(String, UserPasskey)>, UserStorageError> {
        self.check_reload().await?;
        let data = self.data.read().await;

        for (username, user) in &data.users {
            for passkey in &user.passkeys {
                if passkey.credential.cred_id().as_ref() == credential_id {
                    return Ok(Some((username.clone(), passkey.clone())));
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (TempDir, TomlUserStorage) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("users.toml");
        std::fs::write(&path, "[users]\n").unwrap();

        let storage = TomlUserStorage::new(path, "test".to_string())
            .await
            .unwrap();
        (dir, storage)
    }

    #[tokio::test]
    async fn test_add_and_get_user() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();

        let retrieved = storage.get_user("alice").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().email, "alice@example.com");
    }

    #[tokio::test]
    async fn test_add_duplicate_user_fails() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();
        let result = storage.add_user("alice", &user).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UserStorageError::UserAlreadyExists(_)
        ));
    }

    #[tokio::test]
    async fn test_get_user_by_email() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "Alice@Example.COM".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();

        // Case-insensitive search
        let result = storage
            .get_user_by_email("alice@example.com")
            .await
            .unwrap();
        assert!(result.is_some());
        let (username, _) = result.unwrap();
        assert_eq!(username, "alice");
    }

    #[tokio::test]
    async fn test_update_user() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();

        let updated_user = User {
            email: "alice.new@example.com".to_string(),
            passkeys: vec![],
        };

        storage.update_user("alice", &updated_user).await.unwrap();

        let retrieved = storage.get_user("alice").await.unwrap().unwrap();
        assert_eq!(retrieved.email, "alice.new@example.com");
    }

    #[tokio::test]
    async fn test_update_nonexistent_user_fails() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        let result = storage.update_user("alice", &user).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UserStorageError::UserNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_remove_user() {
        let (_dir, storage) = create_test_storage().await;

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();

        let removed = storage.remove_user("alice").await.unwrap();
        assert!(removed);

        let removed_again = storage.remove_user("alice").await.unwrap();
        assert!(!removed_again);

        let retrieved = storage.get_user("alice").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_list_users() {
        let (_dir, storage) = create_test_storage().await;

        storage
            .add_user(
                "alice",
                &User {
                    email: "alice@example.com".to_string(),
                    passkeys: vec![],
                },
            )
            .await
            .unwrap();

        storage
            .add_user(
                "bob",
                &User {
                    email: "bob@example.com".to_string(),
                    passkeys: vec![],
                },
            )
            .await
            .unwrap();

        let users = storage.list_users().await.unwrap();
        assert_eq!(users.len(), 2);

        let usernames: Vec<_> = users.iter().map(|(u, _)| u.as_str()).collect();
        assert!(usernames.contains(&"alice"));
        assert!(usernames.contains(&"bob"));
    }

    #[tokio::test]
    async fn test_backend_name() {
        let (_dir, storage) = create_test_storage().await;
        assert_eq!(storage.backend_name(), "toml");
    }

    #[tokio::test]
    async fn test_file_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("users.toml");
        std::fs::write(&path, "[users]\n").unwrap();

        // Create storage and add user
        {
            let storage = TomlUserStorage::new(path.clone(), "test".to_string())
                .await
                .unwrap();
            storage
                .add_user(
                    "alice",
                    &User {
                        email: "alice@example.com".to_string(),
                        passkeys: vec![],
                    },
                )
                .await
                .unwrap();
        }

        // Create new storage instance and verify persistence
        {
            let storage = TomlUserStorage::new(path, "test".to_string())
                .await
                .unwrap();
            let user = storage.get_user("alice").await.unwrap();
            assert!(user.is_some());
            assert_eq!(user.unwrap().email, "alice@example.com");
        }
    }
}
