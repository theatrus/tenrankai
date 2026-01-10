use crate::copyright::{CopyrightConfig, add_copyright_notice};
use crate::gallery::types::ImageSize as SizeVariant;
use crate::gallery::{Gallery, GalleryError};
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use std::path::{Path, PathBuf};
use tracing::{debug, error};

use super::formats;
#[cfg(feature = "avif")]
use super::formats::avif::AvifImageInfo;
use super::types::{ImageSize, OutputFormat};

// Type alias for AVIF info that works with or without the feature
#[cfg(feature = "avif")]
#[allow(dead_code)] // Used conditionally
type AvifInfoOption = Option<AvifImageInfo>;
#[cfg(not(feature = "avif"))]
type AvifInfoOption = Option<()>;

impl Gallery {
    /// Parse size string and determine dimensions
    pub(super) fn parse_size(&self, size: &str) -> Result<(ImageSize, bool), GalleryError> {
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
            SizeVariant::Large | SizeVariant::LargeRetina => ImageSize::new(self.config.large.width, self.config.large.height),
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
        original_path: &Path,
        relative_path: &str,
        size: &str,
        output_format: OutputFormat,
    ) -> Result<PathBuf, GalleryError> {
        let (dimensions, is_medium) = self.parse_size(size)?;

        // Determine if watermark will be applied
        let apply_watermark = is_medium && self.config.copyright_holder.is_some();

        // Generate consistent cache keys that include watermark status
        let cache_filename = self.generate_cache_filename(
            relative_path,
            size,
            output_format.extension(),
            apply_watermark,
        );
        let cache_path = self.config.cache_directory.join(&cache_filename);

        // Check if cache file exists and is newer than original
        if self.is_cache_valid(&cache_path, original_path).await? {
            return Ok(cache_path);
        }

        // Ensure cache directory exists
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Process image in blocking thread
        let original_path = original_path.to_path_buf();
        let cache_path_clone = cache_path.clone();
        let copyright_holder = self.config.copyright_holder.clone();
        let static_dir = std::path::PathBuf::from("static"); // TODO: Make configurable
        let jpeg_quality = self.config.jpeg_quality.unwrap_or(85);
        let webp_quality = self.config.webp_quality.unwrap_or(85.0);

        tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
            process_image(
                &original_path,
                &cache_path_clone,
                dimensions,
                output_format,
                apply_watermark,
                copyright_holder,
                &static_dir,
                jpeg_quality,
                webp_quality,
            )
        })
        .await??;

        Ok(cache_path)
    }

    /// Get a specific tile from an image
    pub(crate) async fn get_image_tile(
        &self,
        original_path: &Path,
        relative_path: &str,
        tile_x: u32,
        tile_y: u32,
        _output_format: OutputFormat, // Ignored - tiles are always AVIF
    ) -> Result<PathBuf, GalleryError> {
        // Get default tile size from config
        let tile_size = self.config.tiles.as_ref()
            .map(|tc| tc.tile_size)
            .unwrap_or(1024);
        
        self.get_image_tile_with_size(original_path, relative_path, tile_x, tile_y, tile_size, _output_format).await
    }
    
    /// Get a specific tile from an image with custom size (for retina)
    pub(crate) async fn get_image_tile_with_size(
        &self,
        original_path: &Path,
        relative_path: &str,
        tile_x: u32,
        tile_y: u32,
        tile_size: u32,
        _output_format: OutputFormat, // Ignored - tiles are always AVIF
    ) -> Result<PathBuf, GalleryError> {
        // Check if tiles are configured
        let _tile_config = self.config.tiles.as_ref()
            .ok_or(GalleryError::InvalidPath)?;

        // Tiles are always AVIF for best compression and quality
        #[cfg(feature = "avif")]
        let output_format = OutputFormat::Avif;
        #[cfg(not(feature = "avif"))]
        let output_format = OutputFormat::WebP; // Fallback to WebP if AVIF is disabled

        // Generate cache filename for this specific tile
        let cache_filename = self.generate_cache_filename(
            relative_path,
            &format!("tile_{}_{}", tile_x, tile_y),
            output_format.extension(),
            false, // No watermark on tiles
        );
        let cache_path = self.config.cache_directory.join(&cache_filename);

        // Check if cache file exists and is newer than original
        if self.is_cache_valid(&cache_path, original_path).await? {
            return Ok(cache_path);
        }

        // Ensure cache directory exists
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Process tile in blocking thread
        let original_path = original_path.to_path_buf();
        let cache_path_clone = cache_path.clone();

        tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
            process_tile(
                &original_path,
                &cache_path_clone,
                tile_x,
                tile_y,
                tile_size,
                output_format,
            )
        })
        .await??;

        Ok(cache_path)
    }

    /// Check if cache file is valid (exists and newer than source)
    pub(crate) async fn is_cache_valid(
        &self,
        cache_path: &Path,
        original_path: &Path,
    ) -> Result<bool, GalleryError> {
        if !cache_path.exists() {
            return Ok(false);
        }

        let cache_metadata = tokio::fs::metadata(cache_path).await?;
        let original_metadata = tokio::fs::metadata(original_path).await?;

        if let (Ok(cache_modified), Ok(original_modified)) =
            (cache_metadata.modified(), original_metadata.modified())
            && cache_modified >= original_modified
        {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Process and resize image
#[allow(clippy::too_many_arguments)]
fn process_image(
    original_path: &Path,
    cache_path: &Path,
    dimensions: ImageSize,
    output_format: OutputFormat,
    apply_watermark: bool,
    copyright_holder: Option<String>,
    static_dir: &Path,
    jpeg_quality: u8,
    webp_quality: f32,
) -> Result<(), GalleryError> {
    // Detect format and extract ICC profile
    #[allow(unused_variables)] // Used in avif feature
    let (icc_profile, detected_format) = extract_image_info(original_path)?;

    // Load and resize image - special handling for AVIF to preserve color properties
    debug!(
        "Opening image file: {:?}, detected format: {:?}",
        original_path, detected_format
    );

    let (img, _avif_info) = {
        #[cfg(feature = "avif")]
        {
            if detected_format == Some(ImageFormat::Avif) {
                // Use our custom AVIF reader to preserve color properties
                match formats::avif::read_avif_info(original_path) {
                    Ok((img, info)) => (img, Some(info)),
                    Err(e) => {
                        debug!(
                            "Failed to read AVIF with custom reader: {}, falling back",
                            e
                        );
                        (image::open(original_path)?, None)
                    }
                }
            } else {
                (image::open(original_path)?, None)
            }
        }
        #[cfg(not(feature = "avif"))]
        {
            (image::open(original_path)?, None::<()>)
        }
    };

    let resized = resize_image(&img, dimensions)?;

    // Resize gain map if present
    #[cfg(feature = "avif")]
    let mut resized_avif_info = _avif_info.clone();
    #[cfg(not(feature = "avif"))]
    let _resized_avif_info: AvifInfoOption = None;

    #[cfg(feature = "avif")]
    if let Some(ref mut info) = resized_avif_info
        && let Some(ref gm_info) = info.gain_map_info
        && let Some(ref gm_image) = gm_info.gain_map_image
    {
        // Resize gain map to match the proportion of the main image resize
        let (orig_width, orig_height) = (img.width(), img.height());
        let (resized_width, resized_height) = (resized.width(), resized.height());

        // Calculate scale factors
        let scale_x = resized_width as f32 / orig_width as f32;
        let scale_y = resized_height as f32 / orig_height as f32;

        // Apply same scale to gain map
        let (gm_width, gm_height) = (gm_image.width(), gm_image.height());
        let new_gm_width = (gm_width as f32 * scale_x).round() as u32;
        let new_gm_height = (gm_height as f32 * scale_y).round() as u32;

        // Ensure gain map is at least 1x1
        let new_gm_width = new_gm_width.max(1);
        let new_gm_height = new_gm_height.max(1);

        debug!(
            "Resizing gain map from {}x{} to {}x{}",
            gm_width, gm_height, new_gm_width, new_gm_height
        );

        let resized_gain_map =
            gm_image.resize_exact(new_gm_width, new_gm_height, FilterType::Lanczos3);

        // Update the gain map info with resized image
        if let Some(ref mut gm_info_mut) = info.gain_map_info {
            gm_info_mut.gain_map_image = Some(resized_gain_map);
        }
    }

    // Apply watermark if needed
    let final_image = if let (true, Some(holder)) = (apply_watermark, copyright_holder) {
        apply_copyright_watermark(resized, holder, static_dir)?
    } else {
        resized
    };

    // Save in requested format
    save_image(
        &final_image,
        cache_path,
        output_format,
        jpeg_quality,
        webp_quality,
        icc_profile.as_deref(),
        #[cfg(feature = "avif")]
        resized_avif_info.as_ref(),
        #[cfg(not(feature = "avif"))]
        None,
    )?;

    Ok(())
}

/// Extract ICC profile and detect format
fn extract_image_info(path: &Path) -> Result<(Option<Vec<u8>>, Option<ImageFormat>), GalleryError> {
    use std::io::BufReader;

    let file = std::fs::File::open(path)?;
    let buf_reader = BufReader::new(file);
    let decoder = image::ImageReader::new(buf_reader).with_guessed_format()?;
    let detected_format = decoder.format();

    let icc_profile = match detected_format {
        Some(ImageFormat::Jpeg) => formats::jpeg::extract_icc_profile(path),
        Some(ImageFormat::Png) => formats::png::extract_icc_profile(path),
        #[cfg(feature = "avif")]
        Some(ImageFormat::Avif) => formats::avif::extract_icc_profile(path),
        _ => None,
    };

    Ok((icc_profile, detected_format))
}

/// Resize image preserving aspect ratio
fn resize_image(img: &DynamicImage, dimensions: ImageSize) -> Result<DynamicImage, GalleryError> {
    let (orig_width, orig_height) = (img.width(), img.height());

    // Don't upscale - if requested dimensions are larger than original, use original
    let final_width = dimensions.width.min(orig_width);
    let final_height = dimensions.height.min(orig_height);

    // Only resize if dimensions are different
    if final_width != orig_width || final_height != orig_height {
        Ok(img.resize(final_width, final_height, FilterType::Lanczos3))
    } else {
        Ok(img.clone())
    }
}

/// Apply copyright watermark to image
fn apply_copyright_watermark(
    image: DynamicImage,
    copyright_holder: String,
    static_dir: &Path,
) -> Result<DynamicImage, GalleryError> {
    let font_path = static_dir.join("DejaVuSans.ttf");
    if !font_path.exists() {
        debug!("Font file not found at {:?}, skipping watermark", font_path);
        return Ok(image);
    }

    let copyright_config = CopyrightConfig {
        copyright_holder,
        font_size: 20.0,
        padding: 10,
    };

    match add_copyright_notice(&image, &copyright_config, &font_path) {
        Ok(watermarked) => Ok(watermarked),
        Err(e) => {
            error!("Failed to add copyright watermark: {}", e);
            Ok(image)
        }
    }
}

/// Save image in specified format
fn save_image(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
    webp_quality: f32,
    icc_profile: Option<&[u8]>,
    #[cfg(feature = "avif")] avif_info: Option<&AvifImageInfo>,
    #[cfg(not(feature = "avif"))] _avif_info: Option<()>,
) -> Result<(), GalleryError> {
    match format {
        OutputFormat::Jpeg => {
            formats::jpeg::save_with_profile(image, path, jpeg_quality, icc_profile)
        }
        OutputFormat::WebP => {
            formats::webp::save_with_profile(image, path, webp_quality, icc_profile)
        }
        OutputFormat::Png => formats::png::save(image, path),
        #[cfg(feature = "avif")]
        OutputFormat::Avif => {
            // Use the preserved AVIF info if available
            if let Some(info) = avif_info {
                formats::avif::save_with_info(image, path, 85, 6, Some(info))
            } else {
                // Fallback: preserve HDR if the source is 16-bit
                let preserve_hdr = matches!(
                    image,
                    DynamicImage::ImageLuma16(_)
                        | DynamicImage::ImageLumaA16(_)
                        | DynamicImage::ImageRgb16(_)
                        | DynamicImage::ImageRgba16(_)
                );
                formats::avif::save_with_profile(image, path, 85, 6, icc_profile, preserve_hdr)
            }
        }
    }
}

/// Process and extract a specific tile from an image
fn process_tile(
    original_path: &Path,
    cache_path: &Path,
    tile_x: u32,
    tile_y: u32,
    tile_size: u32,
    output_format: OutputFormat,
) -> Result<(), GalleryError> {
    // Detect format and extract ICC profile
    #[allow(unused_variables)] // Used in avif feature
    let (icc_profile, detected_format) = extract_image_info(original_path)?;
    
    // Load image with HDR/gain map preservation for AVIF
    let (img, _avif_info) = {
        #[cfg(feature = "avif")]
        {
            if detected_format == Some(ImageFormat::Avif) {
                // Use our custom AVIF reader to preserve color properties
                match formats::avif::read_avif_info(original_path) {
                    Ok((img, info)) => (img, Some(info)),
                    Err(e) => {
                        debug!(
                            "Failed to read AVIF with custom reader: {}, falling back",
                            e
                        );
                        (image::open(original_path)?, None)
                    }
                }
            } else {
                (image::open(original_path)?, None)
            }
        }
        #[cfg(not(feature = "avif"))]
        {
            (image::open(original_path)?, None::<()>)
        }
    };
    
    let (img_width, img_height) = (img.width(), img.height());
    
    // Resize the image if it's too large - we don't want to serve full resolution tiles
    // Cap the maximum dimension at 8192px for tile generation
    let max_tile_dimension = 8192;
    let max_dimension = img_width.max(img_height);
    
    let resized = if max_dimension > max_tile_dimension {
        // Scale down proportionally
        let scale = max_tile_dimension as f32 / max_dimension as f32;
        let new_width = (img_width as f32 * scale) as u32;
        let new_height = (img_height as f32 * scale) as u32;
        debug!("Resizing image for tiles: {}x{} -> {}x{} (scale: {})", 
               img_width, img_height, new_width, new_height, scale);
        img.resize_exact(new_width, new_height, FilterType::Lanczos3)
    } else {
        debug!("Image within tile dimension limit: {}x{}", img_width, img_height);
        img
    };
    
    let (resized_width, resized_height) = (resized.width(), resized.height());
    
    // Resize gain map if present to match the image resize
    #[cfg(feature = "avif")]
    let mut resized_avif_info = _avif_info.clone();
    #[cfg(not(feature = "avif"))]
    let _resized_avif_info: AvifInfoOption = None;
    
    #[cfg(feature = "avif")]
    if let Some(ref mut info) = resized_avif_info
        && let Some(ref gm_info) = info.gain_map_info
        && let Some(ref gm_image) = gm_info.gain_map_image
    {
        // Calculate scale factors
        let scale_x = resized_width as f32 / img_width as f32;
        let scale_y = resized_height as f32 / img_height as f32;
        
        // Apply same scale to gain map
        let (gm_width, gm_height) = (gm_image.width(), gm_image.height());
        let new_gm_width = (gm_width as f32 * scale_x).round().max(1.0) as u32;
        let new_gm_height = (gm_height as f32 * scale_y).round().max(1.0) as u32;
        
        let resized_gain_map = gm_image.resize_exact(new_gm_width, new_gm_height, FilterType::Lanczos3);
        
        // Update the gain map info with resized image
        if let Some(ref mut gm_info_mut) = info.gain_map_info {
            gm_info_mut.gain_map_image = Some(resized_gain_map);
        }
    }
    
    // Calculate tile boundaries using the fixed tile size
    let tile_start_x = tile_x * tile_size;
    let tile_start_y = tile_y * tile_size;
    
    // Don't go beyond image boundaries
    let tile_actual_width = tile_size.min(resized_width.saturating_sub(tile_start_x));
    let tile_actual_height = tile_size.min(resized_height.saturating_sub(tile_start_y));
    
    debug!("Extracting tile ({}, {}) from {}x{} image: start=({}, {}), size=({}, {})", 
           tile_x, tile_y, resized_width, resized_height,
           tile_start_x, tile_start_y, tile_actual_width, tile_actual_height);
    
    // Extract the tile region
    if tile_actual_width > 0 && tile_actual_height > 0 {
        let tile_img = resized.crop_imm(
            tile_start_x,
            tile_start_y,
            tile_actual_width,
            tile_actual_height,
        );
        
        // Extract corresponding gain map tile if present
        #[cfg(feature = "avif")]
        let mut tile_avif_info = resized_avif_info.clone();
        #[cfg(not(feature = "avif"))]
        let _tile_avif_info: AvifInfoOption = None;
        
        #[cfg(feature = "avif")]
        if let Some(ref mut info) = tile_avif_info
            && let Some(ref gm_info) = info.gain_map_info
            && let Some(ref gm_image) = gm_info.gain_map_image
        {
            // Calculate gain map tile coordinates proportionally
            let gm_scale_x = gm_image.width() as f32 / resized_width as f32;
            let gm_scale_y = gm_image.height() as f32 / resized_height as f32;
            
            let gm_tile_x = (tile_start_x as f32 * gm_scale_x).round() as u32;
            let gm_tile_y = (tile_start_y as f32 * gm_scale_y).round() as u32;
            let gm_tile_width = (tile_actual_width as f32 * gm_scale_x).round().max(1.0) as u32;
            let gm_tile_height = (tile_actual_height as f32 * gm_scale_y).round().max(1.0) as u32;
            
            // Ensure we don't exceed gain map boundaries
            let gm_tile_width = gm_tile_width.min(gm_image.width().saturating_sub(gm_tile_x));
            let gm_tile_height = gm_tile_height.min(gm_image.height().saturating_sub(gm_tile_y));
            
            if gm_tile_width > 0 && gm_tile_height > 0 {
                let gm_tile = gm_image.crop_imm(gm_tile_x, gm_tile_y, gm_tile_width, gm_tile_height);
                
                // Update the gain map info with tile
                if let Some(ref mut gm_info_mut) = info.gain_map_info {
                    gm_info_mut.gain_map_image = Some(gm_tile);
                }
            }
        }
        
        // Save the tile with preserved ICC profile and HDR info
        // For AVIF tiles, use high quality settings
        save_image(
            &tile_img,
            cache_path,
            output_format,
            90, // High quality for JPEG fallback
            90.0, // High quality for WebP fallback
            icc_profile.as_deref(), // Preserve ICC profile
            #[cfg(feature = "avif")]
            tile_avif_info.as_ref(), // Preserve HDR/gain map info for tile
            #[cfg(not(feature = "avif"))]
            None,
        )?;
    } else {
        // Create an empty tile if coordinates are out of bounds
        let empty_tile = DynamicImage::new_rgb8(1, 1);
        save_image(
            &empty_tile,
            cache_path,
            output_format,
            90,
            90.0,
            None,
            #[cfg(feature = "avif")]
            None,
            #[cfg(not(feature = "avif"))]
            None,
        )?;
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
            (0, 0, 0, 0, 1024, 1024),      // Top-left tile
            (1, 0, 1024, 0, 1024, 1024),   // Second tile horizontally
            (0, 1, 0, 1024, 1024, 1024),   // Second tile vertically
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
        let grid_width = (resized_width + 1024 - 1) / 1024;
        let grid_height = (resized_height + 1024 - 1) / 1024;
        
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
        let resized_width = 8192;
        let resized_height = 8192;
        let grid_width = (resized_width + 1024 - 1) / 1024;
        let grid_height = (resized_height + 1024 - 1) / 1024;
        assert_eq!(grid_width, 8);
        assert_eq!(grid_height, 8);
        
        // 2. Image slightly over tile boundary
        let resized_width = 8193; // Just over 8192
        let grid_width = (resized_width + 1024 - 1) / 1024;
        assert_eq!(grid_width, 9); // Should need 9 tiles
        
        // 3. Very small image
        let resized_width = 500;
        let resized_height = 300;
        let grid_width = (resized_width + 1024 - 1) / 1024;
        let grid_height = (resized_height + 1024 - 1) / 1024;
        assert_eq!(grid_width, 1);
        assert_eq!(grid_height, 1);
    }
}
