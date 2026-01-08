use crate::config::{GallerySystemConfig, ImageIndexingMode};
use crate::gallery::Gallery;
use crate::gallery::types::ImageSize;
use image::ImageBuffer;
use tempfile::TempDir;

// Helper function to create a test gallery
async fn create_test_gallery() -> (Gallery, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");

    let config = GallerySystemConfig {
        name: "test".to_string(),
        source_directory: temp_dir.path().to_path_buf(),
        cache_directory: cache_dir,
        image_indexing: ImageIndexingMode::Filename,
        ..Default::default()
    };

    let gallery = Gallery::new(config);

    (gallery, temp_dir)
}

#[tokio::test]
async fn test_watermark_cache_key_differentiation() {
    let (mut gallery, _temp_dir) = create_test_gallery().await;

    // Create a test image
    let img = ImageBuffer::from_pixel(200, 200, image::Rgb([255u8, 128, 64]));
    let source_path = gallery
        .config
        .source_directory
        .join("test_watermark_cache.jpg");
    img.save(&source_path).unwrap();

    let relative_path = "test_watermark_cache.jpg";
    let size = "medium"; // Watermarks are only applied to medium size
    let format = crate::gallery::image_processing::OutputFormat::Jpeg;

    // Test 1: Generate cache without watermark
    gallery.config.copyright_holder = None;
    let result_no_watermark = gallery
        .get_resized_image(&source_path, relative_path, size, format)
        .await;
    assert!(result_no_watermark.is_ok());
    let cache_path_no_watermark = result_no_watermark.unwrap();

    // Test 2: Enable watermark and generate cache again
    gallery.config.copyright_holder = Some("Test Copyright".to_string());
    let result_with_watermark = gallery
        .get_resized_image(&source_path, relative_path, size, format)
        .await;
    assert!(result_with_watermark.is_ok());
    let cache_path_with_watermark = result_with_watermark.unwrap();

    // Test 3: Verify the cache paths are different
    assert_ne!(
        cache_path_no_watermark, cache_path_with_watermark,
        "Watermarked and non-watermarked images should have different cache paths"
    );

    // Test 4: Verify both cache files exist
    assert!(
        cache_path_no_watermark.exists(),
        "Non-watermarked cache file should exist"
    );
    assert!(
        cache_path_with_watermark.exists(),
        "Watermarked cache file should exist"
    );

    // Test 5: Test different sizes - thumbnail should not be affected by watermark setting
    let thumbnail_result1 = gallery
        .get_resized_image(&source_path, relative_path, "thumbnail", format)
        .await;

    gallery.config.copyright_holder = None;
    let thumbnail_result2 = gallery
        .get_resized_image(&source_path, relative_path, "thumbnail", format)
        .await;

    assert!(thumbnail_result1.is_ok());
    assert!(thumbnail_result2.is_ok());

    // Thumbnails should have the same cache path regardless of watermark setting
    assert_eq!(
        thumbnail_result1.unwrap(),
        thumbnail_result2.unwrap(),
        "Thumbnail cache paths should be the same regardless of watermark setting"
    );
}

#[tokio::test]
async fn test_cache_key_generation_with_watermark() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    let path = "test/image.jpg";
    let size = "medium";
    let format = "jpg";

    // Test cache key without watermark
    let key_no_watermark = gallery.generate_image_cache_key(path, size, format, false);

    // Test cache key with watermark
    let key_with_watermark = gallery.generate_image_cache_key(path, size, format, true);

    // Keys should be different
    assert_ne!(
        key_no_watermark, key_with_watermark,
        "Cache keys should differ based on watermark status"
    );

    // Test filename generation
    let filename_no_watermark = gallery.generate_cache_filename(path, size, format, false);

    let filename_with_watermark = gallery.generate_cache_filename(path, size, format, true);

    assert_ne!(
        filename_no_watermark, filename_with_watermark,
        "Cache filenames should differ based on watermark status"
    );

    // Both should end with the correct extension
    assert!(filename_no_watermark.ends_with(".jpg"));
    assert!(filename_with_watermark.ends_with(".jpg"));
}

#[tokio::test]
async fn test_watermark_only_applied_to_medium_size() {
    let (mut gallery, _temp_dir) = create_test_gallery().await;

    // Enable watermarking
    gallery.config.copyright_holder = Some("Test Copyright".to_string());

    let path = "test/image.jpg";
    let format = "jpg";

    // Test all sizes
    let sizes = ImageSize::BASE_SIZES;

    for &size in sizes {
        let key_with_copyright = gallery.generate_image_cache_key(
            path,
            size.as_str(),
            format,
            size.supports_watermark(), // Only medium should have watermark
        );

        // Disable copyright to compare
        let key_without_copyright =
            gallery.generate_image_cache_key(path, size.as_str(), format, false);

        if size.supports_watermark() {
            // Medium size keys should be different when watermark is enabled
            assert_ne!(
                key_with_copyright, key_without_copyright,
                "Medium size should have different keys for watermarked vs non-watermarked"
            );
        } else {
            // Other sizes should have the same key regardless
            assert_eq!(
                key_with_copyright, key_without_copyright,
                "{} size should have the same key regardless of watermark setting",
                size
            );
        }
    }
}
