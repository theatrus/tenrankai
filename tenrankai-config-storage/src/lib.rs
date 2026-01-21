mod error;
mod types;

pub mod file_dir;
pub mod storage;
pub mod url;

pub use error::ConfigStorageError;
pub use types::{
    AuditAction, AuditEntry, GalleryPermissionConfig, Role, RolePermissions, UserRole,
    // Site configuration types
    StoredGalleryConfig, StoredImageSizeConfig, StoredPostsConfig, StoredPreviewConfig,
    StoredPregenerateConfig, StoredPregenerateFormats, StoredPregenerateSizes,
    StoredSiteConfig, StoredSiteEmailConfig, StoredTileConfig,
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
    // Legacy Permission Methods (for backward compatibility)
    // These operate on the default site or a flat structure
    // ========================================================================

    /// Get gallery-level permission configuration (legacy - uses flat structure)
    async fn get_gallery_config(&self, gallery: &str) -> Result<Option<GalleryPermissionConfig>>;

    /// Set gallery-level permission configuration (legacy)
    async fn set_gallery_config(
        &self,
        gallery: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()>;

    /// Delete gallery-level permission configuration (legacy)
    async fn delete_gallery_config(&self, gallery: &str, username: &str) -> Result<bool>;

    /// List all custom role definitions (legacy - flat structure)
    async fn list_roles(&self) -> Result<Vec<(String, Role)>>;

    /// Get a specific role definition (legacy)
    async fn get_role(&self, name: &str) -> Result<Option<Role>>;

    /// Set a role definition (legacy)
    async fn set_role(&self, name: &str, role: &Role, username: &str) -> Result<()>;

    /// Delete a role definition (legacy)
    async fn delete_role(&self, name: &str, username: &str) -> Result<bool>;

    /// Get roles assigned to a user for a specific gallery (legacy)
    async fn get_user_roles(&self, gallery: &str, username: &str) -> Result<Vec<String>>;

    /// Set roles for a user in a specific gallery (legacy)
    async fn set_user_roles(
        &self,
        gallery: &str,
        target_username: &str,
        roles: &[String],
        actor_username: &str,
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
