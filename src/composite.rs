use image::{DynamicImage, ImageBuffer, Rgba};
use std::path::{Path, PathBuf};

/// Load an image from a path, with AVIF support
fn load_image_with_avif_support(
    path: &Path,
) -> Result<DynamicImage, Box<dyn std::error::Error + Send + Sync>> {
    // First try the standard image::open
    match image::open(path) {
        Ok(img) => Ok(img),
        Err(e) => {
            // If it fails, check if it's an AVIF file
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("avif"))
                .unwrap_or(false)
            {
                // Try to load as AVIF
                let (img, _info) =
                    crate::gallery::image_processing::formats::avif::read_avif_info(path)?;
                Ok(img)
            } else {
                // Not an AVIF file, return the original error
                Err(Box::new(e))
            }
        }
    }
}

/// Creates a composite preview image from multiple gallery items in a 2x2 grid
pub fn create_composite_preview(
    source_directory: PathBuf,
    images: Vec<crate::gallery::GalleryItem>,
) -> Result<DynamicImage, Box<dyn std::error::Error + Send + Sync>> {
    let grid_size: usize = 2;
    let cell_size: u32 = 600;
    let padding: u32 = 10;
    let total_size = cell_size * grid_size as u32 + padding * (grid_size as u32 - 1);

    // Create a white background
    let mut composite =
        ImageBuffer::from_pixel(total_size, total_size, Rgba([255u8, 255u8, 255u8, 255u8]));

    // Load and place images
    for (idx, image_item) in images.iter().enumerate() {
        if idx >= grid_size * grid_size {
            break;
        }

        // Load the thumbnail (with AVIF support)
        let image_path = source_directory.join(&image_item.path);
        if let Ok(img) = load_image_with_avif_support(&image_path) {
            // Calculate position in grid
            let row = idx / grid_size;
            let col = idx % grid_size;
            let x = (col as u32) * (cell_size + padding);
            let y = (row as u32) * (cell_size + padding);

            // Resize image to fit cell while maintaining aspect ratio
            let resized =
                img.resize_to_fill(cell_size, cell_size, image::imageops::FilterType::Lanczos3);

            // Copy to composite
            image::imageops::overlay(&mut composite, &resized, x as i64, y as i64);
        }
    }

    // Add a subtle border
    let bordered = add_border(&composite, 2, Rgba([200u8, 200u8, 200u8, 255u8]));

    // Convert to RGB for better compatibility (JPEG doesn't support alpha)
    let rgb_image = DynamicImage::ImageRgba8(bordered).to_rgb8();
    Ok(DynamicImage::ImageRgb8(rgb_image))
}

/// Adds a border around an image with the specified width and color
pub fn add_border(
    img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    border_width: u32,
    border_color: Rgba<u8>,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (width, height) = img.dimensions();
    let new_width = width + 2 * border_width;
    let new_height = height + 2 * border_width;

    let mut bordered = ImageBuffer::from_pixel(new_width, new_height, border_color);

    image::imageops::overlay(&mut bordered, img, border_width as i64, border_width as i64);

    bordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::GalleryItem;
    use image::GenericImageView;
    use tempfile::TempDir;

    fn create_test_image(width: u32, height: u32, color: Rgba<u8>) -> DynamicImage {
        let img = ImageBuffer::from_pixel(width, height, color);
        DynamicImage::ImageRgba8(img)
    }

    fn create_test_gallery_item(name: &str, path: &str) -> GalleryItem {
        GalleryItem {
            name: name.to_string(),
            display_name: None,
            description: None,
            path: path.to_string(),
            parent_path: None,
            is_directory: false,
            thumbnail_url: Some(format!("/gallery/image/{}?size=thumbnail", path)),
            gallery_url: Some(format!("/gallery/image/{}?size=gallery", path)),
            preview_images: None,
            item_count: None,
            dimensions: Some((800, 600)),
            capture_date: None,
            is_new: false,
        }
    }

    #[test]
    fn test_add_border() {
        // Create a small test image
        let original = ImageBuffer::from_pixel(100, 100, Rgba([255, 0, 0, 255])); // Red
        let border_color = Rgba([0, 0, 255, 255]); // Blue
        let border_width = 5;

        let bordered = add_border(&original, border_width, border_color);

        // Check dimensions
        assert_eq!(bordered.dimensions(), (110, 110));

        // Check border pixels (corners should be border color)
        assert_eq!(bordered.get_pixel(0, 0), &border_color);
        assert_eq!(bordered.get_pixel(109, 0), &border_color);
        assert_eq!(bordered.get_pixel(0, 109), &border_color);
        assert_eq!(bordered.get_pixel(109, 109), &border_color);

        // Check that center pixel is from original image
        assert_eq!(bordered.get_pixel(55, 55), &Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn test_create_composite_preview_with_test_images() {
        // Create a temporary directory for test images
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().to_path_buf();

        // Create and save test images
        let test_images = vec![
            ("red.png", Rgba([255, 0, 0, 255])),
            ("green.png", Rgba([0, 255, 0, 255])),
            ("blue.png", Rgba([0, 0, 255, 255])),
            ("yellow.png", Rgba([255, 255, 0, 255])),
        ];

        let mut gallery_items = Vec::new();

        for (filename, color) in &test_images {
            let img = create_test_image(800, 600, *color);
            let path = source_dir.join(filename);
            img.save(&path).unwrap();

            gallery_items.push(create_test_gallery_item(filename, filename));
        }

        // Create composite
        let result = create_composite_preview(source_dir, gallery_items);
        assert!(result.is_ok());

        let composite = result.unwrap();

        // Check dimensions (2x2 grid of 600px cells + 10px padding + 2px border)
        assert_eq!(composite.dimensions(), (1214, 1214));
    }

    #[test]
    fn test_create_composite_preview_with_fewer_images() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().to_path_buf();

        // Create only 2 test images
        let test_images = vec![
            ("image1.png", Rgba([255, 0, 0, 255])),
            ("image2.png", Rgba([0, 255, 0, 255])),
        ];

        let mut gallery_items = Vec::new();

        for (filename, color) in &test_images {
            let img = create_test_image(800, 600, *color);
            let path = source_dir.join(filename);
            img.save(&path).unwrap();

            gallery_items.push(create_test_gallery_item(filename, filename));
        }

        // Create composite with only 2 images
        let result = create_composite_preview(source_dir, gallery_items);
        assert!(result.is_ok());

        let composite = result.unwrap();

        // Should still create full-size composite
        assert_eq!(composite.dimensions(), (1214, 1214));
    }

    #[test]
    fn test_create_composite_preview_empty() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().to_path_buf();

        // Create composite with no images
        let result = create_composite_preview(source_dir, Vec::new());
        assert!(result.is_ok());

        let composite = result.unwrap();

        // Should still create full-size composite (white background)
        assert_eq!(composite.dimensions(), (1214, 1214));
    }

    #[test]
    fn test_create_composite_preview_missing_images() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().to_path_buf();

        // Create gallery items that reference non-existent files
        let gallery_items = vec![
            create_test_gallery_item("missing1.png", "missing1.png"),
            create_test_gallery_item("missing2.png", "missing2.png"),
        ];

        // Should still succeed even with missing images
        let result = create_composite_preview(source_dir, gallery_items);
        assert!(result.is_ok());

        let composite = result.unwrap();
        assert_eq!(composite.dimensions(), (1214, 1214));
    }
}
