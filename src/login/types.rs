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
    
    pub fn get_passkey_mut(&mut self, passkey_id: &uuid::Uuid) -> Option<&mut crate::login::webauthn::UserPasskey> {
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
        let db: UserDatabase = toml::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(db)
    }

    pub async fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        // For new files, use regular toml serialization
        if !path.exists() {
            let contents = toml::to_string_pretty(self)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            fs::write(path, contents).await?;
            return Ok(());
        }
        
        // For existing files, use toml_edit to preserve formatting
        let contents = fs::read_to_string(path).await?;
        let mut doc = contents.parse::<toml_edit::DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        
        // Update the document with our data
        if let Some(users_table) = doc.get_mut("users").and_then(|v| v.as_table_mut()) {
            // Update existing users and add new ones
            for (username, user) in &self.users {
                let user_toml = toml::to_string(user)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let user_value: toml_edit::Item = user_toml.parse()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                users_table[username] = user_value;
            }
            
            // Remove users that are no longer in the database
            let usernames: Vec<String> = users_table.iter().map(|(k, _)| k.to_string()).collect();
            for username in usernames {
                if !self.users.contains_key(&username) {
                    users_table.remove(&username);
                }
            }
        }
        
        fs::write(path, doc.to_string()).await?;
        Ok(())
    }

    pub fn get_user(&self, username: &str) -> Option<&User> {
        self.users.get(username)
    }
    
    pub fn get_user_with_username<'a>(&'a self, username: &'a str) -> Option<UserWithUsername<'a>> {
        self.users.get(username).map(|user| UserWithUsername::new(username, user))
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
    
    pub fn get_user_by_username_or_email_with_username(&self, identifier: &str) -> Option<(String, User)> {
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
    pub pending_authentications: HashMap<String, crate::login::webauthn::PasskeyAuthenticationState>,
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
        self.pending_authentications.retain(|_, a| a.expires_at > now);

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
