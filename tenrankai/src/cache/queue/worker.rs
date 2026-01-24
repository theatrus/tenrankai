//! Background worker that processes cache generation queue.

use super::{CacheCleanupRequest, CacheGenerationRequest, DynCacheQueue, QueueMessage};
use crate::gallery::{ImageSize, SharedGallery};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Background worker that processes cache generation requests.
pub struct CacheQueueWorker {
    queue: DynCacheQueue,
    galleries: HashMap<String, SharedGallery>,
    shutdown_token: CancellationToken,
    concurrency: usize,
}

impl CacheQueueWorker {
    /// Create a new cache queue worker.
    pub fn new(
        queue: DynCacheQueue,
        galleries: HashMap<String, SharedGallery>,
        shutdown_token: CancellationToken,
        concurrency: usize,
    ) -> Self {
        Self {
            queue,
            galleries,
            shutdown_token,
            concurrency,
        }
    }

    /// Start the worker (spawns background task).
    pub fn start(self: Arc<Self>) {
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run().await;
        });
    }

    async fn run(&self) {
        info!(
            "Cache queue worker started (type={}, concurrency={})",
            self.queue.queue_type(),
            self.concurrency
        );

        let semaphore = Arc::new(Semaphore::new(self.concurrency));

        loop {
            tokio::select! {
                _ = self.shutdown_token.cancelled() => {
                    info!("Cache queue worker shutting down");
                    break;
                }
                msg = self.queue.receive() => {
                    match msg {
                        Some(message) => {
                            let permit = semaphore.clone().acquire_owned().await;
                            if let Ok(permit) = permit {
                                let galleries = self.galleries.clone();
                                tokio::spawn(async move {
                                    Self::process_message(message, &galleries).await;
                                    drop(permit);
                                });
                            }
                        }
                        None => {
                            info!("Cache queue closed, worker exiting");
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn process_message(message: QueueMessage, galleries: &HashMap<String, SharedGallery>) {
        match message {
            QueueMessage::Generate(request) => {
                Self::handle_generate(request, galleries).await;
            }
            QueueMessage::Cleanup(request) => {
                Self::handle_cleanup(request, galleries).await;
            }
        }
    }

    async fn handle_generate(
        request: CacheGenerationRequest,
        galleries: &HashMap<String, SharedGallery>,
    ) {
        let gallery = match galleries.get(&request.gallery_name) {
            Some(g) => g,
            None => {
                warn!(
                    "Gallery not found for cache generation: {}",
                    request.gallery_name
                );
                return;
            }
        };

        debug!(
            "Generating cache for {} in gallery {}",
            request.image_path, request.gallery_name
        );

        // Load cache files for fast lookup
        let cache_files = gallery.load_cache_file_set().await;

        // Generate cache using existing pregenerate_image_cache
        if let Err(e) = gallery
            .pregenerate_image_cache(&request.image_path, &cache_files)
            .await
        {
            error!("Failed to generate cache for {}: {}", request.image_path, e);
        } else {
            debug!("Generated cache for {}", request.image_path);
        }
    }

    async fn handle_cleanup(
        request: CacheCleanupRequest,
        galleries: &HashMap<String, SharedGallery>,
    ) {
        let gallery = match galleries.get(&request.gallery_name) {
            Some(g) => g,
            None => {
                warn!(
                    "Gallery not found for cache cleanup: {}",
                    request.gallery_name
                );
                return;
            }
        };

        debug!(
            "Cleaning up cache for {} in gallery {}",
            request.old_path, request.gallery_name
        );

        if let Err(e) = Self::delete_cache_files_for_path(gallery, &request.old_path).await {
            error!("Failed to cleanup cache for {}: {}", request.old_path, e);
        } else {
            debug!("Cleaned up cache for {}", request.old_path);
        }
    }

    /// Delete all cache files for a given image path.
    async fn delete_cache_files_for_path(
        gallery: &SharedGallery,
        image_path: &str,
    ) -> Result<(), crate::gallery::GalleryError> {
        let formats = ["jpg", "webp", "png", "avif"];
        let mut deleted_count = 0;

        for size in ImageSize::ALL {
            let size_str = size.as_str();
            for format in &formats {
                // Without watermark
                let cache_filename =
                    gallery.generate_cache_filename(image_path, &size_str, format, false);

                if gallery
                    .cache_storage()
                    .exists(&cache_filename)
                    .await
                    .unwrap_or(false)
                {
                    if let Err(e) = gallery.cache_storage().delete(&cache_filename).await {
                        debug!("Failed to delete cache file {}: {}", cache_filename, e);
                    } else {
                        deleted_count += 1;
                    }
                }

                // With watermark (only for sizes that support it)
                if size.supports_watermark() {
                    let cache_filename_wm =
                        gallery.generate_cache_filename(image_path, &size_str, format, true);

                    if gallery
                        .cache_storage()
                        .exists(&cache_filename_wm)
                        .await
                        .unwrap_or(false)
                    {
                        if let Err(e) = gallery.cache_storage().delete(&cache_filename_wm).await {
                            debug!(
                                "Failed to delete watermarked cache file {}: {}",
                                cache_filename_wm, e
                            );
                        } else {
                            deleted_count += 1;
                        }
                    }
                }
            }
        }

        if deleted_count > 0 {
            debug!("Deleted {} cache files for {}", deleted_count, image_path);
        }

        Ok(())
    }
}
