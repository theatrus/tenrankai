mod error;
mod types;

pub mod file_dir;
pub mod storage;
pub mod url;

pub use error::ConfigStorageError;
pub use types::{
    AuditAction,
    AuditEntry,
    GalleryPermissionConfig,
    GoogleFontConfig,
    Role,
    RolePermissions,
    // Site configuration types
    StoredGalleryConfig,
    StoredImageSizeConfig,
    StoredImageWatermarkConfig,
    StoredPostsConfig,
    StoredPregenerateConfig,
    StoredPregenerateFormats,
    StoredPregenerateSizes,
    StoredPreviewConfig,
    StoredSiteConfig,
    StoredSiteEmailConfig,
    StoredThemeConfig,
    StoredTileConfig,
    StoredWatermarkPosition,
    ThemeColorSet,
    UserRole,
};
pub use url::ConfigStorageUrl;

use async_trait::async_trait;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, ConfigStorageError>;

#[async_trait]
pub trait ConfigStorage: Send + Sync + 'static {
    // ========================================================================
    // Site Management
    // ========================================================================

    /// List all site names in storage
    async fn list_sites(&self) -> Result<Vec<String>>;

    /// Get site configuration
    async fn get_site_config(&self, site: &str) -> Result<Option<StoredSiteConfig>>;

    /// Set site configuration
    async fn set_site_config(
        &self,
        site: &str,
        config: &StoredSiteConfig,
        username: &str,
    ) -> Result<()>;

    /// Delete a site and all its contents
    async fn delete_site(&self, site: &str, username: &str) -> Result<bool>;

    // ========================================================================
    // Gallery Management (full config, not just permissions)
    // ========================================================================

    /// List all galleries for a site
    async fn list_galleries(&self, site: &str) -> Result<Vec<String>>;

    /// Get full gallery configuration
    async fn get_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
    ) -> Result<Option<StoredGalleryConfig>>;

    /// Set full gallery configuration
    async fn set_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
        config: &StoredGalleryConfig,
        username: &str,
    ) -> Result<()>;

    /// Delete a gallery
    async fn delete_gallery(&self, site: &str, gallery: &str, username: &str) -> Result<bool>;

    // ========================================================================
    // Posts Management
    // ========================================================================

    /// List all posts systems for a site
    async fn list_posts(&self, site: &str) -> Result<Vec<String>>;

    /// Get posts configuration
    async fn get_posts_config(&self, site: &str, posts: &str) -> Result<Option<StoredPostsConfig>>;

    /// Set posts configuration
    async fn set_posts_config(
        &self,
        site: &str,
        posts: &str,
        config: &StoredPostsConfig,
        username: &str,
    ) -> Result<()>;

    /// Delete a posts system
    async fn delete_posts(&self, site: &str, posts: &str, username: &str) -> Result<bool>;

    // ========================================================================
    // Permission Management (per-site, consolidates galleries/ and roles/)
    // ========================================================================

    /// Get site-level permission configuration (includes all gallery permissions and roles)
    async fn get_site_permissions(&self, site: &str) -> Result<Option<GalleryPermissionConfig>>;

    /// Set site-level permission configuration
    async fn set_site_permissions(
        &self,
        site: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()>;

    // ========================================================================
    // Metadata
    // ========================================================================

    /// Get the backend name for logging/debugging
    fn backend_name(&self) -> &'static str;
}

pub type DynConfigStorage = Arc<dyn ConfigStorage>;

pub async fn create_config_storage(url: &ConfigStorageUrl) -> Result<DynConfigStorage> {
    match url {
        ConfigStorageUrl::FileDir { path } => {
            let backend = file_dir::FileDirConfigStorage::new(path.clone()).await?;
            Ok(Arc::new(backend))
        }
        ConfigStorageUrl::Storage { url } => {
            let backend = storage::StorageConfigStorage::new(url.clone()).await?;
            Ok(Arc::new(backend))
        }
    }
}
