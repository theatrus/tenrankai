use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    favicon::FaviconRenderer,
    gallery::SharedGallery,
    login::{types::UserDatabaseManager, LoginState},
    posts::PostsManager,
    static_files::StaticFileHandler,
    templating::TemplateEngine,
    Config, GallerySystemConfig, PostsSystemConfig, StaticConfig, TemplateConfig,
};

/// Configuration for a single site (extracted from legacy Config or multi-site config)
#[derive(Debug, Clone)]
pub struct SiteConfig {
    pub name: String,
    pub templates: TemplateConfig,
    pub static_files: StaticConfig,
    pub galleries: Option<Vec<GallerySystemConfig>>,
    pub posts: Option<Vec<PostsSystemConfig>>,
    pub user_database: Option<PathBuf>,
}

impl SiteConfig {
    /// Create a default site config from legacy Config (backward compatibility)
    pub fn from_legacy_config(config: &Config) -> Self {
        Self {
            name: "default".to_string(),
            templates: config.templates.clone(),
            static_files: config.static_files.clone(),
            galleries: config.galleries.clone(),
            posts: config.posts.clone(),
            user_database: config.app.user_database.clone(),
        }
    }
}

/// Resources for a single site - encapsulates all site-specific components
pub struct SiteResources {
    pub template_engine: Arc<TemplateEngine>,
    pub static_handler: StaticFileHandler,
    pub favicon_renderer: FaviconRenderer,
    pub galleries: Arc<HashMap<String, SharedGallery>>,
    pub posts_managers: Arc<HashMap<String, Arc<PostsManager>>>,
    pub login_state: Arc<RwLock<LoginState>>,
    pub user_database_manager: Option<UserDatabaseManager>,
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

    pub fn user_database_manager(&self) -> &Option<UserDatabaseManager> {
        &self.resources.user_database_manager
    }
}
