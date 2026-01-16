use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    Config, GallerySystemConfig, PostsSystemConfig, StaticConfig, TemplateConfig,
    email::SiteEmailConfig, favicon::FaviconRenderer, gallery::SharedGallery, login::LoginState,
    posts::PostsManager, static_files::StaticFileHandler, templating::TemplateEngine,
    user_storage::DynUserStorage,
};

/// Configuration for a single site (extracted from legacy Config or multi-site config)
#[derive(Debug, Clone)]
pub struct SiteConfig {
    pub name: String,
    pub base_url: Option<String>,
    pub cookie_secret: String,
    pub templates: TemplateConfig,
    pub static_files: StaticConfig,
    pub galleries: Option<Vec<GallerySystemConfig>>,
    pub posts: Option<Vec<PostsSystemConfig>>,
    pub user_database: Option<String>,
    pub email: Option<SiteEmailConfig>,
}

impl SiteConfig {
    /// Create a default site config from legacy Config (backward compatibility)
    pub fn from_legacy_config(
        config: &Config,
        email_config: Option<&crate::email::EmailConfig>,
    ) -> Self {
        Self {
            name: config.app.name.clone(),
            base_url: config.app.base_url.clone(),
            cookie_secret: config.app.cookie_secret.clone(),
            templates: config.templates.clone(),
            static_files: config.static_files.clone(),
            galleries: config.galleries.clone(),
            posts: config.posts.clone(),
            user_database: config
                .app
                .user_database
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            email: email_config.map(SiteEmailConfig::from),
        }
    }
}

/// Resources for a single site - encapsulates all site-specific components
pub struct SiteResources {
    pub base_url: Option<String>,
    pub cookie_secret: String,
    pub template_engine: Arc<TemplateEngine>,
    pub static_handler: StaticFileHandler,
    pub favicon_renderer: FaviconRenderer,
    pub galleries: Arc<HashMap<String, SharedGallery>>,
    pub posts_managers: Arc<HashMap<String, Arc<PostsManager>>>,
    pub login_state: Arc<RwLock<LoginState>>,
    pub user_storage: Option<DynUserStorage>,
    pub email_config: Option<SiteEmailConfig>,
}

/// A Site represents a virtual host with its own resources
pub struct Site {
    pub name: String,
    pub resources: SiteResources,
}

impl Site {
    pub fn new(name: String, resources: SiteResources) -> Self {
        Self { name, resources }
    }

    /// Get the site/app name
    pub fn app_name(&self) -> &str {
        &self.name
    }

    /// Get the base URL for this site
    pub fn base_url(&self) -> Option<&str> {
        self.resources.base_url.as_deref()
    }

    /// Get the cookie secret for this site
    pub fn cookie_secret(&self) -> &str {
        &self.resources.cookie_secret
    }

    pub fn template_engine(&self) -> &Arc<TemplateEngine> {
        &self.resources.template_engine
    }

    pub fn static_handler(&self) -> &StaticFileHandler {
        &self.resources.static_handler
    }

    pub fn favicon_renderer(&self) -> &FaviconRenderer {
        &self.resources.favicon_renderer
    }

    pub fn galleries(&self) -> &Arc<HashMap<String, SharedGallery>> {
        &self.resources.galleries
    }

    pub fn posts_managers(&self) -> &Arc<HashMap<String, Arc<PostsManager>>> {
        &self.resources.posts_managers
    }

    pub fn login_state(&self) -> &Arc<RwLock<LoginState>> {
        &self.resources.login_state
    }

    pub fn user_storage(&self) -> &Option<DynUserStorage> {
        &self.resources.user_storage
    }

    pub fn email_config(&self) -> Option<&SiteEmailConfig> {
        self.resources.email_config.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_site(name: &str, base_url: Option<&str>, cookie_secret: &str) -> Site {
        let resources = SiteResources {
            base_url: base_url.map(|s| s.to_string()),
            cookie_secret: cookie_secret.to_string(),
            template_engine: Arc::new(crate::templating::TemplateEngine::new(vec![])),
            static_handler: crate::static_files::StaticFileHandler::new(vec![]),
            favicon_renderer: FaviconRenderer::new(vec![]),
            galleries: Arc::new(HashMap::new()),
            posts_managers: Arc::new(HashMap::new()),
            login_state: Arc::new(RwLock::new(LoginState::new())),
            user_storage: None,
            email_config: None,
        };
        Site::new(name.to_string(), resources)
    }

    #[test]
    fn test_site_accessors_return_per_site_values() {
        let site1 = create_test_site("site1", Some("https://site1.com"), "secret1");
        let site2 = create_test_site("site2", Some("https://site2.com"), "secret2");

        // Verify each site returns its own values
        assert_eq!(site1.app_name(), "site1");
        assert_eq!(site1.base_url(), Some("https://site1.com"));
        assert_eq!(site1.cookie_secret(), "secret1");

        assert_eq!(site2.app_name(), "site2");
        assert_eq!(site2.base_url(), Some("https://site2.com"));
        assert_eq!(site2.cookie_secret(), "secret2");
    }

    #[test]
    fn test_site_with_no_base_url() {
        let site = create_test_site("local", None, "local-secret");

        assert_eq!(site.app_name(), "local");
        assert_eq!(site.base_url(), None);
        assert_eq!(site.cookie_secret(), "local-secret");
    }
}
