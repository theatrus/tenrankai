use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};

use super::{Site, SiteBuilderError, SiteConfig, SiteResources};
use crate::{
    favicon::FaviconRenderer,
    gallery::{Gallery, SharedGallery},
    login::LoginState,
    posts::{PostsConfig, PostsManager},
    static_files::StaticFileHandler,
    storage,
    templating::TemplateEngine,
    user_storage::{DynUserStorage, create_user_storage},
};

/// Builder for constructing Site instances
pub struct SiteBuilder {
    config: SiteConfig,
    provided_galleries: Option<Arc<HashMap<String, SharedGallery>>>,
}

impl SiteBuilder {
    pub fn new(config: SiteConfig) -> Self {
        Self {
            config,
            provided_galleries: None,
        }
    }

    /// Inject pre-built galleries (useful for testing or when galleries are built externally)
    pub fn with_galleries(mut self, galleries: Arc<HashMap<String, SharedGallery>>) -> Self {
        self.provided_galleries = Some(galleries);
        self
    }

    /// Build the site asynchronously
    pub async fn build(self) -> Result<Site, SiteBuilderError> {
        info!("Building site '{}'", self.config.name);

        // Build template engine
        let (template_engine, static_handler) = self.build_template_and_static().await?;
        let template_engine = Arc::new(template_engine);

        // Build favicon renderer
        let favicon_renderer = FaviconRenderer::new(static_handler.storages().to_vec());

        // Build galleries (or use provided ones)
        let galleries = if let Some(ref provided) = self.provided_galleries {
            provided.clone()
        } else {
            Arc::new(self.build_galleries().await?)
        };

        // Build posts managers
        let posts_managers = Arc::new(self.build_posts_managers(galleries.clone()).await?);

        // Build login state and user storage
        let (login_state, user_storage) = self.build_login_state().await?;

        let resources = SiteResources {
            base_url: self.config.base_url.clone(),
            cookie_secret: self.config.cookie_secret.clone(),
            template_engine,
            static_handler,
            favicon_renderer,
            galleries,
            posts_managers,
            login_state,
            user_storage,
            email_config: self.config.email.clone(),
        };

        info!("Site '{}' built successfully", self.config.name);
        Ok(Site::new(self.config.name.clone(), resources))
    }

    async fn build_template_and_static(
        &self,
    ) -> Result<(TemplateEngine, StaticFileHandler), SiteBuilderError> {
        // Create template storage backends from URLs
        let template_storages =
            storage::create_storages_from_urls(&self.config.templates.directories).await?;

        let mut template_engine = TemplateEngine::new(template_storages);

        // Create static file handler
        let static_handler =
            StaticFileHandler::from_urls(self.config.static_files.directories.clone())
                .await?
                .with_redirects(self.config.static_files.use_redirects);

        static_handler.refresh_file_versions().await;
        template_engine.set_static_handler(static_handler.clone());
        template_engine.set_has_user_auth(self.config.user_database.is_some());
        template_engine.update_file_versions().await;

        Ok((template_engine, static_handler))
    }

    async fn build_galleries(&self) -> Result<HashMap<String, SharedGallery>, SiteBuilderError> {
        let mut galleries = HashMap::new();

        let gallery_configs = self
            .config
            .galleries
            .as_ref()
            .map_or(&[][..], |v| v.as_slice());

        for gallery_config in gallery_configs {
            info!(
                "Initializing gallery '{}' at {}",
                gallery_config.name, gallery_config.url_prefix
            );

            // Create storage backends for source and cache
            let source_storage =
                match storage::create_storage_from_url(&gallery_config.source_directory).await {
                    Ok(storage) => storage,
                    Err(e) => {
                        error!(
                            "Failed to create source storage for gallery '{}': {}",
                            gallery_config.name, e
                        );
                        return Err(SiteBuilderError::Gallery(format!(
                            "Failed to create source storage for gallery '{}': {}",
                            gallery_config.name, e
                        )));
                    }
                };

            let cache_storage =
                match storage::create_storage_from_url(&gallery_config.cache_directory).await {
                    Ok(storage) => storage,
                    Err(e) => {
                        error!(
                            "Failed to create cache storage for gallery '{}': {}",
                            gallery_config.name, e
                        );
                        return Err(SiteBuilderError::Gallery(format!(
                            "Failed to create cache storage for gallery '{}': {}",
                            gallery_config.name, e
                        )));
                    }
                };

            info!(
                "Gallery '{}' using source: {}, cache: {}",
                gallery_config.name,
                source_storage.storage_type(),
                cache_storage.storage_type()
            );

            let gallery = Arc::new(Gallery::new(
                gallery_config.clone(),
                source_storage,
                cache_storage,
            ));

            // Initialize folder cache (mandatory for gallery operations)
            if let Err(e) = gallery.refresh_folder_cache().await {
                error!(
                    "Failed to initialize folder cache for gallery '{}': {}",
                    gallery_config.name, e
                );
            }

            galleries.insert(gallery_config.name.clone(), gallery);
        }

        Ok(galleries)
    }

