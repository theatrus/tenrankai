use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use image::{ImageBuffer, Rgba};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

// Helper function to create a test gallery
async fn create_test_gallery() -> (Gallery, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");

    let config = GallerySystemConfig {
        name: "test".to_string(),
        url_prefix: "gallery".to_string(),
        gallery_template: "gallery.html.liquid".to_string(),
        image_detail_template: "image_detail.html.liquid".to_string(),
        source_directory: temp_dir.path().to_path_buf(),
        cache_directory: cache_dir,
        images_per_page: 50,
        thumbnail: crate::ImageSizeConfig {
            width: 300,
            height: 300,
        },
        gallery_size: crate::ImageSizeConfig {
            width: 800,
            height: 800,
        },
        medium: crate::ImageSizeConfig {
            width: 1200,
            height: 1200,
        },
        large: crate::ImageSizeConfig {
            width: 1600,
            height: 1600,
        },
        preview: crate::PreviewConfig {
            max_images: 6,
            max_depth: 3,
            max_per_folder: 3,
        },
        cache_refresh_interval_minutes: None,
        jpeg_quality: Some(85),
        webp_quality: Some(85.0),
        pregenerate_cache: false,
        new_threshold_days: None,
        approximate_dates_for_public: false,
        copyright_holder: None,
    };

    let gallery = Gallery {
        config,
        metadata_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        cache_metadata: Arc::new(RwLock::new(crate::gallery::CacheMetadata {
            version: String::new(),
            last_full_refresh: std::time::SystemTime::UNIX_EPOCH,
        })),
        metadata_cache_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        metadata_updates_since_save: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };

    (gallery, temp_dir)
}

#[tokio::test]
async fn test_png_support_and_transparency() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a PNG with transparency
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255u8, 128, 64, 128])); // Semi-transparent
    let source_path = gallery.config.source_directory.join("test_transparent.png");
    img.save(&source_path).unwrap();

    // Process the PNG image
    let relative_path = "test_transparent.png";

    // Test that PNG is always output as PNG
    let result = gallery
        .get_resized_image(
            &source_path,
            relative_path,
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Png,
        )
        .await;

    assert!(result.is_ok(), "Failed to process PNG: {:?}", result);

    let cache_path = result.unwrap();

    // Verify it's a PNG file
    assert!(cache_path.to_string_lossy().ends_with(".png"));

    // Load the processed image and verify it has alpha channel
    let processed_img = image::open(&cache_path).unwrap();
    assert!(
        processed_img.color().has_alpha(),
        "Processed PNG lost alpha channel"
    );
}

#[tokio::test]
async fn test_png_icc_profile_extraction() {
    let temp_dir = TempDir::new().unwrap();
    let png_path = temp_dir.path().join("test_icc.png");

    // Create a simple PNG
    let img = ImageBuffer::from_pixel(50, 50, Rgba([255u8, 128, 64, 255]));
    img.save(&png_path).unwrap();

    // For now, just test that extraction doesn't crash
    // Real PNG ICC profile testing would require creating a PNG with iCCP chunk
    let icc_profile = crate::gallery::image_processing::extract_icc_profile_from_png(&png_path);

    // This should return None for a basic PNG without ICC profile
    assert!(icc_profile.is_none());
}

#[tokio::test]
async fn test_png_format_selection() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a PNG with transparency
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255u8, 0, 0, 200])); // Semi-transparent red
    let source_path = gallery
        .config
        .source_directory
        .join("test_format_selection.png");
    img.save(&source_path).unwrap();

    // Test different output formats
    let relative_path = "test_format_selection.png";

    // PNG should preserve transparency
    let png_result = gallery
        .get_resized_image(
            &source_path,
            relative_path,
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Png,
        )
        .await;
    assert!(png_result.is_ok());
    let png_path = png_result.unwrap();
    assert!(png_path.to_string_lossy().ends_with(".png"));

    // JPEG conversion should work but lose transparency
    let jpeg_result = gallery
        .get_resized_image(
            &source_path,
            relative_path,
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Jpeg,
        )
        .await;
    assert!(
        jpeg_result.is_ok(),
        "Failed to convert PNG to JPEG: {:?}",
        jpeg_result
    );
    let jpeg_path = jpeg_result.unwrap();
    assert!(jpeg_path.to_string_lossy().ends_with(".jpg"));

    // WebP should also work
    let webp_result = gallery
        .get_resized_image(
            &source_path,
            relative_path,
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::WebP,
        )
        .await;
    assert!(webp_result.is_ok());
    let webp_path = webp_result.unwrap();
    assert!(webp_path.to_string_lossy().ends_with(".webp"));
}

#[tokio::test]
async fn test_png_resize_quality() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a detailed PNG pattern
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(200, 200);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        // Create a checkerboard pattern with transparency
        let is_checker = (x / 10 + y / 10) % 2 == 0;
        *pixel = if is_checker {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 128]) // Semi-transparent black
        };
    }

    let source_path = gallery
        .config
        .source_directory
        .join("test_resize_quality.png");
    img.save(&source_path).unwrap();

    // Test different sizes
    let sizes = ["thumbnail", "gallery", "medium"];

    for size in &sizes {
        let result = gallery
            .get_resized_image(
                &source_path,
                "test_resize_quality.png",
                size,
                crate::gallery::image_processing::OutputFormat::Png,
            )
            .await;

        assert!(
            result.is_ok(),
            "Failed to resize PNG to {}: {:?}",
            size,
            result
        );

        let cache_path = result.unwrap();
        let resized_img = image::open(&cache_path).unwrap();

        // Verify the image was actually resized
        match *size {
            "thumbnail" => {
                assert!(resized_img.width() <= 300);
                assert!(resized_img.height() <= 300);
            }
            "gallery" => {
                assert!(resized_img.width() <= 800);
                assert!(resized_img.height() <= 800);
            }
            "medium" => {
                assert!(resized_img.width() <= 1200);
                assert!(resized_img.height() <= 1200);
            }
            _ => {}
        }

        // Verify alpha channel is preserved
        assert!(
            resized_img.color().has_alpha(),
            "PNG lost alpha channel during {} resize",
            size
        );
    }
}

#[tokio::test]
async fn test_png_with_extreme_dimensions() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Test very wide PNG
    let wide_img = ImageBuffer::from_pixel(2000, 100, Rgba([0u8, 255, 0, 255]));
    let wide_path = gallery.config.source_directory.join("test_wide.png");
    wide_img.save(&wide_path).unwrap();

    let result = gallery
        .get_resized_image(
            &wide_path,
            "test_wide.png",
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Png,
        )
        .await;

    assert!(result.is_ok());
    let cache_path = result.unwrap();
    let resized = image::open(&cache_path).unwrap();

    // Should maintain aspect ratio
    assert!(resized.width() <= 300);
    assert!(resized.height() < 100); // Should be scaled down proportionally

    // Test very tall PNG
    let tall_img = ImageBuffer::from_pixel(100, 2000, Rgba([0u8, 0, 255, 255]));
    let tall_path = gallery.config.source_directory.join("test_tall.png");
    tall_img.save(&tall_path).unwrap();

    let result = gallery
        .get_resized_image(
            &tall_path,
            "test_tall.png",
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Png,
        )
        .await;

    assert!(result.is_ok());
    let cache_path = result.unwrap();
    let resized = image::open(&cache_path).unwrap();

    // Should maintain aspect ratio
    assert!(resized.height() <= 300);
    assert!(resized.width() < 100); // Should be scaled down proportionally
}
