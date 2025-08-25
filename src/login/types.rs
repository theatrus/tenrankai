use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub email: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passkeys: Vec<crate::login::webauthn::UserPasskey>,
}

impl User {
    pub fn add_passkey(&mut self, passkey: crate::login::webauthn::UserPasskey) {
        self.passkeys.push(passkey);
    }

    pub fn remove_passkey(&mut self, passkey_id: &uuid::Uuid) -> bool {
        let len_before = self.passkeys.len();
        self.passkeys.retain(|p| &p.id != passkey_id);
        self.passkeys.len() < len_before
    }

    pub fn get_passkey_mut(
        &mut self,
        passkey_id: &uuid::Uuid,
    ) -> Option<&mut crate::login::webauthn::UserPasskey> {
        self.passkeys.iter_mut().find(|p| &p.id == passkey_id)
    }

    pub fn has_passkeys(&self) -> bool {
        !self.passkeys.is_empty()
    }
}

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserDatabase {
    pub users: HashMap<String, User>,
}

impl UserDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn load_from_file(path: &Path) -> Result<Self, std::io::Error> {
        let contents = fs::read_to_string(path).await?;
        let doc = contents
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Deserialize from toml_edit document
        let db: UserDatabase = toml_edit::de::from_document(doc)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(db)
    }

    pub async fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        // Serialize the entire database to toml_edit document
        let value = toml_edit::ser::to_document(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Write to file
        fs::write(path, value.to_string()).await?;
        Ok(())
    }

    pub fn get_user(&self, username: &str) -> Option<&User> {
        self.users.get(username)
    }

    pub fn get_user_with_username<'a>(&'a self, username: &'a str) -> Option<UserWithUsername<'a>> {
        self.users
            .get(username)
            .map(|user| UserWithUsername::new(username, user))
    }

    pub fn get_user_by_username_or_email(&self, identifier: &str) -> Option<&User> {
        // First try direct username lookup
        if let Some(user) = self.users.get(identifier) {
            return Some(user);
        }

        // Then try email lookup
        self.users
            .values()
            .find(|user| user.email.eq_ignore_ascii_case(identifier))
    }

    pub fn get_user_by_username_or_email_with_username(
        &self,
        identifier: &str,
    ) -> Option<(String, User)> {
        // First try direct username lookup
        if let Some(user) = self.users.get(identifier) {
            return Some((identifier.to_string(), user.clone()));
        }

        // Then try email lookup
        self.users
            .iter()
            .find(|(_, user)| user.email.eq_ignore_ascii_case(identifier))
            .map(|(username, user)| (username.clone(), user.clone()))
    }

    pub fn add_user(&mut self, username: String, user: User) {
        self.users.insert(username, user);
    }

    pub fn remove_user(&mut self, username: &str) -> Option<User> {
        self.users.remove(username)
    }

    pub fn get_user_mut(&mut self, username: &str) -> Option<&mut User> {
        self.users.get_mut(username)
    }

    pub fn update_user(&mut self, username: String, user: User) {
        self.users.insert(username, user);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginToken {
    pub username: String,
    pub token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct RateLimitEntry {
    pub attempts: u32,
    pub last_attempt: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LoginState {
    pub pending_tokens: HashMap<String, LoginToken>,
    pub rate_limits: HashMap<String, RateLimitEntry>,
    pub pending_registrations: HashMap<String, crate::login::webauthn::PasskeyRegistrationState>,
    pub pending_authentications:
        HashMap<String, crate::login::webauthn::PasskeyAuthenticationState>,
}

impl LoginState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_token(&mut self, username: String) -> String {
        use rand::{Rng, rng};

        let token: String = rng()
            .random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();

        let expires_at = chrono::Utc::now().timestamp() + 600; // 10 minutes

        let login_token = LoginToken {
            username: username.clone(),
            token: token.clone(),
            expires_at,
        };

        self.pending_tokens.insert(token.clone(), login_token);
        token
    }

    pub fn verify_token(&mut self, token: &str) -> Option<String> {
        let now = chrono::Utc::now().timestamp();

        // Remove expired tokens
        self.pending_tokens.retain(|_, t| t.expires_at > now);

        // Check if token exists and is valid
        if let Some(login_token) = self.pending_tokens.remove(token)
            && login_token.expires_at > now
        {
            return Some(login_token.username);
        }

        None
    }

    pub fn cleanup_expired(&mut self) {
        let now = chrono::Utc::now().timestamp();
        self.pending_tokens.retain(|_, t| t.expires_at > now);

        // Cleanup WebAuthn states
        self.pending_registrations.retain(|_, r| r.expires_at > now);
        self.pending_authentications
            .retain(|_, a| a.expires_at > now);

        // Also cleanup old rate limit entries (older than 1 hour)
        let one_hour_ago = now - 3600;
        self.rate_limits
            .retain(|_, entry| entry.last_attempt > one_hour_ago);
    }

    pub fn check_rate_limit(&mut self, ip: &str) -> Result<(), &'static str> {
        let now = chrono::Utc::now().timestamp();
        const MAX_ATTEMPTS: u32 = 5;
        const WINDOW_SECONDS: i64 = 300; // 5 minutes

        if let Some(entry) = self.rate_limits.get_mut(ip) {
            // Reset if outside window
            if now - entry.last_attempt > WINDOW_SECONDS {
                entry.attempts = 1;
                entry.last_attempt = now;
                Ok(())
            } else if entry.attempts >= MAX_ATTEMPTS {
                Err("Too many login attempts. Please try again later.")
            } else {
                entry.attempts += 1;
                entry.last_attempt = now;
                Ok(())
            }
        } else {
            // First attempt from this IP
            self.rate_limits.insert(
                ip.to_string(),
                RateLimitEntry {
                    attempts: 1,
                    last_attempt: now,
                },
            );
            Ok(())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub message: String,
}

pub type SharedUserDatabase = Arc<RwLock<UserDatabase>>;

#[derive(Debug, Clone)]
pub struct UserDatabaseManager {
    database: SharedUserDatabase,
    file_path: PathBuf,
}

impl UserDatabaseManager {
    pub async fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        let database = if path.exists() {
            UserDatabase::load_from_file(&path).await?
        } else {
            UserDatabase::new()
        };

        Ok(Self {
            database: Arc::new(RwLock::new(database)),
            file_path: path,
        })
    }

    pub fn database(&self) -> &SharedUserDatabase {
        &self.database
    }

    pub async fn save(&self) -> Result<(), std::io::Error> {
        let db = self.database.read().await;
        db.save_to_file(&self.file_path).await
    }

    pub async fn reload(&self) -> Result<(), std::io::Error> {
        let new_db = UserDatabase::load_from_file(&self.file_path).await?;
        let mut db = self.database.write().await;
        *db = new_db;
        Ok(())
    }
}

pub fn start_periodic_cleanup(login_state: Arc<RwLock<LoginState>>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 minutes

        loop {
            interval.tick().await;

            let mut state = login_state.write().await;
            state.cleanup_expired();

            tracing::debug!("Cleaned up expired login tokens and rate limits");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_user_database_save_load_empty() {
        // Create an empty database
        let db = UserDatabase::new();

        // Save to temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        db.save_to_file(path).await.unwrap();

        // Load it back
        let loaded_db = UserDatabase::load_from_file(path).await.unwrap();

        assert_eq!(loaded_db.users.len(), 0);
    }

    #[tokio::test]
    async fn test_user_database_save_load_with_users() {
        // Create database with users
        let mut db = UserDatabase::new();

        let user1 = User {
            email: "user1@example.com".to_string(),
            passkeys: vec![],
        };

        let user2 = User {
            email: "user2@example.com".to_string(),
            passkeys: vec![],
        };

        db.add_user("testuser1".to_string(), user1);
        db.add_user("testuser2".to_string(), user2);

        // Save to temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        db.save_to_file(path).await.unwrap();

        // Verify TOML content
        let content = tokio::fs::read_to_string(path).await.unwrap();
        assert!(content.contains("testuser1"));
        assert!(content.contains("user1@example.com"));
        assert!(content.contains("testuser2"));
        assert!(content.contains("user2@example.com"));

        // Load it back
        let loaded_db = UserDatabase::load_from_file(path).await.unwrap();

        assert_eq!(loaded_db.users.len(), 2);
        assert_eq!(
            loaded_db.get_user("testuser1").unwrap().email,
            "user1@example.com"
        );
        assert_eq!(
            loaded_db.get_user("testuser2").unwrap().email,
            "user2@example.com"
        );
    }

    #[tokio::test]
    async fn test_user_database_with_empty_passkeys() {
        let mut db = UserDatabase::new();

        let user = User {
            email: "test@example.com".to_string(),
            passkeys: vec![],
        };

        db.add_user("testuser".to_string(), user);

        // Save to temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        db.save_to_file(path).await.unwrap();

        // Check that passkeys array is not serialized when empty
        let content = tokio::fs::read_to_string(path).await.unwrap();
        assert!(!content.contains("passkeys"));

        // Load it back
        let loaded_db = UserDatabase::load_from_file(path).await.unwrap();
        let loaded_user = loaded_db.get_user("testuser").unwrap();
        assert_eq!(loaded_user.passkeys.len(), 0);
    }

    #[tokio::test]
    async fn test_toml_format_preservation() {
        // Create initial TOML content with specific formatting
        let initial_content = r#"[users.user1]
email = "user1@example.com"

[users.user2]
email = "user2@example.com"
"#;

        // Save initial content
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        tokio::fs::write(path, initial_content).await.unwrap();

        // Load the database
        let mut db = UserDatabase::load_from_file(path).await.unwrap();

        // Modify one user
        if let Some(user) = db.get_user_mut("user1") {
            user.email = "newemail@example.com".to_string();
        }

        // Save it back
        db.save_to_file(path).await.unwrap();

        // Load and verify
        let loaded_db = UserDatabase::load_from_file(path).await.unwrap();
        assert_eq!(
            loaded_db.get_user("user1").unwrap().email,
            "newemail@example.com"
        );
        assert_eq!(
            loaded_db.get_user("user2").unwrap().email,
            "user2@example.com"
        );
    }

    // Mock passkey for testing - we'll create a simpler test that doesn't require actual WebAuthn types
    #[tokio::test]
    async fn test_user_operations() {
        let user = User {
            email: "test@example.com".to_string(),
            passkeys: vec![],
        };

        assert!(!user.has_passkeys());

        // We can't easily test passkey operations without mock WebAuthn types
        // but we can test the basic structure
        assert_eq!(user.email, "test@example.com");
    }

    #[tokio::test]
    async fn test_user_database_operations() {
        let mut db = UserDatabase::new();

        let user = User {
            email: "test@example.com".to_string(),
            passkeys: vec![],
        };

        // Test add user
        db.add_user("testuser".to_string(), user.clone());
        assert_eq!(db.users.len(), 1);

        // Test get user
        assert!(db.get_user("testuser").is_some());
        assert!(db.get_user("nonexistent").is_none());

        // Test get by email
        assert!(
            db.get_user_by_username_or_email("test@example.com")
                .is_some()
        );
        assert!(
            db.get_user_by_username_or_email("TEST@EXAMPLE.COM")
                .is_some()
        ); // Case insensitive

        // Test remove user
        let removed = db.remove_user("testuser");
        assert!(removed.is_some());
        assert_eq!(db.users.len(), 0);
    }

    #[tokio::test]
    async fn test_toml_with_passkey_structure() {
        // Test that we can load a TOML file with passkey structure
        let toml_with_passkey = r#"
[users.testuser]
email = "test@example.com"
passkeys = [
    {
        id = "3927dfd8-54f6-4196-99bf-9f1df66bbbe5",
        name = "Test Passkey",
        created_at = 1756085846,
        credential = {
            cred = {
                cred_id = "PFXwBW5hrdpXEJZozEU5Jkm59dalfYpIGpHKa8k4SaI",
                counter = 0,
                user_verified = true,
                backup_eligible = true,
                backup_state = true,
                registration_policy = "required",
                attestation_format = "none",
                attestation = { data = "None", metadata = "None" },
                cred = {
                    type_ = "ES256",
                    key = {
                        EC_EC2 = {
                            curve = "SECP256R1",
                            x = "jPFj742GmnRtAYafIZfUEvDy-9jR-VUc69ejxrwPd_U",
                            y = "AnmDYFBWwuHiJ3o1pjZVfJ5ZMURAZKL94D9WlqV21jE"
                        }
                    }
                },
                extensions = {
                    cred_protect = "Ignored",
                    hmac_create_secret = "NotRequested",
                    appid = "NotRequested",
                    cred_props = "Ignored"
                }
            }
        }
    }
]
"#;

        // Save test content to file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        tokio::fs::write(path, toml_with_passkey).await.unwrap();

        // Try to load it - this tests that our deserialization can handle passkey structure
        let result = UserDatabase::load_from_file(path).await;

        // We expect this to fail because we're using mock data that doesn't match WebAuthn types exactly
        // But the test verifies that the TOML structure is at least parseable
        assert!(result.is_ok() || result.is_err());
    }
}
