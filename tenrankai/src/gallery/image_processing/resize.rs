use crate::gallery::types::ImageSize as SizeVariant;
use crate::gallery::{Gallery, GalleryError};
use crate::storage::{DynStorage, storage_exists_sync, storage_write_sync};
use bytes::Bytes;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tracing::{debug, info};

use super::types::{ImageSize, LoadedImage, OutputFormat};

impl Gallery {
    /// Process multiple variants of an image in a single batch
    /// This loads the image once and generates all requested sizes/formats
    pub async fn process_image_batch(
        &self,
        relative_path: &str,
        variants: Vec<(String, ImageSize, bool, OutputFormat)>, // (size_str, dimensions, apply_watermark, format)
    ) -> Result<Vec<PathBuf>, GalleryError> {
        // Generate batch deduplication key
        let variants_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            for (size, dims, watermark, format) in &variants {
                size.hash(&mut hasher);
                dims.width.hash(&mut hasher);
                dims.height.hash(&mut hasher);
                watermark.hash(&mut hasher);
                format.hash(&mut hasher);
            }
            hasher.finish()
        };

        let task_key = format!("batch:{}:{}", relative_path, variants_hash);
        let handle = self.task_deduplicator.should_execute(task_key).await;

        let mut result_paths = Vec::new();

        if handle.is_executor() {
            // We're the executor, process all variants

            // Ensure cache directory exists using storage
            self.cache_storage.create_dir("").await?;

            // Process in blocking thread
            let relative_path_owned = relative_path.to_string();
            let copyright_holder = self.config.copyright_holder.clone();
            let static_dir = std::path::PathBuf::from("static");
            let jpeg_quality = self.config.jpeg_quality.unwrap_or(85);
            let webp_quality = self.config.webp_quality.unwrap_or(85.0);
            let image_watermark = self.image_watermark.clone();
            let image_watermark_config = self.config.image_watermark.clone();

            // Pre-generate cache filenames (not full paths)
            let mut variant_configs = Vec::new();
            for (size_str, dimensions, apply_watermark, output_format) in &variants {
                let cache_filename = self.generate_cache_filename(
                    relative_path,
                    size_str,
                    output_format.extension(),
                    *apply_watermark,
                );
                variant_configs.push((
                    size_str.clone(),
                    dimensions.clone(),
                    *apply_watermark,
                    *output_format,
                    cache_filename,
                ));
            }

            // Get cancellation tokens for inner checks (both pregeneration and shutdown)
            let pregen_token = self.pregeneration_token.lock().await.clone();
            let shutdown_token = self.shutdown_token.clone();
            let cache_storage = self.cache_storage.clone();
            let source_storage = self.source_storage.clone();
            let cache_path = self.cache_path.clone();
            let runtime_handle = Handle::current();

            // Acquire semaphore permit to limit concurrent image processing
            let _permit = self
                .image_processing_semaphore
                .acquire()
                .await
                .map_err(|_| {
                    GalleryError::ProcessingError("Image processing semaphore closed".to_string())
                })?;

            // Track this blocking task for graceful shutdown
            let _task_guard = self.track_blocking_task();

            // Process and write all variants in blocking thread
            let generated_paths =
                tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>, GalleryError> {
                    // Guard is moved into blocking task, dropped when task completes
                    let _guard = _task_guard;
                    // Helper to check if cancelled
                    let is_cancelled =
                        || pregen_token.is_cancelled() || shutdown_token.is_cancelled();

                    // Check cancellation before loading
                    if is_cancelled() {
                        debug!("Batch processing cancelled before loading image");
                        return Ok(Vec::new());
                    }

                    // Load image once with all metadata from storage
                    debug!(
                        "Loading image for batch processing from storage: {}",
                        relative_path_owned
                    );
                    let loaded_image = LoadedImage::load_from_storage(
                        &source_storage,
                        &relative_path_owned,
                        &runtime_handle,
                    )?;
                    let mut paths = Vec::new();

                    let total_variants = variant_configs.len();
                    info!(
                        "Processing {} variants for image: {}",
                        total_variants, relative_path_owned
                    );

                    // Process each variant
                    for (
                        idx,
                        (size_str, dimensions, apply_watermark, output_format, cache_filename),
                    ) in variant_configs.into_iter().enumerate()
                    {
                        // Check cancellation between variants
                        if is_cancelled() {
                            info!(
                                "Batch processing cancelled after {}/{} variants",
                                idx, total_variants
                            );
                            break;
                        }

                        debug!(
                            "  [{}/{}] Generating {} {}x{} (format: {}, watermark: {})",
                            idx + 1,
                            total_variants,
                            size_str,
                            dimensions.width,
                            dimensions.height,
                            output_format.extension(),
                            apply_watermark
                        );

                        // Clone the loaded image for this variant
                        let mut variant_image = loaded_image.clone();

                        // Resize to target dimensions
                        variant_image.resize(dimensions.width, dimensions.height)?;

                        // Apply text watermark if needed
                        if apply_watermark && let Some(ref holder) = copyright_holder {
                            let font_path = static_dir.join("DejaVuSans.ttf");
                            variant_image.apply_watermark(holder, &font_path)?;
                        }

                        // Apply image watermark if configured (after text watermark)
                        if apply_watermark
                            && let Some(ref wm) = image_watermark
                            && let Some(ref wm_config) = image_watermark_config
                        {
                            variant_image.apply_image_watermark(wm, wm_config)?;
                        }

                        // Encode the variant (returns bytes)
                        let data =
                            variant_image.encode(output_format, jpeg_quality, webp_quality)?;

                        // Write to cache storage synchronously
                        storage_write_sync(
                            &cache_storage,
                            &cache_filename,
                            Bytes::from(data),
                            &runtime_handle,
                        )?;
                        paths.push(cache_path.join(&cache_filename));
                    }

                    debug!("Completed batch processing for {}", relative_path_owned);
                    Ok(paths)
                })
                .await??;

