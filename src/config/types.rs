use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{LogLevel, email, openai};

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
    #[serde(default)]
    pub openai: Option<openai::OpenAIConfig>,
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
///
/// Directories can be filesystem paths or S3 URLs:
/// - `"templates"` - relative filesystem path
/// - `"/var/data/templates"` - absolute filesystem path
/// - `"s3://bucket/prefix"` - S3 bucket with optional prefix
/// - `"s3://bucket/prefix?region=us-east-1"` - S3 with region
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateConfig {
    #[serde(
        deserialize_with = "super::serialization::deserialize_storage_urls",
        serialize_with = "super::serialization::serialize_storage_urls"
    )]
    pub directories: Vec<String>,
}

/// Static files directory configuration with custom serialization
///
/// Directories can be filesystem paths or S3 URLs:
/// - `"static"` - relative filesystem path
/// - `"/var/data/static"` - absolute filesystem path
/// - `"s3://bucket/prefix"` - S3 bucket with optional prefix
/// - `"s3://bucket/prefix?region=us-east-1"` - S3 with region
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StaticConfig {
    #[serde(
        deserialize_with = "super::serialization::deserialize_storage_urls",
        serialize_with = "super::serialization::serialize_storage_urls"
    )]
    pub directories: Vec<String>,
    /// Whether to use signed URL redirects for S3 backends (default: false).
    /// When true, requests for S3-backed files return a 307 redirect to a signed S3 URL.
    /// When false, the server proxies the content through itself.
    #[serde(default)]
    pub use_redirects: bool,
}

/// Gallery system configuration with image processing settings
///
/// Cache directory can be a filesystem path or storage URL:
/// - `"cache/gallery"` - relative filesystem path
/// - `"/var/cache/tenrankai"` - absolute filesystem path
/// - Note: S3 URLs are not currently supported for cache (processed images require local filesystem)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GallerySystemConfig {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: PathBuf,
    pub cache_directory: String,
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
    /// Pre-generation configuration (None = disabled, Some = enabled with settings)
    #[serde(default)]
    pub pregenerate: Option<PregenerateConfig>,
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
/// Access is controlled by the `can_use_tile_zoom` role permission
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileConfig {
    /// Size of each tile in pixels (e.g., 1024 for 1024x1024 tiles)
    #[serde(default = "super::defaults::default_tile_size")]
    pub tile_size: u32,
}

/// Pre-generation configuration for cache warming
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PregenerateConfig {
    /// Which image formats to pre-generate
    #[serde(default = "super::defaults::default_pregenerate_formats")]
    pub formats: PregenerateFormats,
    /// Which image sizes to pre-generate
    #[serde(default = "super::defaults::default_pregenerate_sizes")]
    pub sizes: PregenerateSizes,
    /// Whether to pre-generate tiles (requires tiles config)
    #[serde(default)]
    pub tiles: bool,
}

/// Which image formats to pre-generate
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PregenerateFormats {
    #[serde(default = "super::defaults::default_true")]
    pub jpeg: bool,
    #[serde(default = "super::defaults::default_true")]
    pub webp: bool,
    #[serde(default)]
    pub avif: bool,
}

/// Which image sizes to pre-generate
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PregenerateSizes {
    #[serde(default = "super::defaults::default_true")]
    pub thumbnail: bool,
    #[serde(default = "super::defaults::default_true")]
    pub gallery: bool,
    #[serde(default = "super::defaults::default_true")]
    pub medium: bool,
    #[serde(default)]
    pub large: bool,
}

/// Posts system configuration for markdown-based content
///
/// Source directory can be a filesystem path or S3 URL:
/// - `"posts/blog"` - relative filesystem path
/// - `"/var/data/posts"` - absolute filesystem path
/// - `"s3://bucket/posts"` - S3 bucket with prefix
/// - `"s3://bucket/posts?region=us-east-1"` - S3 with region
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PostsSystemConfig {
    pub name: String,
    pub source_directory: String,
    pub url_prefix: String,
    #[serde(default = "super::defaults::default_posts_index_template")]
    pub index_template: String,
    #[serde(default = "super::defaults::default_posts_detail_template")]
    pub post_template: String,
    #[serde(default = "super::defaults::default_posts_per_page")]
    pub posts_per_page: usize,
    pub refresh_interval_minutes: Option<u64>,
}
