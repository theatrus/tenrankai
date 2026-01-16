//! SQL database user storage backend (SQLite/PostgreSQL).
//!
//! This backend stores users in a SQL database with normalized
//! schema for users and passkeys.

use super::UserStorage;
use super::error::UserStorageError;
use super::types::{User, UserPasskey};
use async_trait::async_trait;
use sqlx::{AnyPool, Row};
use tracing::{debug, info};
use uuid::Uuid;

/// Helper type aliases for the Any driver.
type AnyRow = sqlx::any::AnyRow;
type AnyConnection = sqlx::pool::PoolConnection<sqlx::any::Any>;
type AnyPoolOptions = sqlx::any::AnyPoolOptions;

/// SQL database user storage.
///
/// Supports SQLite and PostgreSQL through sqlx's `Any` driver.
/// Users and passkeys are stored in separate normalized tables.
#[derive(Debug, Clone)]
pub struct SqlUserStorage {
    pool: AnyPool,
    site_id: String,
}

impl SqlUserStorage {
    /// Create a new SQL user storage.
    ///
    /// The connection string determines the database type:
    /// - `sqlite:///path/to/db.sqlite` - SQLite database
    /// - `postgresql://user:pass@host/db` - PostgreSQL database
    ///
    /// Tables are automatically created if they don't exist.
    pub async fn new(connection_string: &str, site_id: String) -> Result<Self, UserStorageError> {
        // Install the Any driver for the connection string's scheme
        Self::install_driver(connection_string)?;

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(connection_string)
            .await
            .map_err(|e| UserStorageError::Database(e.to_string()))?;

        let storage = Self { pool, site_id };

        // Run migrations
        storage.run_migrations().await?;

        info!(
            "Connected to SQL user storage: {} (site: {})",
            Self::mask_connection_string(connection_string),
            storage.site_id
        );

        Ok(storage)
    }

    /// Install the appropriate sqlx driver based on the connection string.
    fn install_driver(connection_string: &str) -> Result<(), UserStorageError> {
        if connection_string.starts_with("sqlite:")
            || connection_string.starts_with("postgres://")
            || connection_string.starts_with("postgresql://")
        {
            sqlx::any::install_default_drivers();
            Ok(())
        } else {
            Err(UserStorageError::InvalidUrl(format!(
                "Unsupported SQL database scheme: {}",
                connection_string.split(':').next().unwrap_or("unknown")
            )))
        }
    }

    /// Mask sensitive parts of connection string for logging.
    fn mask_connection_string(s: &str) -> String {
        if let Some(at_pos) = s.find('@')
            && let Some(scheme_end) = s.find("://")
        {
            let scheme = &s[..scheme_end + 3];
            let after_at = &s[at_pos..];
            return format!("{}***{}", scheme, after_at);
        }
        s.to_string()
    }

    /// Run database migrations to create tables.
    async fn run_migrations(&self) -> Result<(), UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        // Create users table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                site_id TEXT NOT NULL,
                username TEXT NOT NULL,
                email TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (site_id, username)
            )
            "#,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to create users table: {}", e)))?;

        // Create index on email (case-insensitive lookup)
        // Note: SQLite uses LOWER(), PostgreSQL would use LOWER() too
        let _ = sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_users_email
            ON users(site_id, LOWER(email))
            "#,
        )
        .execute(&mut *conn)
        .await;

        // Create passkeys table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS passkeys (
                id TEXT PRIMARY KEY,
                site_id TEXT NOT NULL,
                username TEXT NOT NULL,
                name TEXT NOT NULL,
                credential_id BLOB NOT NULL,
                credential_json TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                last_used_at TEXT,
                FOREIGN KEY (site_id, username) REFERENCES users(site_id, username) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to create passkeys table: {}", e)))?;

        // Create index on credential_id for WebAuthn lookups
        let _ = sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_passkeys_credential_id
            ON passkeys(credential_id)
            "#,
        )
        .execute(&mut *conn)
        .await;

        // Create index on user lookup
        let _ = sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_passkeys_user
            ON passkeys(site_id, username)
            "#,
        )
        .execute(&mut *conn)
        .await;

        debug!("SQL user storage migrations complete");
        Ok(())
    }

    /// Load passkeys for a user.
    async fn load_passkeys(&self, username: &str) -> Result<Vec<UserPasskey>, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let rows: Vec<AnyRow> = sqlx::query(
            r#"
            SELECT id, name, credential_json, created_at, last_used_at
            FROM passkeys
            WHERE site_id = ? AND username = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(&self.site_id)
        .bind(username)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to load passkeys: {}", e)))?;

        let mut passkeys = Vec::with_capacity(rows.len());
        for row in rows {
            let id_str: String = row.get("id");
            let id = Uuid::parse_str(&id_str)
                .map_err(|e| UserStorageError::Database(format!("Invalid passkey UUID: {}", e)))?;

            let name: String = row.get("name");
            let credential_json: String = row.get("credential_json");
            let created_at: String = row.get("created_at");
            let last_used_at: Option<String> = row.get("last_used_at");

            let credential: webauthn_rs::prelude::Passkey = serde_json::from_str(&credential_json)
                .map_err(|e| {
                    UserStorageError::Database(format!("Failed to deserialize passkey: {}", e))
                })?;

            // Parse timestamps
            let created_at_ts =
                chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.and_utc().timestamp())
                    .unwrap_or(0);

            let last_used_at_ts = last_used_at.and_then(|s: String| {
                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.and_utc().timestamp())
                    .ok()
            });

            passkeys.push(UserPasskey {
                id,
                name,
                credential,
                created_at: created_at_ts,
                last_used_at: last_used_at_ts,
            });
        }

        Ok(passkeys)
    }
}