            result_paths = generated_paths;

            // Mark task as complete
            handle.complete().await;
        } else {
            // We're a waiter, wait for the executor to finish
            handle.wait().await;

            // Reconstruct the paths that should have been generated
            for (size_str, _, apply_watermark, output_format) in &variants {
                let cache_filename = self.generate_cache_filename(
                    relative_path,
                    size_str,
                    output_format.extension(),
                    *apply_watermark,
                );
                let cache_path = self.cache_path.join(&cache_filename);
                result_paths.push(cache_path);
            }
        }

        Ok(result_paths)
    }
    /// Parse size string and determine dimensions
    pub(crate) fn parse_size(&self, size: &str) -> Result<(ImageSize, bool), GalleryError> {
        // Parse the size variant from string
        let size_variant = SizeVariant::parse(size).ok_or(GalleryError::InvalidPath)?;

        // Get base dimensions from config based on the size
        let base_dimensions = match size_variant {
            SizeVariant::Thumbnail | SizeVariant::ThumbnailRetina => {
                ImageSize::new(self.config.thumbnail.width, self.config.thumbnail.height)
            }
            SizeVariant::Gallery | SizeVariant::GalleryRetina => ImageSize::new(
                self.config.gallery_size.width,
                self.config.gallery_size.height,
            ),
            SizeVariant::Medium | SizeVariant::MediumRetina => {
                ImageSize::new(self.config.medium.width, self.config.medium.height)
            }
            SizeVariant::Large | SizeVariant::LargeRetina => {
                ImageSize::new(self.config.large.width, self.config.large.height)
            }
            SizeVariant::Tile(_, _) => {
                // For tiles, use the configured tile size
                if let Some(tile_config) = &self.config.tiles {
                    ImageSize::new(tile_config.tile_size, tile_config.tile_size)
                } else {
                    return Err(GalleryError::InvalidPath);
                }
            }
        };

        // Apply multiplier for retina variants
        let final_dimensions = base_dimensions.with_multiplier(size_variant.multiplier() as u32);
        let supports_watermark = size_variant.supports_watermark();

        Ok((final_dimensions, supports_watermark))
    }

    /// Get resized image from cache or generate it
    pub(crate) async fn get_resized_image(
        &self,
        relative_path: &str,
        size: &str,
        output_format: OutputFormat,
    ) -> Result<PathBuf, GalleryError> {
        let (dimensions, _) = self.parse_size(size)?;

        // Determine if watermark will be applied
        let apply_watermark = self.should_apply_watermark(relative_path, size);

        // Generate consistent cache keys that include watermark status
        let cache_filename = self.generate_cache_filename(
            relative_path,
            size,
            output_format.extension(),
            apply_watermark,
        );
        let cache_path = self.cache_path.join(&cache_filename);

        // Check if cache file exists and is newer than original using storage abstraction
        if self
            .is_cache_valid_by_key(&cache_filename, relative_path)
            .await?
        {
            return Ok(cache_path);
        }

        // Use deduplicator to ensure we only process each image/size/format combo once
        let task_key = format!(
            "resize:{}:{}:{}:{}",
            relative_path,
            size,
            output_format.extension(),
            apply_watermark
        );
        let handle = self.task_deduplicator.should_execute(task_key).await;

        if handle.is_executor() {
            // We're the executor, generate the image

            // Acquire semaphore permit to limit concurrent image processing
            let _permit = self
                .image_processing_semaphore
                .acquire()
                .await
                .map_err(|_| {
                    GalleryError::ProcessingError("Image processing semaphore closed".to_string())
                })?;

            // Ensure cache directory exists using storage
            self.cache_storage.create_dir("").await?;

            // Process image and write to cache in blocking thread
            let relative_path_owned = relative_path.to_string();
            let copyright_holder = self.config.copyright_holder.clone();
            let static_dir = std::path::PathBuf::from("static");
            let jpeg_quality = self.config.jpeg_quality.unwrap_or(85);
            let webp_quality = self.config.webp_quality.unwrap_or(85.0);
            let cache_storage = self.cache_storage.clone();
            let source_storage = self.source_storage.clone();
            let runtime_handle = Handle::current();
            let image_watermark = self.image_watermark.clone();
            let image_watermark_config = self.config.image_watermark.clone();

            // Track this blocking task for graceful shutdown
            let _task_guard = self.track_blocking_task();

            tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
                // Guard is moved into blocking task, dropped when task completes
                let _guard = _task_guard;
                let data = process_image_from_storage(
                    &source_storage,
                    &relative_path_owned,
                    dimensions,
                    output_format,
                    apply_watermark,
                    copyright_holder,
                    &static_dir,
                    jpeg_quality,
                    webp_quality,
                    &runtime_handle,
                    image_watermark.as_deref(),
                    image_watermark_config.as_ref(),
                )?;

                // Write to cache storage synchronously
                storage_write_sync(
                    &cache_storage,
                    &cache_filename,
                    Bytes::from(data),
                    &runtime_handle,
                )?;
                Ok(())
            })
            .await??;

            // Mark task as complete
            handle.complete().await;
        } else {
            // We're a waiter, wait for the executor to finish
            handle.wait().await;
        }

        Ok(cache_path)
    }

    /// Get a specific tile from an image (tiles are always AVIF/WebP based on feature flags)
    pub(crate) async fn get_image_tile(
        &self,
        relative_path: &str,
        tile_x: u32,
        tile_y: u32,
    ) -> Result<PathBuf, GalleryError> {
        // Get default tile size from config
        let tile_size = self
            .config
            .tiles
            .as_ref()
            .map(|tc| tc.tile_size)
            .unwrap_or(1024);

        self.get_image_tile_with_size(relative_path, tile_x, tile_y, tile_size)
            .await
    }

    /// Get a specific tile from an image with custom size (for retina)
    /// Note: Tiles are always AVIF (or WebP fallback) regardless of source format
    pub(crate) async fn get_image_tile_with_size(
        &self,
        relative_path: &str,
        tile_x: u32,
        tile_y: u32,
        tile_size: u32,
    ) -> Result<PathBuf, GalleryError> {
        // Check if tiles are configured
        let _tile_config = self
            .config
            .tiles
            .as_ref()
            .ok_or(GalleryError::InvalidPath)?;

        // Tiles are always AVIF for best compression and quality
        #[cfg(feature = "avif")]
        let output_format = OutputFormat::Avif;
        #[cfg(not(feature = "avif"))]
        let output_format = OutputFormat::WebP; // Fallback to WebP if AVIF is disabled

        // Generate cache filename for this specific tile
        let is_retina = false; // This function handles base tiles, not retina
        let cache_filename = crate::gallery::cache::generate_tile_cache_filename(
            relative_path,
            tile_x,
            tile_y,
            tile_size,
            is_retina,
            output_format.extension(),
        );
        let cache_path = self.cache_path.join(&cache_filename);

        // Check if cache file exists and is newer than original (async)
        if self
            .is_cache_valid_by_key(&cache_filename, relative_path)
            .await?
        {
            return Ok(cache_path);
        }

        // Use deduplicator to ensure we only generate tiles once per image
        let task_key = format!("tile_generation:{}:{}", relative_path, tile_size);
        let handle = self.task_deduplicator.should_execute(task_key).await;

        if handle.is_executor() {
            // We're the executor, generate all tiles for this image

            // Acquire semaphore permit to limit concurrent image processing
            let _permit = self
                .image_processing_semaphore
                .acquire()
                .await
                .map_err(|_| {
                    GalleryError::ProcessingError("Image processing semaphore closed".to_string())
                })?;

            // Ensure cache directory exists using storage
            self.cache_storage.create_dir("").await?;

            // Process all tiles for this image in blocking thread
            // This ensures we load the source image only once
            let relative_path_owned = relative_path.to_string();
            let shutdown_flag = self.shutdown_flag.clone();
            let cache_storage = self.cache_storage.clone();
            let source_storage = self.source_storage.clone();
            let runtime_handle = Handle::current();

            // Track this blocking task for graceful shutdown
            let _task_guard = self.track_blocking_task();

            tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
                // Guard is moved into blocking task, dropped when task completes
                let _guard = _task_guard;
                process_all_tiles_from_storage(
                    &source_storage,
                    &relative_path_owned,
                    tile_size,
                    output_format,
                    &shutdown_flag,
                    &cache_storage,
                    &runtime_handle,
                )
            })
            .await??;

            // Mark task as complete
            handle.complete().await;
        } else {
            // We're a waiter, wait for the executor to finish
            handle.wait().await;
        }

        Ok(cache_path)
    }

    /// Check if cache file is valid (exists and newer than source)
    /// Uses storage abstraction for both cache and source
    pub(crate) async fn is_cache_valid_by_key(
        &self,
        cache_key: &str,
        source_path: &str,
    ) -> Result<bool, GalleryError> {
        // Get cache metadata from storage (returns NotFound if doesn't exist)
        let cache_meta = match self.cache_storage.metadata(cache_key).await {
            Ok(meta) => meta,
            Err(_) => return Ok(false),
        };

        // Get source metadata from source storage
        let source_meta = match self.source_storage.metadata(source_path).await {
            Ok(meta) => meta,
            Err(_) => return Ok(false),
        };

        // Compare timestamps if both are available
        match (cache_meta.last_modified, source_meta.last_modified) {
            // Both have timestamps - compare them
            (Some(cache_modified), Some(source_modified)) => Ok(cache_modified >= source_modified),
            // Cache exists but no timestamps available - consider it valid
            // (optimistic caching: if the file exists, it was created for a reason)
            (None, _) | (_, None) => {
                tracing::trace!(
                    "Cache exists but missing timestamps for {} (cache: {:?}, source: {:?})",
                    cache_key,
                    cache_meta.last_modified,
                    source_meta.last_modified
                );
                Ok(true)
            }
        }
    }
}

