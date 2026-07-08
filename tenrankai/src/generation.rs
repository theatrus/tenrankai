//! Process-global generated image artifact queue.

use crate::concurrency::{self, WorkerPolicy};
use crate::gallery::image_processing::OutputFormat;
use crate::gallery::{GalleryError, ImageSize, SharedGallery};
use crate::storage::StorageError;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info, warn};

const MAX_RECENT_ERRORS: usize = 512;

/// The process-global generation manager. Installed once at startup so paths that
/// don't carry an `AppState` (periodic cache refresh, site reload) can enqueue
/// work onto the same queue instead of spawning parallel unqueued generation.
static GLOBAL_MANAGER: OnceLock<Arc<GenerationManager>> = OnceLock::new();

/// The process-global generation manager, if one has been installed.
pub fn global_manager() -> Option<Arc<GenerationManager>> {
    GLOBAL_MANAGER.get().cloned()
}

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
    // Same-key jobs requested while their key is already running. Re-enqueued when
    // the in-flight job completes so a regeneration request (e.g. an image replaced
    // mid-generation) isn't silently lost.
    rerun: HashMap<GenerationJobKey, GenerationJob>,
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
    // System facts snapshotted once at startup so per-dispatch admission control
    // (can_dispatch) never probes /proc or spawns a sysctl subprocess while
    // holding the queue mutex.
    cores: usize,
    available_memory_bytes: Option<u64>,
    active_interactive_jobs: AtomicUsize,
    started: AtomicBool,
}

