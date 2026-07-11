use std::collections::HashMap;
use tenrankai_config_storage::{
    DynConfigStorage, GalleryPermissionConfig, StoredGalleryConfig, StoredPostsConfig,
    StoredSiteConfig,
};
use thiserror::Error;

use super::types::{
    GallerySystemConfig, ImageSizeConfig, ImageWatermarkConfig, PostsSystemConfig,
    PregenerateConfig, PregenerateFormats, PregenerateSizes, PreviewConfig, StaticConfig,
    TemplateConfig, TileConfig, WatermarkPosition,
};
use crate::email::SiteEmailConfig;
use crate::permissions::types::{PermissionConfig, Role, RolePermissions, UserRole};
use crate::site::SiteConfig;

#[derive(Debug, Error)]
pub enum StorageLoaderError {
    #[error("ConfigStorage error: {0}")]
    Storage(#[from] tenrankai_config_storage::ConfigStorageError),

    #[error("Invalid path '{path}': {reason}")]
    InvalidPath { path: String, reason: String },

    #[error("Site '{0}' not found")]
    SiteNotFound(String),
}

pub type Result<T> = std::result::Result<T, StorageLoaderError>;

pub struct ConfigStorageLoader {
    storage: DynConfigStorage,
    default_cookie_secret: String,
}

impl ConfigStorageLoader {
    pub fn new(storage: DynConfigStorage, default_cookie_secret: String) -> Self {
        Self {
            storage,
            default_cookie_secret,
        }
    }

    pub fn storage(&self) -> &DynConfigStorage {
        &self.storage
    }

    pub async fn load_all_sites(&self) -> Result<HashMap<String, SiteConfig>> {
        let site_names = self.storage.list_sites().await?;
        let mut sites = HashMap::new();

        for name in site_names {
            if let Some(config) = self.load_site(&name).await? {
                sites.insert(name, config);
            }
        }

        Ok(sites)
    }

    pub async fn load_site(&self, name: &str) -> Result<Option<SiteConfig>> {
        let stored_config = match self.storage.get_site_config(name).await? {
            Some(config) => config,
            None => return Ok(None),
        };

        let storage_prefix = stored_config.storage_prefix.as_deref();
        let cache_prefix = stored_config.cache_prefix.as_deref();

        // Load site permissions (to be applied to all galleries)
        let site_permissions = self.load_site_permissions(name).await?;

        let galleries = self
            .load_galleries(
                name,
                storage_prefix,
                cache_prefix,
                site_permissions.as_ref(),
            )
            .await?;
        let posts = self
            .load_posts(name, storage_prefix, site_permissions.as_ref())
            .await?;

        let site_admins = site_permissions
            .as_ref()
            .map(|p| p.site_admins.clone())
            .unwrap_or_default();

        let site_config =
            self.convert_site_config(name, stored_config, galleries, posts, site_admins)?;

        Ok(Some(site_config))
    }

    async fn load_site_permissions(&self, site: &str) -> Result<Option<PermissionConfig>> {
        match self.storage.get_site_permissions(site).await? {
            Some(stored) => Ok(Some(Self::convert_permission_config(&stored))),
            None => Ok(None),
        }
    }

    async fn load_galleries(
        &self,
        site: &str,
        storage_prefix: Option<&str>,
        cache_prefix: Option<&str>,
        site_permissions: Option<&PermissionConfig>,
    ) -> Result<Vec<GallerySystemConfig>> {
        let gallery_names = self.storage.list_galleries(site).await?;
        let mut galleries = Vec::new();

        for gallery_name in gallery_names {
            // Skip hidden/prototype galleries (those starting with underscore)
            if gallery_name.starts_with('_') {
                continue;
            }
            if let Some(stored) = self
                .storage
                .get_gallery_full_config(site, &gallery_name)
                .await?
            {
                let gallery = self.convert_gallery_config(
                    stored,
                    storage_prefix,
                    cache_prefix,
                    site_permissions,
                )?;
                galleries.push(gallery);
            }
        }

        Ok(galleries)
    }

