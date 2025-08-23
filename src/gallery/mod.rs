// Gallery module - Main entry point
mod cache;
mod core;
mod error;
mod handlers;
mod image_processing;
mod metadata;
mod types;

// Re-export public items
pub use core::BreadcrumbItem;
pub use error::GalleryError;
pub use handlers::{gallery_handler, gallery_root_handler, image_detail_handler, image_handler};
pub use types::*;

use std::{
    collections::HashMap,
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::RwLock;

pub type SharedGallery = Arc<Gallery>;

pub struct Gallery {
    pub(crate) config: crate::GalleryConfig,
    pub(crate) app_config: crate::AppConfig,
    pub(crate) cache: Arc<RwLock<HashMap<String, CachedImage>>>,
    pub(crate) metadata_cache: Arc<RwLock<HashMap<String, ImageMetadata>>>,
    pub(crate) cache_metadata: Arc<RwLock<CacheMetadata>>,
}

impl Gallery {
    pub fn new(config: crate::GalleryConfig, app_config: crate::AppConfig) -> Self {
        let metadata_cache = cache::load_metadata_cache(&config).unwrap_or_default();
        let cache_metadata = cache::load_cache_metadata(&config).unwrap_or_else(|_| CacheMetadata {
            version: String::new(), // Empty version will trigger full refresh
            last_full_refresh: SystemTime::UNIX_EPOCH,
        });
        
        Self {
            config,
            app_config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(metadata_cache)),
            cache_metadata: Arc::new(RwLock::new(cache_metadata)),
        }
    }
    
    pub(crate) fn is_image(&self, file_name: &str) -> bool {
        let lower = file_name.to_lowercase();
        lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".bmp")
    }
}