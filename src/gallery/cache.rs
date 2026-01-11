use super::Gallery;
use super::image_processing::OutputFormat;
use super::types::ImageSize;
use crate::{CacheType, FormatCoverage};
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
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

        let mut metadata = self.cache_metadata.write().await;
        let needs_refresh = metadata.version != current_version;

        if needs_refresh {
            info!(
                "Version change detected ({}), refreshing metadata cache",
                current_version
            );

            // Clear the old metadata cache
            let mut cache = self.metadata_cache.write().await;
            cache.clear();
            drop(cache);

            // Update version and trigger refresh
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
        crate::cache::save_image_metadata_cache(&self.config.cache_directory, &cache).await?;

        // Reset dirty flag after successful save
        self.metadata_cache_dirty.store(false, Ordering::Relaxed);
        self.metadata_updates_since_save.store(0, Ordering::Relaxed);

        Ok(())
    }

    pub(crate) async fn save_cache_metadata(&self) -> Result<(), super::GalleryError> {
        let metadata = self.cache_metadata.read().await;
        crate::cache::save_cache_version_metadata(&self.config.cache_directory, &metadata).await?;
        Ok(())
    }

    pub async fn save_caches(&self) -> Result<(), super::GalleryError> {
        // Create cache directory if it doesn't exist
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Save both caches
        self.save_metadata_cache().await?;
        self.save_cache_metadata().await?;

        info!("Saved gallery caches to disk");
        Ok(())
    }

    pub(crate) fn generate_cache_key(&self, path: &str, size: &str) -> String {
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
    pub(crate) fn generate_composite_cache_key_with_context(&self, gallery_path: &str) -> String {
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

        let full_path = self.config.source_directory.join(relative_path);
        if !full_path.exists() {
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
            let formats_to_generate = match self.check_format_coverage(relative_path, *size).await {
                Ok(coverage) => coverage.missing_formats(relative_path),
                Err(e) => {
                    debug!(
                        "Failed to check format coverage for {} {}: {}",
                        relative_path, size, e
                    );
                    continue;
                }
            };

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

        match self
            .process_image_batch(&full_path, relative_path, variants)
            .await
        {
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
    pub async fn pregenerate_tiles_for_image(
        &self,
        relative_path: &str,
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

        let full_path = self.config.source_directory.join(relative_path);
        if !full_path.exists() {
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

        // Tiles are always AVIF for best compression and quality
        #[cfg(feature = "avif")]
        let formats = vec![OutputFormat::Avif];
        #[cfg(not(feature = "avif"))]
        let formats = vec![OutputFormat::WebP]; // Fallback to WebP if AVIF is disabled

        // Generate all tiles at once by requesting any tile
        // The tile generation function will generate all tiles for the image
        if grid_width > 0 && grid_height > 0 {
            // Just request tile 0,0 - the backend will generate all tiles
            for format in &formats {
                // Check for cancellation before each format (both pregeneration and shutdown)
                if self.pregeneration_token.lock().await.is_cancelled()
                    || self.shutdown_token.is_cancelled()
                {
                    return Ok(());
                }

                match self
                    .get_image_tile(&full_path, relative_path, 0, 0, *format)
                    .await
                {
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

                async move {
                    // Helper to check if cancelled
                    let is_cancelled =
                        || pregen_token.is_cancelled() || shutdown_token.is_cancelled();

                    // Check for cancellation before processing
                    if is_cancelled() {
                        cancelled.store(true, Ordering::Relaxed);
                        return (index, image_path, Err(super::GalleryError::InvalidPath));
                    }

                    let result = gallery.pregenerate_image_cache(&image_path).await;

                    // Check for cancellation between operations
                    if is_cancelled() {
                        cancelled.store(true, Ordering::Relaxed);
                        return (index, image_path, Err(super::GalleryError::InvalidPath));
                    }

                    // Also pre-generate tiles if configured and enabled
                    if result.is_ok()
                        && gallery.should_pregenerate_tiles()
                        && let Err(e) = gallery.pregenerate_tiles_for_image(&image_path).await
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

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
        };

        let sizes = ImageSize::ALL;

        for image_path in &image_paths {
            if !self.is_image(image_path) {
                continue;
            }

            for &size in sizes {
                // This will automatically remove outdated cache files via check_format_coverage
                match self.check_format_coverage(image_path, size).await {
                    Ok(_) => {
                        // Coverage check handles cleanup internally
                    }
                    Err(e) => {
                        debug!(
                            "Failed to check format coverage for {} {}: {}",
                            image_path,
                            size.as_str(),
                            e
                        );
                    }
                }
            }
        }

        info!(
            "Completed cache validation and cleanup for gallery '{}'",
            self.config.name
        );
        Ok(())
    }

    /// Check what formats are available for a specific image and size in cache
    pub async fn check_format_coverage(
        &self,
        relative_path: &str,
        size: ImageSize,
    ) -> Result<FormatCoverage, super::GalleryError> {
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
            let cache_path = self.config.cache_directory.join(&cache_filename);
            let source_path = self.config.source_directory.join(relative_path);

            // Check if cache file exists and is newer than source
            *has_format = if cache_path.exists() {
                match (
                    tokio::fs::metadata(&cache_path).await,
                    tokio::fs::metadata(&source_path).await,
                ) {
                    (Ok(cache_meta), Ok(source_meta)) => {
                        match (cache_meta.modified(), source_meta.modified()) {
                            (Ok(cache_time), Ok(source_time)) => {
                                if cache_time >= source_time {
                                    true
                                } else {
                                    // Cache is outdated, remove it
                                    debug!(
                                        "Cache file is outdated, removing: {} (cache: {:?}, source: {:?})",
                                        cache_path.display(),
                                        cache_time,
                                        source_time
                                    );
                                    if let Err(e) = tokio::fs::remove_file(&cache_path).await {
                                        debug!(
                                            "Failed to remove outdated cache file {}: {}",
                                            cache_path.display(),
                                            e
                                        );
                                    }
                                    false
                                }
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            } else {
                false
            };
        }

        Ok(coverage)
    }

    /// Get missing formats for all images and sizes
    pub async fn analyze_missing_formats(
        &self,
    ) -> Result<HashMap<(String, ImageSize), Vec<OutputFormat>>, super::GalleryError> {
        let mut missing_formats = HashMap::new();
        let sizes = ImageSize::ALL;

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
                match self.check_format_coverage(&image_path, size).await {
                    Ok(coverage) => {
                        let missing = coverage.missing_formats(&image_path);
                        if !missing.is_empty() {
                            missing_formats.insert((image_path.clone(), size), missing);
                        }
                    }
                    Err(e) => {
                        debug!(
                            "Failed to check format coverage for {} {}: {}",
                            image_path,
                            size.as_str(),
                            e
                        );
                    }
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

        let missing_formats_map = self.analyze_missing_formats().await?;
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

                match self.check_format_coverage(image_path, size).await {
                    Ok(coverage) => {
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
                    Err(e) => debug!(
                        "Failed to check format coverage for {} {}: {}",
                        image_path, size, e
                    ),
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

        let total_missing = missing_formats_map.len();
        if total_missing > 0 {
            info!(
                "Found {} missing format variants across all sizes",
                total_missing
            );

            // Count missing by format type
            let mut missing_by_format = HashMap::new();
            for (_, formats) in missing_formats_map {
                for format in formats {
                    *missing_by_format.entry(format.extension()).or_insert(0) += 1;
                }
            }

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

    #[test]
    fn test_cache_key_consistency() {
        let gallery_config = crate::config::GallerySystemConfig::default();
        let gallery = Gallery::new(gallery_config);

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
        let gallery = Gallery::new(gallery_config);

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
        let gallery = Gallery::new(gallery_config);

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
            source_directory: source_dir,
            cache_directory: cache_dir,
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let gallery = Gallery::new(config);

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
            source_directory: source_dir,
            cache_directory: cache_dir,
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
            ..Default::default()
        };

        let gallery = Gallery::new(config);

        // Should be able to access hidden folder directly
        let items = gallery.scan_directory("hidden").await.unwrap();
        assert_eq!(items.len(), 1); // Should see the image
        assert_eq!(items[0].name, "test.jpg");
    }
}
