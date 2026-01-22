use serde::{Deserialize, Serialize};

use super::types::{
    AppConfig, GallerySystemConfig, PostsSystemConfig, ServerConfig, StaticConfig, TemplateConfig,
};
use crate::{email, openai};

/// Root configuration structure
///
/// This is the top-level config structure that supports single-site configuration.
/// For multi-site deployments, use ConfigStorage (`config_storage = "config.d"`).
///
/// ## Single-Site Format
/// ```toml
/// [server]
/// host = "0.0.0.0"
/// port = 3000
///
/// [app]
/// name = "My Gallery"
/// config_storage = "config.d"  # For multi-site, point to ConfigStorage
///
/// # Single-site mode (when config_storage is not set):
/// [[galleries]]
/// name = "main"
/// url_prefix = "/gallery"
/// # ...
/// ```
///
/// ## Multi-Site Mode
/// Set `config_storage` in `[app]` to load sites from ConfigStorage:
/// ```toml
/// [app]
/// config_storage = "config.d"  # or "s3://bucket/prefix"
/// ```
/// Site configurations are then loaded from the ConfigStorage backend.
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

    /// Template configuration (single-site mode only)
    #[serde(default)]
    pub templates: Option<TemplateConfig>,

    /// Static files configuration (single-site mode only)
    #[serde(default)]
    pub static_files: Option<StaticConfig>,

    /// Galleries (single-site mode only)
    #[serde(default)]
    pub galleries: Option<Vec<GallerySystemConfig>>,

    /// Posts/blog systems (single-site mode only)
    #[serde(default)]
    pub posts: Option<Vec<PostsSystemConfig>>,
}

impl RootConfig {
    /// Check if this config uses ConfigStorage for multi-site
    pub fn uses_config_storage(&self) -> bool {
        self.app.config_storage.is_some()
    }
}

/// Convert RootConfig to legacy Config
impl From<RootConfig> for super::types::Config {
    fn from(config: RootConfig) -> Self {
        Self {
            server: config.server,
            app: config.app,
            email: config.email,
            openai: config.openai,
            templates: config.templates.unwrap_or_else(|| TemplateConfig {
                directories: vec!["templates".to_string()],
            }),
            static_files: config.static_files.unwrap_or_else(|| StaticConfig {
                directories: vec!["static".to_string()],
                use_redirects: false,
            }),
            galleries: config.galleries,
            posts: config.posts,
        }
    }
}

// Type alias for backward compatibility during transition
pub type MultiSiteConfig = RootConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uses_config_storage_false() {
        let config = RootConfig {
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
                config_storage: None,
            },
            email: None,
            openai: None,
            templates: Some(TemplateConfig {
                directories: vec!["templates".to_string()],
            }),
            static_files: Some(StaticConfig {
                directories: vec!["static".to_string()],
                use_redirects: false,
            }),
            galleries: None,
            posts: None,
        };

        assert!(!config.uses_config_storage());
    }

    #[test]
    fn test_uses_config_storage_true() {
        let config = RootConfig {
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
            templates: None,
            static_files: None,
            galleries: None,
            posts: None,
        };

        assert!(config.uses_config_storage());
    }
}
