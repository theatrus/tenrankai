use axum::http::StatusCode;
use axum_test::TestServer;
use std::path::PathBuf;
use tempfile::TempDir;
use tenrankai::{Config, GallerySystemConfig, ImageIndexingMode, create_app};

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
            source_directory: photos_dir.clone(),
            cache_directory: cache_dir.join("main"),
            images_per_page: 20,
            preview: tenrankai::PreviewConfig {
                max_images: 6,
                max_depth: 3,
                max_per_folder: 3,
            },
            new_threshold_days: Some(7),
            image_indexing: ImageIndexingMode::Filename,
            permissions: tenrankai::permissions::PermissionConfig {
                public_role: Some("viewer".to_string()),
                default_authenticated_role: Some("viewer".to_string()),
                roles: {
                    let mut roles = std::collections::HashMap::new();
                    roles.insert(
                        "viewer".to_string(),
                        tenrankai::permissions::Role::new(
                            "viewer".to_string(),
                            tenrankai::permissions::RolePermissions {
                                can_view: true,
                                can_see_technical_details: true,
                                can_see_exact_dates: true,
                                can_see_location: true,
                                can_download_medium: true,
                                can_download_large: false,
                                can_download_original: false,
                                can_read_metadata: true, // Default to true for the gallery
                                can_add_comments: false,
                                can_edit_own_comments: false,
                                can_delete_own_comments: false,
                                can_set_picks: false,
                                can_add_tags: false,
                                can_edit_any_comments: false,
                                can_delete_any_comments: false,
                                can_use_zoom: false,
                                can_use_tile_zoom: false,
                                can_analyze_images: false,
                                can_see_ai_analysis: true, // Allow viewing AI analysis
                                can_see_ai_alt_text: true, // Allow viewing AI alt-text on images
                                owner_access: false,
                            },
                        ),
                    );
                    roles
                },
                user_roles: vec![],
            },
            ..Default::default()
        },
        GallerySystemConfig {
            name: "portfolio".to_string(),
            url_prefix: "/my-portfolio".to_string(),
            source_directory: portfolio_dir.clone(),
            cache_directory: cache_dir.join("portfolio"),
            images_per_page: 12,
            preview: tenrankai::PreviewConfig {
                max_images: 9,
                max_depth: 2,
                max_per_folder: 4,
            },
            jpeg_quality: Some(90),
            webp_quality: Some(90.0),
            copyright_holder: Some("Test Portfolio".to_string()),
            image_indexing: ImageIndexingMode::Filename,
            ..Default::default()
        },
    ]);

    // Set template directory to the actual project templates
    config.templates.directories = vec![PathBuf::from("templates")];
    config.static_files.directories = vec!["static".to_string()];

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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // Should use the single image for OpenGraph
    assert!(html.contains(r#"property="og:image""#));
    assert!(html.contains("/gallery/_image/test_000.jpg/gallery"));
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

    let app = create_app(config, None).await;
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
async fn test_composite_api_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let root_dir = &config.galleries.as_ref().unwrap()[0].source_directory;

    // Create test images in the root directory
    for i in 0..3 {
        let img = image::ImageBuffer::from_pixel(100, 100, image::Rgb([0u8, 255u8, 0u8]));
        let img_path = root_dir.join(format!("root_{:03}.jpg", i));
        img.save(&img_path).unwrap();
    }

    // Create a subdirectory with test images
    let subdir = root_dir.join("2008-eureka");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create test images in the subdirectory
    for i in 0..5 {
        let img = image::ImageBuffer::from_pixel(100, 100, image::Rgb([255u8, 0u8, 0u8]));
        let img_path = subdir.join(format!("test_{:03}.jpg", i));
        img.save(&img_path).unwrap();
    }

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test composite endpoint for subdirectory
    let response = server.get("/api/gallery/main/composite/2008-eureka").await;
    if response.status_code() != StatusCode::OK {
        let body = response.text();
        println!("Error response body: {}", body);
    }
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Composite endpoint should return OK"
    );

    // Verify content type
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/jpeg",
        "Composite should return JPEG content type"
    );

    // Test root composite endpoint
    let response = server.get("/api/gallery/main/composite/_root").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Root composite endpoint should return OK"
    );

    // Test that cached response works (second request should be served from cache)
    let response2 = server.get("/api/gallery/main/composite/2008-eureka").await;
    assert_eq!(
        response2.status_code(),
        StatusCode::OK,
        "Cached composite should return OK"
    );

    // Verify cache headers are set for cached response
    assert!(
        response2.headers().contains_key("cache-control"),
        "Cached response should have cache-control header"
    );
}

