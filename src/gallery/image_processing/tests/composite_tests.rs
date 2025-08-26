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
    let hash = gallery.generate_cache_key(cache_key, "jpg");
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
    let hash = gallery.generate_cache_key(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);
    assert!(
        tokio::fs::metadata(&cache_path).await.is_ok(),
        "Cache file not created for complex key"
    );
}

#[tokio::test]
async fn test_composite_cache_key_generation() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Test various gallery paths
    let test_cases = vec![
        ("2008-eureka", "composite_2008-eureka"),
        ("folder/subfolder", "composite_folder_subfolder"),
        ("", "composite_root"),
        ("gallery/2024", "composite_gallery_2024"),
    ];

    for (gallery_path, expected_key) in test_cases {
        let cache_key = Gallery::generate_composite_cache_key(gallery_path);
        assert_eq!(
            cache_key, expected_key,
            "Cache key mismatch for path '{}'",
            gallery_path
        );

        // Test that we can generate a proper cache filename
        let hash = gallery.generate_cache_key(&cache_key, "jpg");
        assert!(!hash.is_empty(), "Hash should not be empty");

        let cache_filename = format!("{}.jpg", hash);
        assert!(
            cache_filename.ends_with(".jpg"),
            "Cache filename should end with .jpg"
        );
    }
}

#[tokio::test]
async fn test_serve_cached_composite_with_proper_filename() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a test image
    let img = ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 255, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    // Simulate the composite endpoint workflow
    let gallery_path = "2008-eureka";
    let composite_cache_key = Gallery::generate_composite_cache_key(gallery_path);
    println!("composite_cache_key: {}", composite_cache_key);

    // Store using the base cache key (as store_and_serve_composite expects)
    let result = gallery
        .store_and_serve_composite(&composite_cache_key, dynamic_img.clone())
        .await;
    assert!(result.is_ok(), "Failed to store composite");

    // List all files in cache directory to debug
    println!("Cache directory: {:?}", gallery.config.cache_directory);
    if let Ok(mut entries) = tokio::fs::read_dir(&gallery.config.cache_directory).await {
        println!("Files in cache:");
        while let Ok(Some(entry)) = entries.next_entry().await {
            println!("  - {}", entry.file_name().to_string_lossy());
        }
    }

    // Generate the cache filename the same way the API does
    let hash = gallery.generate_cache_key(&composite_cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);
    println!("Looking for cache_filename: {}", cache_filename);

    // Check if the file actually exists
    let cache_path = gallery.config.cache_directory.join(&cache_filename);
    let exists = tokio::fs::metadata(&cache_path).await.is_ok();
    println!("Cache file exists at {:?}: {}", cache_path, exists);

    // Now try to serve it using the full filename (as the API handler does)
    let serve_result = gallery
        .serve_cached_image(&cache_filename, "composite", "")
        .await;
    assert!(
        serve_result.is_ok(),
        "Failed to serve cached composite with proper filename"
    );

    // Verify the response is not a 404
    if let Ok(response) = serve_result {
        let status = response.status();
        assert_ne!(
            status,
            axum::http::StatusCode::NOT_FOUND,
            "Should not return 404 for existing composite"
        );
    }
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
    let hash = gallery.generate_cache_key(cache_key, "jpg");
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
async fn test_composite_mime_type_for_cached_composite() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create and store a test composite image
    let img = ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 255, 255]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);
    let cache_key = "test_composite_mime";

    // Store the composite
    let store_result = gallery
        .store_and_serve_composite(cache_key, dynamic_img)
        .await;
    assert!(store_result.is_ok());

    // Get the actual cache filename that was created
    let hash = gallery.generate_cache_key(cache_key, "jpg");
    let cache_filename = format!("{}.jpg", hash);

    // Serve the composite from cache
    let cached_response = gallery
        .serve_cached_image(&cache_filename, "composite", "")
        .await;
    assert!(cached_response.is_ok());

    // Check that the response has the correct MIME type for a composite (JPEG)
    let response = cached_response.unwrap();
    let content_type = response.headers().get("content-type");
    assert!(content_type.is_some(), "Content-Type header missing");
    assert_eq!(
        content_type.unwrap().to_str().unwrap(),
        "image/jpeg",
        "Composite images should always be served as JPEG"
    );
}

