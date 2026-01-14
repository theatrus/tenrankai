use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::types::{
    AppConfig, GallerySystemConfig, PostsSystemConfig, ServerConfig, StaticConfig, TemplateConfig,
};
use crate::email::SiteEmailConfig;
use crate::{email, openai};

/// Multi-site configuration
///
/// This is the top-level config structure that supports both legacy single-site
/// configuration and new multi-site configuration.
///
/// ## Legacy Format (single site)
/// ```toml
/// [server]
/// host = "0.0.0.0"
/// port = 3000
///
/// [[galleries]]
/// name = "main"
/// url_prefix = "/gallery"
/// # ...
/// ```
///
/// ## Multi-Site Format
/// ```toml
/// [server]
/// host = "0.0.0.0"
/// port = 3000
///
/// [sites.default]
/// hostnames = ["*"]
///
/// [[sites.default.galleries]]
/// name = "main"
/// url_prefix = "/gallery"
/// # ...
///
/// [sites.photos]
/// hostnames = ["photos.example.com"]
///
/// [[sites.photos.galleries]]
/// name = "portfolio"
/// url_prefix = "/"
/// # ...
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MultiSiteConfig {
    /// Server configuration (shared across all sites)
    pub server: ServerConfig,

    /// Application configuration (shared across all sites)
    pub app: AppConfig,

    /// Email configuration (shared across all sites)
    #[serde(default)]
    pub email: Option<email::EmailConfig>,

    /// OpenAI configuration (shared across all sites)
    #[serde(default)]
    pub openai: Option<openai::OpenAIConfig>,

    /// Site-specific configurations
    /// If empty or None, falls back to legacy config fields
    #[serde(default)]
    pub sites: Option<HashMap<String, SiteConfigSection>>,

    // Legacy fields - used when sites is None/empty for backward compatibility
    #[serde(default)]
    pub templates: Option<TemplateConfig>,
    #[serde(default)]
    pub static_files: Option<StaticConfig>,
    #[serde(default)]
    pub galleries: Option<Vec<GallerySystemConfig>>,
    #[serde(default)]
    pub posts: Option<Vec<PostsSystemConfig>>,
}

/// Configuration section for a single site
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfigSection {
    /// Hostnames that route to this site
    /// Special values:
    /// - `"*"` - catch-all default site
    /// - `"*.example.com"` - wildcard subdomain matching
    #[serde(default)]
    pub hostnames: Vec<String>,

    /// Template configuration for this site
    pub templates: TemplateConfig,

    /// Static files configuration for this site
    pub static_files: StaticConfig,

    /// Galleries for this site
    #[serde(default)]
    pub galleries: Option<Vec<GallerySystemConfig>>,

    /// Posts/blog systems for this site
    #[serde(default)]
    pub posts: Option<Vec<PostsSystemConfig>>,

    /// User database path for this site's authentication
    #[serde(default)]
    pub user_database: Option<PathBuf>,

    /// Per-site email sender configuration (from address, name, reply-to)
    /// The email provider is shared globally, but each site can have its own sender identity
    #[serde(default)]
    pub email: Option<SiteEmailConfig>,
}

