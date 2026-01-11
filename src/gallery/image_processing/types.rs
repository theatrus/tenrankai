use crate::gallery::GalleryError;
use image::{DynamicImage, ImageFormat};

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
            format: detected_format,
            source_path: path.to_path_buf(),
        })
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
        }

        Ok(())
    }

    /// Apply watermark to the image
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

    /// Save the image to a file in the specified format
    pub fn save_as(
        &self,
        path: &std::path::Path,
        format: OutputFormat,
        jpeg_quality: u8,
        webp_quality: f32,
    ) -> Result<(), GalleryError> {
        use crate::gallery::image_processing::formats;

        match format {
            OutputFormat::Jpeg => formats::jpeg::save_with_profile(
                &self.image,
                path,
                jpeg_quality,
                self.icc_profile.as_deref(),
            ),
            OutputFormat::WebP => formats::webp::save_with_profile(
                &self.image,
                path,
                webp_quality,
                self.icc_profile.as_deref(),
            ),
            OutputFormat::Png => formats::png::save(&self.image, path),
            #[cfg(feature = "avif")]
            OutputFormat::Avif => {
                // Use the preserved AVIF info if available
                if let Some(ref info) = self.avif_info {
                    formats::avif::save_with_info(&self.image, path, 85, 6, Some(info))
                } else {
                    // Fallback: preserve HDR if the source is 16-bit
                    let preserve_hdr = matches!(
                        self.image,
                        DynamicImage::ImageLuma16(_)
                            | DynamicImage::ImageLumaA16(_)
                            | DynamicImage::ImageRgb16(_)
                            | DynamicImage::ImageRgba16(_)
                    );
                    formats::avif::save_with_profile(
                        &self.image,
                        path,
                        85,
                        6,
                        self.icc_profile.as_deref(),
                        preserve_hdr,
                    )
                }
            }
        }
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