#[async_trait]
impl UserStorage for SqlUserStorage {
    async fn get_user(&self, username: &str) -> Result<Option<User>, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let row: Option<AnyRow> = sqlx::query(
            r#"
            SELECT email FROM users
            WHERE site_id = ? AND username = ?
            "#,
        )
        .bind(&self.site_id)
        .bind(username)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to get user: {}", e)))?;

        match row {
            Some(row) => {
                let email: String = row.get("email");
                let passkeys = self.load_passkeys(username).await?;

                Ok(Some(User { email, passkeys }))
            }
            None => Ok(None),
        }
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(String, User)>, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let row: Option<AnyRow> = sqlx::query(
            r#"
            SELECT username, email FROM users
            WHERE site_id = ? AND LOWER(email) = LOWER(?)
            "#,
        )
        .bind(&self.site_id)
        .bind(email)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to get user by email: {}", e)))?;

        match row {
            Some(row) => {
                let username: String = row.get("username");
                let email: String = row.get("email");
                let passkeys = self.load_passkeys(&username).await?;

                Ok(Some((username, User { email, passkeys })))
            }
            None => Ok(None),
        }
    }

    async fn list_users(&self) -> Result<Vec<(String, User)>, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let rows: Vec<AnyRow> = sqlx::query(
            r#"
            SELECT username, email FROM users
            WHERE site_id = ?
            ORDER BY username ASC
            "#,
        )
        .bind(&self.site_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to list users: {}", e)))?;

        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let username: String = row.get("username");
            let email: String = row.get("email");
            let passkeys = self.load_passkeys(&username).await?;

            users.push((username, User { email, passkeys }));
        }

        Ok(users)
    }

    async fn add_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        // Check if user already exists
        let existing: Option<AnyRow> =
            sqlx::query("SELECT 1 FROM users WHERE site_id = ? AND username = ?")
                .bind(&self.site_id)
                .bind(username)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| UserStorageError::Database(e.to_string()))?;

        if existing.is_some() {
            return Err(UserStorageError::UserAlreadyExists(username.to_string()));
        }

        sqlx::query(
            r#"
            INSERT INTO users (site_id, username, email)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&self.site_id)
        .bind(username)
        .bind(&user.email)
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to add user: {}", e)))?;

        // Add any passkeys
        for passkey in &user.passkeys {
            self.add_passkey(username, passkey.clone()).await?;
        }

        debug!("Added user {} to SQL storage", username);
        Ok(())
    }

    async fn update_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let result = sqlx::query(
            r#"
            UPDATE users SET email = ?, updated_at = datetime('now')
            WHERE site_id = ? AND username = ?
            "#,
        )
        .bind(&user.email)
        .bind(&self.site_id)
        .bind(username)
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to update user: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(UserStorageError::UserNotFound(username.to_string()));
        }

        debug!("Updated user {} in SQL storage", username);
        Ok(())
    }

    async fn remove_user(&self, username: &str) -> Result<bool, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        // Passkeys will be deleted by CASCADE
        let result = sqlx::query("DELETE FROM users WHERE site_id = ? AND username = ?")
            .bind(&self.site_id)
            .bind(username)
            .execute(&mut *conn)
            .await
            .map_err(|e| UserStorageError::Database(format!("Failed to remove user: {}", e)))?;

        let removed = result.rows_affected() > 0;
        if removed {
            debug!("Removed user {} from SQL storage", username);
        }

        Ok(removed)
    }

    fn backend_name(&self) -> &str {
        "sql"
    }

    async fn add_passkey(
        &self,
        username: &str,
        passkey: UserPasskey,
    ) -> Result<(), UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let credential_json = serde_json::to_string(&passkey.credential)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        let credential_id = passkey.credential.cred_id().as_ref();

        sqlx::query(
            r#"
            INSERT INTO passkeys (id, site_id, username, name, credential_id, credential_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(passkey.id.to_string())
        .bind(&self.site_id)
        .bind(username)
        .bind(&passkey.name)
        .bind(credential_id)
        .bind(&credential_json)
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to add passkey: {}", e)))?;

        debug!("Added passkey {} for user {}", passkey.id, username);
        Ok(())
    }

    async fn remove_passkey(
        &self,
        username: &str,
        passkey_id: &Uuid,
    ) -> Result<bool, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let result =
            sqlx::query("DELETE FROM passkeys WHERE site_id = ? AND username = ? AND id = ?")
                .bind(&self.site_id)
                .bind(username)
                .bind(passkey_id.to_string())
                .execute(&mut *conn)
                .await
                .map_err(|e| {
                    UserStorageError::Database(format!("Failed to remove passkey: {}", e))
                })?;

        let removed = result.rows_affected() > 0;
        if removed {
            debug!("Removed passkey {} for user {}", passkey_id, username);
        }

        Ok(removed)
    }

    async fn update_passkey_after_auth(
        &self,
        username: &str,
        passkey_id: &Uuid,
        auth_result: &webauthn_rs::prelude::AuthenticationResult,
    ) -> Result<(), UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        // First, get the current passkey credential
        let row: Option<AnyRow> = sqlx::query(
            "SELECT credential_json FROM passkeys WHERE site_id = ? AND username = ? AND id = ?",
        )
        .bind(&self.site_id)
        .bind(username)
        .bind(passkey_id.to_string())
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(e.to_string()))?;

        let row = row.ok_or_else(|| UserStorageError::PasskeyNotFound(passkey_id.to_string()))?;
        let credential_json: String = row.get("credential_json");

        // Deserialize, update, and serialize
        let mut credential: webauthn_rs::prelude::Passkey = serde_json::from_str(&credential_json)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        credential.update_credential(auth_result);

        let updated_json = serde_json::to_string(&credential)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        // Update the database
        sqlx::query(
            r#"
            UPDATE passkeys
            SET credential_json = ?, last_used_at = datetime('now')
            WHERE site_id = ? AND username = ? AND id = ?
            "#,
        )
        .bind(&updated_json)
        .bind(&self.site_id)
        .bind(username)
        .bind(passkey_id.to_string())
        .execute(&mut *conn)
        .await
        .map_err(|e| UserStorageError::Database(format!("Failed to update passkey: {}", e)))?;

        debug!(
            "Updated passkey {} after auth for user {}",
            passkey_id, username
        );
        Ok(())
    }

    async fn get_passkey_by_credential_id(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(String, UserPasskey)>, UserStorageError> {
        let mut conn: AnyConnection = self.pool.acquire().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to acquire connection: {}", e))
        })?;

        let row: Option<AnyRow> = sqlx::query(
            r#"
            SELECT id, username, name, credential_json, created_at, last_used_at
            FROM passkeys
            WHERE site_id = ? AND credential_id = ?
            "#,
        )
        .bind(&self.site_id)
        .bind(credential_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| {
            UserStorageError::Database(format!("Failed to get passkey by credential_id: {}", e))
        })?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let id = Uuid::parse_str(&id_str).map_err(|e| {
                    UserStorageError::Database(format!("Invalid passkey UUID: {}", e))
                })?;

                let username: String = row.get("username");
                let name: String = row.get("name");
                let credential_json: String = row.get("credential_json");
                let created_at: String = row.get("created_at");
                let last_used_at: Option<String> = row.get("last_used_at");

                let credential: webauthn_rs::prelude::Passkey =
                    serde_json::from_str(&credential_json).map_err(|e| {
                        UserStorageError::Database(format!("Failed to deserialize passkey: {}", e))
                    })?;

                let created_at_ts =
                    chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| dt.and_utc().timestamp())
                        .unwrap_or(0);

                let last_used_at_ts = last_used_at.and_then(|s: String| {
                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| dt.and_utc().timestamp())
                        .ok()
                });

                let passkey = UserPasskey {
                    id,
                    name,
                    credential,
                    created_at: created_at_ts,
                    last_used_at: last_used_at_ts,
                };

                Ok(Some((username, passkey)))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a unique site_id for test isolation
    fn test_site_id(suffix: &str) -> String {
        format!("test_{}", suffix)
    }

    #[tokio::test]
    async fn test_sql_storage_sqlite_basic() {
        // Use in-memory SQLite for testing with shared cache
        let storage =
            SqlUserStorage::new("sqlite:file::memory:?cache=shared", test_site_id("basic"))
                .await
                .unwrap();

        assert_eq!(storage.backend_name(), "sql");

        // Test add user
        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };
        storage.add_user("alice", &user).await.unwrap();

        // Test get user
        let retrieved = storage.get_user("alice").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().email, "alice@example.com");

        // Test get by email
        let by_email = storage
            .get_user_by_email("alice@example.com")
            .await
            .unwrap();
        assert!(by_email.is_some());
        let (username, _) = by_email.unwrap();
        assert_eq!(username, "alice");

        // Test case-insensitive email
        let by_email_upper = storage
            .get_user_by_email("ALICE@EXAMPLE.COM")
            .await
            .unwrap();
        assert!(by_email_upper.is_some());
    }

    #[tokio::test]
    async fn test_sql_storage_duplicate_user() {
        let storage = SqlUserStorage::new(
            "sqlite:file::memory:?cache=shared",
            test_site_id("duplicate"),
        )
        .await
        .unwrap();

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        storage.add_user("alice", &user).await.unwrap();

        // Try to add duplicate
        let result = storage.add_user("alice", &user).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UserStorageError::UserAlreadyExists(_)
        ));
    }

    #[tokio::test]
    async fn test_sql_storage_update_user() {
        let storage =
            SqlUserStorage::new("sqlite:file::memory:?cache=shared", test_site_id("update"))
                .await
                .unwrap();

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };
        storage.add_user("alice", &user).await.unwrap();

        // Update email
        let updated = User {
            email: "alice.new@example.com".to_string(),
            passkeys: vec![],
        };
        storage.update_user("alice", &updated).await.unwrap();

        let retrieved = storage.get_user("alice").await.unwrap().unwrap();
        assert_eq!(retrieved.email, "alice.new@example.com");
    }

    #[tokio::test]
    async fn test_sql_storage_update_nonexistent() {
        let storage = SqlUserStorage::new(
            "sqlite:file::memory:?cache=shared",
            test_site_id("update_nonexistent"),
        )
        .await
        .unwrap();

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        let result = storage.update_user("nonexistent", &user).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UserStorageError::UserNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_sql_storage_remove_user() {
        let storage =
            SqlUserStorage::new("sqlite:file::memory:?cache=shared", test_site_id("remove"))
                .await
                .unwrap();

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };
        storage.add_user("alice", &user).await.unwrap();

        let removed = storage.remove_user("alice").await.unwrap();
        assert!(removed);

        // Should be gone
        let retrieved = storage.get_user("alice").await.unwrap();
        assert!(retrieved.is_none());

        // Remove again should return false
        let removed_again = storage.remove_user("alice").await.unwrap();
        assert!(!removed_again);
    }

    #[tokio::test]
    async fn test_sql_storage_list_users() {
        let storage =
            SqlUserStorage::new("sqlite:file::memory:?cache=shared", test_site_id("list"))
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
    async fn test_sql_storage_site_isolation() {
        // Two storage instances for different sites sharing the same DB
        // With shared cache, they share the same in-memory database
        let storage1 = SqlUserStorage::new(
            "sqlite:file::memory:?cache=shared",
            test_site_id("isolation_site1"),
        )
        .await
        .unwrap();

        let storage2 = SqlUserStorage::new(
            "sqlite:file::memory:?cache=shared",
            test_site_id("isolation_site2"),
        )
        .await
        .unwrap();

        let user = User {
            email: "alice@example.com".to_string(),
            passkeys: vec![],
        };

        // Add alice to site1
        storage1.add_user("alice", &user).await.unwrap();

        // Verify alice exists in site1
        let alice1 = storage1.get_user("alice").await.unwrap();
        assert!(alice1.is_some());

        // Verify alice does NOT exist in site2 (different site_id)
        let alice2 = storage2.get_user("alice").await.unwrap();
        assert!(alice2.is_none());

        // We can add alice to site2 with the same username
        storage2.add_user("alice", &user).await.unwrap();
        let alice2 = storage2.get_user("alice").await.unwrap();
        assert!(alice2.is_some());
    }
}