impl MultiSiteConfig {
    /// Check if this config uses multi-site format
    pub fn is_multi_site(&self) -> bool {
        self.sites.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Get site configurations
    ///
    /// If using legacy format, returns a single "default" site created from legacy fields.
    /// If using multi-site format, returns all configured sites.
    pub fn get_site_configs(&self) -> HashMap<String, SiteConfigSection> {
        if self.is_multi_site() {
            self.sites.clone().unwrap_or_default()
        } else {
            // Migrate legacy config to a single default site
            let email = self.email.as_ref().map(SiteEmailConfig::from);

            let mut sites = HashMap::new();
            sites.insert(
                "default".to_string(),
                SiteConfigSection {
                    hostnames: vec!["*".to_string()],
                    templates: self.templates.clone().unwrap_or_else(|| TemplateConfig {
                        directories: vec!["templates".to_string()],
                    }),
                    static_files: self.static_files.clone().unwrap_or_else(|| StaticConfig {
                        directories: vec!["static".to_string()],
                        use_redirects: false,
                    }),
                    galleries: self.galleries.clone(),
                    posts: self.posts.clone(),
                    user_database: self.app.user_database.clone(),
                    email,
                },
            );
            sites
        }
    }
}

impl SiteConfigSection {
    /// Convert to SiteConfig for use with SiteBuilder
    pub fn to_site_config(
        &self,
        name: &str,
        app: &AppConfig,
        global_email: Option<&email::EmailConfig>,
    ) -> crate::site::SiteConfig {
        // Use site-specific email config, or fall back to global email config
        let email = self
            .email
            .clone()
            .or_else(|| global_email.map(SiteEmailConfig::from));

        crate::site::SiteConfig {
            name: name.to_string(),
            templates: self.templates.clone(),
            static_files: self.static_files.clone(),
            galleries: self.galleries.clone(),
            posts: self.posts.clone(),
            user_database: self
                .user_database
                .clone()
                .or_else(|| app.user_database.clone()),
            email,
        }
    }
}

/// Convert legacy Config to MultiSiteConfig
impl From<super::types::Config> for MultiSiteConfig {
    fn from(config: super::types::Config) -> Self {
        Self {
            server: config.server,
            app: config.app,
            email: config.email,
            openai: config.openai,
            sites: None, // Legacy config doesn't have sites
            templates: Some(config.templates),
            static_files: Some(config.static_files),
            galleries: config.galleries,
            posts: config.posts,
        }
    }
}

/// Convert MultiSiteConfig to legacy Config
/// In multi-site mode, uses defaults for site-specific fields (they're handled by SiteManager)
impl From<MultiSiteConfig> for super::types::Config {
    fn from(config: MultiSiteConfig) -> Self {
        Self {
            server: config.server,
            app: config.app,
            email: config.email,
            openai: config.openai,
            // Use legacy fields if present, otherwise use defaults
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_multi_site_empty() {
        let config = MultiSiteConfig {
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
            },
            email: None,
            openai: None,
            sites: None,
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

        assert!(!config.is_multi_site());
    }

    #[test]
    fn test_is_multi_site_with_sites() {
        let mut sites = HashMap::new();
        sites.insert(
            "default".to_string(),
            SiteConfigSection {
                hostnames: vec!["*".to_string()],
                templates: TemplateConfig {
                    directories: vec!["templates".to_string()],
                },
                static_files: StaticConfig {
                    directories: vec!["static".to_string()],
                    use_redirects: false,
                },
                galleries: None,
                posts: None,
                user_database: None,
                email: None,
            },
        );

        let config = MultiSiteConfig {
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
            },
            email: None,
            openai: None,
            sites: Some(sites),
            templates: None,
            static_files: None,
            galleries: None,
            posts: None,
        };

        assert!(config.is_multi_site());
    }

    #[test]
    fn test_get_site_configs_legacy() {
        let config = MultiSiteConfig {
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
                user_database: Some("users.toml".into()),
            },
            email: None,
            openai: None,
            sites: None,
            templates: Some(TemplateConfig {
                directories: vec!["my-templates".to_string()],
            }),
            static_files: Some(StaticConfig {
                directories: vec!["my-static".to_string()],
                use_redirects: false,
            }),
            galleries: None,
            posts: None,
        };

        let site_configs = config.get_site_configs();
        assert_eq!(site_configs.len(), 1);
        assert!(site_configs.contains_key("default"));

        let default = site_configs.get("default").unwrap();
        assert_eq!(default.hostnames, vec!["*"]);
        assert_eq!(default.templates.directories, vec!["my-templates"]);
        assert_eq!(default.static_files.directories, vec!["my-static"]);
        assert_eq!(default.user_database, Some("users.toml".into()));
    }
}
