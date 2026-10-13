use super::Gallery;
use super::image_processing::OutputFormat;
use super::types::ImageSize;
use crate::{CacheType, FormatCoverage};
use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tracing::{debug, error, info, warn};

/// Generate a tile cache filename that includes readable tile coordinates
pub fn generate_tile_cache_filename(
    path: &str,
    tile_x: u32,
    tile_y: u32,
    tile_size: u32,
    is_retina: bool,
    format: &str,
) -> String {
    use sha2::{Digest, Sha256};

    // Build the tile spec
    let tile_spec = if is_retina {
        format!("tile_{}_{}_{}", tile_x, tile_y, tile_size) + "@2x"
    } else {
        format!("tile_{}_{}_{}", tile_x, tile_y, tile_size)
    };

    // Match the exact pattern used by Gallery::generate_image_cache_key
    let cache_key = format!("{}_{}", tile_spec, format);

    let mut hasher = Sha256::new();
    hasher.update(path);
    hasher.update(&cache_key);
    let hash = hex::encode(hasher.finalize());

    // Make the filename somewhat readable
    format!(
        "tile_{}_{}{}.{}.{}",
        tile_x,
        tile_y,
        if is_retina { "@2x" } else { "" },
        hash,
        format
    )
}

pub(crate) const CACHE_FORMATS: [&str; 4] = ["jpg", "webp", "png", "avif"];

pub const MAX_TILE_DIMENSION: u32 = 8192;

#[cfg(feature = "avif")]
pub(crate) const TILE_FORMAT: OutputFormat = OutputFormat::Avif;
#[cfg(not(feature = "avif"))]
pub(crate) const TILE_FORMAT: OutputFormat = OutputFormat::WebP;

pub(crate) fn tile_grid_bound(width: u32, height: u32, tile_size: u32) -> u32 {
    width
        .max(height)
        .min(MAX_TILE_DIMENSION)
        .div_ceil(tile_size)
}

fn parse_tile_cache_filename(filename: &str) -> Option<(u32, u32, bool, &str)> {
    let mut parts = filename.split('.');
    let spec = parts.next()?.strip_prefix("tile_")?;
    parts.next()?;
    let format = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let (spec, is_retina) = match spec.strip_suffix("@2x") {
        Some(spec) => (spec, true),
        None => (spec, false),
    };
    let (x, y) = spec.split_once('_')?;
    Some((x.parse().ok()?, y.parse().ok()?, is_retina, format))
}

fn parse_resized_cache_hash(filename: &str) -> Option<&str> {
    let (stem, extension) = filename.split_once('.')?;
    if extension.is_empty() || extension.contains('.') {
        return None;
    }

    let hash = stem.strip_suffix("_watermarked").unwrap_or(stem);
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hash)
}

