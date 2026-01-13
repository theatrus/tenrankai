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
pub(crate) fn generate_tile_cache_filename(
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
    let hash = format!("{:x}", hasher.finalize());

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

impl Gallery {
    pub async fn initialize_and_check_version(&self) -> Result<(), super::GalleryError> {
        let current_version = env!("CARGO_PKG_VERSION");

        // Load caches from storage
        let loaded_metadata = crate::cache::load_image_metadata_cache(&self.cache_storage).await?;
        let loaded_cache_metadata =
            crate::cache::load_cache_version_metadata(&self.cache_storage).await;

        // Update the in-memory caches with loaded data
        {
            let mut cache = self.metadata_cache.write().await;
            *cache = loaded_metadata;
        }

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
        };

        if needs_refresh {
            info!(
                "Version change detected ({}), refreshing metadata cache for gallery '{}'",
                current_version, self.config.name
            );

            // Clear the old metadata cache
            let mut cache = self.metadata_cache.write().await;
            cache.clear();
            drop(cache);

            // Update version and trigger refresh
            let mut metadata = self.cache_metadata.write().await;
            metadata.version = current_version.to_string();
            metadata.last_full_refresh = std::time::SystemTime::now();
            drop(metadata);

            // Save the updated cache metadata
            self.save_cache_metadata().await?;
        } else {
            // Build the indexer from the existing cache
            let all_paths: Vec<String> = {
                let cache = self.metadata_cache.read().await;
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
        use std::sync::atomic::Ordering;

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
                        // Check if cache is dirty
                        if gallery.metadata_cache_dirty.load(Ordering::Relaxed) {
                            debug!("Cache is dirty, saving to disk");

                            if let Err(e) = gallery.save_metadata_cache().await {
                                error!("Failed to save metadata cache: {}", e);
                            } else {
                                // Reset dirty flag and update counter
                                gallery.metadata_cache_dirty.store(false, Ordering::Relaxed);
                                gallery
                                    .metadata_updates_since_save
                                    .store(0, Ordering::Relaxed);
                                info!("Periodic metadata cache save completed");
                            }
                        }
                    }
                }
            }
        });
    }

    pub(crate) async fn save_metadata_cache(&self) -> Result<(), super::GalleryError> {
        use std::sync::atomic::Ordering;

        let cache = self.metadata_cache.read().await;
        crate::cache::save_image_metadata_cache(&self.cache_storage, &cache).await?;

        // Reset dirty flag after successful save
        self.metadata_cache_dirty.store(false, Ordering::Relaxed);
        self.metadata_updates_since_save.store(0, Ordering::Relaxed);

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

        // Save both caches
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
        format!("{:x}", hasher.finalize())
    }

    /// Generate a cache key for regular images with size, format, and watermark status
    pub(crate) fn generate_image_cache_key(
        &self,
        path: &str,
        size: &str,
        format: &str,
        has_watermark: bool,
    ) -> String {
        let cache_key = if has_watermark {
            format!("{}_{}_{}", size, format, "watermarked")
        } else {
            format!("{}_{}", size, format)
        };
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
                let hash_suffix = &format!("{:x}", hasher.finalize())[..8];
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
            }
            Err(e) => {
                error!("Failed to generate cache for {}: {}", relative_path, e);
            }
        }

        Ok(())
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
        let metadata = self.metadata_cache.read().await;
        let image_metadata = match metadata.get(relative_path) {
            Some(meta) => meta,
            None => return Ok(()), // No metadata available
        };

        let (width, height) = image_metadata.dimensions;

        // Calculate grid size based on image dimensions and tile size
        // Cap at 8192px maximum dimension for tiles
        let max_dimension = width.max(height).min(8192);
        let grid_width = max_dimension.div_ceil(tile_size);
        let grid_height = max_dimension.div_ceil(tile_size);
        drop(metadata);

        // Skip if no tiles to generate
        if grid_width == 0 || grid_height == 0 {
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
            #[cfg(feature = "avif")]
            "avif",
            #[cfg(not(feature = "avif"))]
            "webp",
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
                    grid_width, grid_height, relative_path
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
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
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

        Ok(())
    }

    /// Validate and clean up outdated cache entries for all images
    pub async fn validate_and_cleanup_cache(self: Arc<Self>) -> Result<(), super::GalleryError> {
        info!(
            "Starting cache validation and cleanup for gallery '{}'",
            self.config.name
        );

        // Remove orphaned cache files (cache files for images that no longer exist)
        // Stale files (source newer than cache) are regenerated on-demand during serving
        let orphaned_count = self.remove_orphaned_cache_files().await?;
        if orphaned_count > 0 {
            info!("Removed {} orphaned cache files", orphaned_count);
        }

        info!(
            "Completed cache validation and cleanup for gallery '{}'",
            self.config.name
        );
        Ok(())
    }

    /// Remove cache files that don't belong to any current image in the metadata cache
    async fn remove_orphaned_cache_files(&self) -> Result<usize, super::GalleryError> {
        use std::collections::HashSet;

        // Build a set of all valid cache filename prefixes (hashes) for current images
        let valid_prefixes: HashSet<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            let mut prefixes = HashSet::new();

            for image_path in metadata_cache.keys() {
                // Generate the base hash for this image (used as prefix for all cached versions)
                let base_hash = self.generate_cache_key(image_path, "");
                prefixes.insert(base_hash);

                // Also generate hashes for each size variant
                for size in ImageSize::ALL {
                    for format in &["jpg", "webp", "png", "avif"] {
                        let hash = self.generate_cache_key(
                            image_path,
                            &format!("{}_{}", size.as_str(), format),
                        );
                        prefixes.insert(hash);

                        // Also watermarked variants
                        if size.supports_watermark() {
                            let hash_wm = self.generate_cache_key(
                                image_path,
                                &format!("{}_{}_watermarked", size.as_str(), format),
                            );
                            prefixes.insert(hash_wm);
                        }
                    }
                }
            }

            prefixes
        };

        // Files to keep (metadata files, composites handled separately)
        let protected_files = ["metadata_cache.json", "cache_metadata.json"];
        let composite_prefix = format!("composite_{}_", self.config.name);

        let mut removed_count = 0;

        // List files from cache storage
        let entries = match self.cache_storage.list("").await {
            Ok(entries) => entries,
            Err(e) => {
                debug!("Failed to list cache storage: {}", e);
                return Ok(0);
            }
        };

        for entry in entries {
            let filename = &entry.path;

            // Skip directories
            if entry.is_dir {
                continue;
            }

            // Skip protected files
            if protected_files.contains(&filename.as_str()) {
                continue;
            }

            // Skip composite files (they have their own lifecycle)
            if filename.starts_with(&composite_prefix) {
                continue;
            }

            // Check if this is an image cache file (hash-based filename)
            // Cache files are named: {hash}.{ext} or {hash}_watermarked.{ext}
            let hash_part = filename
                .split('.')
                .next()
                .unwrap_or("")
                .trim_end_matches("_watermarked");

            // If the hash doesn't match any valid prefix, it's orphaned
            if !hash_part.is_empty() && !valid_prefixes.contains(hash_part) {
                match self.cache_storage.delete(filename).await {
                    Ok(_) => {
                        debug!("Removed orphaned cache file: {}", filename);
                        removed_count += 1;
                    }
                    Err(e) => {
                        debug!("Failed to remove orphaned cache file {}: {}", filename, e);
                    }
                }
            }
        }

        Ok(removed_count)
    }

    /// Load all cache filenames into a HashSet for fast lookups
    async fn load_cache_file_set(&self) -> HashSet<String> {
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
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
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
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
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
title = "Hidden Folder"
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
        };

        {
            let mut cache = gallery.metadata_cache.write().await;
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
        let cache = gallery.metadata_cache.read().await;
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
        };

        {
            let mut cache = gallery.metadata_cache.write().await;
            cache.insert("valid_image.jpg".to_string(), metadata);
        }

        // Create a cache file for the valid image
        let valid_hash = gallery.generate_cache_key("valid_image.jpg", "thumbnail_webp");
        let valid_cache_file = cache_dir.join(format!("{}.webp", valid_hash));
        std::fs::write(&valid_cache_file, b"valid cache content").unwrap();

        // Create orphaned cache files (no corresponding metadata entry)
        let orphan1 =
            cache_dir.join("orphan123456789abcdef0123456789abcdef0123456789abcdef0123456789ab.jpg");
        let orphan2 = cache_dir
            .join("orphan987654321fedcba0987654321fedcba0987654321fedcba0987654321fe.webp");
        std::fs::write(&orphan1, b"orphan cache 1").unwrap();
        std::fs::write(&orphan2, b"orphan cache 2").unwrap();

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
        let removed_count = gallery.remove_orphaned_cache_files().await.unwrap();

        // Should have removed 2 orphaned files
        assert_eq!(removed_count, 2);

        // Verify valid cache file still exists
        assert!(valid_cache_file.exists(), "Valid cache file should remain");

        // Verify orphaned files were removed
        assert!(!orphan1.exists(), "Orphan 1 should be removed");
        assert!(!orphan2.exists(), "Orphan 2 should be removed");

        // Verify protected files still exist
        assert!(metadata_cache.exists(), "metadata_cache.json should remain");
        assert!(cache_metadata.exists(), "cache_metadata.json should remain");
        assert!(composite.exists(), "Composite files should remain");
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
        };

        {
            let mut cache = gallery.metadata_cache.write().await;
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
            let cache = gallery.metadata_cache.read().await;
            assert_eq!(cache.len(), 2);
        }

        // Call refresh_all_metadata which should detect the deleted image
        // and clean up its metadata entry
        gallery.refresh_all_metadata().await.unwrap();

        // After refresh, only real_image.jpg should be in metadata cache
        {
            let cache = gallery.metadata_cache.read().await;
            assert!(cache.contains_key("real_image.jpg"));
            assert!(!cache.contains_key("deleted_image.jpg"));
        }

        // Run cache validation which should remove orphaned cache files
        gallery.clone().validate_and_cleanup_cache().await.unwrap();

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
}
