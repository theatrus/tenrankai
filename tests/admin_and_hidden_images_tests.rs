use axum::http::{HeaderValue, StatusCode, header};
use axum_test::TestServer;
use base64::{Engine, engine::general_purpose};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tempfile::TempDir;
use tenrankai::{
    GallerySystemConfig, ImageIndexingMode, StaticConfig, TemplateConfig, create_app,
    site::SiteConfig,
};

const TEST_COOKIE_SECRET: &str = "test-secret-key-for-testing-purposes";

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
        image_indexing: indexing_mode,
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
        site_admins: Vec::new(),
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

// ============================================================================
// Admin API Tests - Helpers
// ============================================================================

/// Create a signed authentication cookie for testing
fn create_auth_cookie(username: &str, secret: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(username.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = general_purpose::URL_SAFE_NO_PAD.encode(signature);
    format!("auth={}:{}", username, signature_b64)
}

/// Helper to create a test user database TOML file
fn create_user_database(temp_dir: &TempDir, users: &[(&str, &str)]) -> String {
    let path = temp_dir.path().join("users.toml");
    let mut content = String::from("[users]\n");

    for (username, email) in users {
        content.push_str(&format!(
            r#"
[users.{}]
email = "{}"
"#,
            username, email
        ));
    }

    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

/// Helper to create config with user database for admin tests
fn create_admin_config_with_users(temp_dir: &TempDir) -> SiteConfig {
    let mut config = create_admin_test_config(temp_dir, ImageIndexingMode::Filename);
    let user_db = create_user_database(temp_dir, &[("testadmin", "admin@example.com")]);
    config.user_database = Some(user_db);
    config
}

// ============================================================================
// Admin API Tests - Authentication
// ============================================================================

#[tokio::test]
async fn test_admin_api_requires_authentication() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Admin endpoints should require authentication
    let response = server.get("/_admin/api/galleries").await;
    assert_eq!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "Admin API should require authentication"
    );

    let response = server.get("/_admin/api/roles").await;
    assert_eq!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "Admin API should require authentication"
    );
}

#[tokio::test]
async fn test_admin_api_requires_admin_role() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_admin_test_config(&temp_dir, ImageIndexingMode::Filename);

    // Create user database with a regular user (not admin)
    let user_db = create_user_database(&temp_dir, &[("regularuser", "user@example.com")]);
    config.user_database = Some(user_db);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Authenticated but non-admin user should be forbidden
    let cookie = create_auth_cookie("regularuser", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::FORBIDDEN,
        "Non-admin user should be forbidden"
    );
}

#[tokio::test]
async fn test_admin_api_allows_admin_user() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    // Admin user should have access
    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Admin user should have access"
    );
}

// ============================================================================
// Admin API Tests - Gallery Endpoints
// ============================================================================

#[tokio::test]
async fn test_admin_list_galleries() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let galleries = json.get("galleries").unwrap().as_array().unwrap();
    assert_eq!(galleries.len(), 1);
    assert_eq!(galleries[0].get("name").unwrap().as_str().unwrap(), "main");
}

#[tokio::test]
async fn test_admin_get_gallery() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries/main")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    assert_eq!(json.get("name").unwrap().as_str().unwrap(), "main");
    assert!(json.get("permissions").is_some());
}

#[tokio::test]
async fn test_admin_get_nonexistent_gallery() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries/nonexistent")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Admin API Tests - Role Endpoints
// ============================================================================

#[tokio::test]
async fn test_admin_list_roles() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/roles")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let roles = json.get("roles").unwrap().as_array().unwrap();

    // Should have built-in roles
    let role_names: Vec<&str> = roles
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(role_names.contains(&"viewer"));
    assert!(role_names.contains(&"contributor"));
    assert!(role_names.contains(&"admin"));
}

#[tokio::test]
async fn test_admin_get_role() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/roles/viewer")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    assert_eq!(json.get("name").unwrap().as_str().unwrap(), "viewer");
    assert!(json.get("permissions").is_some());
    assert_eq!(json.get("is_builtin").unwrap().as_bool().unwrap(), true);
}

#[tokio::test]
async fn test_admin_get_nonexistent_role() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/roles/nonexistent")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Admin API Tests - Permission Groups
// ============================================================================

#[tokio::test]
async fn test_admin_list_permission_groups() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/permission-groups")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let groups = json.get("groups").unwrap().as_array().unwrap();
    assert!(!groups.is_empty(), "Should have permission groups");

    // Check that groups have required fields
    for group in groups {
        assert!(group.get("name").is_some());
        assert!(group.get("description").is_some());
        assert!(group.get("permissions").is_some());
    }
}

// ============================================================================
// Admin API Tests - Hide/Unhide Images
// ============================================================================

#[tokio::test]
async fn test_admin_hide_images_api_accepts_json() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);

    // Test that the endpoint accepts JSON and processes it (even if it returns an error
    // due to missing config storage, it should be a different error than 401/400)
    let hide_request = serde_json::json!({
        "paths": [filenames[1].clone()],
        "hide": true
    });

    let response = server
        .post("/_admin/api/galleries/main/folders/_root/images/hide")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .json(&hide_request)
        .await;

    // The endpoint should accept the request (not 401 or 400)
    // It may return 500 if config storage is not configured, which is expected
    // in this minimal test setup
    assert!(
        response.status_code() != StatusCode::UNAUTHORIZED,
        "Admin should be authenticated"
    );
    assert!(
        response.status_code() != StatusCode::BAD_REQUEST,
        "JSON request should be valid"
    );
}

#[tokio::test]
async fn test_admin_hide_images_requires_auth() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    let filenames = create_test_images(photos_dir, 3);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let hide_request = serde_json::json!({
        "paths": [filenames[1].clone()],
        "hide": true
    });

    // Without authentication
    let response = server
        .post("/_admin/api/galleries/main/folders/_root/images/hide")
        .json(&hide_request)
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "Hide API should require authentication"
    );
}

// ============================================================================
// Admin API Tests - User Role Validation
// ============================================================================

#[tokio::test]
async fn test_admin_gallery_permissions_include_user_roles() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries/main")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let permissions = json.get("permissions").unwrap();
    let user_roles = permissions.get("user_roles").unwrap().as_array().unwrap();

    // Should include our testadmin user
    let has_testadmin = user_roles
        .iter()
        .any(|ur| ur.get("username").unwrap().as_str().unwrap() == "testadmin");
    assert!(has_testadmin, "Should include testadmin in user_roles");
}

#[tokio::test]
async fn test_admin_gallery_permissions_include_roles() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_admin_config_with_users(&temp_dir);

    let photos_dir = std::path::Path::new(&config.galleries.as_ref().unwrap()[0].source_directory);
    create_test_images(photos_dir, 1);

    let app = create_app(config, None).await;
    let server = TestServer::new(app).unwrap();

    let cookie = create_auth_cookie("testadmin", TEST_COOKIE_SECRET);
    let response = server
        .get("/_admin/api/galleries/main")
        .add_header(header::COOKIE, HeaderValue::from_str(&cookie).unwrap())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let permissions = json.get("permissions").unwrap();
    let roles = permissions.get("roles").unwrap().as_object().unwrap();

    // Should include our configured roles
    assert!(roles.contains_key("viewer"), "Should have viewer role");
    assert!(roles.contains_key("admin"), "Should have admin role");
}
