// Gallery module - Main entry point
mod cache;
mod core;
mod error;
mod handlers;
mod image_processing;
mod metadata;
mod types;

// Re-export public items
pub use error::GalleryError;
pub use handlers::{
    gallery_handler_for_named,
    gallery_root_handler_for_named, 
    image_detail_handler_for_named,
    image_handler_for_named,
};
pub use types::*;

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::SystemTime,
};
use tokio::sync::RwLock;
use tracing::info;

pub type SharedGallery = Arc<Gallery>;

pub struct Gallery {
    pub(crate) config: crate::GallerySystemConfig,
    pub(crate) app_config: crate::AppConfig,
    pub(crate) metadata_cache: Arc<RwLock<HashMap<String, ImageMetadata>>>,
    pub(crate) cache_metadata: Arc<RwLock<CacheMetadata>>,
    pub(crate) metadata_cache_dirty: Arc<AtomicBool>,
    pub(crate) metadata_updates_since_save: Arc<AtomicUsize>,
}

impl Gallery {
    pub fn new(config: crate::GallerySystemConfig, app_config: crate::AppConfig) -> Self {
        let metadata_cache = cache::load_metadata_cache(&config).unwrap_or_default();
        let cache_metadata =
            cache::load_cache_metadata(&config).unwrap_or_else(|_| CacheMetadata {
                version: String::new(), // Empty version will trigger full refresh
                last_full_refresh: SystemTime::UNIX_EPOCH,
            });

        Self {
            config,
            app_config,
            metadata_cache: Arc::new(RwLock::new(metadata_cache)),
            cache_metadata: Arc::new(RwLock::new(cache_metadata)),
            metadata_cache_dirty: Arc::new(AtomicBool::new(false)),
            metadata_updates_since_save: Arc::new(AtomicUsize::new(0)),
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

    pub fn source_directory(&self) -> &std::path::Path {
        &self.config.source_directory
    }

    pub async fn is_metadata_cache_empty(&self) -> bool {
        self.metadata_cache.read().await.is_empty()
    }

    pub(crate) fn is_new(&self, modification_date: Option<SystemTime>) -> bool {
        match (self.config.new_threshold_days, modification_date) {
            (Some(days), Some(mod_date)) => {
                if let Ok(elapsed) = SystemTime::now().duration_since(mod_date) {
                    let seconds_in_day = 86400;
                    let threshold_seconds = days as u64 * seconds_in_day;
                    elapsed.as_secs() <= threshold_seconds
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub async fn refresh_metadata_and_pregenerate_cache(
        self: Arc<Self>,
        pregenerate: bool,
    ) -> Result<(), GalleryError> {
        // First refresh metadata
        self.clone().refresh_all_metadata().await?;

        // Then optionally pre-generate cache
        if pregenerate {
            info!("Starting cache pre-generation after metadata refresh");
            self.pregenerate_all_images_cache().await?;
        }

        Ok(())
    }

    pub fn get_config(&self) -> &crate::GallerySystemConfig {
        &self.config
    }
}
