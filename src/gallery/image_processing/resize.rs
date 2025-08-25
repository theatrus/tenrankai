use crate::copyright::{CopyrightConfig, add_copyright_notice};
use crate::gallery::{Gallery, GalleryError};
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use std::path::{Path, PathBuf};
use tracing::{debug, error};

use super::formats;
use super::types::{ImageSize, OutputFormat};

impl Gallery {
    /// Parse size string and determine dimensions
    pub(super) fn parse_size(&self, size: &str) -> Result<(ImageSize, bool), GalleryError> {
        // Check if it's a @2x variant
        let (base_size, multiplier) = if size.ends_with("@2x") {
            (size.trim_end_matches("@2x"), 2)
        } else {
            (size, 1)
        };

        let base_dimensions = match base_size {
            "thumbnail" => {
                ImageSize::new(self.config.thumbnail.width, self.config.thumbnail.height)
            }
            "gallery" => ImageSize::new(
                self.config.gallery_size.width,
                self.config.gallery_size.height,
            ),
            "medium" => ImageSize::new(self.config.medium.width, self.config.medium.height),
            "large" => ImageSize::new(self.config.large.width, self.config.large.height),
            _ => return Err(GalleryError::InvalidPath),
        };

        let final_dimensions = base_dimensions.with_multiplier(multiplier);
        let is_medium = base_size == "medium";

        Ok((final_dimensions, is_medium))
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

        // Generate consistent cache keys
        let cache_filename =
            self.generate_cache_filename(relative_path, size, output_format.extension());
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
        let apply_watermark = is_medium && self.config.copyright_holder.is_some();
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

    /// Check if cache file is valid (exists and newer than source)
    async fn is_cache_valid(
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
    let (icc_profile, detected_format) = extract_image_info(original_path)?;

    // Load and resize image
    debug!(
        "Opening image file: {:?}, detected format: {:?}",
        original_path, detected_format
    );
    let img = image::open(original_path)?;

    let resized = resize_image(&img, dimensions)?;

    // Apply watermark if needed
    let final_image = if apply_watermark && copyright_holder.is_some() {
        apply_copyright_watermark(resized, copyright_holder.unwrap(), static_dir)?
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
) -> Result<(), GalleryError> {
    match format {
        OutputFormat::Jpeg => {
            formats::jpeg::save_with_profile(image, path, jpeg_quality, icc_profile)
        }
        OutputFormat::WebP => {
            formats::webp::save_with_profile(image, path, webp_quality, icc_profile)
        }
        OutputFormat::Png => formats::png::save(image, path),
    }
}