    async fn load_posts(
        &self,
        site: &str,
        storage_prefix: Option<&str>,
        site_permissions: Option<&PermissionConfig>,
    ) -> Result<Vec<PostsSystemConfig>> {
        let posts_names = self.storage.list_posts(site).await?;
        let mut posts = Vec::new();

        for posts_name in posts_names {
            if let Some(stored) = self.storage.get_posts_config(site, &posts_name).await? {
                let posts_config =
                    self.convert_posts_config(stored, storage_prefix, site_permissions)?;
                posts.push(posts_config);
            }
        }

        Ok(posts)
    }

    fn convert_site_config(
        &self,
        name: &str,
        stored: StoredSiteConfig,
        galleries: Vec<GallerySystemConfig>,
        posts: Vec<PostsSystemConfig>,
        site_admins: Vec<String>,
    ) -> Result<SiteConfig> {
        let email = stored.email.map(|e| SiteEmailConfig {
            from_address: e.from_address,
            from_name: e.from_name,
            reply_to: e.reply_to,
        });

        Ok(SiteConfig {
            name: name.to_string(),
            base_url: stored.base_url,
            cookie_secret: stored
                .cookie_secret
                .unwrap_or_else(|| self.default_cookie_secret.clone()),
            templates: TemplateConfig {
                directories: stored.templates,
            },
            static_files: StaticConfig {
                directories: stored.static_files,
                use_redirects: stored.static_use_redirects,
            },
            galleries: if galleries.is_empty() {
                None
            } else {
                Some(galleries)
            },
            posts: if posts.is_empty() { None } else { Some(posts) },
            user_database: stored.user_database,
            email,
            config_storage: None, // Will be set by the startup code
            site_admins,
            hosted_mode: false, // Will be set by the startup code from AppConfig
            site_title: stored.site_title,
            copyright_holder: stored.copyright_holder,
        })
    }

