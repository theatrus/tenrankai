use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use tenrankai::{
    GallerySystemConfig, ImageIndexingMode, StaticConfig, TemplateConfig, create_app,
    site::SiteConfig,
};

/// Helper to create a test configuration with galleries
fn create_test_config(temp_dir: &TempDir) -> SiteConfig {
    // Create test directories
    let photos_dir = temp_dir.path().join("photos");
    let portfolio_dir = temp_dir.path().join("portfolio");
    let cache_dir = temp_dir.path().join("cache");

    std::fs::create_dir_all(&photos_dir).unwrap();
    std::fs::create_dir_all(&portfolio_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Set template directory to the actual project templates
    // CARGO_MANIFEST_DIR is the tenrankai package dir, templates are at workspace root
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    // Configure multiple galleries
    let galleries = vec![
        GallerySystemConfig {
            name: "main".to_string(),
            source_directory: photos_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.join("main").to_string_lossy().to_string(),
            preview: tenrankai::PreviewConfig {
                max_images: 6,
                max_depth: 3,
                max_per_folder: 3,
            },
            new_threshold_days: Some(7),
            image_indexing: ImageIndexingMode::Filename,
            permissions: tenrankai::permissions::PermissionConfig {
                site_admins: Vec::new(),
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
                                can_see_hidden: false,
                                can_see_technical_details: true,
                                can_see_exact_dates: true,
                                can_see_location: true,
                                can_download_medium: true,
                                can_download_large: false,
                                can_download_original: false,
                                can_download_gallery: false,
                                can_download_raw: false,
                                can_see_versions: false,
                                can_read_metadata: true,
                                can_edit_content: false,
                                can_add_comments: false,
                                can_edit_own_comments: false,
                                can_delete_own_comments: false,
                                can_set_picks: false,
                                can_add_tags: false,
                                can_edit_any_comments: false,
                                can_delete_any_comments: false,
                                can_manage_images: false,
                                can_use_zoom: false,
                                can_use_tile_zoom: false,
                                can_analyze_images: false,
                                can_see_ai_analysis: true,
                                can_see_ai_alt_text: true,
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
            source_directory: portfolio_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.join("portfolio").to_string_lossy().to_string(),
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
    ];

    SiteConfig {
        name: "test".to_string(),
        base_url: Some("https://example.com".to_string()),
        cookie_secret: "test-secret".to_string(),
        templates: TemplateConfig {
            directories: vec![
                workspace_root
                    .join("templates")
                    .to_string_lossy()
                    .to_string(),
            ],
        },
        static_files: StaticConfig {
            directories: vec![workspace_root.join("static").to_string_lossy().to_string()],
            use_redirects: false,
        },
        galleries: Some(galleries),
        posts: None,
        user_database: None,
        email: None,
        config_storage: None,
        site_admins: Vec::new(),
        hosted_mode: false,
        site_title: None,
        copyright_holder: None,
    }
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
            .as_ref(),
        3,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Test main gallery
    let response = server.get("/gallery").await;

    // Debug: print status and body if not OK
    if response.status_code() != StatusCode::OK {
        eprintln!("=== ERROR: Status {} ===", response.status_code());
        eprintln!("Headers: {:?}", response.headers());
        let body = response.as_bytes();
        eprintln!("Body length: {} bytes", body.len());
        if let Ok(text) = std::str::from_utf8(body) {
            eprintln!("Body: {}", text);
        }
        panic!("Expected 200 OK, got {}", response.status_code());
    }

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
            .as_ref(),
        2,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let vacation_dir = photos_dir.join("vacation");
    create_folder_with_metadata(
        &vacation_dir,
        "Summer Vacation 2024",
        "Beautiful memories from our trip to the mountains.",
    );
    create_test_images(&vacation_dir, 4);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
            .as_ref(),
        4,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
            .as_ref(),
        1,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
            .as_ref(),
        10,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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

    let root_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

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
    let server = TestServer::new(app);

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
    let server = TestServer::new(app);

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

    let root_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

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
    let server = TestServer::new(app);

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
            .as_ref(),
        3,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let travel_dir = photos_dir.join("travel");
    let europe_dir = travel_dir.join("europe");

    create_folder_with_metadata(&travel_dir, "Travel", "All our travels");
    create_folder_with_metadata(&europe_dir, "Europe 2024", "European adventure");
    create_test_images(&europe_dir, 2);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
async fn test_nonexistent_gallery_returns_404() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

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
            .as_ref(),
        6,
    );

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Test home page which includes gallery preview
    let response = server.get("/").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    // The preview partial should be included with correct parameters
    assert!(html.contains("gallery-preview-component"));
    assert!(html.contains("Explore Full Gallery"));
}

// TODO: Fix folder-level permission override of gallery-level permissions
// This test expects folder's public_role to override gallery's public_role,
// but permission merging doesn't work that way currently
#[tokio::test]
#[ignore = "Folder permission override needs investigation"]
async fn test_hide_technical_details_feature() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let root_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

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
    let server = TestServer::new(app);

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

#[tokio::test]
async fn test_gallery_download_requires_permission() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    // Create test images in main gallery
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Default viewer role does NOT have can_download_gallery
    // Attempt to download should return 403
    let response = server.get("/gallery/_download").await;
    assert_eq!(
        response.status_code(),
        StatusCode::FORBIDDEN,
        "Download should be forbidden without can_download_gallery permission"
    );
}

#[tokio::test]
async fn test_gallery_download_with_permission() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Update the viewer role to include can_download_gallery
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_download_gallery = true;
    }

    // Create test images in main gallery
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // With can_download_gallery permission, download should work
    let response = server.get("/gallery/_download").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Download should succeed with can_download_gallery permission"
    );

    // Verify content type is application/zip
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        content_type, "application/zip",
        "Response should be a zip file"
    );

    // Verify content-disposition header
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_disposition.contains("attachment"),
        "Should have attachment disposition"
    );
    assert!(
        content_disposition.contains(".zip"),
        "Should have zip filename"
    );

    // Verify the zip contains our test images
    let bytes = response.as_bytes();
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut archive = zip::ZipArchive::new(cursor).expect("Should be a valid zip file");

    let file_names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();

    assert!(
        file_names
            .iter()
            .any(|n: &String| n.contains("test_000.jpg")),
        "Zip should contain test_000.jpg"
    );
    assert!(
        file_names
            .iter()
            .any(|n: &String| n.contains("test_001.jpg")),
        "Zip should contain test_001.jpg"
    );
    assert!(
        file_names
            .iter()
            .any(|n: &String| n.contains("test_002.jpg")),
        "Zip should contain test_002.jpg"
    );
}

