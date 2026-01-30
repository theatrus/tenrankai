use crate::gallery::GalleryError;
use crate::storage::{DynStorage, PreloadedReader};
use image::{DynamicImage, ImageFormat};
use std::io::{BufReader, Cursor};
use tokio::runtime::Handle;

#[cfg(all(feature = "heif", feature = "avif"))]
use crate::gallery::image_processing::formats;

/// Convert HeifImageInfo to local AvifImageInfo for encoding
#[cfg(all(feature = "heif", feature = "avif"))]
fn heif_to_avif_info(heif_info: &tenrankai_image::HeifImageInfo) -> formats::avif::AvifImageInfo {
    let has_gm_image = heif_info
        .gain_map_info
        .as_ref()
        .and_then(|gm| gm.gain_map_image.as_ref())
        .is_some();
    tracing::debug!(
        "Converting HEIF to AVIF info: is_hdr={}, has_gain_map={}, has_gm_image={}, primaries={}, transfer={}",
        heif_info.is_hdr,
        heif_info.has_gain_map,
        has_gm_image,
        heif_info.color_primaries,
        heif_info.transfer_characteristics
    );
    formats::avif::AvifImageInfo {
        bit_depth: heif_info.bit_depth,
        has_alpha: heif_info.has_alpha,
        is_hdr: heif_info.is_hdr,
        icc_profile: heif_info.icc_profile.clone(),
        color_primaries: heif_info.color_primaries,
        transfer_characteristics: heif_info.transfer_characteristics,
        matrix_coefficients: heif_info.matrix_coefficients,
        max_cll: heif_info.max_cll,
        max_pall: heif_info.max_pall,
        has_gain_map: heif_info.has_gain_map,
        gain_map_info: heif_info
            .gain_map_info
            .as_ref()
            .map(|gm| formats::avif::GainMapInfo {
                has_image: gm.has_image,
                gamma: gm.gamma,
                min: gm.min,
                max: gm.max,
                base_offset: gm.base_offset,
                alternate_offset: gm.alternate_offset,
                base_hdr_headroom: gm.base_hdr_headroom,
                alternate_hdr_headroom: gm.alternate_hdr_headroom,
                use_base_color_space: gm.use_base_color_space,
                gain_map_image: gm.gain_map_image.clone(),
            }),
        exif_data: heif_info.exif_data.clone(),
    }
}

/// A loaded image with all its metadata preserved
/// This struct encapsulates an image and its associated metadata
/// to enable batch processing without reloading the image multiple times
#[derive(Clone)]
pub struct LoadedImage {
    /// The actual image data
    pub image: DynamicImage,
    /// ICC color profile if present
    pub icc_profile: Option<Vec<u8>>,
    /// AVIF-specific metadata including gain maps and HDR info
    #[cfg(feature = "avif")]
    pub avif_info: Option<crate::gallery::image_processing::formats::avif::AvifImageInfo>,
    /// HEIF/HEIC-specific metadata including gain maps and HDR info
    #[cfg(feature = "heif")]
    pub heif_info: Option<tenrankai_image::HeifImageInfo>,
    /// The original format of the loaded image
    pub format: Option<ImageFormat>,
    /// The original path of the image (for error messages)
    pub source_path: std::path::PathBuf,
}

impl LoadedImage {
    /// Create a new LoadedImage from basic components
    pub fn new(image: DynamicImage, source_path: std::path::PathBuf) -> Self {
        Self {
            image,
            icc_profile: None,
            #[cfg(feature = "avif")]
            avif_info: None,
            #[cfg(feature = "heif")]
            heif_info: None,
            format: None,
            source_path,
        }
    }

