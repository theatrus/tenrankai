use serde::{Deserialize, Serialize};

use super::types::{AppConfig, ServerConfig};
use crate::{email, openai};

/// Root configuration structure (config.toml)
///
/// This is the bootstrap configuration that specifies server settings and
/// points to ConfigStorage for site configuration.
///
/// ## Format
/// ```toml
/// [server]
/// host = "0.0.0.0"
/// port = 3000
///
/// [app]
/// name = "My Gallery"
/// config_storage = "config.d"  # Required: path to ConfigStorage
///
/// [email]
/// # Global email provider configuration
///
/// [openai]
/// # Global OpenAI configuration
/// ```
///
/// Site configurations (galleries, posts, static files, templates) are loaded
/// from ConfigStorage, not from this file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootConfig {
    /// Server configuration
    pub server: ServerConfig,

    /// Application configuration
    pub app: AppConfig,

    /// Email configuration (shared across all sites)
    #[serde(default)]
    pub email: Option<email::EmailConfig>,

    /// OpenAI configuration (shared across all sites)
    #[serde(default)]
    pub openai: Option<openai::OpenAIConfig>,
}

/// Convert RootConfig to Config
impl From<RootConfig> for super::types::Config {
    fn from(config: RootConfig) -> Self {
        Self {
            server: config.server,
            app: config.app,
            email: config.email,
            openai: config.openai,
            cache_queue: None, // RootConfig doesn't have cache_queue yet
        }
    }
}

// Type alias for backward compatibility during transition
pub type MultiSiteConfig = RootConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_config_to_config() {
        let root = RootConfig {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3000,
            },
            app: AppConfig {
                name: "Test".to_string(),
                log_level: crate::LogLevel::Info,
                aws_log_level: crate::LogLevel::Warn,
                cookie_secret: "test".to_string(),
                base_url: None,
                user_database: None,
                config_storage: Some("config.d".to_string()),
            },
            email: None,
            openai: None,
        };

        let config: super::super::types::Config = root.into();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.app.config_storage, Some("config.d".to_string()));
    }
}
