use chrono::Datelike;
use image::DynamicImage;
use std::error::Error;

// Re-export from tenrankai-image for compatibility
pub use tenrankai_image::CopyrightConfig;

/// Add a copyright notice to an image
///
/// This wraps tenrankai_image::add_copyright_notice, automatically
/// formatting the copyright text with the current year.
pub fn add_copyright_notice(
    image: &DynamicImage,
    config: &CopyrightConfig,
    font_path: &std::path::Path,
) -> Result<DynamicImage, Box<dyn Error>> {
    // Get current year
    let current_year = chrono::Local::now().year();
    let copyright_text = format!("© {} {}", current_year, config.copyright_holder);

    tenrankai_image::add_copyright_notice(
        image,
        &copyright_text,
        config.font_size,
        config.padding,
        font_path,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};

    #[test]
    fn test_copyright_config_default() {
        let config = CopyrightConfig::default();
        assert_eq!(config.copyright_holder, "");
        assert_eq!(config.font_size, 20.0);
        assert_eq!(config.padding, 10);
    }

    #[test]
    fn test_add_copyright_notice() {
        // Skip test if font file doesn't exist
        let font_path = std::path::Path::new("static/DejaVuSans.ttf");
        if !font_path.exists() {
            // Can't test without font file
            return;
        }

        // Create a test image
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 200, Rgba([128, 128, 128, 255])));

        let config = CopyrightConfig {
            copyright_holder: "Test Photographer".to_string(),
            font_size: 16.0,
            padding: 5,
        };

        let result = add_copyright_notice(&img, &config, font_path);
        assert!(result.is_ok());

        let output_img = result.unwrap();
        assert_eq!(output_img.width(), 200);
        assert_eq!(output_img.height(), 200);
    }
}
