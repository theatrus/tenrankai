use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use tenrankai::{
    GallerySystemConfig, ImageIndexingMode, StaticConfig, TemplateConfig, create_app,
    site::SiteConfig,
};

/// Helper to create a test configuration with admin capabilities
fn create_admin_test_config(temp_dir: &TempDir, indexing_mode: ImageIndexingMode) -> SiteConfig {
    let photos_dir = temp_dir.path().join("photos");
    let cache_dir = temp_dir.path().join("cache");

    std::fs::create_dir_all(&photos_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let galleries = vec![GallerySystemConfig {
        name: "main".to_string(),
        source_directory: photos_dir.to_string_lossy().to_string(),
        cache_directory: cache_dir.join("main").to_string_lossy().to_string(),
        images_per_page: 20,
        image_indexing: indexing_mode,
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
                            can_see_hidden: false,
                            ..Default::default()
                        },
                    ),
                );
                roles.insert(
                    "admin".to_string(),
                    tenrankai::permissions::Role::new(
                        "admin".to_string(),
                        tenrankai::permissions::RolePermissions {
                            can_view: true,
                            can_see_hidden: true,
                            owner_access: true,
                            ..Default::default()
                        },
                    ),
                );
                roles
            },
            user_roles: vec![tenrankai::permissions::UserRole::new(
                "testadmin".to_string(),
                vec!["admin".to_string()],
            )],
        },
        ..Default::default()
    }];

    SiteConfig {
        name: "test".to_string(),
        base_url: Some("https://example.com".to_string()),
        cookie_secret: "test-secret-key-for-testing-purposes".to_string(),
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
    }
}

/// Helper to create test images
fn create_test_images(dir: &std::path::Path, count: usize) -> Vec<String> {
    use image::{ImageBuffer, Rgb};

    let mut filenames = Vec::new();
    for i in 0..count {
        let img = ImageBuffer::from_fn(100, 100, |x, y| {
            Rgb([(x * 2) as u8, (y * 2) as u8, (i * 50) as u8])
        });

        let filename = format!("test_{:03}.jpg", i);
        let path = dir.join(&filename);
        img.save(&path).unwrap();
        filenames.push(filename);
    }
    filenames
}

/// Helper to create a folder with hidden images configuration
fn create_folder_with_hidden_images(dir: &std::path::Path, hidden_images: &[&str]) {
    std::fs::create_dir_all(dir).unwrap();

    let hidden_list: Vec<String> = hidden_images.iter().map(|s| format!("\"{}\"", s)).collect();
    let config = format!(
        r#"+++
hidden = false
hidden_images = [{}]
+++"#,
        hidden_list.join(", ")
    );

    std::fs::write(dir.join("_folder.md"), config).unwrap();
}

// ============================================================================
// Hidden Images Tests - Gallery Listing
// ============================================================================

#[tokio::test]
async fn test_hidden_images_not_visible_in_gallery_listing() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create test images
    let filenames = create_test_images(photos_dir, 3);

    // Mark one image as hidden
    create_folder_with_hidden_images(photos_dir, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Gallery listing should not show hidden image for unauthenticated users
    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains(&filenames[0]), "Should show first image");
    assert!(
        !html.contains(&filenames[1]),
        "Should NOT show hidden image"
    );
    assert!(html.contains(&filenames[2]), "Should show third image");
}

#[tokio::test]
async fn test_hidden_images_in_subfolder() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let subfolder = photos_dir.join("vacation");
    std::fs::create_dir_all(&subfolder).unwrap();

    let filenames = create_test_images(&subfolder, 3);
    create_folder_with_hidden_images(&subfolder, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Gallery listing for subfolder should not show hidden image
    let response = server.get("/gallery/vacation").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains(&filenames[0]));
    assert!(!html.contains(&filenames[1])); // Hidden
    assert!(html.contains(&filenames[2]));
}

// ============================================================================
// Hidden Images Tests - Direct Access Blocking
// ============================================================================

#[tokio::test]
async fn test_hidden_image_direct_access_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    let filenames = create_test_images(photos_dir, 3);
    create_folder_with_hidden_images(photos_dir, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Direct access to hidden image should return 404 for unauthenticated users
    let response = server
        .get(&format!("/gallery/detail/{}", filenames[1]))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::NOT_FOUND,
        "Hidden image should not be accessible"
    );

    // Non-hidden images should still be accessible
    let response = server
        .get(&format!("/gallery/detail/{}", filenames[0]))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Non-hidden image should be accessible"
    );
}