    /// Load an image from disk with all metadata preserved
    pub fn load(path: &std::path::Path) -> Result<Self, GalleryError> {
        use std::io::BufReader;

        // First detect the format and extract ICC profile
        let file = std::fs::File::open(path)?;
        let buf_reader = BufReader::new(file);
        let decoder = image::ImageReader::new(buf_reader).with_guessed_format()?;
        let detected_format = decoder.format();

        // Extract ICC profile based on format
        let icc_profile = match detected_format {
            Some(ImageFormat::Jpeg) => {
                crate::gallery::image_processing::formats::jpeg::extract_icc_profile(path)
            }
            Some(ImageFormat::Png) => {
                crate::gallery::image_processing::formats::png::extract_icc_profile(path)
            }
            #[cfg(feature = "avif")]
            Some(ImageFormat::Avif) => {
                crate::gallery::image_processing::formats::avif::extract_icc_profile(path)
            }
            _ => None,
        };

        // Load the image with special handling for AVIF
        #[cfg(feature = "avif")]
        let (image, avif_info) = if detected_format == Some(ImageFormat::Avif) {
            // Use our custom AVIF reader to preserve color properties and gain maps
            match crate::gallery::image_processing::formats::avif::read_avif_info(path) {
                Ok((img, info)) => (img, Some(info)),
                Err(e) => {
                    tracing::debug!(
                        "Failed to read AVIF with custom reader: {}, falling back",
                        e
                    );
                    (image::open(path)?, None)
                }
            }
        } else {
            (image::open(path)?, None)
        };

        #[cfg(not(feature = "avif"))]
        let image = image::open(path)?;

        Ok(Self {
            image,
            icc_profile,
            #[cfg(feature = "avif")]
            avif_info,
            #[cfg(feature = "heif")]
            heif_info: None,
            format: detected_format,
            source_path: path.to_path_buf(),
        })
    }

    /// Load an image from storage with all metadata preserved.
    ///
    /// This method uses the storage abstraction to load images from either
    /// local filesystem or remote storage (S3). It uses the FullFetch strategy
    /// since we need the entire image for processing.
    ///
    /// # Arguments
    /// * `storage` - The storage backend to read from
    /// * `relative_path` - Path relative to the storage root
    /// * `handle` - Tokio runtime handle for async operations
    pub fn load_from_storage(
        storage: &DynStorage,
        relative_path: &str,
        handle: &Handle,
    ) -> Result<Self, GalleryError> {
        // Use FullFetch strategy since we need the entire image
        let reader = PreloadedReader::open(storage, relative_path, handle)?;
        let data = reader.into_bytes();

        Self::load_from_bytes(&data, relative_path)
    }

    /// Load an image from bytes with all metadata preserved.
    ///
    /// This is useful when the image data is already in memory (e.g., from storage).
    pub fn load_from_bytes(data: &[u8], source_path_hint: &str) -> Result<Self, GalleryError> {
        // Check for HEIF/HEIC first since the image crate doesn't recognize it
        #[cfg(feature = "heif")]
        if tenrankai_image::is_heif_format(data) {
            return Self::load_heif_from_bytes(data, source_path_hint);
        }

        // Detect the format from the data
        let cursor = Cursor::new(data);
        let buf_reader = BufReader::new(cursor);
        let decoder = image::ImageReader::new(buf_reader).with_guessed_format()?;
        let detected_format = decoder.format();

        // Extract ICC profile based on format
        let icc_profile = match detected_format {
            Some(ImageFormat::Jpeg) => {
                crate::gallery::image_processing::formats::jpeg::extract_icc_profile_from_bytes(
                    data,
                )
            }
            Some(ImageFormat::Png) => {
                crate::gallery::image_processing::formats::png::extract_icc_profile_from_bytes(data)
            }
            #[cfg(feature = "avif")]
            Some(ImageFormat::Avif) => {
                crate::gallery::image_processing::formats::avif_container::extract_icc_profile_from_container(data)
            }
            _ => None,
        };

        // Load the image with special handling for AVIF
        #[cfg(feature = "avif")]
        let (image, avif_info) = if detected_format == Some(ImageFormat::Avif) {
            // Use our custom AVIF reader to preserve color properties and gain maps
            match crate::gallery::image_processing::formats::avif::read_avif_info_from_bytes(data) {
                Ok((img, info)) => (img, Some(info)),
                Err(e) => {
                    tracing::debug!(
                        "Failed to read AVIF with custom reader: {}, falling back",
                        e
                    );
                    // Fall back to standard decoding
                    match image::load_from_memory(data) {
                        Ok(img) => (img, None),
                        Err(e) => {
                            return Err(GalleryError::ImageError(e));
                        }
                    }
                }
            }
        } else {
            (image::load_from_memory(data)?, None)
        };

        #[cfg(not(feature = "avif"))]
        let image = image::load_from_memory(data)?;

        Ok(Self {
            image,
            icc_profile,
            #[cfg(feature = "avif")]
            avif_info,
            #[cfg(feature = "heif")]
            heif_info: None,
            format: detected_format,
            source_path: std::path::PathBuf::from(source_path_hint),
        })
    }

