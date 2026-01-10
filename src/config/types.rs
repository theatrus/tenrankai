use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{LogLevel, email};

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub app: AppConfig,
    pub templates: TemplateConfig,
    pub static_files: StaticConfig,
    #[serde(default)]
    pub galleries: Option<Vec<GallerySystemConfig>>,
    #[serde(default)]
    pub posts: Option<Vec<PostsSystemConfig>>,
    #[serde(default)]
    pub email: Option<email::EmailConfig>,
}

/// Server configuration for host and port settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// Application-level configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub log_level: LogLevel,
    pub cookie_secret: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub user_database: Option<PathBuf>,
}

/// Template directory configuration with custom serialization
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateConfig {
    #[serde(
        deserialize_with = "super::serialization::deserialize_template_directories",
        serialize_with = "super::serialization::serialize_template_directories"
    )]
    pub directories: Vec<PathBuf>,
}

/// Static files directory configuration with custom serialization
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StaticConfig {
    #[serde(
        deserialize_with = "super::serialization::deserialize_static_directories",
        serialize_with = "super::serialization::serialize_static_directories"
    )]
    pub directories: Vec<PathBuf>,
}

/// Gallery system configuration with image processing settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GallerySystemConfig {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: PathBuf,
    pub cache_directory: PathBuf,
    #[serde(default = "super::defaults::default_gallery_template")]
    pub gallery_template: String,
    #[serde(default = "super::defaults::default_image_detail_template")]
    pub image_detail_template: String,
    #[serde(default = "super::defaults::default_images_per_page")]
    pub images_per_page: usize,
    #[serde(default = "super::defaults::default_thumbnail_size")]
    pub thumbnail: ImageSizeConfig,
    #[serde(default = "super::defaults::default_gallery_size")]
    pub gallery_size: ImageSizeConfig,
    #[serde(default = "super::defaults::default_medium_size")]
    pub medium: ImageSizeConfig,
    #[serde(default = "super::defaults::default_large_size")]
    pub large: ImageSizeConfig,
    #[serde(default = "super::defaults::default_preview_config")]
    pub preview: PreviewConfig,
    /// Tile configuration for protected zoom feature
    #[serde(default)]
    pub tiles: Option<TileConfig>,
    pub cache_refresh_interval_minutes: Option<u64>,
    pub jpeg_quality: Option<u8>,
    pub webp_quality: Option<f32>,
    #[serde(default)]
    pub pregenerate_cache: bool,
    /// Number of days to consider an image as "new" (based on file modification date)
    pub new_threshold_days: Option<u32>,
    /// Copyright holder name for watermarking medium-sized images
    #[serde(default)]
    pub copyright_holder: Option<String>,
    /// Image indexing mode for URLs: "filename" (default), "sequence", or "unique_id"
    #[serde(default = "super::defaults::default_image_indexing")]
    pub image_indexing: ImageIndexingMode,
    /// Permission configuration for this gallery
    #[serde(default)]
    pub permissions: crate::permissions::types::PermissionConfig,
}

/// Image indexing mode for gallery URLs
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageIndexingMode {
    /// Use actual filenames in URLs (default)
    Filename,
    /// Use sequential numbers based on sorted filenames
    Sequence,
    /// Use unique base36 identifiers generated from path hash
    UniqueId,
}

/// Image size configuration for gallery processing
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSizeConfig {
    pub width: u32,
    pub height: u32,
}

/// Preview configuration for gallery previews
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewConfig {
    pub max_images: usize,
    pub max_depth: usize,
    pub max_per_folder: usize,
}

/// Tile configuration for protected zoom feature
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileConfig {
    /// Size of each tile in pixels (e.g., 1024 for 1024x1024 tiles)
    #[serde(default = "super::defaults::default_tile_size")]
    pub tile_size: u32,
    /// Whether tiles require authentication
    #[serde(default = "super::defaults::default_tiles_require_auth")]
    pub require_auth: bool,
    /// Whether to pre-generate tiles during cache pre-generation
    #[serde(default = "super::defaults::default_tiles_pregenerate")]
    pub pregenerate: bool,
}

/// Posts system configuration for markdown-based content
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PostsSystemConfig {
    pub name: String,
    pub source_directory: PathBuf,
    pub url_prefix: String,
    #[serde(default = "super::defaults::default_posts_index_template")]
    pub index_template: String,
    #[serde(default = "super::defaults::default_posts_detail_template")]
    pub post_template: String,
    #[serde(default = "super::defaults::default_posts_per_page")]
    pub posts_per_page: usize,
    pub refresh_interval_minutes: Option<u64>,
}