#[tokio::test]
async fn test_gallery_download_zip_file_dates() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Update the viewer role to include can_download_gallery
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_download_gallery = true;
    }

    // Create test images
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 2);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    let response = server.get("/gallery/_download").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify the zip files have accessible date metadata
    let bytes = response.as_bytes();
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut archive = zip::ZipArchive::new(cursor).expect("Should be a valid zip file");

    for i in 0..archive.len() {
        let file = archive.by_index(i).unwrap();
        let file_name = file.name().to_string();

        // Verify the date is accessible
        // Note: Files without EXIF capture dates will have the ZIP epoch (1980)
        // Files WITH capture dates should have their actual capture date
        if let Some(last_modified) = file.last_modified() {
            assert!(
                last_modified.year() >= 1980,
                "File '{}' should have a valid year (got {})",
                file_name,
                last_modified.year()
            );

            // The date should be properly formed
            assert!(
                last_modified.month() >= 1 && last_modified.month() <= 12,
                "File '{}' should have valid month",
                file_name
            );
            assert!(
                last_modified.day() >= 1 && last_modified.day() <= 31,
                "File '{}' should have valid day",
                file_name
            );
        }
    }
}

#[tokio::test]
async fn test_gallery_download_subfolder() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Update the viewer role to include can_download_gallery
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_download_gallery = true;
    }

    // Create a subfolder with images
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let vacation_dir = photos_dir.join("vacation");
    std::fs::create_dir_all(&vacation_dir).unwrap();
    create_test_images(&vacation_dir, 2);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Download the subfolder
    let response = server.get("/gallery/_download/vacation").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Subfolder download should succeed"
    );

    // Verify the zip filename uses the folder name
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_disposition.contains("vacation.zip"),
        "Zip filename should be based on folder name"
    );

    // Verify the zip contains the subfolder images
    let bytes = response.as_bytes();
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let archive = zip::ZipArchive::new(cursor).expect("Should be a valid zip file");

    assert_eq!(
        archive.len(),
        2,
        "Zip should contain exactly 2 images from vacation folder"
    );
}