    /// Load HEIF/HEIC image from bytes
    #[cfg(feature = "heif")]
    fn load_heif_from_bytes(data: &[u8], source_path_hint: &str) -> Result<Self, GalleryError> {
        match tenrankai_image::read_heif_info_from_bytes(data) {
            Ok((image, heif_info)) => {
                let icc_profile = heif_info.icc_profile.clone();
                tracing::debug!(
                    "Loaded HEIF image: {}x{}, HDR={}, has_gain_map={}",
                    image.width(),
                    image.height(),
                    heif_info.is_hdr,
                    heif_info.has_gain_map
                );
                Ok(Self {
                    image,
                    icc_profile,
                    #[cfg(feature = "avif")]
                    avif_info: None,
                    heif_info: Some(heif_info),
                    format: None, // image crate doesn't have HEIF variant
                    source_path: std::path::PathBuf::from(source_path_hint),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load HEIF image {}: {}", source_path_hint, e);
                Err(GalleryError::ProcessingError(format!(
                    "Failed to load HEIF: {}",
                    e
                )))
            }
        }
    }

    /// Get image dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.image.width(), self.image.height())
    }

    /// Check if the image has an alpha channel
    pub fn has_alpha(&self) -> bool {
        use image::DynamicImage;
        matches!(
            self.image,
            DynamicImage::ImageRgba8(_)
                | DynamicImage::ImageRgba16(_)
                | DynamicImage::ImageRgba32F(_)
        )
    }

    /// Resize the image preserving aspect ratio
    /// This also resizes any associated gain maps proportionally
    pub fn resize(&mut self, max_width: u32, max_height: u32) -> Result<(), GalleryError> {
        use image::imageops::FilterType;

        let (orig_width, orig_height) = (self.image.width(), self.image.height());

        // Don't upscale - if requested dimensions are larger than original, keep original
        let final_width = max_width.min(orig_width);
        let final_height = max_height.min(orig_height);

        // Only resize if dimensions are different
        if final_width != orig_width || final_height != orig_height {
            self.image = self
                .image
                .resize(final_width, final_height, FilterType::Lanczos3);

            // Resize gain map if present (AVIF only)
            #[cfg(feature = "avif")]
            if let Some(ref mut avif_info) = self.avif_info
                && let Some(ref gm_info) = avif_info.gain_map_info
                && let Some(ref gm_image) = gm_info.gain_map_image
            {
                // Calculate scale factors
                let scale_x = self.image.width() as f32 / orig_width as f32;
                let scale_y = self.image.height() as f32 / orig_height as f32;

                // Apply same scale to gain map
                let (gm_width, gm_height) = (gm_image.width(), gm_image.height());
                let new_gm_width = ((gm_width as f32 * scale_x).round() as u32).max(1);
                let new_gm_height = ((gm_height as f32 * scale_y).round() as u32).max(1);

                tracing::debug!(
                    "Resizing gain map from {}x{} to {}x{}",
                    gm_width,
                    gm_height,
                    new_gm_width,
                    new_gm_height
                );

                let resized_gain_map =
                    gm_image.resize_exact(new_gm_width, new_gm_height, FilterType::Lanczos3);

                // Update the gain map info with resized image
                if let Some(ref mut gm_info_mut) = avif_info.gain_map_info {
                    gm_info_mut.gain_map_image = Some(resized_gain_map);
                }
            }

            // Resize gain map if present (HEIF source with gain map)
            #[cfg(all(feature = "heif", feature = "avif"))]
            if let Some(ref mut heif_info) = self.heif_info
                && let Some(ref gm_info) = heif_info.gain_map_info
                && let Some(ref gm_image) = gm_info.gain_map_image
            {
                // Calculate scale factors
                let scale_x = self.image.width() as f32 / orig_width as f32;
                let scale_y = self.image.height() as f32 / orig_height as f32;

                // Apply same scale to gain map
                let (gm_width, gm_height) = (gm_image.width(), gm_image.height());
                let new_gm_width = ((gm_width as f32 * scale_x).round() as u32).max(1);
                let new_gm_height = ((gm_height as f32 * scale_y).round() as u32).max(1);

                tracing::debug!(
                    "Resizing HEIF gain map from {}x{} to {}x{}",
                    gm_width,
                    gm_height,
                    new_gm_width,
                    new_gm_height
                );

                let resized_gain_map =
                    gm_image.resize_exact(new_gm_width, new_gm_height, FilterType::Lanczos3);

                // Update the gain map info with resized image
                if let Some(ref mut gm_info_mut) = heif_info.gain_map_info {
                    gm_info_mut.gain_map_image = Some(resized_gain_map);
                }
            }
        }

        Ok(())
    }

    /// Apply text watermark to the image
    pub fn apply_watermark(
        &mut self,
        copyright_holder: &str,
        font_path: &std::path::Path,
    ) -> Result<(), GalleryError> {
        use crate::copyright::{CopyrightConfig, add_copyright_notice};

        if !font_path.exists() {
            tracing::debug!("Font file not found at {:?}, skipping watermark", font_path);
            return Ok(());
        }

        let copyright_config = CopyrightConfig {
            copyright_holder: copyright_holder.to_string(),
            font_size: 20.0,
            padding: 10,
        };

        match add_copyright_notice(&self.image, &copyright_config, font_path) {
            Ok(watermarked) => {
                self.image = watermarked;
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to add copyright watermark: {}", e);
                Ok(()) // Don't fail, just log the error
            }
        }
    }

    /// Apply image watermark to the image
    pub fn apply_image_watermark(
        &mut self,
        watermark: &tenrankai_image::ImageWatermark,
        config: &crate::config::ImageWatermarkConfig,
    ) -> Result<(), GalleryError> {
        let wm_config = tenrankai_image::WatermarkConfig {
            position: match config.position {
                crate::config::WatermarkPosition::BottomLeft => {
                    tenrankai_image::WatermarkPosition::BottomLeft
                }
                crate::config::WatermarkPosition::BottomRight => {
                    tenrankai_image::WatermarkPosition::BottomRight
                }
                crate::config::WatermarkPosition::TopLeft => {
                    tenrankai_image::WatermarkPosition::TopLeft
                }
                crate::config::WatermarkPosition::TopRight => {
                    tenrankai_image::WatermarkPosition::TopRight
                }
                crate::config::WatermarkPosition::Center => {
                    tenrankai_image::WatermarkPosition::Center
                }
                crate::config::WatermarkPosition::Tiled => {
                    tenrankai_image::WatermarkPosition::Tiled
                }
            },
            opacity: config.opacity,
            scale: config.scale,
            padding: config.padding,
            adaptive: config.adaptive,
        };

        match watermark.apply(&mut self.image, &wm_config) {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::error!("Failed to apply image watermark: {}", e);
                Ok(()) // Don't fail, just log the error
            }
        }
    }

