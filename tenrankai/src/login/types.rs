use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