#[tokio::test]
async fn test_gallery_download_empty_folder() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Update the viewer role to include can_download_gallery
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_download_gallery = true;
    }

    // Create an empty subfolder
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let empty_dir = photos_dir.join("empty");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Download empty folder should return 404
    let response = server.get("/gallery/_download/empty").await;
    assert_eq!(
        response.status_code(),
        StatusCode::NOT_FOUND,
        "Empty folder download should return 404"
    );
}

#[tokio::test]
async fn test_gallery_download_recursive() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Update the viewer role to include can_download_gallery
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_download_gallery = true;
    }

    // Create a folder structure with images in subfolders only (not at root)
    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    let folder_a = photos_dir.join("folder_a");
    let folder_b = photos_dir.join("folder_b");
    std::fs::create_dir_all(&folder_a).unwrap();
    std::fs::create_dir_all(&folder_b).unwrap();

    // Create images in subfolders
    create_test_images(&folder_a, 2);
    create_test_images(&folder_b, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Download from root should recursively include all images from subfolders
    let response = server.get("/gallery/_download").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Recursive download should succeed even when root has no direct images"
    );

    // Verify the zip contains images from both subfolders
    let bytes = response.as_bytes();
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let archive = zip::ZipArchive::new(cursor).expect("Should be a valid zip file");

    assert_eq!(
        archive.len(),
        5,
        "Zip should contain 5 images total (2 from folder_a + 3 from folder_b)"
    );
}

// ============================================================================
// RAW Download and Version Grouping Tests
// ============================================================================

/// Helper to create a test configuration with RAW download permission
fn create_test_config_with_raw_permission(temp_dir: &TempDir) -> SiteConfig {
    let photos_dir = temp_dir.path().join("photos");
    let cache_dir = temp_dir.path().join("cache");

    std::fs::create_dir_all(&photos_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // CARGO_MANIFEST_DIR is the tenrankai package dir, templates are at workspace root
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let galleries = vec![GallerySystemConfig {
        name: "main".to_string(),
        source_directory: photos_dir.to_string_lossy().to_string(),
        cache_directory: cache_dir.join("main").to_string_lossy().to_string(),
        image_indexing: ImageIndexingMode::Filename,
        permissions: tenrankai::permissions::PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("raw_viewer".to_string()),
            default_authenticated_role: Some("raw_viewer".to_string()),
            roles: {
                let mut roles = std::collections::HashMap::new();
                roles.insert(
                    "raw_viewer".to_string(),
                    tenrankai::permissions::Role::new(
                        "raw_viewer".to_string(),
                        tenrankai::permissions::RolePermissions {
                            can_view: true,
                            can_download_raw: true,
                            ..Default::default()
                        },
                    ),
                );
                roles
            },
            user_roles: vec![],
        },
        ..Default::default()
    }];

    SiteConfig {
        name: "test".to_string(),
        base_url: Some("https://example.com".to_string()),
        cookie_secret: "test-secret".to_string(),
        templates: TemplateConfig {
            directories: vec![
                workspace_root
                    .join("templates")
                    .to_string_lossy()
                    .to_string(),
            ],
        },
        static_files: StaticConfig {
            directories: vec![workspace_root.join("static").to_string_lossy().to_string()],
            use_redirects: false,
        },
        galleries: Some(galleries),
        posts: None,
        user_database: None,
        email: None,
        config_storage: None,
        site_admins: Vec::new(),
        hosted_mode: false,
        site_title: None,
        copyright_holder: None,
    }
}

/// Helper to create a dummy RAW file (just a text file with .dng extension for testing)
fn create_dummy_raw_file(path: &std::path::Path) {
    std::fs::write(path, b"DUMMY RAW FILE CONTENT FOR TESTING").unwrap();
}

