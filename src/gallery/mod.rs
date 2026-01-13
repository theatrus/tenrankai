// Gallery module - Main entry point
mod cache;
mod core;
mod error;
mod handlers;
pub mod image_processing;
mod indexing;
mod metadata;
pub mod metadata_sources;
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
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::SystemTime,
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;

use crate::storage::DynStorage;

use self::task_deduplicator::TaskDeduplicator;

pub type SharedGallery = Arc<Gallery>;

pub struct Gallery {
    pub(crate) config: crate::GallerySystemConfig,
    /// Resolved cache directory path (parsed from config.cache_directory URL)
    pub(crate) cache_path: PathBuf,
    /// Storage backend for cache files
    pub(crate) cache_storage: DynStorage,
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
    /// Atomic flag for synchronous cancellation checks (e.g., in spawn_blocking)
    pub(crate) shutdown_flag: Arc<AtomicBool>,
    /// Semaphore to limit concurrent image processing operations (prevents memory exhaustion)
    pub(crate) image_processing_semaphore: Arc<Semaphore>,
    /// Counter for active blocking tasks (for graceful shutdown)
    pub(crate) active_blocking_tasks: Arc<AtomicUsize>,
}

impl Gallery {
    pub fn new(config: crate::GallerySystemConfig, cache_storage: DynStorage) -> Self {
        // Parse the cache_directory URL to get the clean path
        // For filesystem: actual path. For S3: use a placeholder (we use storage abstraction)
        let cache_path = crate::storage::StorageUrl::parse(&config.cache_directory)
            .ok()
            .and_then(|url| url.filesystem_path().cloned())
            .unwrap_or_else(|| PathBuf::from("cache")); // Placeholder for S3

        // Start with empty caches - they will be loaded asynchronously via initialize_and_check_version()
        let metadata_cache = HashMap::new();
        let cache_metadata = CacheMetadata {
            version: String::new(), // Empty version will trigger loading in initialize
            last_full_refresh: SystemTime::UNIX_EPOCH,
        };

        let image_indexer = indexing::ImageIndexer::new(config.image_indexing);

        // Create sidecar metadata storage for now
        // TODO: Make this configurable through config
        let user_metadata_storage: Arc<dyn crate::metadata_storage::MetadataStorage> =
            Arc::new(crate::metadata_storage::SidecarMetadataStorage::new());

        Self {
            config,
            cache_path,
            cache_storage,
            metadata_cache: Arc::new(RwLock::new(metadata_cache)),
            cache_metadata: Arc::new(RwLock::new(cache_metadata)),
            metadata_cache_dirty: Arc::new(AtomicBool::new(false)),
            metadata_updates_since_save: Arc::new(AtomicUsize::new(0)),
            image_indexer: Arc::new(RwLock::new(image_indexer)),
            user_metadata_storage,
            task_deduplicator: TaskDeduplicator::new(),
            shutdown_token: CancellationToken::new(),
            pregeneration_token: Arc::new(Mutex::new(CancellationToken::new())),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            // Limit concurrent image processing to prevent memory exhaustion
            // Use number of CPUs as the limit for a reasonable balance
            image_processing_semaphore: Arc::new(Semaphore::new(num_cpus::get())),
            active_blocking_tasks: Arc::new(AtomicUsize::new(0)),
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

    /// Trigger shutdown of all background tasks and wait for them to complete
    pub async fn shutdown(&self) {
        info!("Shutting down gallery '{}'", self.config.name);
        // Set atomic flag for synchronous code (e.g., blocking tile generation)
        self.shutdown_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.shutdown_token.cancel();
        // Also cancel any running pre-generation tasks
        self.pregeneration_token.lock().await.cancel();

        // Wait for active blocking tasks to complete
        let mut wait_count = 0;
        loop {
            let active = self
                .active_blocking_tasks
                .load(std::sync::atomic::Ordering::SeqCst);
            if active == 0 {
                break;
            }
            wait_count += 1;
            if wait_count == 1 {
                info!(
                    "Waiting for {} active image processing task(s) to complete...",
                    active
                );
            } else if wait_count % 20 == 0 {
                // Log every 2 seconds
                info!(
                    "Still waiting for {} active image processing task(s)...",
                    active
                );
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        if wait_count > 0 {
            info!("All image processing tasks completed");
        }
    }

    /// Create a guard that tracks an active blocking task.
    /// The counter is incremented when created and decremented when dropped.
    pub(crate) fn track_blocking_task(&self) -> BlockingTaskGuard {
        self.active_blocking_tasks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        BlockingTaskGuard {
            counter: self.active_blocking_tasks.clone(),
        }
    }
}

/// RAII guard for tracking active blocking tasks.
/// Increments counter on creation, decrements on drop.
pub(crate) struct BlockingTaskGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for BlockingTaskGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
