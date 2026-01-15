//! Pluggable user storage backends for authentication.
//!
//! This module provides a trait-based abstraction for user storage,
//! allowing different backends (TOML files, SQL databases, DynamoDB)
//! to be used interchangeably.
//!
//! # Example
//!
//! ```ignore
//! use tenrankai::user_storage::{create_user_storage, UserStorage};
//!
//! // Create storage from URL
//! let storage = create_user_storage("users.toml", "default").await?;
//!
//! // Use the storage
//! if let Some(user) = storage.get_user("alice").await? {
//!     println!("Found user: {}", user.email);
//! }
//! ```

#[cfg(feature = "user-storage-dynamodb")]
mod dynamodb;
mod error;
#[cfg(feature = "user-storage-sql")]
mod sql;
mod toml_backend;
mod types;
mod url;

#[cfg(feature = "user-storage-dynamodb")]
pub use dynamodb::DynamoUserStorage;
pub use error::UserStorageError;
#[cfg(feature = "user-storage-sql")]
pub use sql::SqlUserStorage;
pub use toml_backend::TomlUserStorage;
pub use types::{User, UserPasskey, UserWithUsername, UserWithUsernameMut};
pub use url::UserStorageUrl;

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// Type alias for a thread-safe, dynamically-dispatched user storage.
pub type DynUserStorage = Arc<dyn UserStorage>;

/// Trait for pluggable user storage backends.
///
/// Implementations must be thread-safe (`Send + Sync`) and support
/// async operations. The `site_id` is baked into the storage instance
/// at construction time, so all operations are automatically scoped
/// to the correct site.
#[async_trait]
pub trait UserStorage: Send + Sync + 'static {
    /// Get a user by username (site-scoped).
    async fn get_user(&self, username: &str) -> Result<Option<User>, UserStorageError>;

    /// Get a user by email address (site-scoped, case-insensitive).
    ///
    /// Returns the username along with the user data.
    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(String, User)>, UserStorageError>;

    /// Get a user by username or email address (site-scoped).
    ///
    /// First tries username lookup, then falls back to email lookup.
    /// Returns the username along with the user data.
    async fn get_user_by_username_or_email(
        &self,
        identifier: &str,
    ) -> Result<Option<(String, User)>, UserStorageError> {
        // First try direct username lookup
        if let Some(user) = self.get_user(identifier).await? {
            return Ok(Some((identifier.to_string(), user)));
        }

        // Then try email lookup
        self.get_user_by_email(identifier).await
    }

    /// List all users (site-scoped).
    ///
    /// Returns a vector of (username, user) tuples.
    async fn list_users(&self) -> Result<Vec<(String, User)>, UserStorageError>;

    /// Add a new user.
    ///
    /// Returns an error if a user with the same username already exists.
    async fn add_user(&self, username: &str, user: &User) -> Result<(), UserStorageError>;

    /// Update an existing user.
    ///
    /// Returns an error if the user doesn't exist.
    async fn update_user(&self, username: &str, user: &User) -> Result<(), UserStorageError>;

    /// Remove a user.
    ///
    /// Returns `true` if the user was removed, `false` if they didn't exist.
    async fn remove_user(&self, username: &str) -> Result<bool, UserStorageError>;

    /// Get the backend name for logging purposes.
    fn backend_name(&self) -> &str;

    // Passkey-specific operations

    /// Add a passkey to a user.
    async fn add_passkey(
        &self,
        username: &str,
        passkey: UserPasskey,
    ) -> Result<(), UserStorageError>;

    /// Remove a passkey from a user.
    ///
    /// Returns `true` if the passkey was removed, `false` if it didn't exist.
    async fn remove_passkey(
        &self,
        username: &str,
        passkey_id: &Uuid,
    ) -> Result<bool, UserStorageError>;

    /// Update a passkey after successful authentication.
    ///
    /// This updates the counter and last-used timestamp based on the authentication result.
    async fn update_passkey_after_auth(
        &self,
        username: &str,
        passkey_id: &Uuid,
        auth_result: &webauthn_rs::prelude::AuthenticationResult,
    ) -> Result<(), UserStorageError>;

    /// Find a passkey by its credential ID (cross-user lookup for WebAuthn).
    ///
    /// Returns the username and passkey if found.
    async fn get_passkey_by_credential_id(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(String, UserPasskey)>, UserStorageError>;
}

/// Create a user storage instance from a URL string.
///
/// # URL Formats
///
/// - `users.toml` or `file:///path/to/users.toml` - TOML file backend
/// - `sqlite:///path/to/users.db` - SQLite database (requires `user-storage-sql` feature)
/// - `postgresql://user:pass@host/db` - PostgreSQL (requires `user-storage-sql` feature)
/// - `dynamodb://table-name?region=us-west-2` - DynamoDB (requires `user-storage-dynamodb` feature)
///
/// # Arguments
///
/// * `url` - The storage URL
/// * `site_id` - The site identifier for multi-tenant isolation
pub async fn create_user_storage(
    url: &str,
    site_id: &str,
) -> Result<DynUserStorage, UserStorageError> {
    let parsed = UserStorageUrl::parse(url)?;
    parsed.into_storage(site_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_toml_storage() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("users.toml");
        std::fs::write(&path, "[users]\n").unwrap();

        let storage = create_user_storage(path.to_str().unwrap(), "default")
            .await
            .unwrap();

        assert_eq!(storage.backend_name(), "toml");
    }

    #[tokio::test]
    async fn test_get_user_by_username_or_email_default_impl() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("users.toml");
        std::fs::write(
            &path,
            r#"[users.alice]
email = "alice@example.com"
"#,
        )
        .unwrap();

        let storage = create_user_storage(path.to_str().unwrap(), "default")
            .await
            .unwrap();

        // Test username lookup
        let result = storage
            .get_user_by_username_or_email("alice")
            .await
            .unwrap();
        assert!(result.is_some());
        let (username, user) = result.unwrap();
        assert_eq!(username, "alice");
        assert_eq!(user.email, "alice@example.com");

        // Test email lookup
        let result = storage
            .get_user_by_username_or_email("alice@example.com")
            .await
            .unwrap();
        assert!(result.is_some());
        let (username, _) = result.unwrap();
        assert_eq!(username, "alice");

        // Test non-existent user
        let result = storage.get_user_by_username_or_email("bob").await.unwrap();
        assert!(result.is_none());
    }
}