    fn convert_gallery_config(
        &self,
        stored: StoredGalleryConfig,
        storage_prefix: Option<&str>,
        cache_prefix: Option<&str>,
        site_permissions: Option<&PermissionConfig>,
    ) -> Result<GallerySystemConfig> {
        let source_directory = self.resolve_path(storage_prefix, &stored.source_directory)?;
        let cache_directory = self.resolve_path(cache_prefix, &stored.cache_directory)?;

        let tiles = stored.tiles.map(|t| TileConfig {
            tile_size: t.tile_size,
        });

        let pregenerate = stored.pregenerate.map(|p| PregenerateConfig {
            formats: PregenerateFormats {
                jpeg: p.formats.jpeg,
                webp: p.formats.webp,
                avif: p.formats.avif,
            },
            sizes: PregenerateSizes {
                thumbnail: p.sizes.thumbnail,
                gallery: p.sizes.gallery,
                medium: p.sizes.medium,
                large: p.sizes.large,
            },
            tiles: p.tiles,
        });

        let preview = stored
            .preview
            .map(|p| PreviewConfig {
                max_images: p.max_images,
                max_depth: p.max_depth,
                max_per_folder: p.max_per_folder,
            })
            .unwrap_or_else(super::defaults::default_preview_config);

        let image_indexing = match stored.image_indexing.as_str() {
            "sequence" => super::types::ImageIndexingMode::Sequence,
            "unique_id" => super::types::ImageIndexingMode::UniqueId,
            _ => super::types::ImageIndexingMode::Filename,
        };

        // Use site permissions if available, otherwise use defaults
        let permissions = site_permissions.cloned().unwrap_or_default();

        let image_watermark = stored.image_watermark.map(|wm| ImageWatermarkConfig {
            image: wm.image,
            position: match wm.position {
                tenrankai_config_storage::StoredWatermarkPosition::BottomLeft => {
                    WatermarkPosition::BottomLeft
                }
                tenrankai_config_storage::StoredWatermarkPosition::BottomRight => {
                    WatermarkPosition::BottomRight
                }
                tenrankai_config_storage::StoredWatermarkPosition::TopLeft => {
                    WatermarkPosition::TopLeft
                }
                tenrankai_config_storage::StoredWatermarkPosition::TopRight => {
                    WatermarkPosition::TopRight
                }
                tenrankai_config_storage::StoredWatermarkPosition::Center => {
                    WatermarkPosition::Center
                }
                tenrankai_config_storage::StoredWatermarkPosition::Tiled => {
                    WatermarkPosition::Tiled
                }
            },
            opacity: wm.opacity,
            scale: wm.scale,
            padding: wm.padding,
            adaptive: wm.adaptive,
            apply_to_gallery: wm.apply_to_gallery,
            apply_to_medium: wm.apply_to_medium,
            apply_to_large: wm.apply_to_large,
        });

        Ok(GallerySystemConfig {
            name: stored.name,
            url_prefix: stored.url_prefix,
            source_directory,
            cache_directory,
            gallery_template: stored.gallery_template,
            image_detail_template: stored.image_detail_template,
            thumbnail: ImageSizeConfig {
                width: stored.thumbnail.width,
                height: stored.thumbnail.height,
            },
            gallery_size: ImageSizeConfig {
                width: stored.gallery_size.width,
                height: stored.gallery_size.height,
            },
            medium: ImageSizeConfig {
                width: stored.medium.width,
                height: stored.medium.height,
            },
            large: ImageSizeConfig {
                width: stored.large.width,
                height: stored.large.height,
            },
            preview,
            tiles,
            cache_refresh_interval_minutes: stored.cache_refresh_interval_minutes,
            jpeg_quality: stored.jpeg_quality,
            webp_quality: stored.webp_quality,
            pregenerate,
            new_threshold_days: stored.new_threshold_days,
            copyright_holder: stored.copyright_holder,
            image_watermark,
            image_indexing,
            permissions,
            metadata_cache_size: stored.metadata_cache_size,
            grid_mode: match stored.grid_mode {
                tenrankai_config_storage::StoredGridMode::Masonry => {
                    super::types::GridMode::Masonry
                }
                tenrankai_config_storage::StoredGridMode::Square => super::types::GridMode::Square,
            },
            max_columns: stored.max_columns,
        })
    }

    fn convert_posts_config(
        &self,
        stored: StoredPostsConfig,
        storage_prefix: Option<&str>,
        site_permissions: Option<&PermissionConfig>,
    ) -> Result<PostsSystemConfig> {
        let source_directory = self.resolve_path(storage_prefix, &stored.source_directory)?;

        Ok(PostsSystemConfig {
            name: stored.name,
            source_directory,
            url_prefix: stored.url_prefix,
            index_template: stored.index_template,
            post_template: stored.post_template,
            posts_per_page: stored.posts_per_page,
            refresh_interval_minutes: stored.refresh_interval_minutes,
            permissions: site_permissions.cloned().unwrap_or_default(),
        })
    }

    fn resolve_path(&self, storage_prefix: Option<&str>, relative_path: &str) -> Result<String> {
        Self::validate_relative_path(relative_path)?;

        match storage_prefix {
            Some(prefix) => {
                if prefix.is_empty() {
                    Ok(relative_path.to_string())
                } else if prefix.ends_with('/') {
                    Ok(format!("{}{}", prefix, relative_path))
                } else {
                    Ok(format!("{}/{}", prefix, relative_path))
                }
            }
            None => Ok(relative_path.to_string()),
        }
    }

    fn validate_relative_path(path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(StorageLoaderError::InvalidPath {
                path: path.to_string(),
                reason: "path cannot be empty".to_string(),
            });
        }

        if path.starts_with('/') {
            return Err(StorageLoaderError::InvalidPath {
                path: path.to_string(),
                reason: "path must be relative (cannot start with '/')".to_string(),
            });
        }

        if path.starts_with("./") || path.starts_with(".\\") {
            return Err(StorageLoaderError::InvalidPath {
                path: path.to_string(),
                reason: "path cannot start with './'".to_string(),
            });
        }

