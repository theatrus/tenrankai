use crate::copyright::{CopyrightConfig, add_copyright_notice};
use image::DynamicImage;
use std::path::Path;
use tracing::{debug, error};

/// Apply copyright watermark to an image
#[allow(dead_code)]
pub fn apply_watermark(
    image: DynamicImage,
    copyright_holder: &str,
    font_path: &Path,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    if !font_path.exists() {
        debug!("Font file not found at {:?}, skipping watermark", font_path);
        return Ok(image);
    }

    let config = CopyrightConfig {
        copyright_holder: copyright_holder.to_string(),
        font_size: 20.0,
        padding: 10,
    };

    match add_copyright_notice(&image, &config, font_path) {
        Ok(watermarked) => Ok(watermarked),
        Err(e) => {
            error!("Failed to add copyright watermark: {}", e);
            Ok(image) // Return original image on error
        }
    }
}
