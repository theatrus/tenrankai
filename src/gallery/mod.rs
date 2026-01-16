// Gallery module - Main entry point
mod cache;
mod core;
mod error;
mod handlers;
pub mod image_processing;
mod indexing;
mod metadata;
pub mod metadata_sources;
pub mod path_utils;
mod task_deduplicator;
mod types;

// Re-export public items
pub use cache::generate_tile_cache_filename;
pub use core::BreadcrumbItem;
pub use error::GalleryError;
pub use handlers::{
    download_folder_handler, gallery_handler_for_named, gallery_root_handler_for_named,
    image_detail_handler_for_named, image_handler_for_named, image_handler_for_named_v2,
};
pub use types::*;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::SystemTime,
};
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::cache::PersistentCache;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;

use crate::storage::DynStorage;

use self::task_deduplicator::TaskDeduplicator;

pub type SharedGallery = Arc<Gallery>;

pub struct Gallery {
    pub(crate) config: crate::GallerySystemConfig,
    /// Resolved source directory path (for filesystem sources only, for security checks)
    pub(crate) source_path: PathBuf,
    /// Storage backend for source images (will be used in upcoming phases)
    #[allow(dead_code)]
    pub(crate) source_storage: DynStorage,
    /// Resolved cache directory path (parsed from config.cache_directory URL)
    pub(crate) cache_path: PathBuf,
    /// Storage backend for cache files
    pub(crate) cache_storage: DynStorage,
    /// Image metadata cache (dimensions, EXIF, etc.)
    pub(crate) image_cache: Arc<PersistentCache<ImageMetadata>>,
    /// Folder cache (metadata, contents, counts, previews - unified)
    pub(crate) folder_cache: Arc<PersistentCache<CachedFolderMetadata>>,
    /// Cache version and refresh timestamp
    pub(crate) cache_metadata: Arc<RwLock<CacheMetadata>>,
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
    pub fn new(
        config: crate::GallerySystemConfig,
        source_storage: DynStorage,
        cache_storage: DynStorage,
    ) -> Self {
        // Parse the source_directory URL to get the path (for filesystem security checks)
        // For filesystem: actual path. For S3: use empty path (security handled by storage abstraction)
        let source_path = crate::storage::StorageUrl::parse(&config.source_directory)
            .ok()
            .and_then(|url| url.filesystem_path().cloned())
            .unwrap_or_default(); // Empty for S3 (not used)

        // Parse the cache_directory URL to get the clean path
        // For filesystem: actual path. For S3: use a placeholder (we use storage abstraction)
        let cache_path = crate::storage::StorageUrl::parse(&config.cache_directory)
            .ok()
            .and_then(|url| url.filesystem_path().cloned())
            .unwrap_or_else(|| PathBuf::from("cache")); // Placeholder for S3

        // Start with empty caches - they will be loaded asynchronously via initialize_and_check_version()
        let image_cache = Arc::new(PersistentCache::new("metadata_cache.json"));
        let folder_cache = Arc::new(PersistentCache::new("folder_cache.json"));
        let cache_metadata = CacheMetadata {
            version: String::new(), // Empty version will trigger loading in initialize
            last_full_refresh: SystemTime::UNIX_EPOCH,
        };

        let image_indexer = indexing::ImageIndexer::new(config.image_indexing);

        // Create storage-backed metadata storage using source storage
        // User metadata is stored alongside source images in the same storage backend
        let user_metadata_storage: Arc<dyn crate::metadata_storage::MetadataStorage> = Arc::new(
            crate::metadata_storage::StorageMetadataBackend::with_cache_size(
                source_storage.clone(),
                config.metadata_cache_size,
            ),
        );

        Self {
            config,
            source_path,
            source_storage,
            cache_path,
            cache_storage,
            image_cache,
            folder_cache,
            cache_metadata: Arc::new(RwLock::new(cache_metadata)),
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

    /// Returns the source directory path for filesystem-based sources.
    /// For S3-backed sources, this returns an empty path.
    pub fn source_directory(&self) -> &std::path::Path {
        &self.source_path
    }

    /// Returns true if the source is backed by S3 storage
    pub fn is_source_remote(&self) -> bool {
        self.source_path.as_os_str().is_empty()
    }

    /// Returns the source storage backend
    pub fn source_storage(&self) -> &DynStorage {
        &self.source_storage
    }

    /// Returns the cache storage backend
    pub fn cache_storage(&self) -> &DynStorage {
        &self.cache_storage
    }

    pub async fn is_metadata_cache_empty(&self) -> bool {
        self.image_cache.is_empty().await
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

    // === URL Building Helpers ===

    /// Build an image URL for a specific size variant.
    /// Format: `{url_prefix}/_image/{url_id}/{size}`
    pub(crate) fn build_image_url(&self, url_id: &str, size: &str) -> String {
        format!("{}/_image/{}/{}", self.config.url_prefix, url_id, size)
    }

    /// Build a thumbnail URL for an image.
    pub(crate) fn build_thumbnail_url(&self, url_id: &str) -> String {
        self.build_image_url(url_id, "thumbnail")
    }

    /// Build a gallery-size URL for an image.
    pub(crate) fn build_gallery_url(&self, url_id: &str) -> String {
        self.build_image_url(url_id, "gallery")
    }

    /// Build a medium-size URL for an image.
    pub(crate) fn build_medium_url(&self, url_id: &str) -> String {
        self.build_image_url(url_id, "medium")
    }

    /// Build the base image URL (without size).
    /// Format: `{url_prefix}/_image/{url_id}`
    pub(crate) fn build_image_base_url(&self, url_id: &str) -> String {
        format!("{}/_image/{}", self.config.url_prefix, url_id)
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