        if path.contains("..") {
            return Err(StorageLoaderError::InvalidPath {
                path: path.to_string(),
                reason: "path cannot contain '..' (directory traversal)".to_string(),
            });
        }

        if path.contains('\0') {
            return Err(StorageLoaderError::InvalidPath {
                path: path.to_string(),
                reason: "path cannot contain null bytes".to_string(),
            });
        }

        Ok(())
    }

    fn convert_permission_config(stored: &GalleryPermissionConfig) -> PermissionConfig {
        let mut roles = HashMap::new();

        for (name, stored_role) in &stored.roles {
            let permissions = Self::convert_role_permissions(&stored_role.permissions);
            let role = if let Some(inherits) = &stored_role.inherits {
                Role::with_inheritance(name.clone(), permissions, inherits.clone())
            } else {
                Role::new(name.clone(), permissions)
            };
            roles.insert(name.clone(), role);
        }

        let user_roles = stored
            .user_roles
            .iter()
            .map(|ur| UserRole::new(ur.username.clone(), ur.roles.clone()))
            .collect();

        PermissionConfig {
            site_admins: stored.site_admins.clone(),
            public_role: stored.public_role.clone(),
            default_authenticated_role: stored.default_authenticated_role.clone(),
            roles,
            user_roles,
        }
    }

    fn convert_role_permissions(
        stored: &tenrankai_config_storage::RolePermissions,
    ) -> RolePermissions {
        RolePermissions {
            can_view: stored.can_view,
            can_see_hidden: stored.can_see_hidden,
            can_see_technical_details: stored.can_see_technical_details,
            can_see_exact_dates: stored.can_see_exact_dates,
            can_see_location: stored.can_see_location,
            can_download_medium: stored.can_download_medium,
            can_download_large: stored.can_download_large,
            can_download_original: stored.can_download_original,
            can_download_gallery: stored.can_download_gallery,
            can_download_raw: stored.can_download_raw,
            can_see_versions: stored.can_see_versions,
            can_read_metadata: stored.can_read_metadata,
            can_edit_content: stored.can_edit_content,
            can_add_comments: stored.can_add_comments,
            can_edit_own_comments: stored.can_edit_own_comments,
            can_delete_own_comments: stored.can_delete_own_comments,
            can_set_picks: stored.can_set_picks,
            can_add_tags: stored.can_add_tags,
            can_edit_any_comments: stored.can_edit_any_comments,
            can_delete_any_comments: stored.can_delete_any_comments,
            can_manage_images: stored.can_manage_images,
            can_use_zoom: stored.can_use_zoom,
            can_use_tile_zoom: stored.can_use_tile_zoom,
            can_analyze_images: stored.can_analyze_images,
            can_see_ai_analysis: stored.can_see_ai_analysis,
            can_see_ai_alt_text: stored.can_see_ai_alt_text,
            owner_access: stored.owner_access,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_relative_path_valid() {
        assert!(ConfigStorageLoader::validate_relative_path("photos").is_ok());
        assert!(ConfigStorageLoader::validate_relative_path("photos/vacation").is_ok());
        assert!(ConfigStorageLoader::validate_relative_path("cache/main").is_ok());
        assert!(ConfigStorageLoader::validate_relative_path("a/b/c/d").is_ok());
    }

    #[test]
    fn test_validate_relative_path_empty() {
        let result = ConfigStorageLoader::validate_relative_path("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_relative_path_absolute() {
        let result = ConfigStorageLoader::validate_relative_path("/photos");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be relative"));
    }

    #[test]
    fn test_validate_relative_path_dot_slash() {
        let result = ConfigStorageLoader::validate_relative_path("./photos");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot start with './'")
        );
    }

    #[test]
    fn test_validate_relative_path_traversal() {
        let result = ConfigStorageLoader::validate_relative_path("photos/../secrets");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("directory traversal")
        );

        let result = ConfigStorageLoader::validate_relative_path("..\\secrets");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_relative_path_null_byte() {
        let result = ConfigStorageLoader::validate_relative_path("photos\0bad");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }
}