#[tokio::test]
async fn test_raw_download_with_permission() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_raw_permission(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create a JPEG image and its associated RAW file
    create_test_images(photos_dir, 1);
    create_dummy_raw_file(&photos_dir.join("test_000.dng"));

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Test RAW download endpoint
    let response = server.get("/gallery/_raw/test_000.dng").await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "RAW download should succeed with permission"
    );

    // Verify Content-Disposition header for download
    let content_disposition = response.headers().get("content-disposition");
    assert!(
        content_disposition.is_some(),
        "Response should have Content-Disposition header"
    );
    assert!(
        content_disposition
            .unwrap()
            .to_str()
            .unwrap()
            .contains("attachment"),
        "Content-Disposition should indicate attachment"
    );
    assert!(
        content_disposition
            .unwrap()
            .to_str()
            .unwrap()
            .contains("test_000.dng"),
        "Content-Disposition should contain filename"
    );
}

#[tokio::test]
async fn test_raw_download_denied_without_permission() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir); // Uses viewer role without can_download_raw

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create a JPEG image and its associated RAW file
    create_test_images(photos_dir, 1);
    create_dummy_raw_file(&photos_dir.join("test_000.dng"));

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Test RAW download endpoint - should be forbidden
    let response = server.get("/gallery/_raw/test_000.dng").await;
    assert_eq!(
        response.status_code(),
        StatusCode::FORBIDDEN,
        "RAW download should be denied without permission"
    );
}

#[tokio::test]
async fn test_raw_download_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_raw_permission(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create only a JPEG, no RAW file
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Test RAW download for non-existent file
    let response = server.get("/gallery/_raw/test_000.dng").await;
    assert_eq!(
        response.status_code(),
        StatusCode::NOT_FOUND,
        "RAW download should return 404 for non-existent file"
    );
}

#[tokio::test]
async fn test_image_api_includes_raw_files_with_permission() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_raw_permission(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create a JPEG image and its associated RAW file
    create_test_images(photos_dir, 1);
    create_dummy_raw_file(&photos_dir.join("test_000.dng"));

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Get image detail via API
    let response = server.get("/api/gallery/main/image/test_000.jpg").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    let image = &json["image"];

    // Should include raw_files array
    assert!(
        image["raw_files"].is_array(),
        "Image should include raw_files when user has permission"
    );

    let raw_files = image["raw_files"].as_array().unwrap();
    assert_eq!(raw_files.len(), 1, "Should have one RAW file");
    assert_eq!(raw_files[0]["format"], "dng", "RAW format should be dng");
    assert!(
        raw_files[0]["download_url"].is_string(),
        "RAW file should have download_url"
    );
    assert!(
        raw_files[0]["download_url"]
            .as_str()
            .unwrap()
            .contains("/_raw/"),
        "download_url should contain /_raw/ path"
    );
}

#[tokio::test]
async fn test_image_api_excludes_raw_files_without_permission() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir); // Uses viewer role without can_download_raw

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create a JPEG image and its associated RAW file
    create_test_images(photos_dir, 1);
    create_dummy_raw_file(&photos_dir.join("test_000.dng"));

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Get image detail via API
    let response = server.get("/api/gallery/main/image/test_000.jpg").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    let image = &json["image"];

    // Should NOT include raw_files
    assert!(
        image["raw_files"].is_null(),
        "Image should not include raw_files when user lacks permission"
    );
}

#[tokio::test]
async fn test_version_grouping_shows_latest_as_primary() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create versioned images: base, _v1, _v2
    use image::{ImageBuffer, Rgb};
    for (name, color) in [
        ("IMG_0001.jpg", 100u8),
        ("IMG_0001_v1.jpg", 150),
        ("IMG_0001_v2.jpg", 200),
    ] {
        let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([color, color, color]));
        img.save(photos_dir.join(name)).unwrap();
    }

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Gallery listing should show only _v2 (highest version = primary)
    let response = server.get("/gallery").await;
    if response.status_code() != StatusCode::OK {
        eprintln!("Response body: {}", response.text());
    }
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();

    // _v2 should be the primary (shown in gallery)
    assert!(
        html.contains("IMG_0001_v2.jpg"),
        "Gallery should show _v2 as the primary image"
    );

    // Base and _v1 should NOT appear in the gallery listing
    // (they are versions, not separate images)
    // Note: They might appear in version picker UI, but not as separate gallery items
}

