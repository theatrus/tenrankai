//! Background worker that processes cache generation queue.

use super::{CacheCleanupRequest, CacheGenerationRequest, DynCacheQueue, QueueMessage};
use crate::{
    gallery::{ImageSize, SharedGallery},
    site::SiteManager,
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Background worker that processes cache generation requests.
pub struct CacheQueueWorker {
    queue: DynCacheQueue,
    site_manager: Arc<SiteManager>,
    shutdown_token: CancellationToken,
    concurrency: usize,
}

impl CacheQueueWorker {
    /// Create a new cache queue worker.
    pub fn new(
        queue: DynCacheQueue,
        site_manager: Arc<SiteManager>,
        shutdown_token: CancellationToken,
        concurrency: usize,
    ) -> Self {
        Self {
            queue,
            site_manager,
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
                            let permit = tokio::select! {
                                biased;
                                _ = self.shutdown_token.cancelled() => break,
                                permit = semaphore.clone().acquire_owned() => permit,
                            };
                            if let Ok(permit) = permit {
                                let site_manager = self.site_manager.clone();
                                tokio::spawn(async move {
                                    Self::process_message(message, &site_manager).await;
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

    async fn process_message(message: QueueMessage, site_manager: &SiteManager) {
        match message {
            QueueMessage::Generate(request) => {
                Self::handle_generate(request, site_manager).await;
            }
            QueueMessage::Cleanup(request) => {
                Self::handle_cleanup(request, site_manager).await;
            }
        }
    }

    async fn handle_generate(request: CacheGenerationRequest, site_manager: &SiteManager) {
        let key = request.queue_key();
        let site = site_manager.get_site_by_name(&request.site_name).await;
        let gallery = match site
            .as_ref()
            .and_then(|site| site.galleries().get(&request.gallery_name))
        {
            Some(g) => g,
            None => {
                warn!(
                    "Gallery not found for cache generation: {} (key: {})",
                    request.gallery_name, key
                );
                return;
            }
        };

        debug!(
            "Generating cache for {} in gallery {}",
            request.image_path, request.gallery_name
        );

        if tenrankai_video::is_video_path(&request.image_path) {
            if let Err(error) = gallery.prepare_gallery_video(&request.image_path).await {
                error!(%error, path = %request.image_path, "Failed to prepare queued video");
            }
            return;
        }

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
            // Update readiness after generation
            let fresh_cache_files = gallery.load_cache_file_set().await;
            gallery
                .update_preview_readiness(&request.image_path, &fresh_cache_files)
                .await;
        }
    }

    async fn handle_cleanup(request: CacheCleanupRequest, site_manager: &SiteManager) {
        let key = request.queue_key();
        let site = site_manager.get_site_by_name(&request.site_name).await;
        let gallery = match site
            .as_ref()
            .and_then(|site| site.galleries().get(&request.gallery_name))
        {
            Some(g) => g,
            None => {
                warn!(
                    "Gallery not found for cache cleanup: {} (key: {})",
                    request.gallery_name, key
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gallery::Gallery,
        site::{SiteBuilder, SiteConfig},
    };
    use std::time::{Duration, SystemTime};

    async fn site(gallery: SharedGallery) -> Arc<crate::site::Site> {
        Arc::new(
            SiteBuilder::new(SiteConfig {
                name: "site".into(),
                base_url: None,
                cookie_secret: "test".into(),
                templates: crate::TemplateConfig {
                    directories: vec![],
                },
                static_files: crate::StaticConfig {
                    directories: vec![],
                    use_redirects: false,
                },
                galleries: None,
                posts: None,
                user_database: None,
                email: None,
                config_storage: None,
                site_admins: vec![],
                hosted_mode: false,
                site_title: None,
                copyright_holder: None,
            })
            .with_galleries(Arc::new(
                [("gallery".into(), gallery)].into_iter().collect(),
            ))
            .build()
            .await
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn queued_video_resolves_replacement_gallery_and_shares_its_pool() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("clip.mov"), b"original").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(source.path().join("clip.mov"))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        let gallery = || {
            Gallery::new(
                crate::GallerySystemConfig {
                    video: Some(Default::default()),
                    ..Default::default()
                },
                Arc::new(crate::storage::FilesystemStorage::new(source.path())),
                Arc::new(crate::storage::FilesystemStorage::new(cache.path())),
            )
        };
        let old = Arc::new(gallery());
        let mut new = gallery();
        new.image_processing_semaphore = Arc::new(Semaphore::new(0));
        let new = Arc::new(new);
        let manager = SiteManager::new();
        manager
            .add_site(site(old.clone()).await, vec!["*".into()])
            .await;
        manager
            .replace_site("site", site(new.clone()).await, vec!["*".into()])
            .await;
        assert!(old.shutdown_token.is_cancelled());
        let job = CacheQueueWorker::handle_generate(
            CacheGenerationRequest::new("site", "gallery", "clip.mov"),
            &manager,
        );
        tokio::pin!(job);
        // A stale startup gallery would return immediately because it is
        // cancelled. The replacement waits for its existing processing pool.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut job)
                .await
                .is_err()
        );
        assert_eq!(
            new.active_blocking_tasks
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        new.shutdown_token.cancel();
        tokio::time::timeout(Duration::from_secs(1), job)
            .await
            .unwrap();
        assert_eq!(
            new.active_blocking_tasks
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        manager.remove_site("site").await;
    }
}