fn resized_cache_variants(
    sizes: &[ImageSize],
) -> impl Iterator<Item = (String, &'static str, bool)> {
    sizes.iter().flat_map(|size| {
        let watermark_variants: &[bool] = if size.supports_watermark() {
            &[false, true]
        } else {
            &[false]
        };
        CACHE_FORMATS.into_iter().flat_map(move |format| {
            watermark_variants
                .iter()
                .map(move |&watermarked| (size.as_str(), format, watermarked))
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFileKind {
    Resized,
    Tile,
}

#[derive(Debug, Clone)]
pub struct CacheFile {
    pub path: String,
    pub kind: CacheFileKind,
    pub size: Option<u64>,
}

impl Gallery {
    pub async fn initialize_and_check_version(&self) -> Result<(), super::GalleryError> {
        let current_version = env!("CARGO_PKG_VERSION");

        // Load image metadata cache from storage
        if let Err(e) = self.image_cache.load(&self.cache_storage).await {
            warn!("Failed to load image metadata cache: {}", e);
        }

        // Load folder metadata cache from storage
        if let Err(e) = self.folder_cache.load(&self.cache_storage).await {
            warn!("Failed to load folder metadata cache: {}", e);
        }

        // Disabling video changes discovery even when the binary version has
        // not changed. Do not restore video entries from the previous config.
        let disabled_video_cached = if self.config.video.is_none() {
            let image_video = self
                .image_cache
                .read_all()
                .await
                .keys()
                .any(|path| tenrankai_video::is_video_path(path));
            let folder_video = self.folder_cache.read_all().await.values().any(|folder| {
                folder
                    .images
                    .iter()
                    .any(|path| tenrankai_video::is_video_path(path))
            });
            image_video || folder_video
        } else {
            false
        };

        // Load cache version metadata
        let loaded_cache_metadata =
            crate::cache::load_cache_version_metadata(&self.cache_storage).await;

        let needs_refresh = match loaded_cache_metadata {
            Ok(cm) => {
                let mut metadata = self.cache_metadata.write().await;
                *metadata = cm;
                metadata.version != current_version
            }
            Err(_) => {
                // No cache metadata found, needs refresh
                true
            }
        } || disabled_video_cached;

        if needs_refresh {
            info!(
                "Cache version or media configuration changed ({}), refreshing gallery '{}'",
                current_version, self.config.name
            );

            // Clear the old caches
            self.image_cache.clear().await;
            self.folder_cache.clear().await;

            // Update version and trigger refresh
            let mut metadata = self.cache_metadata.write().await;
            metadata.version = current_version.to_string();
            metadata.last_full_refresh = std::time::SystemTime::now();
            drop(metadata);

            // Save the updated cache metadata
            self.save_cache_metadata().await?;

            // Pre-populate folder metadata cache for fast initial page loads
            info!("Pre-populating folder metadata cache...");
            if let Err(e) = self.refresh_folder_metadata().await {
                warn!("Failed to pre-populate folder metadata cache: {}", e);
            }
        } else {
            // Check if folder cache is empty (cache file might not have existed)
            if self.folder_cache.is_empty().await {
                info!("Folder cache is empty, pre-populating...");
                if let Err(e) = self.refresh_folder_metadata().await {
                    warn!("Failed to pre-populate folder metadata cache: {}", e);
                }
            }
            // Build the indexer from the existing cache
            let all_paths: Vec<String> = {
                let cache = self.image_cache.read_all().await;
                cache.keys().cloned().collect()
            };

            if !all_paths.is_empty() {
                let mut indexer = self.image_indexer.write().await;
                debug!(
                    "Building index for paths: {:?}",
                    &all_paths[..5.min(all_paths.len())]
                );
                indexer.build_index(&all_paths);
                info!(
                    "Initialized image index with {} images from cache for gallery '{}'",
                    all_paths.len(),
                    self.config.name
                );
            } else {
                warn!(
                    "No paths found in cache to build image index for gallery '{}'",
                    self.config.name
                );
            }
        }

        Ok(())
    }

    pub fn start_background_cache_refresh(gallery: super::SharedGallery, interval_minutes: u64) {
        let shutdown_token = gallery.shutdown_token.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick

            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        info!("Background cache refresh task shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        info!("Starting scheduled metadata cache refresh");

                        let pregenerate = gallery.config.pregenerate.is_some();
                        if let Err(e) = gallery
                            .clone()
                            .refresh_metadata_and_pregenerate_cache(pregenerate)
                            .await
                        {
                            error!("Failed to refresh metadata cache: {}", e);
                        }
                    }
                }
            }
        });
    }

    pub fn start_periodic_cache_save(gallery: super::SharedGallery, interval_minutes: u64) {
        let shutdown_token = gallery.shutdown_token.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick

            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        info!("Periodic cache save task shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        // Save image cache if dirty (dirty flag check is internal)
                        match gallery.image_cache.save_if_dirty(&gallery.cache_storage).await {
                            Ok(true) => info!("Periodic image metadata cache save completed"),
                            Ok(false) => debug!("Image cache not dirty, skipping save"),
                            Err(e) => error!("Failed to save image metadata cache: {}", e),
                        }

                        // Save folder cache if dirty
                        match gallery.folder_cache.save_if_dirty(&gallery.cache_storage).await {
                            Ok(true) => info!("Periodic folder metadata cache save completed"),
                            Ok(false) => debug!("Folder cache not dirty, skipping save"),
                            Err(e) => error!("Failed to save folder metadata cache: {}", e),
                        }
                    }
                }
            }
        });
    }

    pub(crate) async fn save_metadata_cache(&self) -> Result<(), super::GalleryError> {
        // Save image metadata cache if dirty
        let image_saved = self
            .image_cache
            .save_if_dirty(&self.cache_storage)
            .await
            .map_err(|e| super::GalleryError::CacheError(e.to_string()))?;

        // Save folder metadata cache if dirty
        let folder_saved = self
            .folder_cache
            .save_if_dirty(&self.cache_storage)
            .await
            .map_err(|e| super::GalleryError::CacheError(e.to_string()))?;

        if image_saved || folder_saved {
            debug!(
                "Saved metadata caches (image: {}, folder: {})",
                image_saved, folder_saved
            );
        }

        Ok(())
    }

    pub(crate) async fn save_cache_metadata(&self) -> Result<(), super::GalleryError> {
        let metadata = self.cache_metadata.read().await;
        crate::cache::save_cache_version_metadata(&self.cache_storage, &metadata).await?;
        Ok(())
    }

    pub async fn save_caches(&self) -> Result<(), super::GalleryError> {
        // Ensure cache storage is ready (creates directory for filesystem, no-op for S3)
        self.cache_storage.create_dir("").await?;

        // Save all caches
        self.save_metadata_cache().await?;
        self.save_cache_metadata().await?;

        info!("Saved gallery caches to storage");
        Ok(())
    }

    pub fn generate_cache_key(&self, path: &str, size: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(path);
        hasher.update(size);
        hex::encode(hasher.finalize())
    }

    /// Generate a cache key for regular images with size, format, and watermark status
    /// This is the primary method for generating cache keys that match the serving code.
    ///
    /// Cache key format includes:
    /// - Size (thumbnail, medium, etc.)
    /// - Format (jpg, webp, etc.)
    /// - Text watermark status ("watermarked" if copyright_holder is set for medium sizes)
    /// - Image watermark hash (if configured, for cache invalidation on watermark changes)
    pub fn generate_image_cache_key(
        &self,
        path: &str,
        size: &str,
        format: &str,
        has_watermark: bool,
    ) -> String {
        let mut cache_key = format!("{}_{}", size, format);

        if has_watermark {
            cache_key.push_str("_watermarked");
        }

        if let Some(ref hash) = self.image_watermark_hash {
            cache_key.push_str("_imgwm_");
            cache_key.push_str(hash);
        }

        self.generate_cache_key(path, &cache_key)
    }

    /// Generate a cache filename for storing in filesystem with watermark status
    pub(crate) fn generate_cache_filename(
        &self,
        path: &str,
        size: &str,
        format_str: &str,
        has_watermark: bool,
    ) -> String {
        // Parse the format string to OutputFormat, default to JPEG if unknown
        let format = OutputFormat::from_file_extension(format_str).unwrap_or(OutputFormat::Jpeg);
        let cache_type = CacheType::processed_image(format, has_watermark);

        let hash = self.generate_image_cache_key(path, size, format_str, has_watermark);
        cache_type.filename(Some(&hash))
    }

    pub(crate) fn resized_cache_hashes(
        &self,
        image_path: &str,
        sizes: &[ImageSize],
    ) -> impl Iterator<Item = String> {
        resized_cache_variants(sizes).map(move |(size, format, watermarked)| {
            self.generate_image_cache_key(image_path, &size, format, watermarked)
        })
    }

    pub(crate) fn resized_cache_filenames(
        &self,
        image_path: &str,
        sizes: &[ImageSize],
    ) -> impl Iterator<Item = String> {
        resized_cache_variants(sizes).map(move |(size, format, watermarked)| {
            self.generate_cache_filename(image_path, &size, format, watermarked)
        })
    }

    pub(crate) async fn tile_cache_filenames(&self, image_path: &str) -> Vec<String> {
        let Some(tile_config) = &self.config.tiles else {
            return Vec::new();
        };

        let (width, height) = self
            .image_cache
            .read_all()
            .await
            .get(image_path)
            .map(|metadata| metadata.dimensions)
            .filter(|&(width, height)| width > 0 && height > 0)
            .unwrap_or((MAX_TILE_DIMENSION, MAX_TILE_DIMENSION));
        let grid = tile_grid_bound(width, height, tile_config.tile_size);

        let mut filenames = Vec::new();
        for tile_y in 0..grid {
            for tile_x in 0..grid {
                for is_retina in [false, true] {
                    filenames.push(generate_tile_cache_filename(
                        image_path,
                        tile_x,
                        tile_y,
                        tile_config.tile_size,
                        is_retina,
                        TILE_FORMAT.extension(),
                    ));
                }
            }
        }
        filenames
    }

    /// Generate a composite cache filename using the type-safe CacheType system
    pub(crate) fn generate_composite_cache_filename(&self, gallery_path: &str) -> String {
        let path_key = self.generate_safe_path_key(gallery_path);
        let cache_type = CacheType::composite(self.config.name.clone(), path_key.clone());

        // Generate a unique identifier for this specific composite
        let content_key = self.generate_cache_key(
            &format!("composite_{}_{}", self.config.name, path_key),
            "jpg",
        );
        cache_type.filename(Some(&content_key))
    }

    /// Generate a safe path key for cache identification
    fn generate_safe_path_key(&self, gallery_path: &str) -> String {
        use sha2::{Digest, Sha256};

        if gallery_path.is_empty() {
            "root".to_string()
        } else {
            // Use base64 encoding for path to handle special characters safely
            use base64::{Engine as _, engine::general_purpose};
            let encoded_path = general_purpose::URL_SAFE_NO_PAD.encode(gallery_path);

            // Limit length and add hash suffix for very long paths
            if encoded_path.len() > 40 {
                let mut hasher = Sha256::new();
                hasher.update(gallery_path);
                let hash_suffix = &hex::encode(hasher.finalize())[..8];
                format!("{}_{}", &encoded_path[..32], hash_suffix)
            } else {
                encoded_path
            }
        }
    }

    /// Generate a composite cache key with full context (legacy API compatibility)
    pub fn generate_composite_cache_key_with_context(&self, gallery_path: &str) -> String {
        let path_key = self.generate_safe_path_key(gallery_path);
        format!("composite_{}_{}", self.config.name, path_key)
    }

    /// Get sizes to pregenerate based on config
    fn get_pregenerate_sizes(&self) -> Vec<ImageSize> {
        let config = match &self.config.pregenerate {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut sizes = Vec::new();
        if config.sizes.thumbnail {
            sizes.push(ImageSize::Thumbnail);
            sizes.push(ImageSize::ThumbnailRetina);
        }
        if config.sizes.gallery {
            sizes.push(ImageSize::Gallery);
            sizes.push(ImageSize::GalleryRetina);
        }
        if config.sizes.medium {
            sizes.push(ImageSize::Medium);
            sizes.push(ImageSize::MediumRetina);
        }
        if config.sizes.large {
            sizes.push(ImageSize::Large);
            sizes.push(ImageSize::LargeRetina);
        }
        sizes
    }

    /// Get formats to pregenerate based on config
    fn get_pregenerate_formats(&self) -> Vec<OutputFormat> {
        let config = match &self.config.pregenerate {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut formats = Vec::new();
        if config.formats.jpeg {
            formats.push(OutputFormat::Jpeg);
        }
        if config.formats.webp {
            formats.push(OutputFormat::WebP);
        }
        #[cfg(feature = "avif")]
        if config.formats.avif {
            formats.push(OutputFormat::Avif);
        }
        formats
    }

    /// Check if tiles should be pregenerated
    fn should_pregenerate_tiles(&self) -> bool {
        self.config
            .pregenerate
            .as_ref()
            .map(|c| c.tiles)
            .unwrap_or(false)
            && self.config.tiles.is_some()
    }

    pub fn is_pregeneration_enabled(&self) -> bool {
        self.config.pregenerate.is_some()
    }

    /// Check format coverage for an image and update its preview_ready flag.
    /// Returns the new readiness value.
    pub async fn update_preview_readiness(
        &self,
        relative_path: &str,
        cache_files: &HashSet<String>,
    ) -> bool {
        if !self.is_pregeneration_enabled() {
            return true;
        }

        let sizes: Vec<_> = self
            .get_pregenerate_sizes()
            .into_iter()
            .filter(|s| !s.is_tile())
            .collect();
        let allowed_formats = self.get_pregenerate_formats();
        if sizes.is_empty() || allowed_formats.is_empty() {
            return true;
        }

        let is_ready = sizes.iter().all(|size| {
            let coverage = self.check_format_coverage_fast(relative_path, *size, cache_files);
            coverage
                .missing_formats(relative_path)
                .into_iter()
                .all(|format| !allowed_formats.contains(&format))
        });

        if tenrankai_video::is_video_path(relative_path) {
            return self
                .video_record(relative_path)
                .await
                .is_some_and(|r| r.poster_ready);
        }
        let mut cache = self.image_cache.write_all().await;
        if let Some(metadata) = cache.get_mut(relative_path)
            && metadata.preview_ready != is_ready
        {
            metadata.preview_ready = is_ready;
            drop(cache);
            self.image_cache.mark_dirty();
        }

        is_ready
    }

    /// Pre-generate cache for a single image (only generates missing formats)
    pub async fn pregenerate_image_cache(
        &self,
        relative_path: &str,
        cache_files: &HashSet<String>,
    ) -> Result<(), super::GalleryError> {
        // Check for cancellation (both pregeneration and shutdown)
        if self.pregeneration_token.lock().await.is_cancelled()
            || self.shutdown_token.is_cancelled()
        {
            return Ok(());
        }

        if !self.is_image(relative_path) {
            return Ok(());
        }

        // Check if source image exists using storage
        if !self
            .source_storage
            .exists(relative_path)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }

        let sizes = self.get_pregenerate_sizes();
        let allowed_formats = self.get_pregenerate_formats();

        if sizes.is_empty() || allowed_formats.is_empty() {
            return Ok(());
        }

        let mut variants = Vec::new();

        // Collect all missing variants to generate
        for size in &sizes {
            let coverage = self.check_format_coverage_fast(relative_path, *size, cache_files);
            let formats_to_generate = coverage.missing_formats(relative_path);

            // Parse size and determine watermark
            let (dimensions, supports_watermark) = match self.parse_size(&size.as_str()) {
                Ok(dims) => dims,
                Err(e) => {
                    debug!("Failed to parse size {} for {}: {}", size, relative_path, e);
                    continue;
                }
            };

            let apply_watermark = supports_watermark && self.config.copyright_holder.is_some();

            // Add each format as a variant (only if in allowed formats)
            for format in formats_to_generate {
                if allowed_formats.contains(&format) {
                    variants.push((
                        size.as_str().to_string(),
                        dimensions.clone(),
                        apply_watermark,
                        format,
                    ));
                }
            }
        }

        if variants.is_empty() {
            return Ok(());
        }

        // Use batch processing to generate all variants at once
        info!(
            "Generating {} missing variants for: {}",
            variants.len(),
            relative_path
        );
        let start = std::time::Instant::now();

        match self.process_image_batch(relative_path, variants).await {
            Ok(paths) => {
                let elapsed = start.elapsed();
                info!(
                    "Generated {} cache entries for {} in {:.2}s",
                    paths.len(),
                    relative_path,
                    elapsed.as_secs_f32()
                );
                Ok(())
            }
            Err(e) => {
                error!("Failed to generate cache for {}: {}", relative_path, e);
                Err(e)
            }
        }
    }

    /// Pre-generate tiles for a single image
    ///
    /// Uses the cache_files set for fast lookup to avoid regenerating existing tiles.
    pub async fn pregenerate_tiles_for_image(
        &self,
        relative_path: &str,
        cache_files: &HashSet<String>,
    ) -> Result<(), super::GalleryError> {
        // Check for cancellation (both pregeneration and shutdown)
        if self.pregeneration_token.lock().await.is_cancelled()
            || self.shutdown_token.is_cancelled()
        {
            return Ok(());
        }

        // Check if tiles are configured
        let tile_config = match &self.config.tiles {
            Some(config) => config,
            None => return Ok(()), // No tiles configured, nothing to do
        };

        if !self.is_image(relative_path) {
            return Ok(());
        }

        let tile_size = tile_config.tile_size;

        // Get image dimensions to calculate grid size
        let grid = match self.image_cache.read_all().await.get(relative_path) {
            Some(meta) => tile_grid_bound(meta.dimensions.0, meta.dimensions.1, tile_size),
            None => return Ok(()), // No metadata available
        };

        // Skip if no tiles to generate
        if grid == 0 {
            return Ok(());
        }

        // Fast check: see if tiles already exist using the cache_files set
        // We check for tile (0,0) as a representative - if it exists, all tiles should exist
        let first_tile_filename = generate_tile_cache_filename(
            relative_path,
            0,
            0,
            tile_size,
            false, // non-retina
            TILE_FORMAT.extension(),
        );

        if cache_files.contains(&first_tile_filename) {
            // Tiles already exist, skip regeneration
            return Ok(());
        }

        // Check for cancellation (both pregeneration and shutdown)
        if self.pregeneration_token.lock().await.is_cancelled()
            || self.shutdown_token.is_cancelled()
        {
            return Ok(());
        }

        // Just request tile 0,0 - the backend will generate all tiles
        match self.get_image_tile(relative_path, 0, 0).await {
            Ok(_) => {
                info!(
                    "Pre-generated all tiles ({}x{} grid) for {}",
                    grid, grid, relative_path
                );
            }
            Err(e) => {
                warn!("Failed to pre-generate tiles for {}: {}", relative_path, e);
            }
        }

        Ok(())
    }

    /// Pre-generate cache for all images in the gallery
    ///
    /// Processes images in parallel using up to the number of CPU cores available.
    /// Only generates missing formats - skips files that already exist in cache.
    /// Each image loads once and generates all missing variants (sizes/formats) in a batch.
    pub async fn pregenerate_all_images_cache(self: Arc<Self>) -> Result<(), super::GalleryError> {
        info!(
            "Starting cache pre-generation for gallery '{}' (missing formats only)",
            self.config.name
        );

        // Load all cache files once for fast lookups (avoids per-image storage API calls)
        let cache_files = Arc::new(self.load_cache_file_set().await);

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let cache = self.image_cache.read_all().await;
            cache.keys().cloned().collect()
        };

        let total_images = image_paths.len();
        let num_cores = num_cpus::get();
        info!(
            "Checking {} images for missing cache formats (using {} parallel workers)",
            total_images, num_cores
        );

        // Get cancellation tokens (both pregeneration and shutdown)
        let pregen_token = self.pregeneration_token.lock().await.clone();
        let shutdown_token = self.shutdown_token.clone();

        // Track progress across threads
        let completed = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));

        // Process images in parallel with concurrency limit
        let _results: Vec<_> = stream::iter(image_paths)
            .enumerate()
            .map(|(index, image_path)| {
                let gallery = self.clone();
                let completed = completed.clone();
                let failed = failed.clone();
                let cancelled = cancelled.clone();
                let pregen_token = pregen_token.clone();
                let shutdown_token = shutdown_token.clone();
                let cache_files = cache_files.clone();

                async move {
                    // Helper to check if cancelled
                    let is_cancelled =
                        || pregen_token.is_cancelled() || shutdown_token.is_cancelled();

                    // Check for cancellation before processing
                    if is_cancelled() {
                        cancelled.store(true, Ordering::Relaxed);
                        return (index, image_path, Err(super::GalleryError::InvalidPath));
                    }

                    let result = gallery
                        .pregenerate_image_cache(&image_path, &cache_files)
                        .await;

                    // Check for cancellation between operations
                    if is_cancelled() {
                        cancelled.store(true, Ordering::Relaxed);
                        return (index, image_path, Err(super::GalleryError::InvalidPath));
                    }

                    // Also pre-generate tiles if configured and enabled
                    if result.is_ok()
                        && gallery.should_pregenerate_tiles()
                        && let Err(e) = gallery
                            .pregenerate_tiles_for_image(&image_path, &cache_files)
                            .await
                    {
                        error!("Failed to pre-generate tiles for {}: {}", image_path, e);
                        failed.fetch_add(1, Ordering::Relaxed);
                    }

                    match &result {
                        Ok(_) => {
                            // Mark image as preview-ready immediately so it appears in listings
                            let mut cache = gallery.image_cache.write_all().await;
                            if let Some(metadata) = cache.get_mut(&image_path)
                                && !metadata.preview_ready
                            {
                                metadata.preview_ready = true;
                                drop(cache);
                                gallery.image_cache.mark_dirty();
                            }

                            let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
                            if count.is_multiple_of(10) || count == total_images {
                                info!("Pre-generated cache for {}/{} images", count, total_images);
                            }
                        }
                        Err(e) => {
                            if !is_cancelled() {
                                error!("Failed to pre-generate cache for {}: {}", image_path, e);
                                failed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }

                    (index, image_path, result)
                }
            })
            .buffer_unordered(num_cores)
            .take_while(|_| {
                let check_cancelled = pregen_token.is_cancelled() || shutdown_token.is_cancelled();
                if check_cancelled {
                    cancelled.store(true, Ordering::Relaxed);
                }
                async move { !check_cancelled }
            })
            .collect()
            .await;

        let total_failed = failed.load(Ordering::Relaxed);
        let total_completed = completed.load(Ordering::Relaxed);
        let was_cancelled = cancelled.load(Ordering::Relaxed);

        if was_cancelled {
            info!(
                "Cache pre-generation cancelled for gallery '{}': {} completed before cancellation",
                self.config.name, total_completed
            );
        } else {
            info!(
                "Completed cache pre-generation for gallery '{}': {} succeeded, {} failed",
                self.config.name, total_completed, total_failed
            );
        }

        if total_failed > 0 && !was_cancelled {
            warn!("{} images failed during cache pre-generation", total_failed);
        }

        // Update preview readiness flags for all images
        if self.is_pregeneration_enabled() && !was_cancelled {
            info!("Updating preview readiness flags...");
            let fresh_cache_files = self.load_cache_file_set().await;
            let all_paths: Vec<String> = {
                let cache = self.image_cache.read_all().await;
                cache.keys().cloned().collect()
            };
            let mut ready_count = 0usize;
            let mut not_ready_count = 0usize;
            for path in &all_paths {
                if self
                    .update_preview_readiness(path, &fresh_cache_files)
                    .await
                {
                    ready_count += 1;
                } else {
                    not_ready_count += 1;
                }
            }
            info!(
                "Preview readiness: {} ready, {} not ready",
                ready_count, not_ready_count
            );

            if let Err(e) = self.save_metadata_cache().await {
                error!("Failed to save readiness flags: {}", e);
            }
        }

        Ok(())
    }

    /// Remove cache files that don't belong to any current image in the metadata cache
    pub async fn remove_orphaned_cache_files(
        &self,
        dry_run: bool,
    ) -> Result<Vec<CacheFile>, super::GalleryError> {
        info!(
            "Starting cache validation and cleanup for gallery '{}'",
            self.config.name
        );

        // Stale files (source newer than cache) are regenerated on-demand during serving
        let orphaned = self.find_orphaned_cache_files().await?;
        if dry_run {
            return Ok(orphaned);
        }

        let removed = self.delete_cache_files(orphaned).await;
        info!(
            "Removed {} orphaned cache files from gallery '{}'",
            removed.len(),
            self.config.name
        );
        Ok(removed)
    }

    pub async fn find_orphaned_cache_files(&self) -> Result<Vec<CacheFile>, super::GalleryError> {
        let image_paths: Vec<String> = {
            let cache = self.image_cache.read_all().await;
            cache.keys().cloned().collect()
        };

        let entries = self.list_cache_entries().await?;
        let (_, orphaned) = self.partition_cache_files(entries, &image_paths, ImageSize::ALL, true);
        Ok(orphaned)
    }

    pub async fn list_cache_entries(
        &self,
    ) -> Result<Vec<crate::storage::StorageEntry>, super::GalleryError> {
        match self.cache_storage.list("").await {
            Ok(entries) => Ok(entries),
            Err(crate::storage::StorageError::NotFound(_)) => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn partition_cache_files(
        &self,
        entries: Vec<crate::storage::StorageEntry>,
        image_paths: &[String],
        sizes: &[ImageSize],
        include_tiles: bool,
    ) -> (Vec<CacheFile>, Vec<CacheFile>) {
        let image_hashes: HashSet<String> = image_paths
            .iter()
            .flat_map(|path| self.resized_cache_hashes(path, sizes))
            .collect();

        let mut matched = Vec::new();
        let mut unmatched = Vec::new();
        let mut tiles_by_position: HashMap<(u32, u32, bool, String), Vec<CacheFile>> =
            HashMap::new();

        for entry in entries.into_iter().filter(|entry| !entry.is_dir) {
            let size = entry.metadata.map(|metadata| metadata.size);

            if let Some((x, y, is_retina, format)) = parse_tile_cache_filename(&entry.path) {
                let position = (x, y, is_retina, format.to_string());
                tiles_by_position
                    .entry(position)
                    .or_default()
                    .push(CacheFile {
                        path: entry.path,
                        kind: CacheFileKind::Tile,
                        size,
                    });
            } else if let Some(hash) = parse_resized_cache_hash(&entry.path) {
                let belongs = image_hashes.contains(hash);
                let file = CacheFile {
                    path: entry.path,
                    kind: CacheFileKind::Resized,
                    size,
                };
                if belongs {
                    matched.push(file);
                } else {
                    unmatched.push(file);
                }
            }
        }

        let tile_size = self
            .config
            .tiles
            .as_ref()
            .filter(|_| include_tiles)
            .map(|tiles| tiles.tile_size);
        for ((x, y, is_retina, format), tiles) in tiles_by_position {
            let image_tiles: HashSet<String> = match tile_size {
                Some(tile_size) => image_paths
                    .iter()
                    .map(|path| {
                        generate_tile_cache_filename(path, x, y, tile_size, is_retina, &format)
                    })
                    .collect(),
                None => HashSet::new(),
            };
            for tile in tiles {
                if image_tiles.contains(&tile.path) {
                    matched.push(tile);
                } else {
                    unmatched.push(tile);
                }
            }
        }

        matched.sort_by(|a, b| a.path.cmp(&b.path));
        unmatched.sort_by(|a, b| a.path.cmp(&b.path));
        (matched, unmatched)
    }

    pub async fn delete_cache_files(&self, files: Vec<CacheFile>) -> Vec<CacheFile> {
        let mut deleted = Vec::with_capacity(files.len());
        for file in files {
            match self.cache_storage.delete(&file.path).await {
                Ok(_) => {
                    debug!("Deleted cache file: {}", file.path);
                    deleted.push(file);
                }
                Err(e) => {
                    warn!("Failed to delete cache file {}: {}", file.path, e);
                }
            }
        }
        deleted
    }

    /// Load all cache filenames into a HashSet for fast lookups.
    pub async fn load_cache_file_set(&self) -> HashSet<String> {
        match self.cache_storage.list("").await {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| !e.is_dir)
                .map(|e| e.path)
                .collect(),
            Err(e) => {
                debug!("Failed to list cache files: {}", e);
                HashSet::new()
            }
        }
    }

    /// Check what formats are available for a specific image and size in cache
    /// Uses pre-loaded cache file set for fast lookups (no storage API calls)
    pub fn check_format_coverage_fast(
        &self,
        relative_path: &str,
        size: ImageSize,
        cache_files: &HashSet<String>,
    ) -> FormatCoverage {
        let mut coverage = FormatCoverage::default();

        let formats_to_check = vec![
            (OutputFormat::Jpeg, &mut coverage.has_jpeg),
            (OutputFormat::WebP, &mut coverage.has_webp),
            (OutputFormat::Png, &mut coverage.has_png),
            #[cfg(feature = "avif")]
            (OutputFormat::Avif, &mut coverage.has_avif),
        ];

        for (format, has_format) in formats_to_check {
            // Skip WebP for PNG sources
            if format == OutputFormat::WebP && relative_path.to_lowercase().ends_with(".png") {
                *has_format = true; // Mark as "has" since we don't expect it
                continue;
            }

            // Determine if watermark applies (only for medium + copyright holder)
            let apply_watermark =
                size.supports_watermark() && self.config.copyright_holder.is_some();

            let cache_filename = self.generate_cache_filename(
                relative_path,
                &size.as_str(),
                format.extension(),
                apply_watermark,
            );

            // Fast lookup in pre-loaded set
            *has_format = cache_files.contains(&cache_filename);
        }

        coverage
    }

    /// Get missing formats for all images and sizes
    pub async fn analyze_missing_formats(
        &self,
    ) -> Result<HashMap<(String, ImageSize), Vec<OutputFormat>>, super::GalleryError> {
        let mut missing_formats = HashMap::new();
        let sizes = ImageSize::ALL;

        // Load all cache files once for fast lookups
        let cache_files = self.load_cache_file_set().await;

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let cache = self.image_cache.read_all().await;
            cache.keys().cloned().collect()
        };

        for image_path in image_paths {
            if !self.is_image(&image_path) {
                continue;
            }

            for &size in sizes {
                let coverage = self.check_format_coverage_fast(&image_path, size, &cache_files);
                let missing = coverage.missing_formats(&image_path);
                if !missing.is_empty() {
                    missing_formats.insert((image_path.clone(), size), missing);
                }
            }
        }

        Ok(missing_formats)
    }

    /// Analyze and report format coverage statistics for the gallery
    pub async fn report_format_coverage(&self) -> Result<(), super::GalleryError> {
        info!(
            "Analyzing format coverage for gallery '{}'",
            self.config.name
        );

        // Load all cache files once for fast lookups
        let cache_files = self.load_cache_file_set().await;
        let sizes = ImageSize::ALL;

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let cache = self.image_cache.read_all().await;
            cache.keys().cloned().collect()
        };

        let total_images = image_paths.len();
        if total_images == 0 {
            info!("No images found in gallery '{}'", self.config.name);
            return Ok(());
        }

        // Count format coverage for each size
        let mut coverage_stats = HashMap::new();
        for &size in sizes {
            let mut format_counts = HashMap::new();

            // Initialize counters
            format_counts.insert("jpeg", 0);
            format_counts.insert("webp", 0);
            format_counts.insert("png", 0);
            #[cfg(feature = "avif")]
            format_counts.insert("avif", 0);

            for image_path in &image_paths {
                if !self.is_image(image_path) {
                    continue;
                }

                let coverage = self.check_format_coverage_fast(image_path, size, &cache_files);
                if coverage.has_jpeg {
                    *format_counts.get_mut("jpeg").unwrap() += 1;
                }
                if coverage.has_webp || image_path.to_lowercase().ends_with(".png") {
                    *format_counts.get_mut("webp").unwrap() += 1;
                }
                if coverage.has_png {
                    *format_counts.get_mut("png").unwrap() += 1;
                }
                #[cfg(feature = "avif")]
                if coverage.has_avif {
                    *format_counts.get_mut("avif").unwrap() += 1;
                }
            }

            coverage_stats.insert(size.as_str(), format_counts);
        }

        // Report statistics
        info!(
            "=== Format Coverage Report for Gallery '{}' ===",
            self.config.name
        );
        info!("Total images: {}", total_images);

        for &size in sizes {
            if let Some(format_counts) = coverage_stats.get(&size.as_str()) {
                info!("Size '{}' coverage:", size.as_str());

                for (format, &count) in format_counts {
                    let percentage = if total_images > 0 {
                        (count as f64 / total_images as f64) * 100.0
                    } else {
                        0.0
                    };

                    info!(
                        "  {}: {}/{} ({:.1}%)",
                        format.to_uppercase(),
                        count,
                        total_images,
                        percentage
                    );
                }
            }
        }

        // Calculate total missing across all sizes and formats
        let mut total_missing = 0;
        let mut missing_by_format: HashMap<&str, usize> = HashMap::new();

        for &size in sizes {
            if let Some(format_counts) = coverage_stats.get(&size.as_str()) {
                for (&format, &count) in format_counts {
                    let missing = total_images.saturating_sub(count);
                    if missing > 0 {
                        total_missing += missing;
                        *missing_by_format.entry(format).or_insert(0) += missing;
                    }
                }
            }
        }

        if total_missing > 0 {
            info!(
                "Found {} missing format variants across all sizes",
                total_missing
            );

            info!("Missing formats breakdown:");
            for (format, count) in missing_by_format {
                info!("  {}: {} missing", format.to_uppercase(), count);
            }
        } else {
            info!("✓ All expected formats are available for all images");
        }

        info!("=== End Format Coverage Report ===");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Gallery;
    use crate::storage::FilesystemStorage;
    use std::sync::Arc;

    fn create_test_storage(dir: &str) -> crate::storage::DynStorage {
        let path = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    fn create_test_storage_from_path(path: &std::path::Path) -> crate::storage::DynStorage {
        std::fs::create_dir_all(path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    #[test]
    fn test_cache_key_consistency() {
        let gallery_config = crate::config::GallerySystemConfig::default();
        let source_storage = create_test_storage(&gallery_config.source_directory);
        let cache_storage = create_test_storage(&gallery_config.cache_directory);
        let gallery = Gallery::new(gallery_config, source_storage, cache_storage);

        // Test regular image cache keys
        let path = "vacation/beach.jpg";
        let size = "thumbnail";
        let format = "webp";

        // These should produce consistent keys
        let key1 = gallery.generate_image_cache_key(path, size, format, false);
        let key2 = gallery.generate_image_cache_key(path, size, format, false);
        assert_eq!(key1, key2, "Cache keys should be identical for same inputs");

        // Different inputs should produce different keys
        let key3 = gallery.generate_image_cache_key(path, "medium", format, false);
        assert_ne!(key1, key3, "Different sizes should produce different keys");

        // Test that the same inputs always produce the same hash
        let another_key = gallery.generate_image_cache_key(path, size, format, false);
        assert_eq!(key1, another_key, "Keys should be deterministic");

        // Test watermark differentiation
        let key_with_watermark = gallery.generate_image_cache_key(path, size, format, true);
        assert_ne!(
            key1, key_with_watermark,
            "Watermarked and non-watermarked keys should differ"
        );

        // Test new composite cache keys with context
        let comp_key1 = gallery.generate_composite_cache_key_with_context("gallery/2024");
        let comp_key2 = gallery.generate_composite_cache_key_with_context("gallery/2024");
        assert_eq!(comp_key1, comp_key2, "Composite keys should be consistent");

        // Test that the new keys contain gallery context and base64 encoding
        assert!(comp_key1.starts_with("composite_default_"));

        // Test root composite with context
        let root_key = gallery.generate_composite_cache_key_with_context("");
        assert_eq!(root_key, "composite_default_root");
    }

    #[test]
    fn test_improved_composite_cache_structure() {
        use base64::{Engine as _, engine::general_purpose};
        let gallery_config = crate::config::GallerySystemConfig::default();
        let source_storage = create_test_storage(&gallery_config.source_directory);
        let cache_storage = create_test_storage(&gallery_config.cache_directory);
        let gallery = Gallery::new(gallery_config, source_storage, cache_storage);

        // Test safe path key generation
        let safe_key_simple = gallery.generate_safe_path_key("vacation/2024");
        assert_eq!(
            safe_key_simple,
            general_purpose::URL_SAFE_NO_PAD.encode("vacation/2024")
        );

        let safe_key_root = gallery.generate_safe_path_key("");
        assert_eq!(safe_key_root, "root");

        // Test very long path handling
        let long_path = "a".repeat(100);
        let safe_key_long = gallery.generate_safe_path_key(&long_path);
        assert!(safe_key_long.len() <= 42); // Should be truncated with hash
        assert!(safe_key_long.contains("_")); // Should have hash suffix

        // Test new structured cache filename generation
        let filename = gallery.generate_composite_cache_filename("vacation/2024");
        assert!(filename.starts_with("composite_default_"));
        assert!(filename.ends_with(".jpg"));
        assert!(filename.contains(&general_purpose::URL_SAFE_NO_PAD.encode("vacation/2024")));

        // Test cache key with context
        let context_key = gallery.generate_composite_cache_key_with_context("gallery/photos");
        assert!(context_key.starts_with("composite_default_"));
        assert!(context_key.contains(&general_purpose::URL_SAFE_NO_PAD.encode("gallery/photos")));

        // Test consistency
        let filename1 = gallery.generate_composite_cache_filename("test/path");
        let filename2 = gallery.generate_composite_cache_filename("test/path");
        assert_eq!(
            filename1, filename2,
            "Cache filenames should be deterministic"
        );
    }

    #[test]
    fn test_cache_filename_generation() {
        let gallery_config = crate::config::GallerySystemConfig::default();
        let source_storage = create_test_storage(&gallery_config.source_directory);
        let cache_storage = create_test_storage(&gallery_config.cache_directory);
        let gallery = Gallery::new(gallery_config, source_storage, cache_storage);

        let filename = gallery.generate_cache_filename("test.jpg", "thumbnail", "webp", false);
        assert!(
            filename.ends_with(".webp"),
            "Filename should end with correct extension"
        );

        // Verify the hash part is consistent
        let hash = gallery.generate_image_cache_key("test.jpg", "thumbnail", "webp", false);
        assert_eq!(filename, format!("{}.webp", hash));
    }

    #[tokio::test]
    async fn test_preview_readiness_respects_configured_formats() {
        use super::super::ImageMetadata;
        use std::collections::HashSet;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            pregenerate: Some(crate::PregenerateConfig {
                formats: crate::config::defaults::default_pregenerate_formats(),
                sizes: crate::config::defaults::default_pregenerate_sizes(),
                tiles: false,
            }),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        gallery
            .image_cache
            .insert(
                "test.jpg".to_string(),
                ImageMetadata {
                    dimensions: (100, 100),
                    capture_date: None,
                    camera_info: None,
                    location_info: None,
                    modification_date: None,
                    color_profile: None,
                    preview_ready: false,
                },
            )
            .await;

        let mut cache_files = HashSet::new();
        for size in [
            super::ImageSize::Thumbnail,
            super::ImageSize::ThumbnailRetina,
            super::ImageSize::Gallery,
            super::ImageSize::GalleryRetina,
            super::ImageSize::Medium,
            super::ImageSize::MediumRetina,
        ] {
            for format in ["jpg", "webp"] {
                cache_files.insert(gallery.generate_cache_filename(
                    "test.jpg",
                    &size.as_str(),
                    format,
                    false,
                ));
            }
        }

        assert!(
            gallery
                .update_preview_readiness("test.jpg", &cache_files)
                .await
        );

        let cache = gallery.image_cache.read_all().await;
        assert!(cache.get("test.jpg").unwrap().preview_ready);
    }

    #[tokio::test]
    async fn test_hidden_folder_not_in_listing() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();

        // Create visible folder
        let visible_dir = source_dir.join("visible");
        fs::create_dir_all(&visible_dir).unwrap();
        fs::write(
            visible_dir.join("_folder.md"),
            "# Visible Folder\nThis is visible",
        )
        .unwrap();

        // Create hidden folder with TOML front matter
        let hidden_dir = source_dir.join("hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(
            hidden_dir.join("_folder.md"),
            r#"+++
hidden = true
+++

# Hidden Content

This folder should not appear in listings.
"#,
        )
        .unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        // Populate the folder cache first (mandatory for scan_directory)
        gallery.refresh_folder_cache().await.unwrap();

        let items = gallery.scan_directory("").await.unwrap();

        // Should have 1 visible folder, hidden folder should not appear
        assert_eq!(items.len(), 1);
        assert!(items.iter().any(|i| i.name == "visible"));
        assert!(!items.iter().any(|i| i.name == "hidden"));
    }

    #[tokio::test]
    async fn test_hidden_folder_directly_accessible() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();

        // Create hidden folder
        let hidden_dir = source_dir.join("hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(
            hidden_dir.join("_folder.md"),
            r#"+++
hidden = true
+++
Hidden folder
"#,
        )
        .unwrap();
        // Create a test image file
        fs::write(hidden_dir.join("test.jpg"), vec![0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        // Populate the folder cache first (mandatory for scan_directory)
        gallery.refresh_folder_cache().await.unwrap();

        // Should be able to access hidden folder directly
        let items = gallery.scan_directory("hidden").await.unwrap();
        assert_eq!(items.len(), 1); // Should see the image
        assert_eq!(items[0].name, "test.jpg");
    }

    #[tokio::test]
    async fn test_remove_stale_metadata_entries() {
        use super::super::ImageMetadata;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        // Insert metadata for "existing" and "deleted" images
        let metadata = ImageMetadata {
            dimensions: (100, 100),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };

        {
            let mut cache = gallery.image_cache.write_all().await;
            cache.insert("existing.jpg".to_string(), metadata.clone());
            cache.insert("deleted.jpg".to_string(), metadata.clone());
            cache.insert("also_deleted.jpg".to_string(), metadata);
        }

        // Only "existing.jpg" is in the current image paths
        let current_paths = vec!["existing.jpg".to_string()];

        // Call remove_stale_metadata_entries
        let removed_count = gallery.remove_stale_metadata_entries(&current_paths).await;

        // Should have removed 2 entries
        assert_eq!(removed_count, 2);

        // Verify remaining cache entries
        let cache = gallery.image_cache.read_all().await;
        assert!(cache.contains_key("existing.jpg"));
        assert!(!cache.contains_key("deleted.jpg"));
        assert!(!cache.contains_key("also_deleted.jpg"));
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_orphaned_cache_files() {
        use super::super::ImageMetadata;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        // Insert metadata for an image that "exists"
        let metadata = ImageMetadata {
            dimensions: (100, 100),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };

        {
            let mut cache = gallery.image_cache.write_all().await;
            cache.insert("valid_image.jpg".to_string(), metadata);
        }

        // Create a cache file for the valid image
        let valid_hash = gallery.generate_cache_key("valid_image.jpg", "thumbnail_webp");
        let valid_cache_file = cache_dir.join(format!("{}.webp", valid_hash));
        std::fs::write(&valid_cache_file, b"valid cache content").unwrap();

        // Create orphaned cache files (no corresponding metadata entry)
        let orphan1 = cache_dir.join(format!(
            "{}.jpg",
            gallery.generate_cache_key("deleted_image.jpg", "thumbnail_jpg")
        ));
        let orphan2 = cache_dir.join(format!(
            "{}_watermarked.webp",
            gallery.generate_cache_key("deleted_image.jpg", "medium_webp_watermarked")
        ));
        std::fs::write(&orphan1, b"orphan cache 1").unwrap();
        std::fs::write(&orphan2, b"orphan cache 2").unwrap();

        // Files that are not recognizable cache files should NOT be removed
        let unknown = cache_dir.join("orphan123456789abcdef.jpg");
        let notes = cache_dir.join("notes.txt");
        std::fs::write(&unknown, b"unknown").unwrap();
        std::fs::write(&notes, b"notes").unwrap();

        // Create protected files that should NOT be removed
        let metadata_cache = cache_dir.join("metadata_cache.json");
        let cache_metadata = cache_dir.join("cache_metadata.json");
        std::fs::write(&metadata_cache, b"{}").unwrap();
        std::fs::write(&cache_metadata, b"{}").unwrap();

        // Create a composite file that should NOT be removed
        let composite = cache_dir.join("composite_test_abc123.jpg");
        std::fs::write(&composite, b"composite").unwrap();

        // Verify all files exist before cleanup
        assert!(valid_cache_file.exists());
        assert!(orphan1.exists());
        assert!(orphan2.exists());
        assert!(metadata_cache.exists());
        assert!(cache_metadata.exists());
        assert!(composite.exists());

        // Call remove_orphaned_cache_files
        let removed = gallery.remove_orphaned_cache_files(false).await.unwrap();

        // Should have removed 2 orphaned files
        assert_eq!(removed.len(), 2);

        // Verify valid cache file still exists
        assert!(valid_cache_file.exists(), "Valid cache file should remain");

        // Verify orphaned files were removed
        assert!(!orphan1.exists(), "Orphan 1 should be removed");
        assert!(!orphan2.exists(), "Orphan 2 should be removed");

        // Verify protected files still exist
        assert!(metadata_cache.exists(), "metadata_cache.json should remain");
        assert!(cache_metadata.exists(), "cache_metadata.json should remain");
        assert!(composite.exists(), "Composite files should remain");
        assert!(unknown.exists(), "Unrecognized files should remain");
        assert!(notes.exists(), "Unrecognized files should remain");
    }

    #[tokio::test]
    async fn test_cache_cleanup_integration() {
        use super::super::ImageMetadata;
        use std::sync::Arc;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create an actual image file
        std::fs::write(
            source_dir.join("real_image.jpg"),
            vec![0xFF, 0xD8, 0xFF, 0xE0],
        )
        .unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Arc::new(Gallery::new(config, source_storage, cache_storage));

        // Simulate: metadata for an image that was deleted
        let metadata = ImageMetadata {
            dimensions: (100, 100),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };

        {
            let mut cache = gallery.image_cache.write_all().await;
            // Add both real and deleted image metadata
            cache.insert("real_image.jpg".to_string(), metadata.clone());
            cache.insert("deleted_image.jpg".to_string(), metadata);
        }

        // Create cache files for both
        let real_hash = gallery.generate_cache_key("real_image.jpg", "thumbnail_webp");
        let deleted_hash = gallery.generate_cache_key("deleted_image.jpg", "thumbnail_webp");
        std::fs::write(cache_dir.join(format!("{}.webp", real_hash)), b"cache").unwrap();
        std::fs::write(cache_dir.join(format!("{}.webp", deleted_hash)), b"cache").unwrap();

        // Verify initial state
        {
            let cache = gallery.image_cache.read_all().await;
            assert_eq!(cache.len(), 2);
        }

        // Call refresh_all_metadata which should detect the deleted image
        // and clean up its metadata entry
        gallery.refresh_all_metadata().await.unwrap();

        // After refresh, only real_image.jpg should be in metadata cache
        {
            let cache = gallery.image_cache.read_all().await;
            assert!(cache.contains_key("real_image.jpg"));
            assert!(!cache.contains_key("deleted_image.jpg"));
        }

        // Run cache validation which should remove orphaned cache files
        gallery.remove_orphaned_cache_files(false).await.unwrap();

        // The deleted image's cache file should now be orphaned and removed
        let deleted_cache_file = cache_dir.join(format!("{}.webp", deleted_hash));
        assert!(
            !deleted_cache_file.exists(),
            "Deleted image cache file should be removed"
        );

        // Real image's cache file should still exist
        let real_cache_file = cache_dir.join(format!("{}.webp", real_hash));
        assert!(
            real_cache_file.exists(),
            "Real image cache file should remain"
        );
    }

    #[test]
    fn test_parse_tile_cache_filename() {
        use super::{generate_tile_cache_filename, parse_tile_cache_filename};

        for (x, y, is_retina, format) in [(0, 0, false, "avif"), (3, 12, true, "webp")] {
            let filename = generate_tile_cache_filename("a/b.jpg", x, y, 1024, is_retina, format);
            assert_eq!(
                parse_tile_cache_filename(&filename),
                Some((x, y, is_retina, format))
            );
        }

        assert_eq!(parse_tile_cache_filename("3f9ae1.avif"), None);
        assert_eq!(parse_tile_cache_filename("tile_0_0.avif"), None);
        assert_eq!(parse_tile_cache_filename("tile_a_0.3f9ae1.avif"), None);
    }

    #[tokio::test]
    async fn test_remove_orphaned_cache_files_keeps_current_tiles() {
        use super::super::ImageMetadata;
        use super::{CacheFileKind, generate_tile_cache_filename};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            tiles: Some(crate::config::TileConfig { tile_size: 512 }),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        let metadata = ImageMetadata {
            dimensions: (1024, 768),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };
        gallery
            .image_cache
            .write_all()
            .await
            .insert("current.jpg".to_string(), metadata);

        let mut current_tiles = Vec::new();
        for (x, y) in [(0, 0), (1, 1)] {
            for is_retina in [false, true] {
                current_tiles.push(generate_tile_cache_filename(
                    "current.jpg",
                    x,
                    y,
                    512,
                    is_retina,
                    "avif",
                ));
            }
        }
        let mut orphaned_tiles = vec![
            generate_tile_cache_filename("deleted.jpg", 0, 0, 512, false, "avif"),
            generate_tile_cache_filename("deleted.jpg", 1, 1, 512, true, "avif"),
            generate_tile_cache_filename("current.jpg", 0, 0, 1024, false, "avif"),
        ];
        orphaned_tiles.sort();

        for filename in current_tiles.iter().chain(&orphaned_tiles) {
            std::fs::write(cache_dir.join(filename), b"tile").unwrap();
        }

        let would_remove = gallery.remove_orphaned_cache_files(true).await.unwrap();
        let would_remove_paths: Vec<_> = would_remove.iter().map(|f| f.path.clone()).collect();
        assert_eq!(would_remove_paths, orphaned_tiles);
        assert!(
            would_remove
                .iter()
                .all(|f| f.kind == CacheFileKind::Tile && f.size == Some(4))
        );
        for filename in current_tiles.iter().chain(&orphaned_tiles) {
            assert!(
                cache_dir.join(filename).exists(),
                "Dry run should not remove {}",
                filename
            );
        }

        let removed = gallery.remove_orphaned_cache_files(false).await.unwrap();
        assert_eq!(removed.len(), orphaned_tiles.len());
        for filename in &current_tiles {
            assert!(
                cache_dir.join(filename).exists(),
                "Tile for current image should remain: {}",
                filename
            );
        }
        for filename in &orphaned_tiles {
            assert!(
                !cache_dir.join(filename).exists(),
                "Orphaned tile should be removed: {}",
                filename
            );
        }
    }

    #[tokio::test]
    async fn test_remove_orphaned_cache_files_keeps_image_watermark_variants() {
        use super::super::ImageMetadata;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let mut gallery = Gallery::new(config, source_storage, cache_storage);
        gallery.image_watermark_hash = Some("0123abcd".to_string());

        let metadata = ImageMetadata {
            dimensions: (100, 100),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };
        gallery
            .image_cache
            .write_all()
            .await
            .insert("current.jpg".to_string(), metadata);

        let current_files = [
            gallery.generate_cache_filename("current.jpg", "thumbnail", "webp", false),
            gallery.generate_cache_filename("current.jpg", "medium@2x", "jpg", true),
        ];
        let stale_file = format!(
            "{}.webp",
            gallery.generate_cache_key("current.jpg", "thumbnail_webp")
        );

        for filename in current_files.iter().chain([&stale_file]) {
            std::fs::write(cache_dir.join(filename), b"cache").unwrap();
        }

        let removed = gallery.remove_orphaned_cache_files(false).await.unwrap();
        let removed_paths: Vec<_> = removed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(removed_paths, [stale_file.as_str()]);
        for filename in &current_files {
            assert!(
                cache_dir.join(filename).exists(),
                "Watermarked cache file should remain: {}",
                filename
            );
        }
    }

    #[test]
    fn test_parse_resized_cache_hash() {
        use super::parse_resized_cache_hash;

        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(
            parse_resized_cache_hash(&format!("{}.jpg", hash)),
            Some(hash.as_str())
        );
        assert_eq!(
            parse_resized_cache_hash(&format!("{}_watermarked.webp", hash)),
            Some(hash.as_str())
        );

        assert_eq!(parse_resized_cache_hash("notes.txt"), None);
        assert_eq!(parse_resized_cache_hash(&hash), None);
        assert_eq!(parse_resized_cache_hash(&format!("{}.", hash)), None);
        assert_eq!(parse_resized_cache_hash(&format!("{}.a.b", hash)), None);
        assert_eq!(
            parse_resized_cache_hash(&format!("tile_0_0.{}.avif", hash)),
            None
        );
    }

    #[tokio::test]
    async fn test_partition_cache_files_respects_sizes_and_tiles() {
        use super::super::ImageSize;
        use super::generate_tile_cache_filename;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            tiles: Some(crate::config::TileConfig { tile_size: 512 }),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        let target_thumbnail = gallery.generate_cache_filename("a.jpg", "thumbnail", "jpg", false);
        let target_medium = gallery.generate_cache_filename("a.jpg", "medium", "webp", true);
        let target_tile = generate_tile_cache_filename("a.jpg", 1, 0, 512, true, "avif");
        let other_thumbnail = gallery.generate_cache_filename("b.jpg", "thumbnail", "jpg", false);
        let other_tile = generate_tile_cache_filename("b.jpg", 1, 0, 512, true, "avif");
        for filename in [
            &target_thumbnail,
            &target_medium,
            &target_tile,
            &other_thumbnail,
            &other_tile,
        ] {
            std::fs::write(cache_dir.join(filename), b"cache").unwrap();
        }

        let image_paths = vec!["a.jpg".to_string()];
        let paths = |files: Vec<super::CacheFile>| {
            let mut paths: Vec<String> = files.into_iter().map(|f| f.path).collect();
            paths.sort();
            paths
        };

        let entries = gallery.list_cache_entries().await.unwrap();
        let (matched, _) = gallery.partition_cache_files(
            entries,
            &image_paths,
            &[ImageSize::Thumbnail, ImageSize::ThumbnailRetina],
            false,
        );
        assert_eq!(paths(matched), vec![target_thumbnail.clone()]);

        let entries = gallery.list_cache_entries().await.unwrap();
        let (matched, unmatched) =
            gallery.partition_cache_files(entries, &image_paths, ImageSize::ALL, true);
        let mut expected = vec![target_thumbnail, target_medium, target_tile];
        expected.sort();
        assert_eq!(paths(matched), expected);
        let mut expected = vec![other_thumbnail, other_tile];
        expected.sort();
        assert_eq!(paths(unmatched), expected);
    }

    #[tokio::test]
    async fn test_tile_cache_filenames_cover_generated_tiles() {
        use super::super::ImageMetadata;
        use super::{TILE_FORMAT, generate_tile_cache_filename};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: source_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            tiles: Some(crate::config::TileConfig { tile_size: 512 }),
            ..Default::default()
        };

        let source_storage = create_test_storage_from_path(&source_dir);
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        let metadata = ImageMetadata {
            dimensions: (768, 1024),
            capture_date: None,
            camera_info: None,
            location_info: None,
            modification_date: None,
            color_profile: None,
            preview_ready: false,
        };
        gallery
            .image_cache
            .write_all()
            .await
            .insert("known.jpg".to_string(), metadata);

        let tile = |path: &str, x: u32, y: u32, is_retina: bool| {
            generate_tile_cache_filename(path, x, y, 512, is_retina, TILE_FORMAT.extension())
        };

        let known = gallery.tile_cache_filenames("known.jpg").await;
        assert_eq!(known.len(), 2 * 2 * 2);
        assert!(known.contains(&tile("known.jpg", 1, 1, true)));

        let unknown = gallery.tile_cache_filenames("unknown.jpg").await;
        assert_eq!(unknown.len(), 16 * 16 * 2);
        assert!(unknown.contains(&tile("unknown.jpg", 15, 15, false)));
    }
}