    /// Encode the image to bytes in the specified format
    pub fn encode(
        &self,
        format: OutputFormat,
        jpeg_quality: u8,
        webp_quality: f32,
    ) -> Result<Vec<u8>, GalleryError> {
        use crate::gallery::image_processing::formats;

        match format {
            OutputFormat::Jpeg => formats::jpeg::encode_with_profile(
                &self.image,
                jpeg_quality,
                self.icc_profile.as_deref(),
            ),
            OutputFormat::WebP => formats::webp::encode_with_profile(
                &self.image,
                webp_quality,
                self.icc_profile.as_deref(),
            ),
            OutputFormat::Png => formats::png::encode(&self.image),
            #[cfg(feature = "avif")]
            OutputFormat::Avif => {
                // Log what info we have
                tracing::debug!(
                    "AVIF encode: avif_info={}, heif_info={}",
                    self.avif_info.is_some(),
                    {
                        #[cfg(feature = "heif")]
                        {
                            self.heif_info.is_some()
                        }
                        #[cfg(not(feature = "heif"))]
                        {
                            false
                        }
                    }
                );

                // Use AVIF info if available
                if let Some(ref info) = self.avif_info {
                    tracing::debug!(
                        "Using AVIF info for encoding: has_gain_map={}",
                        info.has_gain_map
                    );
                    return formats::avif::encode_with_info(&self.image, 85, 6, Some(info));
                }

                // Use HEIF info (converted to local AVIF info) if available
                #[cfg(feature = "heif")]
                if let Some(ref heif_info) = self.heif_info {
                    tracing::debug!(
                        "Using HEIF info for encoding: has_gain_map={}, has_gm_image={}",
                        heif_info.has_gain_map,
                        heif_info
                            .gain_map_info
                            .as_ref()
                            .and_then(|g| g.gain_map_image.as_ref())
                            .is_some()
                    );
                    let avif_info = heif_to_avif_info(heif_info);
                    return formats::avif::encode_with_info(&self.image, 85, 6, Some(&avif_info));
                }

                // Fall back to basic encoding
                let preserve_hdr = matches!(
                    self.image,
                    DynamicImage::ImageLuma16(_)
                        | DynamicImage::ImageLumaA16(_)
                        | DynamicImage::ImageRgb16(_)
                        | DynamicImage::ImageRgba16(_)
                );
                formats::avif::encode_with_profile(
                    &self.image,
                    85,
                    6,
                    self.icc_profile.as_deref(),
                    preserve_hdr,
                )
            }
        }
    }