/// Process and resize image from storage, returning encoded bytes
#[allow(clippy::too_many_arguments)]
fn process_image_from_storage(
    source_storage: &DynStorage,
    relative_path: &str,
    dimensions: ImageSize,
    output_format: OutputFormat,
    apply_watermark: bool,
    copyright_holder: Option<String>,
    static_dir: &std::path::Path,
    jpeg_quality: u8,
    webp_quality: f32,
    runtime_handle: &Handle,
    image_watermark: Option<&tenrankai_image::ImageWatermark>,
    image_watermark_config: Option<&crate::config::ImageWatermarkConfig>,
) -> Result<Vec<u8>, GalleryError> {
    // Load image with all metadata from storage
    let mut loaded_image =
        LoadedImage::load_from_storage(source_storage, relative_path, runtime_handle)?;

    // Resize the image (this also handles gain maps)
    loaded_image.resize(dimensions.width, dimensions.height)?;

    // Apply text watermark if needed
    if apply_watermark && let Some(holder) = copyright_holder {
        let font_path = static_dir.join("DejaVuSans.ttf");
        loaded_image.apply_watermark(&holder, &font_path)?;
    }

    // Apply image watermark if configured (after text watermark)
    if apply_watermark
        && let Some(wm) = image_watermark
        && let Some(wm_config) = image_watermark_config
    {
        loaded_image.apply_image_watermark(wm, wm_config)?;
    }

    // Encode in requested format (returns bytes)
    loaded_image.encode(output_format, jpeg_quality, webp_quality)
}

