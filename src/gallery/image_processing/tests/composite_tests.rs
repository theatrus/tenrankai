use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use axum::http::StatusCode;
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
async fn test_store_and_serve_composite() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a simple test image
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255, 0, 0, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    let cache_key = "test_composite";

    // Store the composite
    let result = gallery
        .store_and_serve_composite(cache_key, dynamic_img.clone())
        .await;
    assert!(result.is_ok(), "Failed to store composite: {:?}", result);

    // Check that the file was created
    let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);
    assert!(
        tokio::fs::metadata(&cache_path).await.is_ok(),
        "Cache file not created"
    );

    // Verify the file exists on disk
    assert!(cache_path.exists(), "Composite file should exist on disk");

    // Test serving from cache using the generated filename
    let cached_result = gallery
        .serve_cached_image(&cache_filename, "composite", "")
        .await;
    assert!(
        cached_result.is_ok(),
        "Failed to serve from cache: {:?}",
        cached_result
    );
}

#[tokio::test]
async fn test_serve_cached_image_not_found() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Try to serve non-existent composite
    let result = gallery
        .serve_cached_image("non_existent.jpg", "composite", "")
        .await;

    match result {
        Ok(response) => {
            // Extract status from response
            let status = response.status();
            assert_eq!(status, StatusCode::NOT_FOUND, "Expected 404 status");
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[tokio::test]
async fn test_store_composite_with_complex_key() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a test image
    let img = ImageBuffer::from_pixel(50, 50, Rgba([0, 255, 0, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    // Use a cache key that would have created subdirectories with the old format
    let cache_key = "composite_folder_subfolder_item";

    // Store the composite
    let result = gallery
        .store_and_serve_composite(cache_key, dynamic_img)
        .await;
    assert!(
        result.is_ok(),
        "Failed to store composite with complex key: {:?}",
        result
    );

    // Verify it was stored
    let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);
    assert!(
        tokio::fs::metadata(&cache_path).await.is_ok(),
        "Cache file not created for complex key"
    );
}

#[tokio::test]
async fn test_cached_composite_mime_type() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create and store a test image
    let img = ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 255, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);
    let cache_key = "test_mime_type";

    // Store the composite
    let store_result = gallery
        .store_and_serve_composite(cache_key, dynamic_img)
        .await;
    assert!(store_result.is_ok());

    // Get the actual cache filename that was created
    let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);

    // Serve from cache
    let cached_response = gallery
        .serve_cached_image(&cache_filename, "composite", "")
        .await;
    assert!(cached_response.is_ok());

    // Check that the response has the correct MIME type
    let response = cached_response.unwrap();
    let content_type = response.headers().get("content-type");
    assert!(content_type.is_some(), "Content-Type header missing");
    assert_eq!(
        content_type.unwrap().to_str().unwrap(),
        "image/jpeg",
        "Wrong MIME type for cached composite"
    );
}

#[tokio::test]
async fn test_composite_rgb_conversion() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create an RGBA image with transparency
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255, 0, 0, 128])); // Semi-transparent red
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    let cache_key = "test_rgb_conversion";

    // Store the composite (should convert to RGB for JPEG)
    let result = gallery
        .store_and_serve_composite(cache_key, dynamic_img)
        .await;
    assert!(
        result.is_ok(),
        "Failed to store RGBA composite: {:?}",
        result
    );

    // Check that the file was created
    let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);

    // Load the saved image and verify it's RGB (no alpha)
    let saved_img = image::open(&cache_path).unwrap();
    assert!(
        !saved_img.color().has_alpha(),
        "JPEG composite should not have alpha channel"
    );
}

#[tokio::test]
async fn test_composite_cache_headers() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a test image
    let img = ImageBuffer::from_pixel(50, 50, Rgba([128, 255, 128, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    let cache_key = "test_cache_headers";

    // Store the composite
    let response = gallery
        .store_and_serve_composite(cache_key, dynamic_img)
        .await
        .unwrap();

    // Check cache headers
    let cache_control = response.headers().get("cache-control");
    assert!(cache_control.is_some(), "Cache-Control header missing");
    assert_eq!(
        cache_control.unwrap().to_str().unwrap(),
        "public, max-age=31536000, immutable",
        "Wrong Cache-Control header for composite"
    );

    // Check content type
    let content_type = response.headers().get("content-type");
    assert!(content_type.is_some(), "Content-Type header missing");
    assert_eq!(
        content_type.unwrap().to_str().unwrap(),
        "image/jpeg",
        "Wrong Content-Type for composite"
    );

    // Check content length is set
    let content_length = response.headers().get("content-length");
    assert!(content_length.is_some(), "Content-Length header missing");
}

#[tokio::test]
async fn test_serve_cached_image_mime_types() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create test files with different extensions
    let test_data = b"dummy image data";

    // Test JPEG
    let jpeg_path = gallery.config.cache_directory.join("test.jpg");
    tokio::fs::create_dir_all(&gallery.config.cache_directory)
        .await
        .unwrap();
    tokio::fs::write(&jpeg_path, test_data).await.unwrap();

    let jpeg_response = gallery
        .serve_cached_image("test.jpg", "", "")
        .await
        .unwrap();
    assert_eq!(
        jpeg_response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "image/jpeg"
    );

    // Test WebP
    let webp_path = gallery.config.cache_directory.join("test.webp");
    tokio::fs::write(&webp_path, test_data).await.unwrap();

    let webp_response = gallery
        .serve_cached_image("test.webp", "", "")
        .await
        .unwrap();
    assert_eq!(
        webp_response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "image/webp"
    );

    // Test PNG
    let png_path = gallery.config.cache_directory.join("test.png");
    tokio::fs::write(&png_path, test_data).await.unwrap();

    let png_response = gallery
        .serve_cached_image("test.png", "", "")
        .await
        .unwrap();
    assert_eq!(
        png_response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "image/png"
    );
}