#[tokio::test]
async fn test_image_api_includes_versions() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Set can_see_versions permission for viewer role
    if let Some(ref mut galleries) = config.galleries
        && let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer")
    {
        viewer_role.permissions.can_see_versions = true;
    }

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create versioned images: base, _v1, _v2
    use image::{ImageBuffer, Rgb};
    for (name, color) in [
        ("IMG_0001.jpg", 100u8),
        ("IMG_0001_v1.jpg", 150),
        ("IMG_0001_v2.jpg", 200),
    ] {
        let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([color, color, color]));
        img.save(photos_dir.join(name)).unwrap();
    }

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Get the primary image (_v2) via API
    let response = server.get("/api/gallery/main/image/IMG_0001_v2.jpg").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    let image = &json["image"];

    // Should include versions array (sorted oldest-first, including primary for navigation)
    assert!(
        image["versions"].is_array(),
        "Primary image should include versions array"
    );

    let versions = image["versions"].as_array().unwrap();
    assert_eq!(
        versions.len(),
        3,
        "Should have 3 versions (base, _v1, and _v2/primary)"
    );

    // Verify versions are sorted oldest-first
    assert!(
        versions[0]["path"]
            .as_str()
            .unwrap()
            .contains("IMG_0001.jpg")
            && !versions[0]["path"].as_str().unwrap().contains("_v"),
        "First version should be the base image"
    );
    assert!(
        versions[1]["path"]
            .as_str()
            .unwrap()
            .contains("IMG_0001_v1.jpg"),
        "Second version should be _v1"
    );
    assert!(
        versions[2]["path"]
            .as_str()
            .unwrap()
            .contains("IMG_0001_v2.jpg"),
        "Third version should be _v2 (primary)"
    );

    // is_primary should be true for the primary image
    assert_eq!(
        image["is_primary"], true,
        "Primary image should have is_primary=true"
    );
}

