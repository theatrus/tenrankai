use super::types::*;
use crate::LogLevel;
use std::path::PathBuf;

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

impl Default for GallerySystemConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: PathBuf::from("photos"),
            cache_directory: PathBuf::from("cache"),
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
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
            copyright_holder: None,
            hide_location_from_public: false,
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
                cookie_secret: "change-me-in-production-use-a-long-random-string".to_string(),
                base_url: None,
                user_database: None,
            },
            templates: TemplateConfig {
                directories: vec![PathBuf::from("templates")],
            },
            static_files: StaticConfig {
                directories: vec![PathBuf::from("static")],
            },
            galleries: Some(vec![GallerySystemConfig {
                name: "default".to_string(),
                url_prefix: "/gallery".to_string(),
                source_directory: PathBuf::from("photos"),
                cache_directory: PathBuf::from("cache"),
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
                pregenerate_cache: false,
                new_threshold_days: None,
                approximate_dates_for_public: false,
                copyright_holder: None,
                hide_location_from_public: false,
            }]),
            posts: None,
            email: None,
        }
    }
}