#[tokio::test]
async fn test_hidden_image_in_subfolder_direct_access_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let subfolder = photos_dir.join("vacation");
    std::fs::create_dir_all(&subfolder).unwrap();

    let filenames = create_test_images(&subfolder, 3);
    create_folder_with_hidden_images(&subfolder, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Direct access to hidden image in subfolder should be blocked
    let response = server
        .get(&format!("/gallery/detail/vacation/{}", filenames[1]))
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Hidden Images Tests - Navigation Filtering
// ============================================================================

#[tokio::test]
async fn test_hidden_images_filtered_from_navigation() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create test images: 0, 1 (hidden), 2
    let filenames = create_test_images(photos_dir, 3);
    create_folder_with_hidden_images(photos_dir, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // View image 0, next should be image 2 (skipping hidden image 1)
    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[0]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let next_image = json.get("next_image");
    assert!(next_image.is_some(), "Should have next image");

    let next_path = next_image.unwrap().get("path").unwrap().as_str().unwrap();
    assert!(
        next_path.contains(&filenames[2]),
        "Next image should be test_002.jpg, not hidden test_001.jpg. Got: {}",
        next_path
    );
}

#[tokio::test]
async fn test_prev_navigation_skips_hidden() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create test images: 0, 1 (hidden), 2
    let filenames = create_test_images(photos_dir, 3);
    create_folder_with_hidden_images(photos_dir, &[&filenames[1]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // View image 2, prev should be image 0 (skipping hidden image 1)
    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[2]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let prev_image = json.get("prev_image");
    assert!(prev_image.is_some(), "Should have prev image");

    let prev_path = prev_image.unwrap().get("path").unwrap().as_str().unwrap();
    assert!(
        prev_path.contains(&filenames[0]),
        "Prev image should be test_000.jpg, not hidden test_001.jpg. Got: {}",
        prev_path
    );
}

#[tokio::test]
async fn test_first_image_hidden_navigation() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create test images: 0 (hidden), 1, 2
    let filenames = create_test_images(photos_dir, 3);
    create_folder_with_hidden_images(photos_dir, &[&filenames[0]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // View image 1, prev should be None (image 0 is hidden)
    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[1]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let prev_image = json.get("prev_image");
    assert!(
        prev_image.is_none() || prev_image.unwrap().is_null(),
        "Should have no prev image when first image is hidden"
    );
}

#[tokio::test]
async fn test_last_image_hidden_navigation() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);

    // Create test images: 0, 1, 2 (hidden)
    let filenames = create_test_images(photos_dir, 3);
    create_folder_with_hidden_images(photos_dir, &[&filenames[2]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // View image 1, next should be None (image 2 is hidden)
    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[1]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let next_image = json.get("next_image");
    assert!(
        next_image.is_none() || next_image.unwrap().is_null(),
        "Should have no next image when last image is hidden"
    );
}

// ============================================================================
// Image Detail API Tests
// ============================================================================

#[tokio::test]
async fn test_image_detail_api_basic() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[1]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();

    assert!(json.get("image").is_some());
    assert!(json.get("prev_image").is_some());
    assert!(json.get("next_image").is_some());
    assert!(json.get("permissions").is_some());

    // Check navigation
    let prev = json.get("prev_image").unwrap();
    assert!(
        prev.get("path")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("test_000")
    );

    let next = json.get("next_image").unwrap();
    assert!(
        next.get("path")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("test_002")
    );
}

#[tokio::test]
async fn test_image_detail_api_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[0]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let permissions = json.get("permissions").unwrap();

    // Public viewer role permissions
    assert_eq!(
        permissions.get("can_view").unwrap().as_bool().unwrap(),
        true
    );
    assert_eq!(
        permissions
            .get("can_see_hidden")
            .unwrap()
            .as_bool()
            .unwrap(),
        false
    );
    assert_eq!(
        permissions.get("owner_access").unwrap().as_bool().unwrap(),
        false
    );
}

// ============================================================================
// Indexing Mode Tests
// ============================================================================

#[tokio::test]
async fn test_filename_indexing_mode() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Access by filename
    for filename in &filenames {
        let response = server.get(&format!("/gallery/detail/{}", filename)).await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Should access image by filename: {}",
            filename
        );
    }
}

// ============================================================================
// Permissions Tests
// ============================================================================

#[tokio::test]
async fn test_folder_permissions_override() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let private_folder = photos_dir.join("private");
    std::fs::create_dir_all(&private_folder).unwrap();

    create_test_images(&private_folder, 2);

    // Set folder to require authentication (no public role)
    let config_content = r#"+++
[permissions]
public_role = "none"
+++"#;
    std::fs::write(private_folder.join("_folder.md"), config_content).unwrap();

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Unauthenticated access should be forbidden or redirect to login
    let response = server.get("/gallery/private").await;
    // Could be 403 Forbidden or 307 redirect to login
    assert!(
        response.status_code() == StatusCode::FORBIDDEN
            || response.status_code() == StatusCode::TEMPORARY_REDIRECT,
        "Private folder should require authentication, got: {}",
        response.status_code()
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_nonexistent_image() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery/detail/nonexistent.jpg").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_nonexistent_subfolder() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let response = server.get("/gallery/nonexistent-folder").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hidden_image_multiple_in_folder() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 5);

    // Hide images 1 and 3
    create_folder_with_hidden_images(photos_dir, &[&filenames[1], &filenames[3]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Gallery listing should only show images 0, 2, 4
    let response = server.get("/gallery").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let html = response.text();
    assert!(html.contains(&filenames[0]), "Should show image 0");
    assert!(
        !html.contains(&filenames[1]),
        "Should NOT show hidden image 1"
    );
    assert!(html.contains(&filenames[2]), "Should show image 2");
    assert!(
        !html.contains(&filenames[3]),
        "Should NOT show hidden image 3"
    );
    assert!(html.contains(&filenames[4]), "Should show image 4");
}

#[tokio::test]
async fn test_navigation_with_multiple_hidden_images() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 5);

    // Hide images 1 and 2
    create_folder_with_hidden_images(photos_dir, &[&filenames[1], &filenames[2]]);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // View image 0, next should be image 3 (skipping hidden 1 and 2)
    let response = server
        .get(&format!("/api/gallery/main/image/{}", filenames[0]))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let next_image = json.get("next_image");
    assert!(next_image.is_some(), "Should have next image");

    let next_path = next_image.unwrap().get("path").unwrap().as_str().unwrap();
    assert!(
        next_path.contains(&filenames[3]),
        "Next image should be test_003.jpg, skipping hidden test_001.jpg and test_002.jpg. Got: {}",
        next_path
    );
}
