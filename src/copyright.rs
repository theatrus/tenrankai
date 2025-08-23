use ab_glyph::{FontVec, PxScale};
use chrono::Datelike;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use std::error::Error;

/// Configuration for copyright notice
pub struct CopyrightConfig {
    /// The copyright holder name (e.g., "John Doe Photography")
    pub copyright_holder: String,
    /// Font size for the copyright text
    pub font_size: f32,
    /// Padding from the bottom-left corner in pixels
    pub padding: u32,
}

impl Default for CopyrightConfig {
    fn default() -> Self {
        Self {
            copyright_holder: String::new(),
            font_size: 20.0,
            padding: 10,
        }
    }
}

// We'll load the font at runtime instead of compile time
// This allows the font to be optional

/// Add a copyright notice to an image
pub fn add_copyright_notice(
    image: &DynamicImage,
    config: &CopyrightConfig,
    font_path: &std::path::Path,
) -> Result<DynamicImage, Box<dyn Error>> {
    // Convert to RGBA if not already
    let mut rgba_image = image.to_rgba8();

    // Get current year
    let current_year = chrono::Local::now().year();
    let copyright_text = format!("© {} {}", current_year, config.copyright_holder);

    // Load the font from file
    let font_data = std::fs::read(font_path)?;
    let font = FontVec::try_from_vec(font_data).map_err(|_| "Failed to parse font")?;

    let scale = PxScale::from(config.font_size);

    // Calculate text dimensions
    let (text_width, text_height) = text_size(scale, &font, &copyright_text);

    // Calculate position (bottom-left with padding)
    let _image_width = rgba_image.width();
    let image_height = rgba_image.height();
    let x = config.padding as i32;
    let y = (image_height - config.padding - text_height) as i32;

    // Sample the area where text will be drawn to determine if we need black or white text
    let text_color = determine_text_color(&rgba_image, x as u32, y as u32, text_width, text_height);

    // Draw the text
    draw_text_mut(
        &mut rgba_image,
        text_color,
        x,
        y,
        scale,
        &font,
        &copyright_text,
    );

    // Convert to RGB for JPEG compatibility
    let rgb_image = DynamicImage::ImageRgba8(rgba_image).to_rgb8();
    Ok(DynamicImage::ImageRgb8(rgb_image))
}

/// Determine whether to use black or white text based on the background
fn determine_text_color(image: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> Rgba<u8> {
    let mut total_luminance = 0.0;
    let mut pixel_count = 0;

    // Sample the region where text will be drawn
    let x_start = x;
    let y_start = y;
    let x_end = (x + width).min(image.width());
    let y_end = (y + height).min(image.height());

    for py in y_start..y_end {
        for px in x_start..x_end {
            let pixel = image.get_pixel(px, py);
            // Calculate relative luminance using the formula from WCAG
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;

            let r_linear = if r <= 0.03928 {
                r / 12.92
            } else {
                ((r + 0.055) / 1.055).powf(2.4)
            };
            let g_linear = if g <= 0.03928 {
                g / 12.92
            } else {
                ((g + 0.055) / 1.055).powf(2.4)
            };
            let b_linear = if b <= 0.03928 {
                b / 12.92
            } else {
                ((b + 0.055) / 1.055).powf(2.4)
            };

            let luminance = 0.2126 * r_linear + 0.7152 * g_linear + 0.0722 * b_linear;
            total_luminance += luminance;
            pixel_count += 1;
        }
    }

    if pixel_count == 0 {
        return Rgba([255, 255, 255, 255]); // Default to white if no pixels sampled
    }

    let average_luminance = total_luminance / pixel_count as f32;

    // If the background is dark (luminance < 0.5), use white text; otherwise use black
    if average_luminance < 0.5 {
        Rgba([255, 255, 255, 255]) // White text
    } else {
        Rgba([0, 0, 0, 255]) // Black text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};

    #[test]
    fn test_copyright_config_default() {
        let config = CopyrightConfig::default();
        assert_eq!(config.copyright_holder, "");
        assert_eq!(config.font_size, 20.0);
        assert_eq!(config.padding, 10);
    }

    #[test]
    fn test_determine_text_color_dark_background() {
        // Create a dark image (black)
        let img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        let color = determine_text_color(&img, 0, 0, 50, 20);
        assert_eq!(color, Rgba([255, 255, 255, 255])); // Should be white text
    }

    #[test]
    fn test_determine_text_color_light_background() {
        // Create a light image (white)
        let img = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let color = determine_text_color(&img, 0, 0, 50, 20);
        assert_eq!(color, Rgba([0, 0, 0, 255])); // Should be black text
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
