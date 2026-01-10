// Gallery module - Main entry point
mod cache;
mod core;
mod error;
mod handlers;
pub mod image_processing;
mod indexing;
mod metadata;
mod metadata_sources;
mod task_deduplicator;
mod types;

// Re-export public items
pub use core::BreadcrumbItem;
pub use error::GalleryError;
pub use handlers::{
    gallery_handler_for_named, gallery_root_handler_for_named, image_detail_handler_for_named,
    image_handler_for_named, image_handler_for_named_v2,
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
use tracing::error;
use tracing::info;

use self::task_deduplicator::TaskDeduplicator;

pub type SharedGallery = Arc<Gallery>;

pub struct Gallery {
    pub(crate) config: crate::GallerySystemConfig,
    pub(crate) metadata_cache: Arc<RwLock<HashMap<String, ImageMetadata>>>,
    pub(crate) cache_metadata: Arc<RwLock<CacheMetadata>>,
    pub(crate) metadata_cache_dirty: Arc<AtomicBool>,
    pub(crate) metadata_updates_since_save: Arc<AtomicUsize>,
    pub(crate) image_indexer: Arc<RwLock<indexing::ImageIndexer>>,
    pub(crate) user_metadata_storage: Arc<dyn crate::metadata_storage::MetadataStorage>,
    pub(crate) task_deduplicator: TaskDeduplicator,
}

impl Gallery {
    pub fn new(config: crate::GallerySystemConfig) -> Self {
        let metadata_cache = crate::cache::load_image_metadata_cache(&config).unwrap_or_default();
        let cache_metadata =
            crate::cache::load_cache_version_metadata(&config).unwrap_or_else(|_| CacheMetadata {
                version: String::new(), // Empty version will trigger full refresh
                last_full_refresh: SystemTime::UNIX_EPOCH,
            });

        let image_indexer = indexing::ImageIndexer::new(config.image_indexing);

        // Create sidecar metadata storage for now
        // TODO: Make this configurable through config
        let user_metadata_storage: Arc<dyn crate::metadata_storage::MetadataStorage> =
            Arc::new(crate::metadata_storage::SidecarMetadataStorage::new());

        Self {
            config,
            metadata_cache: Arc::new(RwLock::new(metadata_cache)),
            cache_metadata: Arc::new(RwLock::new(cache_metadata)),
            metadata_cache_dirty: Arc::new(AtomicBool::new(false)),
            metadata_updates_since_save: Arc::new(AtomicUsize::new(0)),
            image_indexer: Arc::new(RwLock::new(image_indexer)),
            user_metadata_storage,
            task_deduplicator: TaskDeduplicator::new(),
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
            || lower.ends_with(".avif")
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

        // Generate any missing formats (especially AVIF) after metadata refresh
        // This runs regardless of pregenerate setting to ensure format completeness
        {
            let gallery_clone = self.clone();
            tokio::spawn(async move {
                info!("Starting missing format generation in background after metadata refresh");
                if let Err(e) = gallery_clone.generate_all_missing_formats().await {
                    error!("Failed to generate missing formats: {}", e);
                } else {
                    info!("Background missing format generation completed successfully");
                }
            });
        }

        // Then optionally pre-generate cache in background (full regeneration)
        if pregenerate {
            info!("Spawning background task for full cache pre-generation");
            let gallery_clone = self.clone();
            tokio::spawn(async move {
                info!("Starting full cache pre-generation in background after metadata refresh");
                if let Err(e) = gallery_clone.pregenerate_all_images_cache().await {
                    error!("Failed to pre-generate image cache: {}", e);
                } else {
                    info!("Background full cache pre-generation completed successfully");
                }
            });
        }

        Ok(())
    }

    pub fn get_config(&self) -> &crate::GallerySystemConfig {
        &self.config
    }
}