/// Process all tiles for an image from storage, writing each tile to cache storage
fn process_all_tiles_from_storage(
    source_storage: &DynStorage,
    relative_path: &str,
    tile_size: u32,
    output_format: OutputFormat,
    shutdown_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    cache_storage: &DynStorage,
    runtime_handle: &Handle,
) -> Result<(), GalleryError> {
    debug!(
        "Loading image from storage to generate all tiles for: {}",
        relative_path
    );

    // Load image with all metadata preserved from storage
    let mut loaded_image =
        LoadedImage::load_from_storage(source_storage, relative_path, runtime_handle)?;

    let (img_width, img_height) = loaded_image.dimensions();

    // Resize the image if it's too large - we don't want to serve full resolution tiles
    // Cap the maximum dimension at 8192px for tile generation
    let max_tile_dimension = 8192;
    let max_dimension = img_width.max(img_height);

    if max_dimension > max_tile_dimension {
        // Scale down proportionally
        let scale = max_tile_dimension as f32 / max_dimension as f32;
        let new_width = (img_width as f32 * scale) as u32;
        let new_height = (img_height as f32 * scale) as u32;
        debug!(
            "Resizing image for tiles: {}x{} -> {}x{} (scale: {})",
            img_width, img_height, new_width, new_height, scale
        );
        loaded_image.resize(new_width, new_height)?;
    } else {
        debug!(
            "Image within tile dimension limit: {}x{}",
            img_width, img_height
        );
    }

    let (resized_width, resized_height) = loaded_image.dimensions();

    // Calculate grid dimensions
    let grid_width = resized_width.div_ceil(tile_size);
    let grid_height = resized_height.div_ceil(tile_size);

    debug!(
        "Generating {} tiles ({}x{} grid) for image",
        grid_width * grid_height,
        grid_width,
        grid_height
    );

    let mut written_count = 0;
    let mut skipped_count = 0;

    // Generate all tiles for this image
    for tile_y in 0..grid_height {
        // Check for shutdown at the start of each row
        if shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
            info!(
                "Tile generation cancelled for {} after {} tiles written",
                relative_path, written_count
            );
            return Ok(());
        }

        for tile_x in 0..grid_width {
            // Calculate tile boundaries using the fixed tile size
            let tile_start_x = tile_x * tile_size;
            let tile_start_y = tile_y * tile_size;

            // Don't go beyond image boundaries
            let tile_actual_width = tile_size.min(resized_width.saturating_sub(tile_start_x));
            let tile_actual_height = tile_size.min(resized_height.saturating_sub(tile_start_y));

            // Extract the tile region once
            if tile_actual_width > 0 && tile_actual_height > 0 {
                // Extract tile from the loaded image - use actual dimensions, not minimum
                let tile_img = loaded_image.image.crop_imm(
                    tile_start_x,
                    tile_start_y,
                    tile_actual_width,
                    tile_actual_height,
                );

                // Create a new LoadedImage for the tile with preserved metadata
                let mut tile_loaded_image =
                    LoadedImage::new(tile_img, loaded_image.source_path.clone());

                // Preserve ICC profile
                tile_loaded_image.icc_profile = loaded_image.icc_profile.clone();
                tile_loaded_image.format = loaded_image.format;

                // Handle gain map extraction for AVIF tiles
                #[cfg(feature = "avif")]
                if let Some(ref avif_info) = loaded_image.avif_info {
                    // Clone the AVIF info for the tile
                    let mut tile_avif_info = avif_info.clone();

                    // Extract corresponding gain map tile if present
                    if let Some(ref gm_info) = avif_info.gain_map_info
                        && let Some(ref gm_image) = gm_info.gain_map_image
                    {
                        // Calculate gain map tile coordinates proportionally
                        let gm_scale_x = gm_image.width() as f32 / resized_width as f32;
                        let gm_scale_y = gm_image.height() as f32 / resized_height as f32;

                        let gm_tile_x = (tile_start_x as f32 * gm_scale_x).round() as u32;
                        let gm_tile_y = (tile_start_y as f32 * gm_scale_y).round() as u32;
                        let gm_tile_width =
                            (tile_actual_width as f32 * gm_scale_x).round().max(1.0) as u32;
                        let gm_tile_height =
                            (tile_actual_height as f32 * gm_scale_y).round().max(1.0) as u32;

                        // Ensure we don't exceed gain map boundaries
                        let gm_tile_width =
                            gm_tile_width.min(gm_image.width().saturating_sub(gm_tile_x));
                        let gm_tile_height =
                            gm_tile_height.min(gm_image.height().saturating_sub(gm_tile_y));

                        if gm_tile_width > 0 && gm_tile_height > 0 {
                            let gm_tile = gm_image.crop_imm(
                                gm_tile_x,
                                gm_tile_y,
                                gm_tile_width,
                                gm_tile_height,
                            );

                            // Update the gain map info with tile
                            if let Some(ref mut gm_info_mut) = tile_avif_info.gain_map_info {
                                gm_info_mut.gain_map_image = Some(gm_tile);
                            }
                        }
                    }

                    tile_loaded_image.avif_info = Some(tile_avif_info);
                }

                // Handle gain map extraction for HEIF tiles
                #[cfg(feature = "heif")]
                if let Some(ref heif_info) = loaded_image.heif_info {
                    // Clone the HEIF info for the tile
                    let mut tile_heif_info = heif_info.clone();

                    // Extract corresponding gain map tile if present
                    if let Some(ref gm_info) = heif_info.gain_map_info
                        && let Some(ref gm_image) = gm_info.gain_map_image
                    {
                        // Calculate gain map tile coordinates proportionally
                        let gm_scale_x = gm_image.width() as f32 / resized_width as f32;
                        let gm_scale_y = gm_image.height() as f32 / resized_height as f32;

                        let gm_tile_x = (tile_start_x as f32 * gm_scale_x).round() as u32;
                        let gm_tile_y = (tile_start_y as f32 * gm_scale_y).round() as u32;
                        let gm_tile_width =
                            (tile_actual_width as f32 * gm_scale_x).round().max(1.0) as u32;
                        let gm_tile_height =
                            (tile_actual_height as f32 * gm_scale_y).round().max(1.0) as u32;

                        // Ensure we don't exceed gain map boundaries
                        let gm_tile_width =
                            gm_tile_width.min(gm_image.width().saturating_sub(gm_tile_x));
                        let gm_tile_height =
                            gm_tile_height.min(gm_image.height().saturating_sub(gm_tile_y));

                        if gm_tile_width > 0 && gm_tile_height > 0 {
                            let gm_tile = gm_image.crop_imm(
                                gm_tile_x,
                                gm_tile_y,
                                gm_tile_width,
                                gm_tile_height,
                            );

                            // Update the gain map info with tile
                            if let Some(ref mut gm_info_mut) = tile_heif_info.gain_map_info {
                                gm_info_mut.gain_map_image = Some(gm_tile);
                            }
                        }
                    }

                    tile_loaded_image.heif_info = Some(tile_heif_info);
                }

                // Encode both regular and @2x versions of this tile
                for is_retina in [false, true] {
                    let cache_filename = crate::gallery::cache::generate_tile_cache_filename(
                        relative_path,
                        tile_x,
                        tile_y,
                        tile_size,
                        is_retina,
                        output_format.extension(),
                    );

                    // Skip if this tile already exists (using sync adapter for storage)
                    if storage_exists_sync(cache_storage, &cache_filename, runtime_handle) {
                        skipped_count += 1;
                        continue;
                    }

                    // Encode the tile with preserved metadata (returns bytes)
                    // For tiles, use high quality settings
                    let data = tile_loaded_image.encode(
                        output_format,
                        90,   // High quality for JPEG fallback
                        90.0, // High quality for WebP fallback
                    )?;

                    // Write to cache storage synchronously
                    storage_write_sync(
                        cache_storage,
                        &cache_filename,
                        Bytes::from(data),
                        runtime_handle,
                    )?;
                    written_count += 1;
                }
            }
        }
    }

    if written_count > 0 {
        info!(
            "Wrote {} tiles for image, skipped {} existing tiles",
            written_count, skipped_count
        );
    } else if skipped_count > 0 {
        debug!("All {} tiles already exist for image", skipped_count);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgb};

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let mut img = image::ImageBuffer::new(width, height);

        // Create a pattern where each tile region has a different color
        // This helps verify we're extracting the right regions
        for y in 0..height {
            for x in 0..width {
                let tile_x = x / 1024;
                let tile_y = y / 1024;

                // Create a unique color for each tile based on its coordinates
                let r = ((tile_x * 50) % 256) as u8;
                let g = ((tile_y * 50) % 256) as u8;
                let b = (((tile_x + tile_y) * 50) % 256) as u8;

                img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }

        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_tile_extraction_normal_image() {
        // Test with an image that doesn't need resizing
        let img = create_test_image(4096, 3072);
        let (width, height) = (img.width(), img.height());

        // Should not be resized
        assert!(width <= 8192 && height <= 8192);

        // Test tile extraction at different positions
        let test_cases = vec![
            (0, 0, 0, 0, 1024, 1024),       // Top-left tile
            (1, 0, 1024, 0, 1024, 1024),    // Second tile horizontally
            (0, 1, 0, 1024, 1024, 1024),    // Second tile vertically
            (3, 2, 3072, 2048, 1024, 1024), // Last complete tile
        ];

        for (tile_x, tile_y, expected_x, expected_y, expected_w, expected_h) in test_cases {
            let tile_start_x = tile_x * 1024;
            let tile_start_y = tile_y * 1024;
            let tile_actual_width = 1024.min(width.saturating_sub(tile_start_x));
            let tile_actual_height = 1024.min(height.saturating_sub(tile_start_y));

            assert_eq!(tile_start_x, expected_x, "Tile {} x start", tile_x);
            assert_eq!(tile_start_y, expected_y, "Tile {} y start", tile_y);
            assert_eq!(tile_actual_width, expected_w, "Tile {} width", tile_x);
            assert_eq!(tile_actual_height, expected_h, "Tile {} height", tile_y);
        }
    }

    #[test]
    fn test_tile_extraction_large_image() {
        // Test with an image that needs resizing (10000x8000 -> 8192x6554)
        let original_width = 10000;
        let original_height = 8000;
        let max_dimension = 8192;

        // Calculate expected resize
        let scale = max_dimension as f32 / original_width as f32;
        let resized_width = (original_width as f32 * scale).round() as u32;
        let resized_height = (original_height as f32 * scale).round() as u32;

        assert_eq!(resized_width, 8192);
        assert_eq!(resized_height, 6554); // Verify proportional scaling

        // Test tile calculations on resized dimensions
        let grid_width = resized_width.div_ceil(1024);
        let grid_height = resized_height.div_ceil(1024);

        assert_eq!(grid_width, 8);
        assert_eq!(grid_height, 7); // 6554 / 1024 = 6.4, rounds up to 7

        // Test edge tiles
        let last_tile_x = 7;
        let last_tile_y = 6;

        let tile_start_x = last_tile_x * 1024;
        let tile_start_y = last_tile_y * 1024;
        let tile_actual_width = 1024.min(resized_width.saturating_sub(tile_start_x));
        let tile_actual_height = 1024.min(resized_height.saturating_sub(tile_start_y));

        assert_eq!(tile_start_x, 7168);
        assert_eq!(tile_start_y, 6144);
        assert_eq!(tile_actual_width, 1024); // Full width
        assert_eq!(tile_actual_height, 410); // Partial height (6554 - 6144)
    }

    #[test]
    fn test_coordinate_mapping() {
        // Test the coordinate mapping from display -> image -> tiled
        let image_width = 9433;
        let image_height = 6289;

        // Calculate tiled dimensions (as done in API)
        let max_dimension = image_width.max(image_height);
        let (tiled_width, tiled_height) = if max_dimension > 8192 {
            let scale = 8192.0 / max_dimension as f32;
            let new_width = (image_width as f32 * scale).round() as u32;
            let new_height = (image_height as f32 * scale).round() as u32;
            (new_width, new_height)
        } else {
            (image_width, image_height)
        };

        assert_eq!(tiled_width, 8192);
        assert_eq!(tiled_height, 5462);

        // Test coordinate mapping for center click
        let click_percent_x = 50.0;
        let click_percent_y = 50.0;

        // Map to image coordinates
        let img_x = (click_percent_x / 100.0) * image_width as f32;
        let img_y = (click_percent_y / 100.0) * image_height as f32;

        assert_eq!(img_x as u32, 4716);
        assert_eq!(img_y as u32, 3144);

        // Map to tiled coordinates
        let scale_x = tiled_width as f32 / image_width as f32;
        let scale_y = tiled_height as f32 / image_height as f32;

        let tiled_x = img_x * scale_x;
        let tiled_y = img_y * scale_y;

        assert_eq!(tiled_x.round() as u32, 4096); // Should be near center of 8192
        assert_eq!(tiled_y.round() as u32, 2731); // Should be near center of 5462

        // Calculate tile
        let tile_x = (tiled_x / 1024.0).floor() as u32;
        let tile_y = (tiled_y / 1024.0).floor() as u32;

        assert_eq!(tile_x, 4); // Tile 4 (0-indexed) since 4096/1024 = 4
        assert_eq!(tile_y, 2); // Tile 2 (0-indexed) since 2731/1024 = 2.66
    }

    #[test]
    fn test_tile_edge_cases() {
        // Test various edge cases

        // 1. Square image at exactly 8192
        let resized_width: u32 = 8192;
        let resized_height: u32 = 8192;
        let grid_width = resized_width.div_ceil(1024);
        let grid_height = resized_height.div_ceil(1024);
        assert_eq!(grid_width, 8);
        assert_eq!(grid_height, 8);

        // 2. Image slightly over tile boundary
        let resized_width: u32 = 8193; // Just over 8192
        let grid_width = resized_width.div_ceil(1024);
        assert_eq!(grid_width, 9); // Should need 9 tiles

        // 3. Very small image
        let resized_width: u32 = 500;
        let resized_height: u32 = 300;
        let grid_width = resized_width.div_ceil(1024);
        let grid_height = resized_height.div_ceil(1024);
        assert_eq!(grid_width, 1);
        assert_eq!(grid_height, 1);
    }
}
