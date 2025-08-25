use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use axum::http::StatusCode;
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
async fn test_serve_cached_image_not_found() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Try to serve non-existent cached file
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
async fn test_serve_cached_image_mime_types() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create test files with different extensions
    let test_data = b"dummy image data";

    // Ensure cache directory exists
    tokio::fs::create_dir_all(&gallery.config.cache_directory)
        .await
        .unwrap();

    // Test JPEG
    let jpeg_path = gallery.config.cache_directory.join("test.jpg");
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

#[tokio::test]
async fn test_cache_headers_for_cached_images() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a test cached file
    let test_data = b"test image data";
    tokio::fs::create_dir_all(&gallery.config.cache_directory)
        .await
        .unwrap();

    let cache_file = gallery.config.cache_directory.join("cached.jpg");
    tokio::fs::write(&cache_file, test_data).await.unwrap();

    // Serve the cached file
    let response = gallery
        .serve_cached_image("cached.jpg", "", "")
        .await
        .unwrap();

    // Check that cache headers are present
    let cache_control = response.headers().get("cache-control");
    assert!(
        cache_control.is_some(),
        "Cache-Control header should be present"
    );

    // Check content length
    let content_length = response.headers().get("content-length");
    assert!(
        content_length.is_some(),
        "Content-Length header should be present"
    );
    assert_eq!(
        content_length.unwrap().to_str().unwrap(),
        test_data.len().to_string()
    );
}

#[tokio::test]
async fn test_serve_file_with_cache_header() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a test file
    let test_file = gallery.config.source_directory.join("test.jpg");
    tokio::fs::write(&test_file, b"test data").await.unwrap();

    // Test serving with was_cached = false
    let response_not_cached = gallery
        .serve_file_with_cache_header(&test_file, false)
        .await;

    assert_eq!(response_not_cached.status(), StatusCode::OK);
    let headers = response_not_cached.headers();

    // Should have standard cache headers
    assert!(headers.get("cache-control").is_some());
    assert!(headers.get("content-type").is_some());

    // Test serving with was_cached = true
    let response_cached = gallery.serve_file_with_cache_header(&test_file, true).await;

    assert_eq!(response_cached.status(), StatusCode::OK);
}