#[cfg(feature = "avif")]
#[tokio::test]
async fn test_image_crate_avif_support() {
    let temp_dir = TempDir::new().unwrap();

    // Create a simple AVIF image
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255u8, 0u8, 0u8, 255u8]));
    let dynamic_img = image::DynamicImage::ImageRgba8(img);

    let avif_path = temp_dir.path().join("test.avif");

    // Save as AVIF using our function
    crate::gallery::image_processing::formats::avif::save_with_profile(
        &dynamic_img,
        &avif_path,
        85,
        6,
        None,
        false,
    )
    .expect("Failed to save AVIF");

    // Try to open it with image::open
    let result = image::open(&avif_path);

    println!("image::open result for AVIF: {:?}", result.is_ok());
    if let Err(e) = &result {
        println!("Error: {:?}", e);
    }

    // The image crate does not support AVIF directly
    assert!(result.is_err(), "image::open should fail on AVIF files");
}

#[cfg(feature = "avif")]
#[tokio::test]
async fn test_composite_with_avif_images() {
    let (_gallery, temp_dir) = create_test_gallery().await;

    // Create test AVIF images with distinct colors
    let mut images = vec![];
    let colors = [
        [255u8, 0u8, 0u8, 255u8],   // Red
        [0u8, 255u8, 0u8, 255u8],   // Green
        [0u8, 0u8, 255u8, 255u8],   // Blue
        [255u8, 255u8, 0u8, 255u8], // Yellow
    ];

    for (i, color) in colors.iter().enumerate() {
        let img = ImageBuffer::from_pixel(100, 100, Rgba(*color));
        let dynamic_img = image::DynamicImage::ImageRgba8(img);

        // Save as AVIF
        let filename = format!("test_{:03}.avif", i);
        let avif_path = temp_dir.path().join(&filename);

        // Use our AVIF save function
        crate::gallery::image_processing::formats::avif::save_with_profile(
            &dynamic_img,
            &avif_path,
            85,    // quality
            6,     // speed
            None,  // no ICC profile
            false, // not HDR
        )
        .expect("Failed to save AVIF");

        images.push(crate::gallery::GalleryItem {
            name: format!("test_{:03}", i),
            display_name: Some(format!("Test {}", i)),
            description: None,
            path: filename,
            parent_path: None,
            is_directory: false,
            thumbnail_url: None,
            gallery_url: None,
            preview_images: None,
            item_count: None,
            dimensions: Some((100, 100)),
            capture_date: None,
            is_new: false,
        });
    }

    // Create composite
    let result = tokio::task::spawn_blocking(move || {
        crate::composite::create_composite_preview(temp_dir.path().to_path_buf(), images)
    })
    .await
    .expect("Task failed");

    assert!(result.is_ok(), "Should create composite with AVIF images");

    if let Ok(composite_img) = result {
        // Verify it's not all white (which would indicate images weren't loaded)
        let rgb_img = composite_img.to_rgb8();
        let (width, height) = rgb_img.dimensions();

        // Sample center of each quadrant where the images should be
        let sample_points = [
            (width / 4, height / 4),         // Top-left - should be red
            (3 * width / 4, height / 4),     // Top-right - should be green
            (width / 4, 3 * height / 4),     // Bottom-left - should be blue
            (3 * width / 4, 3 * height / 4), // Bottom-right - should be yellow
        ];

        // Count unique colors found
        let mut unique_colors = std::collections::HashSet::new();
        let mut non_white_pixels = 0;

        for (x, y) in &sample_points {
            let pixel = rgb_img.get_pixel(*x, *y);
            let color = (pixel[0], pixel[1], pixel[2]);
            unique_colors.insert(color);

            if color != (255, 255, 255) && color != (200, 200, 200) {
                // Not white or border
                non_white_pixels += 1;
            }

            println!("Pixel at ({}, {}): {:?}", x, y, color);
        }

        assert!(
            unique_colors.len() >= 3,
            "Composite should show different colored images. Found {} unique colors",
            unique_colors.len()
        );

        assert!(
            non_white_pixels >= 3,
            "At least 3 quadrants should have non-white colors. Found {}",
            non_white_pixels
        );
    }
}
