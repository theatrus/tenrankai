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
use tokio::sync::{RwLock, Mutex};
use tokio_util::sync::CancellationToken;
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
    /// Cancellation token for background tasks
    pub(crate) shutdown_token: CancellationToken,
    /// Token for cancelling pre-generation tasks
    pub(crate) pregeneration_token: Arc<Mutex<CancellationToken>>,
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
            shutdown_token: CancellationToken::new(),
            pregeneration_token: Arc::new(Mutex::new(CancellationToken::new())),
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
        // Cancel any existing pre-generation tasks
        {
            let mut token_guard = self.pregeneration_token.lock().await;
            token_guard.cancel();
            *token_guard = CancellationToken::new();
        }

        // First refresh metadata
        self.clone().refresh_all_metadata().await?;

        // Report format coverage status
        if let Err(e) = self.report_format_coverage().await {
            error!("Failed to report format coverage: {}", e);
        }

        // Pre-generate missing formats if enabled
        if pregenerate {
            info!("Spawning background task for cache pre-generation (missing formats only)");
            let gallery_clone = self.clone();
            let token = self.pregeneration_token.lock().await.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = token.cancelled() => {
                        info!("Cache pre-generation cancelled");
                    }
                    _ = async {
                        info!("Starting cache pre-generation in background after metadata refresh");
                        if let Err(e) = gallery_clone.pregenerate_all_images_cache().await {
                            error!("Failed to pre-generate image cache: {}", e);
                        } else {
                            info!("Background cache pre-generation completed successfully");
                        }
                    } => {}
                }
            });
        }

        Ok(())
    }

    pub fn get_config(&self) -> &crate::GallerySystemConfig {
        &self.config
    }

    /// Trigger shutdown of all background tasks
    pub async fn shutdown(&self) {
        info!("Shutting down gallery '{}'", self.config.name);
        self.shutdown_token.cancel();
        // Also cancel any running pre-generation tasks
        self.pregeneration_token.lock().await.cancel();
    }
}
