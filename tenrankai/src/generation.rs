//! Process-global generated image artifact queue.

use crate::concurrency::{self, WorkerPolicy};
use crate::gallery::image_processing::OutputFormat;
use crate::gallery::{GalleryError, ImageSize, SharedGallery};
use crate::storage::StorageError;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info, warn};

const MAX_RECENT_ERRORS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationPriority {
    Background,
    Normal,
    Cleanup,
    Interactive,
}

impl GenerationPriority {
    fn score(self) -> u8 {
        match self {
            Self::Background => 10,
            Self::Normal => 50,
            Self::Cleanup => 80,
            Self::Interactive => 100,
        }
    }

    fn is_background(self) -> bool {
        matches!(self, Self::Background)
    }

    fn concurrency_priority(self) -> concurrency::Priority {
        match self {
            Self::Background => concurrency::Priority::Background,
            Self::Normal | Self::Cleanup | Self::Interactive => concurrency::Priority::Interactive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GenerationJobKindKey {
    Resized {
        size: String,
        output_format: OutputFormat,
        apply_watermark: bool,
    },
    TileSet {
        tile_size: u32,
        output_format: OutputFormat,
    },
    Pregenerate,
    Cleanup,
}

#[derive(Debug, Clone, Eq)]
struct GenerationJobKey {
    gallery_key: String,
    image_path: String,
    kind: GenerationJobKindKey,
}

impl PartialEq for GenerationJobKey {
    fn eq(&self, other: &Self) -> bool {
        self.gallery_key == other.gallery_key
            && self.image_path == other.image_path
            && self.kind == other.kind
    }
}

impl Hash for GenerationJobKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.gallery_key.hash(state);
        self.image_path.hash(state);
        self.kind.hash(state);
    }
}

#[derive(Clone)]
enum GenerationJobKind {
    Resized {
        size: String,
        output_format: OutputFormat,
    },
    TileSet {
        tile_size: u32,
        output_format: OutputFormat,
    },
    Pregenerate,
    Cleanup,
}

#[derive(Clone)]
struct GenerationJob {
    key: GenerationJobKey,
    gallery: SharedGallery,
    priority: GenerationPriority,
    sequence: u64,
    estimated_pixels: Option<usize>,
    kind: GenerationJobKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct QueueEntry {
    priority: u8,
    sequence: u64,
    key: GenerationJobKey,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            // Earlier sequence wins for otherwise-equivalent priority.
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
struct GenerationQueueState {
    pending: HashMap<GenerationJobKey, GenerationJob>,
    running: HashSet<GenerationJobKey>,
    recent_errors: HashMap<GenerationJobKey, String>,
    heap: BinaryHeap<QueueEntry>,
    sequence: u64,
    running_count: usize,
    running_background: usize,
}

pub struct GenerationManager {
    inner: Mutex<GenerationQueueState>,
    notify: Notify,
    policy: WorkerPolicy,
    interactive_workers: usize,
    background_workers: usize,
    active_interactive_jobs: AtomicUsize,
    started: AtomicBool,
}

impl GenerationManager {
    pub fn new(policy: WorkerPolicy) -> Arc<Self> {
        let interactive_budget =
            concurrency::plan_workers(None, &policy, concurrency::Priority::Interactive, None);
        let background_budget =
            concurrency::plan_workers(None, &policy, concurrency::Priority::Background, None);

        info!(
            "Generation queue budgets: interactive={} ({}) background={} ({})",
            interactive_budget.workers,
            interactive_budget.rationale,
            background_budget.workers,
            background_budget.rationale
        );

        Arc::new(Self {
            inner: Mutex::new(GenerationQueueState::default()),
            notify: Notify::new(),
            policy,
            interactive_workers: interactive_budget.workers.max(1),
            background_workers: background_budget.workers.max(1),
            active_interactive_jobs: AtomicUsize::new(0),
            started: AtomicBool::new(false),
        })
    }

    pub fn start(self: &Arc<Self>) {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        for worker_index in 0..self.interactive_workers {
            let manager = self.clone();
            tokio::spawn(async move {
                manager.worker_loop(worker_index).await;
            });
        }
    }

    pub async fn enqueue_resized(
        &self,
        gallery_key: impl Into<String>,
        gallery: SharedGallery,
        image_path: impl Into<String>,
        size: impl Into<String>,
        output_format: OutputFormat,
        apply_watermark: bool,
        priority: GenerationPriority,
    ) {
        let image_path = image_path.into();
        let size = size.into();
        let estimated_pixels = estimate_image_pixels(&gallery, &image_path).await;
        let key = GenerationJobKey {
            gallery_key: gallery_key.into(),
            image_path: image_path.clone(),
            kind: GenerationJobKindKey::Resized {
                size: size.clone(),
                output_format,
                apply_watermark,
            },
        };
        let job = GenerationJob {
            key,
            gallery,
            priority,
            sequence: 0,
            estimated_pixels,
            kind: GenerationJobKind::Resized {
                size,
                output_format,
            },
        };
        self.enqueue_job(job).await;
    }

    pub async fn enqueue_tile_set(
        &self,
        gallery_key: impl Into<String>,
        gallery: SharedGallery,
        image_path: impl Into<String>,
        tile_size: u32,
        priority: GenerationPriority,
    ) {
        let image_path = image_path.into();
        let output_format = tile_output_format();
        let estimated_pixels = estimate_image_pixels(&gallery, &image_path).await;
        let key = GenerationJobKey {
            gallery_key: gallery_key.into(),
            image_path: image_path.clone(),
            kind: GenerationJobKindKey::TileSet {
                tile_size,
                output_format,
            },
        };
        let job = GenerationJob {
            key,
            gallery,
            priority,
            sequence: 0,
            estimated_pixels,
            kind: GenerationJobKind::TileSet {
                tile_size,
                output_format,
            },
        };
        self.enqueue_job(job).await;
    }

    pub async fn enqueue_pregenerate(
        &self,
        gallery_key: impl Into<String>,
        gallery: SharedGallery,
        image_path: impl Into<String>,
        priority: GenerationPriority,
    ) {
        let image_path = image_path.into();
        let estimated_pixels = estimate_image_pixels(&gallery, &image_path).await;
        let key = GenerationJobKey {
            gallery_key: gallery_key.into(),
            image_path: image_path.clone(),
            kind: GenerationJobKindKey::Pregenerate,
        };
        let job = GenerationJob {
            key,
            gallery,
            priority,
            sequence: 0,
            estimated_pixels,
            kind: GenerationJobKind::Pregenerate,
        };
        self.enqueue_job(job).await;
    }

    pub async fn enqueue_gallery_pregeneration(
        &self,
        gallery_key: impl Into<String>,
        gallery: SharedGallery,
    ) -> usize {
        let gallery_key = gallery_key.into();
        let image_paths: Vec<String> = {
            let cache = gallery.image_cache.read_all().await;
            cache
                .keys()
                .filter(|path| gallery.is_image(path))
                .cloned()
                .collect()
        };

        let count = image_paths.len();
        for image_path in image_paths {
            self.enqueue_pregenerate(
                gallery_key.clone(),
                gallery.clone(),
                image_path,
                GenerationPriority::Background,
            )
            .await;
        }

        count
    }

    pub async fn enqueue_cleanup(
        &self,
        gallery_key: impl Into<String>,
        gallery: SharedGallery,
        image_path: impl Into<String>,
    ) {
        let image_path = image_path.into();
        let key = GenerationJobKey {
            gallery_key: gallery_key.into(),
            image_path: image_path.clone(),
            kind: GenerationJobKindKey::Cleanup,
        };
        let job = GenerationJob {
            key,
            gallery,
            priority: GenerationPriority::Cleanup,
            sequence: 0,
            estimated_pixels: None,
            kind: GenerationJobKind::Cleanup,
        };
        self.enqueue_job(job).await;
    }

    pub async fn queue_depth(&self) -> usize {
        self.inner.lock().await.pending.len()
    }

    async fn enqueue_job(&self, mut job: GenerationJob) {
        let mut state = self.inner.lock().await;

        if state.running.contains(&job.key) {
            debug!(path = %job.key.image_path, "Generation job is already running");
            return;
        }

        if state.pending.contains_key(&job.key) {
            let should_upgrade = state
                .pending
                .get(&job.key)
                .map(|existing| job.priority.score() > existing.priority.score())
                .unwrap_or(false);

            if should_upgrade {
                state.sequence += 1;
                let sequence = state.sequence;
                let entry = {
                    let existing = state.pending.get_mut(&job.key).expect("job exists");
                    existing.priority = job.priority;
                    existing.sequence = sequence;
                    QueueEntry {
                        priority: existing.priority.score(),
                        sequence: existing.sequence,
                        key: existing.key.clone(),
                    }
                };
                state.heap.push(entry);
                self.notify.notify_one();
            }
            return;
        }

        state.sequence += 1;
        job.sequence = state.sequence;
        state.heap.push(QueueEntry {
            priority: job.priority.score(),
            sequence: job.sequence,
            key: job.key.clone(),
        });
        state.pending.insert(job.key.clone(), job);
        self.notify.notify_one();
    }

    async fn worker_loop(self: Arc<Self>, worker_index: usize) {
        debug!(worker_index, "Generation worker started");
        loop {
            let job = self.next_job().await;
            self.execute_job(job).await;
        }
    }

    async fn next_job(&self) -> GenerationJob {
        loop {
            if let Some(job) = self.try_pop_job().await {
                return job;
            }
            self.notify.notified().await;
        }
    }

    async fn try_pop_job(&self) -> Option<GenerationJob> {
        let mut state = self.inner.lock().await;

        while let Some(entry) = state.heap.pop() {
            let Some(candidate) = state.pending.get(&entry.key).cloned() else {
                continue;
            };

            if candidate.priority.score() != entry.priority || candidate.sequence != entry.sequence
            {
                continue;
            }

            if !self.can_dispatch(&state, &candidate) {
                state.heap.push(entry);
                return None;
            }

            let job = state.pending.remove(&entry.key)?;
            state.running.insert(entry.key);
            state.running_count += 1;
            if job.priority.is_background() {
                state.running_background += 1;
            }
            return Some(job);
        }

        None
    }

    fn can_dispatch(&self, state: &GenerationQueueState, job: &GenerationJob) -> bool {
        if job.priority.is_background() {
            if self.active_interactive_jobs.load(AtomicOrdering::SeqCst) > 0 {
                return false;
            }
            if state.running_background >= self.background_workers {
                return false;
            }
        }

        let budget = concurrency::plan_workers(
            None,
            &self.policy,
            job.priority.concurrency_priority(),
            job.estimated_pixels,
        );
        state.running_count < budget.workers.max(1)
    }

    async fn execute_job(&self, job: GenerationJob) {
        let is_interactive = matches!(job.priority, GenerationPriority::Interactive);
        if is_interactive {
            self.active_interactive_jobs
                .fetch_add(1, AtomicOrdering::SeqCst);
        }

        let key = job.key.clone();
        let priority = job.priority;
        let result = execute_generation_job(job).await;

        if is_interactive {
            self.active_interactive_jobs
                .fetch_sub(1, AtomicOrdering::SeqCst);
        }

        let mut state = self.inner.lock().await;
        state.running.remove(&key);
        state.running_count = state.running_count.saturating_sub(1);
        if priority.is_background() {
            state.running_background = state.running_background.saturating_sub(1);
        }

        match result {
            Ok(()) => {
                state.recent_errors.remove(&key);
            }
            Err(error) => {
                warn!(path = %key.image_path, error = %error, "Generation job failed");
                if state.recent_errors.len() >= MAX_RECENT_ERRORS {
                    state.recent_errors.clear();
                }
                state.recent_errors.insert(key, error.to_string());
            }
        }

        self.notify.notify_waiters();
    }
}

async fn execute_generation_job(job: GenerationJob) -> Result<(), GalleryError> {
    match job.kind {
        GenerationJobKind::Resized {
            size,
            output_format,
        } => {
            debug!(path = %job.key.image_path, size = %size, "Generating resized image");
            job.gallery
                .get_resized_image(&job.key.image_path, &size, output_format)
                .await?;
            update_readiness(&job.gallery, &job.key.image_path).await;
            Ok(())
        }
        GenerationJobKind::TileSet {
            tile_size,
            output_format,
        } => {
            debug!(
                path = %job.key.image_path,
                tile_size,
                format = output_format.extension(),
                "Generating tile set"
            );
            job.gallery
                .get_image_tile_with_size(&job.key.image_path, 0, 0, tile_size)
                .await?;
            Ok(())
        }
        GenerationJobKind::Pregenerate => {
            debug!(path = %job.key.image_path, "Pre-generating image artifacts");
            let cache_files = job.gallery.load_cache_file_set().await;
            job.gallery
                .pregenerate_image_cache(&job.key.image_path, &cache_files)
                .await?;
            job.gallery
                .pregenerate_tiles_for_image(&job.key.image_path, &cache_files)
                .await?;
            update_readiness(&job.gallery, &job.key.image_path).await;
            Ok(())
        }
        GenerationJobKind::Cleanup => {
            cleanup_cache_files_for_path(&job.gallery, &job.key.image_path).await
        }
    }
}

async fn update_readiness(gallery: &SharedGallery, image_path: &str) {
    let fresh_cache_files = gallery.load_cache_file_set().await;
    gallery
        .update_preview_readiness(image_path, &fresh_cache_files)
        .await;
}

async fn estimate_image_pixels(gallery: &SharedGallery, image_path: &str) -> Option<usize> {
    let cache = gallery.image_cache.read_all().await;
    cache.get(image_path).map(|metadata| {
        let (width, height) = metadata.dimensions;
        (width as usize).saturating_mul(height as usize)
    })
}

pub fn tile_output_format() -> OutputFormat {
    #[cfg(feature = "avif")]
    {
        OutputFormat::Avif
    }
    #[cfg(not(feature = "avif"))]
    {
        OutputFormat::WebP
    }
}

async fn cleanup_cache_files_for_path(
    gallery: &SharedGallery,
    image_path: &str,
) -> Result<(), GalleryError> {
    let formats = OutputFormat::ALL;
    let mut deleted_count = 0usize;

    for size in ImageSize::ALL {
        let size_str = size.as_str();
        for format in formats {
            deleted_count += delete_cache_if_exists(
                gallery,
                &gallery.generate_cache_filename(image_path, &size_str, format.extension(), false),
            )
            .await?;

            if size.supports_watermark() {
                deleted_count += delete_cache_if_exists(
                    gallery,
                    &gallery.generate_cache_filename(
                        image_path,
                        &size_str,
                        format.extension(),
                        true,
                    ),
                )
                .await?;
            }
        }
    }

    deleted_count += cleanup_tile_files_for_path(gallery, image_path).await?;

    if deleted_count > 0 {
        debug!(path = %image_path, deleted_count, "Deleted generated cache files");
    }

    Ok(())
}

async fn cleanup_tile_files_for_path(
    gallery: &SharedGallery,
    image_path: &str,
) -> Result<usize, GalleryError> {
    let Some(tile_config) = &gallery.config.tiles else {
        return Ok(0);
    };

    let Some((width, height)) = ({
        let cache = gallery.image_cache.read_all().await;
        cache.get(image_path).map(|metadata| metadata.dimensions)
    }) else {
        return Ok(0);
    };

    let max_tile_dimension = 8192;
    let max_dimension = width.max(height);
    let (tiled_width, tiled_height) = if max_dimension > max_tile_dimension {
        let scale = max_tile_dimension as f32 / max_dimension as f32;
        (
            (width as f32 * scale) as u32,
            (height as f32 * scale) as u32,
        )
    } else {
        (width, height)
    };

    let grid_width = tiled_width.div_ceil(tile_config.tile_size);
    let grid_height = tiled_height.div_ceil(tile_config.tile_size);
    let mut deleted_count = 0usize;

    for tile_y in 0..grid_height {
        for tile_x in 0..grid_width {
            for is_retina in [false, true] {
                let cache_filename = crate::gallery::generate_tile_cache_filename(
                    image_path,
                    tile_x,
                    tile_y,
                    tile_config.tile_size,
                    is_retina,
                    tile_output_format().extension(),
                );
                deleted_count += delete_cache_if_exists(gallery, &cache_filename).await?;
            }
        }
    }

    Ok(deleted_count)
}

async fn delete_cache_if_exists(
    gallery: &SharedGallery,
    cache_filename: &str,
) -> Result<usize, GalleryError> {
    match gallery.cache_storage().exists(cache_filename).await {
        Ok(true) => match gallery.cache_storage().delete(cache_filename).await {
            Ok(()) | Err(StorageError::NotFound(_)) => Ok(1),
            Err(e) => {
                error!(cache_filename, error = %e, "Failed to delete cache file");
                Err(e.into())
            }
        },
        Ok(false) | Err(StorageError::NotFound(_)) => Ok(0),
        Err(e) => {
            error!(cache_filename, error = %e, "Failed to check cache file before delete");
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_depth_deduplicates_and_upgrades_priority() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        let gallery = test_gallery();

        manager
            .enqueue_resized(
                "site:gallery",
                gallery.clone(),
                "image.jpg",
                "gallery",
                OutputFormat::WebP,
                false,
                GenerationPriority::Background,
            )
            .await;
        manager
            .enqueue_resized(
                "site:gallery",
                gallery,
                "image.jpg",
                "gallery",
                OutputFormat::WebP,
                false,
                GenerationPriority::Interactive,
            )
            .await;

        assert_eq!(manager.queue_depth().await, 1);
        let state = manager.inner.lock().await;
        let job = state.pending.values().next().unwrap();
        assert_eq!(job.priority, GenerationPriority::Interactive);
    }

    #[tokio::test]
    async fn queue_keeps_tile_sets_as_single_job() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        let gallery = test_gallery();

        manager
            .enqueue_tile_set(
                "site:gallery",
                gallery.clone(),
                "image.jpg",
                1024,
                GenerationPriority::Interactive,
            )
            .await;
        manager
            .enqueue_tile_set(
                "site:gallery",
                gallery,
                "image.jpg",
                1024,
                GenerationPriority::Interactive,
            )
            .await;

        assert_eq!(manager.queue_depth().await, 1);
    }

    fn test_gallery() -> SharedGallery {
        use crate::config::defaults;
        use crate::storage::FilesystemStorage;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let config = crate::GallerySystemConfig {
            name: "gallery".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            url_prefix: "/gallery".to_string(),
            thumbnail: defaults::default_thumbnail_size(),
            gallery_size: defaults::default_gallery_size(),
            medium: defaults::default_medium_size(),
            large: defaults::default_large_size(),
            tiles: Some(crate::TileConfig { tile_size: 1024 }),
            ..Default::default()
        };

        let source_storage = Arc::new(FilesystemStorage::new(source_dir));
        let cache_storage = Arc::new(FilesystemStorage::new(cache_dir));
        Arc::new(crate::gallery::Gallery::new(
            config,
            source_storage,
            cache_storage,
        ))
    }
}
