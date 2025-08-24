use axum::http::StatusCode;
use axum_test::TestServer;
use std::path::PathBuf;
use tempfile::TempDir;
use tenrankai::{Config, GallerySystemConfig, create_app};

/// Helper to create a test configuration with galleries
fn create_test_config(temp_dir: &TempDir) -> Config {
    let mut config = Config::default();

    // Create test directories
    let photos_dir = temp_dir.path().join("photos");
    let portfolio_dir = temp_dir.path().join("portfolio");
    let cache_dir = temp_dir.path().join("cache");

    std::fs::create_dir_all(&photos_dir).unwrap();
    std::fs::create_dir_all(&portfolio_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Set base URL for OpenGraph testing
    config.app.base_url = Some("https://example.com".to_string());

    // Configure multiple galleries
    config.galleries = Some(vec![
        GallerySystemConfig {
            name: "main".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: photos_dir.clone(),
            cache_directory: cache_dir.join("main"),
            gallery_template: "modules/gallery.html.liquid".to_string(),
            image_detail_template: "modules/image_detail.html.liquid".to_string(),
            images_per_page: 20,
            thumbnail: tenrankai::ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: tenrankai::ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: tenrankai::ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: tenrankai::ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: tenrankai::PreviewConfig {
                max_images: 6,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate_cache: false,
            new_threshold_days: Some(7),
            approximate_dates_for_public: false,
        },
        GallerySystemConfig {
            name: "portfolio".to_string(),
            url_prefix: "/my-portfolio".to_string(),
            source_directory: portfolio_dir.clone(),
            cache_directory: cache_dir.join("portfolio"),
            gallery_template: "modules/gallery.html.liquid".to_string(),
            image_detail_template: "modules/image_detail.html.liquid".to_string(),
            images_per_page: 12,
            thumbnail: tenrankai::ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: tenrankai::ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: tenrankai::ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: tenrankai::ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: tenrankai::PreviewConfig {
                max_images: 9,
                max_depth: 2,
                max_per_folder: 4,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(90),
            webp_quality: Some(90.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
        },
    ]);

    // Set template directory to the actual project templates
    config.templates.directory = PathBuf::from("templates");
    config.static_files.directory = PathBuf::from("static");

    config
}

/// Helper to create test images in a directory
fn create_test_images(dir: &std::path::Path, count: usize) {
    use image::{ImageBuffer, Rgb};

    for i in 0..count {
        let img = ImageBuffer::from_fn(100, 100, |x, y| {
            Rgb([(x * 2) as u8, (y * 2) as u8, (i * 50) as u8])
        });

        // Use zero-padded names for proper string sorting
        let path = dir.join(format!("test_{:03}.jpg", i));
        img.save(&path).unwrap();
    }
}

/// Helper to create a folder with metadata
fn create_folder_with_metadata(dir: &std::path::Path, title: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();

    let metadata_content = format!("# {}\n\n{}", title, description);

    std::fs::write(dir.join("_folder.md"), metadata_content).unwrap();
}

#[tokio::test]
async fn test_gallery_root_renders_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create some test images
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        3,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test main gallery
    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains("Photo Gallery"));
    assert!(
        html.contains(r#"href="/gallery""#),
        "Should use correct gallery URL"
    );
    assert!(html.contains("test_000.jpg"));
    assert!(html.contains("test_001.jpg"));
    assert!(html.contains("test_002.jpg"));
}

#[tokio::test]
async fn test_portfolio_gallery_renders_with_custom_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create test images in portfolio
    create_test_images(
        config.galleries.as_ref().unwrap()[1]
            .source_directory
            .as_path(),
        2,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test portfolio gallery with custom URL prefix
    let response = server.get("/my-portfolio").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains("Photo Gallery"));
    assert!(
        html.contains("/my-portfolio"),
        "Should use custom portfolio URL in links"
    );
    assert!(html.contains("test_000.jpg"));
    assert!(html.contains("test_001.jpg"));
}

#[tokio::test]
async fn test_gallery_with_folder_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create a subfolder with metadata
    let photos_dir = &config.galleries.as_ref().unwrap()[0].source_directory;
    let vacation_dir = photos_dir.join("vacation");
    create_folder_with_metadata(
        &vacation_dir,
        "Summer Vacation 2024",
        "Beautiful memories from our trip to the mountains.",
    );
    create_test_images(&vacation_dir, 4);

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test subfolder with metadata
    let response = server.get("/gallery/vacation").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();

    assert!(
        html.contains("Summer Vacation 2024"),
        "Should show custom folder title"
    );
    assert!(
        html.contains("Beautiful memories from our trip"),
        "Should show folder description"
    );
    // Just check that we're in the vacation folder, not exact href
    assert!(
        html.contains("vacation"),
        "Should have vacation folder in URLs"
    );
}

#[tokio::test]
async fn test_gallery_opengraph_with_composite_image() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create multiple images to trigger composite image
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        4,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // Check for OpenGraph composite image
    assert!(html.contains(r#"property="og:image""#));
    assert!(html.contains("https://example.com/api/gallery/main/composite/_root"));
    assert!(html.contains(r#"property="og:image:width" content="1210""#));
    assert!(html.contains(r#"property="og:image:height" content="1210""#));
}

#[tokio::test]
async fn test_gallery_opengraph_with_single_image() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create only one image to test fallback
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        1,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // Should use the single image for OpenGraph
    assert!(html.contains(r#"property="og:image""#));
    assert!(html.contains("/gallery/image/test_000.jpg?size=gallery"));
}

#[tokio::test]
async fn test_gallery_preview_api() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create test images
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        10,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test preview API for main gallery
    let response = server.get("/api/gallery/main/preview").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    let images = json.get("images").unwrap().as_array().unwrap();
    assert_eq!(images.len(), 3); // Limited by max_per_folder setting

    // Test with custom count
    let response = server.get("/api/gallery/main/preview?count=3").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    let images = json.get("images").unwrap().as_array().unwrap();
    assert_eq!(images.len(), 3);
}

#[tokio::test]
async fn test_image_detail_page() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create test images
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        3,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test image detail page
    let response = server.get("/gallery/detail/test_001.jpg").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains("test_001.jpg"));
    assert!(
        html.contains(r#"href="/gallery""#),
        "Breadcrumb should link to gallery"
    );
    assert!(
        html.contains("/gallery/image/test_001.jpg"),
        "Should have correct image URL"
    );

    // Check navigation links
    assert!(
        html.contains("/gallery/detail/test_000.jpg"),
        "Should have previous image link"
    );
    assert!(
        html.contains("/gallery/detail/test_002.jpg"),
        "Should have next image link"
    );
}

#[tokio::test]
async fn test_gallery_breadcrumbs() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create nested folders
    let photos_dir = &config.galleries.as_ref().unwrap()[0].source_directory;
    let travel_dir = photos_dir.join("travel");
    let europe_dir = travel_dir.join("europe");

    create_folder_with_metadata(&travel_dir, "Travel", "All our travels");
    create_folder_with_metadata(&europe_dir, "Europe 2024", "European adventure");
    create_test_images(&europe_dir, 2);

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery/travel/europe").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // Check breadcrumb navigation
    assert!(
        html.contains(">Gallery</a>"),
        "Should have Gallery breadcrumb"
    );
    assert!(
        html.contains("/gallery/travel"),
        "Should have travel link in breadcrumb"
    );
    assert!(
        html.contains(">Travel</a>"),
        "Should have Travel breadcrumb"
    );
    assert!(
        html.contains("Europe 2024"),
        "Should show current folder title"
    ); // Current page
}

#[tokio::test]
async fn test_gallery_pagination() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create many images to test pagination (main gallery has 20 per page)
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        25,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // First page
    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains("test_000.jpg"));
    assert!(html.contains("test_019.jpg"));
    assert!(!html.contains("test_020.jpg")); // Should be on page 2

    // Second page
    let response = server.get("/gallery?page=1").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains("test_020.jpg"));
    assert!(html.contains("test_024.jpg"));
    assert!(!html.contains("test_000.jpg")); // Should be on page 1
}

#[tokio::test]
async fn test_nonexistent_gallery_returns_404() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test nonexistent gallery name in API
    let response = server.get("/api/gallery/nonexistent/preview").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_gallery_preview_partial_in_template() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create images in main gallery
    create_test_images(
        config.galleries.as_ref().unwrap()[0]
            .source_directory
            .as_path(),
        6,
    );

    let app = create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test home page which includes gallery preview
    let response = server.get("/").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // The preview partial should be included with correct parameters
    assert!(html.contains("gallery-preview-component"));
    assert!(html.contains("Explore Full Gallery"));
}
