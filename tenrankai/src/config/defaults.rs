use super::types::*;
use crate::LogLevel;

/// Default posts index template path
pub fn default_posts_index_template() -> String {
    "modules/posts_index.html.liquid".to_string()
}

/// Default posts detail template path
pub fn default_posts_detail_template() -> String {
    "modules/post_detail.html.liquid".to_string()
}

/// Default number of posts per page
pub fn default_posts_per_page() -> usize {
    20
}

/// Default false value for boolean configuration options
pub fn default_false() -> bool {
    false
}

/// Default true value for boolean configuration options
pub fn default_true() -> bool {
    true
}

/// Default image indexing mode (filename)
pub fn default_image_indexing() -> ImageIndexingMode {
    ImageIndexingMode::Filename
}

/// Default LRU cache size for user metadata storage
pub fn default_metadata_cache_size() -> usize {
    1000
}

/// Default AWS SDK log level (warn to reduce noise)
pub fn default_aws_log_level() -> LogLevel {
    LogLevel::Warn
}

/// Default gallery template path
pub fn default_gallery_template() -> String {
    "modules/gallery.html.liquid".to_string()
}

/// Default image detail template path
pub fn default_image_detail_template() -> String {
    "modules/image_detail.html.liquid".to_string()
}

/// Default number of images per page in gallery
pub fn default_images_per_page() -> usize {
    50
}

/// Default thumbnail size configuration
pub fn default_thumbnail_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 300,
        height: 300,
    }
}

/// Default gallery image size configuration
pub fn default_gallery_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 800,
        height: 800,
    }
}

/// Default medium image size configuration
pub fn default_medium_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 1200,
        height: 1200,
    }
}

/// Default large image size configuration
pub fn default_large_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 1600,
        height: 1600,
    }
}

/// Default preview configuration for gallery previews
pub fn default_preview_config() -> PreviewConfig {
    PreviewConfig {
        max_images: 4,
        max_depth: 3,
        max_per_folder: 3,
    }
}

/// Default tile size in pixels (1024x1024)
pub fn default_tile_size() -> u32 {
    1024
}

/// Default pregenerate formats (jpeg + webp, not avif)
pub fn default_pregenerate_formats() -> super::types::PregenerateFormats {
    super::types::PregenerateFormats {
        jpeg: true,
        webp: true,
        avif: false,
    }
}

/// Default pregenerate sizes (thumbnail, gallery, medium - not large)
pub fn default_pregenerate_sizes() -> super::types::PregenerateSizes {
    super::types::PregenerateSizes {
        thumbnail: true,
        gallery: true,
        medium: true,
        large: false,
    }
}

impl Default for GallerySystemConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: "photos".to_string(),
            cache_directory: "cache".to_string(),
            gallery_template: default_gallery_template(),
            image_detail_template: default_image_detail_template(),
            images_per_page: default_images_per_page(),
            thumbnail: default_thumbnail_size(),
            gallery_size: default_gallery_size(),
            medium: default_medium_size(),
            large: default_large_size(),
            preview: default_preview_config(),
            cache_refresh_interval_minutes: Some(60),
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate: None,
            new_threshold_days: None,
            copyright_holder: None,
            image_indexing: default_image_indexing(),
            permissions: Default::default(),
            tiles: None,
            metadata_cache_size: default_metadata_cache_size(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            app: AppConfig {
                name: "Tenrankai".to_string(),
                log_level: LogLevel::Info,
                aws_log_level: default_aws_log_level(),
                cookie_secret: "change-me-in-production-use-a-long-random-string".to_string(),
                base_url: None,
                user_database: None,
                config_storage: None,
            },
            templates: TemplateConfig {
                directories: vec!["templates".to_string()],
            },
            static_files: StaticConfig {
                directories: vec!["static".to_string()],
                use_redirects: false,
            },
            galleries: Some(vec![GallerySystemConfig {
                name: "default".to_string(),
                url_prefix: "/gallery".to_string(),
                source_directory: "photos".to_string(),
                cache_directory: "cache".to_string(),
                gallery_template: default_gallery_template(),
                image_detail_template: default_image_detail_template(),
                images_per_page: default_images_per_page(),
                thumbnail: default_thumbnail_size(),
                gallery_size: default_gallery_size(),
                medium: default_medium_size(),
                large: default_large_size(),
                preview: default_preview_config(),
                cache_refresh_interval_minutes: Some(60),
                jpeg_quality: Some(85),
                webp_quality: Some(85.0),
                pregenerate: None,
                new_threshold_days: None,
                copyright_holder: None,
                image_indexing: default_image_indexing(),
                permissions: Default::default(),
                tiles: None,
                metadata_cache_size: default_metadata_cache_size(),
            }]),
            posts: None,
            email: None,
            openai: None,
        }
    }
}