impl GenerationManager {
    pub fn new(policy: WorkerPolicy) -> Arc<Self> {
        let cores = concurrency::logical_cores();
        let available_memory_bytes = concurrency::available_memory_bytes();

        let interactive_budget = concurrency::plan_workers_with_facts(
            None,
            &policy,
            concurrency::Priority::Interactive,
            None,
            cores,
            available_memory_bytes,
        );
        let background_budget = concurrency::plan_workers_with_facts(
            None,
            &policy,
            concurrency::Priority::Background,
            None,
            cores,
            available_memory_bytes,
        );

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
            cores,
            available_memory_bytes,
            active_interactive_jobs: AtomicUsize::new(0),
            started: AtomicBool::new(false),
        })
    }

    /// Register this manager as the process-global instance. Call once at startup,
    /// before any site reload or periodic refresh can run.
    pub fn install_global(self: &Arc<Self>) {
        if GLOBAL_MANAGER.set(self.clone()).is_err() {
            warn!("Process-global generation manager already installed; ignoring");
        }
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

    #[allow(clippy::too_many_arguments)]
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

    /// Remove and return the most recent failure recorded for a resized-image job,
    /// if any. Callers use this to break out of the poll/re-enqueue loop when a
    /// generation persistently fails; taking the error lets the next request retry.
    pub async fn take_recent_resized_error(
        &self,
        gallery_key: &str,
        image_path: &str,
        size: &str,
        output_format: OutputFormat,
        apply_watermark: bool,
    ) -> Option<String> {
        let key = GenerationJobKey {
            gallery_key: gallery_key.to_string(),
            image_path: image_path.to_string(),
            kind: GenerationJobKindKey::Resized {
                size: size.to_string(),
                output_format,
                apply_watermark,
            },
        };
        self.inner.lock().await.recent_errors.remove(&key)
    }

    /// Remove and return the most recent failure recorded for a tile-set job, if any.
    pub async fn take_recent_tile_error(
        &self,
        gallery_key: &str,
        image_path: &str,
        tile_size: u32,
        output_format: OutputFormat,
    ) -> Option<String> {
        let key = GenerationJobKey {
            gallery_key: gallery_key.to_string(),
            image_path: image_path.to_string(),
            kind: GenerationJobKindKey::TileSet {
                tile_size,
                output_format,
            },
        };
        self.inner.lock().await.recent_errors.remove(&key)
    }

    async fn enqueue_job(&self, mut job: GenerationJob) {
        let mut state = self.inner.lock().await;

        if state.running.contains(&job.key) {
            // The job is mid-flight. Remember to run it again on completion so this
            // request isn't lost; keep the highest-priority pending rerun.
            debug!(
                path = %job.key.image_path,
                "Generation job already running; scheduling rerun on completion"
            );
            let replace = state
                .rerun
                .get(&job.key)
                .map(|existing| job.priority.score() > existing.priority.score())
                .unwrap_or(true);
            if replace {
                state.rerun.insert(job.key.clone(), job);
            }
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

        // Entries that are valid but can't be dispatched right now (e.g. a large
        // image that exceeds the current memory budget). We hold them aside and
        // return them to the heap so we can keep scanning for a job that *can*
        // run — otherwise one undispatchable head-of-queue entry would stall every
        // other ready job behind it.
        let mut deferred: Vec<QueueEntry> = Vec::new();
        let mut popped = None;

        while let Some(entry) = state.heap.pop() {
            let Some(candidate) = state.pending.get(&entry.key).cloned() else {
                continue;
            };

            if candidate.priority.score() != entry.priority || candidate.sequence != entry.sequence
            {
                continue;
            }

            if !self.can_dispatch(&state, &candidate) {
                deferred.push(entry);
                continue;
            }

            let job = state
                .pending
                .remove(&entry.key)
                .expect("pending entry present for dispatchable candidate");
            state.running.insert(entry.key);
            state.running_count += 1;
            if job.priority.is_background() {
                state.running_background += 1;
            }
            if matches!(job.priority, GenerationPriority::Interactive) {
                // Count the interactive job under the queue lock, atomically with
                // dispatch, so a concurrent background dispatch can't observe zero
                // interactive jobs in the window before the worker starts running.
                self.active_interactive_jobs
                    .fetch_add(1, AtomicOrdering::SeqCst);
            }
            popped = Some(job);
            break;
        }

        // Return deferred entries so they're retried on a later dispatch.
        for entry in deferred {
            state.heap.push(entry);
        }

        popped
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

        let budget = concurrency::plan_workers_with_facts(
            None,
            &self.policy,
            job.priority.concurrency_priority(),
            job.estimated_pixels,
            self.cores,
            self.available_memory_bytes,
        );
        state.running_count < budget.workers.max(1)
    }

    async fn execute_job(&self, job: GenerationJob) {
        // The interactive counter is incremented at dispatch time (in try_pop_job,
        // under the queue lock). Decrement it here on completion.
        let is_interactive = matches!(job.priority, GenerationPriority::Interactive);
        let key = job.key.clone();
        let priority = job.priority;
        let result = execute_generation_job(job).await;

        let mut state = self.inner.lock().await;
        state.running.remove(&key);
        state.running_count = state.running_count.saturating_sub(1);
        if priority.is_background() {
            state.running_background = state.running_background.saturating_sub(1);
        }
        if is_interactive {
            self.active_interactive_jobs
                .fetch_sub(1, AtomicOrdering::SeqCst);
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
                state.recent_errors.insert(key.clone(), error.to_string());
            }
        }

        // If a same-key job was requested while this one ran, queue it now.
        if let Some(mut rerun_job) = state.rerun.remove(&key) {
            state.sequence += 1;
            rerun_job.sequence = state.sequence;
            state.heap.push(QueueEntry {
                priority: rerun_job.priority.score(),
                sequence: rerun_job.sequence,
                key: rerun_job.key.clone(),
            });
            state.pending.insert(rerun_job.key.clone(), rerun_job);
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
                .generate_all_tiles(&job.key.image_path, tile_size)
                .await?;
            Ok(())
        }
        GenerationJobKind::Pregenerate => {
            debug!(path = %job.key.image_path, "Pre-generating image artifacts");
            let cache_files = job.gallery.load_cache_file_set().await;
            job.gallery
                .pregenerate_image_cache(&job.key.image_path, &cache_files)
                .await?;
            // Only pre-build tiles when tile pre-generation is enabled; on-demand
            // tile generation still happens lazily via the serve path otherwise.
            if job.gallery.should_pregenerate_tiles() {
                job.gallery
                    .pregenerate_tiles_for_image(&job.key.image_path, &cache_files)
                    .await?;
            }
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

/// Every cache-file extension we may have written across build configurations.
/// Cleanup must delete all of them regardless of this build's feature flags — e.g.
/// a `--no-default-features` (no AVIF) binary still has to remove `.avif` artifacts
/// left behind by an AVIF-enabled build. Using `OutputFormat::ALL` here would omit
/// AVIF in non-AVIF builds and leak those files.
const CLEANUP_IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "webp", "png", "avif"];
/// Tile artifacts are only ever AVIF (or a WebP fallback), but must likewise be
/// cleaned regardless of this build's feature flags.
const CLEANUP_TILE_EXTENSIONS: [&str; 2] = ["avif", "webp"];

/// Build the processed-image cache filename for a literal extension string.
///
/// Unlike [`crate::gallery::Gallery::generate_cache_filename`], this does not
/// round-trip the extension through `OutputFormat` — which maps unknown extensions
/// (notably "avif" in a `--no-default-features` build) back to Jpeg and would name
/// the file `.jpg`. The cache key hash is derived from the extension string either
/// way, so this reproduces exactly the name an AVIF-enabled build wrote, letting a
/// non-AVIF build delete it. Matches `CacheType::filename` for ProcessedImage.
fn processed_image_cache_filename(
    gallery: &SharedGallery,
    image_path: &str,
    size: &str,
    ext: &str,
    has_watermark: bool,
) -> String {
    let hash = gallery.generate_image_cache_key(image_path, size, ext, has_watermark);
    let suffix = if has_watermark { "_watermarked" } else { "" };
    format!("{hash}{suffix}.{ext}")
}

async fn cleanup_cache_files_for_path(
    gallery: &SharedGallery,
    image_path: &str,
) -> Result<(), GalleryError> {
    let mut deleted_count = 0usize;

    for size in ImageSize::ALL {
        let size_str = size.as_str();
        for ext in CLEANUP_IMAGE_EXTENSIONS {
            deleted_count += delete_cache_if_exists(
                gallery,
                &processed_image_cache_filename(gallery, image_path, &size_str, ext, false),
            )
            .await?;

            if size.supports_watermark() {
                deleted_count += delete_cache_if_exists(
                    gallery,
                    &processed_image_cache_filename(gallery, image_path, &size_str, ext, true),
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
                for ext in CLEANUP_TILE_EXTENSIONS {
                    let cache_filename = crate::gallery::generate_tile_cache_filename(
                        image_path,
                        tile_x,
                        tile_y,
                        tile_config.tile_size,
                        is_retina,
                        ext,
                    );
                    deleted_count += delete_cache_if_exists(gallery, &cache_filename).await?;
                }
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

    #[tokio::test]
    async fn enqueue_while_running_schedules_rerun() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        let gallery = test_gallery();

        let key = GenerationJobKey {
            gallery_key: "site:gallery".to_string(),
            image_path: "image.jpg".to_string(),
            kind: GenerationJobKindKey::Pregenerate,
        };

        // Simulate the job being in-flight.
        {
            let mut state = manager.inner.lock().await;
            state.running.insert(key.clone());
            state.running_count = 1;
        }

        // A same-key enqueue while running must not be dropped; it's stashed as a rerun.
        manager
            .enqueue_pregenerate(
                "site:gallery",
                gallery.clone(),
                "image.jpg",
                GenerationPriority::Normal,
            )
            .await;
        {
            let state = manager.inner.lock().await;
            assert!(
                state.pending.is_empty(),
                "must not be pending while running"
            );
            assert_eq!(
                state.rerun.get(&key).map(|j| j.priority),
                Some(GenerationPriority::Normal)
            );
        }

        // A higher-priority re-enqueue upgrades the stashed rerun.
        manager
            .enqueue_pregenerate(
                "site:gallery",
                gallery,
                "image.jpg",
                GenerationPriority::Interactive,
            )
            .await;
        {
            let state = manager.inner.lock().await;
            assert_eq!(
                state.rerun.get(&key).map(|j| j.priority),
                Some(GenerationPriority::Interactive)
            );
        }
    }

    #[tokio::test]
    async fn completed_job_requeues_pending_rerun() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        let gallery = test_gallery();

        let key = GenerationJobKey {
            gallery_key: "site:gallery".to_string(),
            image_path: "image.jpg".to_string(),
            kind: GenerationJobKindKey::Cleanup,
        };
        let running_job = GenerationJob {
            key: key.clone(),
            gallery: gallery.clone(),
            priority: GenerationPriority::Cleanup,
            sequence: 1,
            estimated_pixels: None,
            kind: GenerationJobKind::Cleanup,
        };

        {
            let mut state = manager.inner.lock().await;
            state.running.insert(key.clone());
            state.running_count = 1;
        }
        // Same Cleanup key enqueued while running -> stashed as rerun.
        manager
            .enqueue_cleanup("site:gallery", gallery.clone(), "image.jpg")
            .await;
        assert!(manager.inner.lock().await.rerun.contains_key(&key));

        // Completing the in-flight job re-queues the stashed rerun into pending.
        manager.execute_job(running_job).await;

        let state = manager.inner.lock().await;
        assert!(state.rerun.is_empty(), "rerun should be drained");
        assert!(state.running.is_empty(), "running should be cleared");
        assert!(
            state.pending.contains_key(&key),
            "rerun should now be pending"
        );
    }

    #[tokio::test]
    async fn install_global_registers_manager() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        manager.install_global();
        // After installation the process-global accessor resolves a manager, so
        // AppState-less paths (periodic refresh, site reload) can enqueue work.
        assert!(global_manager().is_some());
    }

    #[tokio::test]
    async fn take_recent_resized_error_returns_and_clears_matching_entry() {
        let manager = GenerationManager::new(WorkerPolicy::default());

        let key = GenerationJobKey {
            gallery_key: "site:gallery".to_string(),
            image_path: "image.jpg".to_string(),
            kind: GenerationJobKindKey::Resized {
                size: "gallery".to_string(),
                output_format: OutputFormat::WebP,
                apply_watermark: false,
            },
        };
        manager
            .inner
            .lock()
            .await
            .recent_errors
            .insert(key, "boom".to_string());

        // A non-matching lookup (different size) leaves the entry in place.
        assert_eq!(
            manager
                .take_recent_resized_error(
                    "site:gallery",
                    "image.jpg",
                    "thumbnail",
                    OutputFormat::WebP,
                    false,
                )
                .await,
            None
        );

        // The matching lookup returns the error and removes it, so the next
        // request re-enqueues (self-healing) rather than serving stale failures.
        assert_eq!(
            manager
                .take_recent_resized_error(
                    "site:gallery",
                    "image.jpg",
                    "gallery",
                    OutputFormat::WebP,
                    false,
                )
                .await,
            Some("boom".to_string())
        );
        assert_eq!(
            manager
                .take_recent_resized_error(
                    "site:gallery",
                    "image.jpg",
                    "gallery",
                    OutputFormat::WebP,
                    false,
                )
                .await,
            None
        );
    }

    #[tokio::test]
    async fn try_pop_job_skips_undispatchable_head() {
        // Head-of-line: the highest-priority job needs more memory than the budget
        // allows (workers = 1) while a worker is already busy, but a lower-priority
        // job with no memory pressure can still run. try_pop_job must return the
        // runnable job instead of stalling the whole queue behind the blocked head.
        let cores = crate::concurrency::logical_cores();
        if cores < 2 {
            // With a single worker slot there is no head-of-line to demonstrate.
            return;
        }

        let policy = WorkerPolicy {
            interactive_ratio: 1.0,
            ..WorkerPolicy::default()
        };
        let manager = GenerationManager::new(policy);
        let gallery = test_gallery();

        let make_job = |path: &str, seq: u64, pixels: Option<usize>| GenerationJob {
            key: GenerationJobKey {
                gallery_key: "site:gallery".to_string(),
                image_path: path.to_string(),
                kind: GenerationJobKindKey::Resized {
                    size: "gallery".to_string(),
                    output_format: OutputFormat::WebP,
                    apply_watermark: false,
                },
            },
            gallery: gallery.clone(),
            priority: GenerationPriority::Interactive,
            sequence: seq,
            estimated_pixels: pixels,
            kind: GenerationJobKind::Resized {
                size: "gallery".to_string(),
                output_format: OutputFormat::WebP,
            },
        };

        // `big` (earlier sequence) sorts to the head; its huge pixel estimate makes
        // per-frame memory exceed the budget, capping its worker budget at 1.
        // 1e15 px * 32 B/px = 32 PB, larger than any real memory budget, while
        // staying well clear of u64 overflow in the budget math.
        let big = make_job("big.jpg", 1, Some(1_000_000_000_000_000));
        let small = make_job("small.jpg", 2, None);

        {
            let mut state = manager.inner.lock().await;
            for job in [big, small] {
                state.heap.push(QueueEntry {
                    priority: job.priority.score(),
                    sequence: job.sequence,
                    key: job.key.clone(),
                });
                state.pending.insert(job.key.clone(), job);
            }
            // One worker already busy: big (budget 1) can't dispatch, small can.
            state.running_count = 1;
        }

        let job = manager
            .try_pop_job()
            .await
            .expect("a dispatchable job should be found past the blocked head");
        assert_eq!(job.key.image_path, "small.jpg");

        // The blocked head stays queued for a later attempt.
        assert_eq!(manager.queue_depth().await, 1);
        assert!(
            manager
                .inner
                .lock()
                .await
                .pending
                .keys()
                .any(|k| k.image_path == "big.jpg")
        );
    }

    #[tokio::test]
    async fn interactive_dispatch_blocks_background_before_execution() {
        let manager = GenerationManager::new(WorkerPolicy::default());
        let gallery = test_gallery();

        manager
            .enqueue_resized(
                "site:gallery",
                gallery.clone(),
                "a.jpg",
                "gallery",
                OutputFormat::WebP,
                false,
                GenerationPriority::Interactive,
            )
            .await;

        let job = manager
            .try_pop_job()
            .await
            .expect("interactive job should dispatch");
        assert_eq!(job.priority, GenerationPriority::Interactive);

        let background = GenerationJob {
            key: GenerationJobKey {
                gallery_key: "site:gallery".to_string(),
                image_path: "b.jpg".to_string(),
                kind: GenerationJobKindKey::Pregenerate,
            },
            gallery: gallery.clone(),
            priority: GenerationPriority::Background,
            sequence: 0,
            estimated_pixels: None,
            kind: GenerationJobKind::Pregenerate,
        };

        // The interactive counter is bumped at dispatch time (under the queue lock),
        // not deferred to execute_job, so a background job is blocked immediately —
        // no window where it observes zero active interactive jobs.
        let state = manager.inner.lock().await;
        assert!(!manager.can_dispatch(&state, &background));
    }

    #[tokio::test]
    async fn take_recent_tile_error_returns_and_clears_matching_entry() {
        let manager = GenerationManager::new(WorkerPolicy::default());

        let key = GenerationJobKey {
            gallery_key: "site:gallery".to_string(),
            image_path: "image.jpg".to_string(),
            kind: GenerationJobKindKey::TileSet {
                tile_size: 1024,
                output_format: OutputFormat::WebP,
            },
        };
        manager
            .inner
            .lock()
            .await
            .recent_errors
            .insert(key, "tile-boom".to_string());

        assert_eq!(
            manager
                .take_recent_tile_error("site:gallery", "image.jpg", 512, OutputFormat::WebP)
                .await,
            None
        );
        assert_eq!(
            manager
                .take_recent_tile_error("site:gallery", "image.jpg", 1024, OutputFormat::WebP)
                .await,
            Some("tile-boom".to_string())
        );
        assert_eq!(
            manager
                .take_recent_tile_error("site:gallery", "image.jpg", 1024, OutputFormat::WebP)
                .await,
            None
        );
    }

    #[tokio::test]
    async fn should_pregenerate_tiles_respects_config() {
        use crate::config::defaults;
        use crate::storage::FilesystemStorage;
        use tempfile::TempDir;

        // Returns (guard, gallery); the TempDir guard must outlive the gallery.
        let build = |pregenerate: Option<crate::PregenerateConfig>,
                     tiles: Option<crate::TileConfig>| {
            let temp = TempDir::new().unwrap();
            let src = temp.path().join("s");
            let cache = temp.path().join("c");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::create_dir_all(&cache).unwrap();
            let config = crate::GallerySystemConfig {
                name: "g".to_string(),
                source_directory: src.to_string_lossy().to_string(),
                cache_directory: cache.to_string_lossy().to_string(),
                url_prefix: "/g".to_string(),
                thumbnail: defaults::default_thumbnail_size(),
                gallery_size: defaults::default_gallery_size(),
                medium: defaults::default_medium_size(),
                large: defaults::default_large_size(),
                tiles,
                pregenerate,
                ..Default::default()
            };
            let gallery = Arc::new(crate::gallery::Gallery::new(
                config,
                Arc::new(FilesystemStorage::new(src)),
                Arc::new(FilesystemStorage::new(cache)),
            ));
            (temp, gallery)
        };

        let pregen = |tiles: bool| crate::PregenerateConfig {
            formats: defaults::default_pregenerate_formats(),
            sizes: defaults::default_pregenerate_sizes(),
            tiles,
        };
        let tile_cfg = || Some(crate::TileConfig { tile_size: 1024 });

        // Tiles configured and pregenerate.tiles = true -> pre-generate.
        let (_g1, g1) = build(Some(pregen(true)), tile_cfg());
        assert!(g1.should_pregenerate_tiles());

        // Tiles configured but pregenerate.tiles = false -> do NOT pre-generate
        // (on-demand tile generation still works via the serve path).
        let (_g2, g2) = build(Some(pregen(false)), tile_cfg());
        assert!(!g2.should_pregenerate_tiles());

        // pregenerate.tiles = true but no tiles configured -> nothing to build.
        let (_g3, g3) = build(Some(pregen(true)), None);
        assert!(!g3.should_pregenerate_tiles());

        // No pregenerate config at all -> false.
        let (_g4, g4) = build(None, tile_cfg());
        assert!(!g4.should_pregenerate_tiles());
    }

    #[tokio::test]
    async fn cleanup_filename_matches_canonical_and_handles_avif() {
        let gallery = test_gallery();

        // For formats representable in this build, the cleanup filename must match
        // the canonical generator exactly, so jpg/webp/png cleanup is unchanged.
        for ext in ["jpg", "webp", "png"] {
            for watermark in [false, true] {
                assert_eq!(
                    processed_image_cache_filename(&gallery, "photo.jpg", "medium", ext, watermark),
                    gallery.generate_cache_filename("photo.jpg", "medium", ext, watermark),
                    "cleanup name should match canonical for ext={ext} watermark={watermark}"
                );
            }
        }

        // AVIF must always yield an .avif name, even in a --no-default-features
        // build where generate_cache_filename would fall back to .jpg.
        let avif_name =
            processed_image_cache_filename(&gallery, "photo.jpg", "medium", "avif", false);
        assert!(
            avif_name.ends_with(".avif"),
            "expected .avif, got {avif_name}"
        );
    }

    #[tokio::test]
    async fn cleanup_deletes_avif_artifacts() {
        use crate::config::defaults;
        use crate::storage::FilesystemStorage;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let src = temp.path().join("s");
        let cache = temp.path().join("c");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&cache).unwrap();

        let config = crate::GallerySystemConfig {
            name: "g".to_string(),
            source_directory: src.to_string_lossy().to_string(),
            cache_directory: cache.to_string_lossy().to_string(),
            url_prefix: "/g".to_string(),
            thumbnail: defaults::default_thumbnail_size(),
            gallery_size: defaults::default_gallery_size(),
            medium: defaults::default_medium_size(),
            large: defaults::default_large_size(),
            tiles: None,
            ..Default::default()
        };
        let gallery: SharedGallery = Arc::new(crate::gallery::Gallery::new(
            config,
            Arc::new(FilesystemStorage::new(src)),
            Arc::new(FilesystemStorage::new(cache.clone())),
        ));

        // An artifact an AVIF-enabled build would have written.
        let avif_name =
            processed_image_cache_filename(&gallery, "photo.jpg", "medium", "avif", false);
        std::fs::write(cache.join(&avif_name), b"x").unwrap();
        assert!(cache.join(&avif_name).exists());

        cleanup_cache_files_for_path(&gallery, "photo.jpg")
            .await
            .unwrap();

        assert!(
            !cache.join(&avif_name).exists(),
            "avif artifact must be cleaned even without the avif feature"
        );
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