    /// Save the image to a file in the specified format
    pub fn save_as(
        &self,
        path: &std::path::Path,
        format: OutputFormat,
        jpeg_quality: u8,
        webp_quality: f32,
    ) -> Result<(), GalleryError> {
        let data = self.encode(format, jpeg_quality, webp_quality)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Extract a tile from the image
    pub fn extract_tile(&self, x: u32, y: u32, size: u32) -> Result<DynamicImage, GalleryError> {
        let (width, height) = self.dimensions();

        // Ensure tile doesn't go out of bounds
        let tile_width = size.min(width.saturating_sub(x));
        let tile_height = size.min(height.saturating_sub(y));

        if tile_width == 0 || tile_height == 0 {
            return Err(GalleryError::InvalidPath);
        }

        Ok(self.image.crop_imm(x, y, tile_width, tile_height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Jpeg,
    WebP,
    Png,
    #[cfg(feature = "avif")]
    Avif,
}

impl OutputFormat {
    /// All supported output formats
    pub const ALL: &'static [OutputFormat] = &[
        OutputFormat::Jpeg,
        OutputFormat::WebP,
        OutputFormat::Png,
        #[cfg(feature = "avif")]
        OutputFormat::Avif,
    ];

    /// Formats that support transparency
    pub const TRANSPARENCY_FORMATS: &'static [OutputFormat] = &[
        OutputFormat::Png,
        OutputFormat::WebP,
        #[cfg(feature = "avif")]
        OutputFormat::Avif,
    ];

    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::WebP => "webp",
            OutputFormat::Png => "png",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "avif",
        }
    }