    async fn build_posts_managers(
        &self,
        galleries: Arc<HashMap<String, SharedGallery>>,
    ) -> Result<HashMap<String, Arc<PostsManager>>, SiteBuilderError> {
        let mut posts_managers = HashMap::new();

        let posts_configs = self.config.posts.as_ref().map_or(&[][..], |v| v.as_slice());

        for posts_system_config in posts_configs {
            info!(
                "Initializing posts system '{}' at {}",
                posts_system_config.name, posts_system_config.url_prefix
            );

            let posts_storage =
                match storage::create_storage_from_url(&posts_system_config.source_directory).await
                {
                    Ok(storage) => storage,
                    Err(e) => {
                        error!(
                            "Failed to create storage for posts system '{}': {}",
                            posts_system_config.name, e
                        );
                        return Err(SiteBuilderError::Posts(format!(
                            "Failed to create storage for posts system '{}': {}",
                            posts_system_config.name, e
                        )));
                    }
                };

            info!(
                "Posts '{}' from {} (storage: {})",
                posts_system_config.name,
                posts_system_config.source_directory,
                posts_storage.storage_type()
            );

            // Convert PostsSystemConfig to PostsConfig
            let posts_config = PostsConfig {
                source_directory: posts_system_config.source_directory.clone(),
                url_prefix: posts_system_config.url_prefix.clone(),
                index_template: posts_system_config.index_template.clone(),
                post_template: posts_system_config.post_template.clone(),
                posts_per_page: posts_system_config.posts_per_page,
                refresh_interval_minutes: posts_system_config.refresh_interval_minutes,
            };

            let mut posts_manager = PostsManager::new(posts_config, posts_storage);
            posts_manager.set_galleries(galleries.clone());

            let posts_manager = Arc::new(posts_manager);

            // Load posts on startup
            if let Err(e) = posts_manager.refresh_posts().await {
                error!(
                    "Failed to initialize posts for '{}': {}",
                    posts_system_config.name, e
                );
            }

            posts_managers.insert(posts_system_config.name.clone(), posts_manager);
        }

        Ok(posts_managers)
    }

    async fn build_login_state(
        &self,
    ) -> Result<(Arc<RwLock<LoginState>>, Option<DynUserStorage>), SiteBuilderError> {
        let login_state = Arc::new(RwLock::new(LoginState::new()));

        let user_storage = if let Some(user_db_url) = &self.config.user_database {
            // Use site name as the site_id for multi-tenant isolation
            let site_id = &self.config.name;
            match create_user_storage(user_db_url, site_id).await {
                Ok(storage) => {
                    info!(
                        "Loaded user storage from '{}' (backend: {}, site: {})",
                        user_db_url,
                        storage.backend_name(),
                        site_id
                    );
                    Some(storage)
                }
                Err(e) => {
                    error!("Failed to load user storage: {}", e);
                    return Err(SiteBuilderError::Login(format!(
                        "Failed to load user storage: {}",
                        e
                    )));
                }
            }
        } else {
            None
        };

        Ok((login_state, user_storage))
    }
}
