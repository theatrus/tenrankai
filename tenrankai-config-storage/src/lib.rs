mod error;
mod types;

#[cfg(feature = "dynamodb")]
pub mod dynamodb;
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
    StoredGridMode,
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
use std::collections::HashMap;
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
    // Change Detection
    // ========================================================================

    /// Get config version numbers for change detection.
    ///
    /// Returns a map of shard ID to version number. Backends that support
    /// versioning bump the shard version atomically after every mutation.
    /// The poll loop compares this against a cached snapshot to skip full
    /// reloads when nothing has changed.
    ///
    /// Returns an empty map by default (backends without versioning), which
    /// causes the poll loop to fall back to a full reload every cycle.
    async fn get_config_versions(&self) -> Result<HashMap<String, u64>> {
        Ok(HashMap::new())
    }

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
        #[cfg(feature = "dynamodb")]
        ConfigStorageUrl::DynamoDb {
            table_name,
            region,
            endpoint,
            shard,
        } => {
            let backend = dynamodb::DynamoConfigStorage::new(
                table_name.clone(),
                region.clone(),
                endpoint.clone(),
                shard.clone(),
            )
            .await?;
            Ok(Arc::new(backend))
        }
        #[cfg(not(feature = "dynamodb"))]
        ConfigStorageUrl::DynamoDb { .. } => Err(ConfigStorageError::BackendNotAvailable(
            "DynamoDB support requires the 'dynamodb' feature".to_string(),
        )),
    }
}