    #[allow(dead_code)]
    pub fn image_format(&self) -> ImageFormat {
        match self {
            OutputFormat::Jpeg => ImageFormat::Jpeg,
            OutputFormat::WebP => ImageFormat::WebP,
            OutputFormat::Png => ImageFormat::Png,
            #[cfg(feature = "avif")]
            OutputFormat::Avif => ImageFormat::Avif,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::WebP => "image/webp",
            OutputFormat::Png => "image/png",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "image/avif",
        }
    }

    /// Determine output format from Accept header
    /// Used for content negotiation with browser support detection
    pub fn from_accept_header(accept_header: &str) -> OutputFormat {
        // Check browser support in priority order (best quality first)
        #[cfg(feature = "avif")]
        if accept_header.contains("image/avif") {
            return OutputFormat::Avif;
        }

        if accept_header.contains("image/webp") {
            OutputFormat::WebP
        } else {
            OutputFormat::Jpeg
        }
    }

    /// Determine output format from file extension
    pub fn from_file_extension(path: &str) -> Option<OutputFormat> {
        let extension = path.rsplit('.').next()?.to_lowercase();
        match extension.as_str() {
            "jpg" | "jpeg" => Some(OutputFormat::Jpeg),
            "webp" => Some(OutputFormat::WebP),
            "png" => Some(OutputFormat::Png),
            #[cfg(feature = "avif")]
            "avif" => Some(OutputFormat::Avif),
            _ => None,
        }
    }

    /// Check if this format supports transparency (alpha channel)
    pub fn supports_transparency(&self) -> bool {
        Self::TRANSPARENCY_FORMATS.contains(self)
    }

    /// Get the quality range for this format (min, max)
    pub fn quality_range(&self) -> (f32, f32) {
        match self {
            OutputFormat::Jpeg => (1.0, 100.0),
            OutputFormat::WebP => (0.0, 100.0),
            OutputFormat::Png => (0.0, 9.0), // PNG compression level
            #[cfg(feature = "avif")]
            OutputFormat::Avif => (0.0, 63.0), // AVIF quality parameter range
        }
    }

    /// Get the default quality for this format
    pub fn default_quality(&self) -> f32 {
        match self {
            OutputFormat::Jpeg => 85.0,
            OutputFormat::WebP => 85.0,
            OutputFormat::Png => 6.0, // PNG compression level
            #[cfg(feature = "avif")]
            OutputFormat::Avif => 50.0,
        }
    }

    /// Check if quality parameter applies to this format
    pub fn uses_quality(&self) -> bool {
        match self {
            OutputFormat::Jpeg | OutputFormat::WebP => true,
            OutputFormat::Png => false, // PNG uses compression level, not quality
            #[cfg(feature = "avif")]
            OutputFormat::Avif => true,
        }
    }

    /// Get display name for this format (for UI/logging)
    pub fn display_name(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "JPEG",
            OutputFormat::WebP => "WebP",
            OutputFormat::Png => "PNG",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "AVIF",
        }
    }

    /// Check if this format should be used based on source format
    /// PNG sources should stay PNG to preserve transparency
    pub fn should_preserve_source_format(source_path: &str) -> bool {
        source_path.to_lowercase().ends_with(".png")
    }
}

#[derive(Debug, Clone)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

impl ImageSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn with_multiplier(&self, multiplier: u32) -> Self {
        Self {
            width: self.width * multiplier,
            height: self.height * multiplier,
        }
    }
}