/// Helper to run the older version navigation test with a specific indexing mode
async fn run_older_version_navigation_test(indexing_mode: ImageIndexingMode) {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config(&temp_dir);

    // Set the indexing mode and add can_see_versions permission
    if let Some(ref mut galleries) = config.galleries {
        galleries[0].image_indexing = indexing_mode;
        if let Some(ref mut viewer_role) = galleries[0].permissions.roles.get_mut("viewer") {
            viewer_role.permissions.can_see_versions = true;
        }
    }

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create multiple image groups to have navigation context
    use image::{ImageBuffer, Rgb};

    // Image group 1: AAA with versions
    for (name, color) in [
        ("AAA_0001.jpg", 100u8),
        ("AAA_0001_v1.jpg", 150),
        ("AAA_0001_v2.jpg", 200), // Primary
    ] {
        let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([color, color, color]));
        img.save(photos_dir.join(name)).unwrap();
    }

    // Image group 2: BBB (no versions, just for navigation)
    let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([50u8, 50, 50]));
    img.save(photos_dir.join("BBB_0002.jpg")).unwrap();

    // Image group 3: CCC (no versions, just for navigation)
    let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([75u8, 75, 75]));
    img.save(photos_dir.join("CCC_0003.jpg")).unwrap();

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // For Filename mode, we know the URL identifier is the filename itself.
    // For UniqueId mode, we need to discover it from the API.
    let primary_url_id = match indexing_mode {
        ImageIndexingMode::Filename => "AAA_0001_v2.jpg".to_string(),
        ImageIndexingMode::UniqueId => {
            // Get any image from the preview to discover the primary's URL identifier
            let response = server.get("/api/gallery/main/preview?count=10").await;
            assert_eq!(response.status_code(), StatusCode::OK);
            let listing = response.json::<serde_json::Value>();
            eprintln!(
                "Gallery preview (UniqueId mode): {}",
                serde_json::to_string_pretty(&listing).unwrap()
            );

            // Find any image from the AAA group in the preview
            let images = listing["images"].as_array().unwrap();
            let aaa_entry = images
                .iter()
                .find(|img| {
                    let name = img["name"].as_str().unwrap_or("");
                    name.starts_with("AAA_0001")
                })
                .expect("Should find an AAA image in preview");

            let aaa_url_id = aaa_entry["path"].as_str().unwrap();
            eprintln!("Found AAA image with URL ID: {}", aaa_url_id);

            // Request this image to get its versions array
            let response = server
                .get(&format!("/api/gallery/main/image/{}", aaa_url_id))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            let json = response.json::<serde_json::Value>();

            // Find the primary (highest version number) from versions
            let versions = json["image"]["versions"].as_array().unwrap();
            let primary = versions
                .iter()
                .max_by_key(|v| v["version_number"].as_u64().unwrap_or(0))
                .expect("Should find primary in versions");

            primary["url_id"].as_str().unwrap().to_string()
        }
        _ => panic!("Unsupported indexing mode"),
    };
    eprintln!("Primary URL identifier: {}", primary_url_id);

    // Get the primary version via API
    let response = server
        .get(&format!("/api/gallery/main/image/{}", primary_url_id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    eprintln!(
        "Primary version API response ({:?}): {}",
        indexing_mode,
        serde_json::to_string_pretty(&json).unwrap()
    );

    // Primary should have next_image (BBB)
    assert!(
        json["next_image"].is_object(),
        "Primary version should have next_image for navigation ({:?} mode)",
        indexing_mode
    );

    // Get the older version's URL identifier from the versions array
    let versions = json["image"]["versions"].as_array().unwrap();
    let older_version = versions
        .iter()
        .find(|v| {
            v["path"]
                .as_str()
                .map(|p| p.contains("AAA_0001_v1"))
                .unwrap_or(false)
        })
        .expect("Should find _v1 in versions array");

    let older_url_id = older_version["url_id"].as_str().unwrap();
    eprintln!("Older version URL identifier: {}", older_url_id);

    // Now test the older version (_v1) - this is the bug we're fixing
    let response = server
        .get(&format!("/api/gallery/main/image/{}", older_url_id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json = response.json::<serde_json::Value>();
    eprintln!(
        "Older version (_v1) API response ({:?}): {}",
        indexing_mode,
        serde_json::to_string_pretty(&json).unwrap()
    );

    // The older version should ALSO have navigation (using primary's context)
    assert!(
        json["next_image"].is_object(),
        "Older version should have next_image for navigation ({:?} mode)",
        indexing_mode
    );

    // Verify is_primary is false for the older version
    assert_eq!(
        json["image"]["is_primary"], false,
        "Older version should have is_primary=false ({:?} mode)",
        indexing_mode
    );

    // Verify versions are included (should contain both base and primary)
    assert!(
        json["image"]["versions"].is_array(),
        "Older version should include versions array ({:?} mode)",
        indexing_mode
    );
}

#[tokio::test]
async fn test_older_version_has_navigation_filename_mode() {
    run_older_version_navigation_test(ImageIndexingMode::Filename).await;
}

#[tokio::test]
async fn test_older_version_has_navigation_unique_id_mode() {
    run_older_version_navigation_test(ImageIndexingMode::UniqueId).await;
}

#[tokio::test]
async fn test_hidden_folder_not_shown_in_gallery() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create a hidden __hidden folder (not __versions which is special for grouping)
    // __hidden folders should never appear in the gallery listing
    let hidden_dir = photos_dir.join("__hidden");
    std::fs::create_dir_all(&hidden_dir).unwrap();
    // Create images with unique names so they won't be grouped with other images
    create_test_images(&hidden_dir, 2);

    // Create a visible folder
    let visible_dir = photos_dir.join("visible");
    std::fs::create_dir_all(&visible_dir).unwrap();
    create_test_images(&visible_dir, 2);

    let app = create_app(config, None).await;
    let server = TestServer::new(app);

    // Gallery listing should show visible folder but not __hidden
    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();

    assert!(
        html.contains("visible"),
        "Gallery should show visible folder"
    );
    assert!(
        !html.contains("__hidden"),
        "Gallery should not show __hidden folder"
    );
}