#[tokio::test]
async fn test_composite_api_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test composite endpoint for non-existent directory
    let response = server.get("/api/gallery/main/composite/non-existent").await;
    assert_eq!(
        response.status_code(),
        StatusCode::NOT_FOUND,
        "Non-existent directory should return 404"
    );
}

#[cfg(feature = "avif")]
#[tokio::test]
async fn test_composite_api_with_avif_images() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let root_dir = &config.galleries.as_ref().unwrap()[0].source_directory;

    // Create AVIF images in a subdirectory
    let subdir = root_dir.join("avif-test");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create test AVIF images
    for i in 0..4 {
        let img = image::ImageBuffer::from_pixel(
            100,
            100,
            image::Rgba([(i * 60) as u8, (i * 80) as u8, (i * 40) as u8, 255u8]),
        );
        let dynamic_img = image::DynamicImage::ImageRgba8(img);

        let avif_path = subdir.join(format!("test_{:03}.avif", i));

        // Save as AVIF
        tenrankai::gallery::image_processing::formats::avif::save_with_profile(
            &dynamic_img,
            &avif_path,
            85,
            6,
            None,
            false,
        )
        .expect("Failed to save AVIF");
    }

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test composite endpoint for AVIF directory
    let response = server.get("/api/gallery/main/composite/avif-test").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Composite endpoint should work with AVIF images"
    );

    // Verify content type
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/jpeg",
        "Composite should return JPEG content type"
    );

    // Verify the image data is not empty
    let body = response.as_bytes();
    assert!(
        body.len() > 1000,
        "Composite image should have substantial data"
    );
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

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test image detail page
    let response = server.get("/gallery/detail/test_001.jpg").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();

    // Also check the API endpoint to see what data is being passed
    let api_response = server.get("/api/gallery/main/image/test_001.jpg").await;
    let api_json = api_response.text();
    eprintln!("API Response: {}", api_json);

    // Check if the HTML contains the initial data script tag
    if html.contains("window.__INITIAL_DATA__") {
        eprintln!("Found initial data in HTML");
    } else {
        eprintln!("No initial data found in HTML - React won't render navigation");
    }
    assert!(html.contains("test_001.jpg"));
    assert!(
        html.contains(r#"href="/gallery""#),
        "Breadcrumb should link to gallery"
    );
    assert!(
        html.contains("/gallery/_image/test_001.jpg"),
        "Should have correct image URL"
    );

    // Since React renders client-side, check that the JSON data contains navigation info
    // The actual navigation links won't be in the HTML until React renders
    assert!(
        api_json.contains(r#""prev_image":{"path":"test_000.jpg""#),
        "API should include previous image data"
    );
    assert!(
        api_json.contains(r#""next_image":{"path":"test_002.jpg""#),
        "API should include next image data"
    );

    // Also verify the embedded JSON data in the HTML contains the navigation
    assert!(
        html.contains(r#"<script type="application/json" id="image-detail-data">"#),
        "HTML should contain embedded JSON data for React"
    );

    // The JSON should contain navigation data
    assert!(
        html.contains(r#""prev_image":{"path":"test_000.jpg""#),
        "Embedded JSON should include previous image data"
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
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

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test home page which includes gallery preview
    let response = server.get("/").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // The preview partial should be included with correct parameters
    assert!(html.contains("gallery-preview-component"));
    assert!(html.contains("Explore Full Gallery"));
}

#[tokio::test]
async fn test_hide_technical_details_feature() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let root_dir = &config.galleries.as_ref().unwrap()[0].source_directory;

    // Create a test image with EXIF data
    let img = image::ImageBuffer::from_pixel(300, 200, image::Rgb([100u8, 150u8, 200u8]));
    let img_path = root_dir.join("test_with_metadata.jpg");
    img.save(&img_path).unwrap();

    // Create a folder structure for testing different configurations
    let hidden_folder = root_dir.join("hidden-details");
    std::fs::create_dir_all(&hidden_folder).unwrap();

    let visible_folder = root_dir.join("visible-details");
    std::fs::create_dir_all(&visible_folder).unwrap();

    // Create folder configs using new permission system
    let hidden_folder_config = r#"+++
title = "Portfolio with Hidden Details"

[permissions]
public_role = "restricted_viewer"

[permissions.roles.restricted_viewer]
name = "restricted_viewer"

[permissions.roles.restricted_viewer.permissions]
can_view = true
can_read_metadata = false  # This replaces hide_technical_details = true
can_see_technical_details = false
can_download_medium = true
+++

# Clean Portfolio

Professional presentation without technical metadata.
"#;

    let visible_folder_config = r#"+++
title = "Technical Gallery"

[permissions]
public_role = "full_viewer"

[permissions.roles.full_viewer]
name = "full_viewer"

[permissions.roles.full_viewer.permissions]
can_view = true
can_read_metadata = true  # This replaces hide_technical_details = false
can_see_technical_details = true
can_download_medium = true
+++

# Technical Photography

Gallery showing full technical details.
"#;

    std::fs::write(hidden_folder.join("_folder.md"), hidden_folder_config).unwrap();
    std::fs::write(visible_folder.join("_folder.md"), visible_folder_config).unwrap();

    // Create test images in both folders
    let hidden_img = image::ImageBuffer::from_pixel(400, 300, image::Rgb([255u8, 100u8, 50u8]));
    let hidden_img_path = hidden_folder.join("portfolio_image.jpg");
    hidden_img.save(&hidden_img_path).unwrap();

    let visible_img = image::ImageBuffer::from_pixel(500, 400, image::Rgb([50u8, 255u8, 100u8]));
    let visible_img_path = visible_folder.join("technical_image.jpg");
    visible_img.save(&visible_img_path).unwrap();

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Test image detail page in folder with hidden technical details
    let response = server
        .get("/gallery/detail/hidden-details/portfolio_image.jpg")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();

    // Check the API response to verify hide_technical_details is working
    let api_response = server
        .get("/api/gallery/main/image/hidden-details/portfolio_image.jpg")
        .await;
    let api_json = api_response.text();
    eprintln!("API response for hidden-details: {}", api_json);

    // The API response should include the permissions in JSON data
    // Since this is rendered by React, we check the JSON data instead of HTML
    assert!(html.contains("portfolio_image.jpg"));

    // Parse the JSON to check permissions
    let json_value: serde_json::Value = serde_json::from_str(&api_json).unwrap();
    let permissions = &json_value["permissions"];

    // With the new permission system, the folder sets can_read_metadata = false
    // which should hide metadata
    assert_eq!(
        permissions["can_read_metadata"], false,
        "Folder with can_read_metadata = false should block metadata access"
    );

    // Test image detail page in folder with visible technical details
    let response = server
        .get("/gallery/detail/visible-details/technical_image.jpg")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // When technical details are visible, can_read_metadata should be true
    assert!(html.contains("technical_image.jpg"));
    // Check the API response for proper permissions
    let api_response2 = server
        .get("/api/gallery/main/image/visible-details/technical_image.jpg")
        .await;
    let api_json2 = api_response2.text();
    eprintln!("API response for visible-details: {}", api_json2);

    // Parse the JSON to check individual fields
    let json_value: serde_json::Value = serde_json::from_str(&api_json2).unwrap();
    let permissions = &json_value["permissions"];

    // With the new permission system, the folder sets can_read_metadata = true
    // which controls whether metadata (camera info, etc.) is shown
    assert_eq!(
        permissions["can_read_metadata"], true,
        "Folder with can_read_metadata = true should allow metadata access"
    );

    // Also verify that technical details are controlled by can_see_technical_details
    // In the visible folder, this should be based on the permissions configuration
}
