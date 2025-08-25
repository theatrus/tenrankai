use axum::http::StatusCode;
use axum_test::TestServer;
use std::fs;
use tempfile::TempDir;
use tenrankai::{Config, create_app};

async fn setup_test_server() -> (TempDir, TestServer) {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let templates_dir = temp_dir.path().join("templates");
    let static_dir = temp_dir.path().join("static");
    let gallery_dir = temp_dir.path().join("gallery");
    let cache_dir = temp_dir.path().join("cache");

    fs::create_dir_all(&templates_dir).unwrap();
    fs::create_dir_all(templates_dir.join("pages")).unwrap();
    fs::create_dir_all(templates_dir.join("partials")).unwrap();
    fs::create_dir_all(&static_dir).unwrap();
    fs::create_dir_all(&gallery_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    // Create test templates
    let header_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>{% if page_title %}{{ page_title }} - {% endif %}Test Site</title>
</head>
<body>
    <header><h1>Test Site</h1></header>
    <main>"#;
    fs::write(
        templates_dir.join("partials/_header.html.liquid"),
        header_content,
    )
    .unwrap();

    let footer_content = r#"    </main>
    <footer><p>&copy; {{ current_year }} Test</p></footer>
</body>
</html>"#;
    fs::write(
        templates_dir.join("partials/_footer.html.liquid"),
        footer_content,
    )
    .unwrap();

    let index_content = r#"{% assign page_title = "Home" %}
{% include "_header.html.liquid" %}
<h2>Welcome</h2>
<p>Test home page</p>
{% include "_footer.html.liquid" %}"#;
    fs::write(templates_dir.join("pages/index.html.liquid"), index_content).unwrap();

    let gallery_content = r#"{% assign page_title = "Gallery" %}
{% include "_header.html.liquid" %}
<h2>Gallery</h2>
<div class="gallery">Test gallery</div>
{% include "_footer.html.liquid" %}"#;
    // Create modules directory for module templates
    let modules_dir = templates_dir.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    fs::write(modules_dir.join("gallery.html.liquid"), gallery_content).unwrap();

    // Create test config
    let config = Config {
        server: tenrankai::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0, // Let OS assign port
        },
        app: tenrankai::AppConfig {
            name: "TestServer".to_string(),
            log_level: "error".to_string(),
            cookie_secret: "test-cookie-secret".to_string(),
            copyright_holder: None,
            base_url: Some("http://localhost:3000".to_string()),
            user_database: None,
        },
        templates: tenrankai::TemplateConfig {
            directory: templates_dir,
        },
        static_files: tenrankai::StaticConfig {
            directories: vec![static_dir],
        },
        galleries: Some(vec![tenrankai::GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: gallery_dir,
            cache_directory: cache_dir,
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
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
        }]),
        posts: None,
        email: None,
    };

    let app = create_app(config).await;
    let server = TestServer::new(app.into_make_service()).unwrap();

    (temp_dir, server)
}

#[tokio::test]
async fn test_index_page_renders() {
    let (_temp_dir, server) = setup_test_server().await;

    let response = server.get("/").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    assert!(html.contains("<title>Home - Test Site</title>"));
    assert!(html.contains("Welcome"));
    assert!(html.contains("Test home page"));
}

#[tokio::test]
async fn test_gallery_page_renders() {
    let (_temp_dir, server) = setup_test_server().await;

    let response = server.get("/gallery").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    assert!(html.contains("<title>Gallery - Test Site</title>"));
    assert!(html.contains("Test gallery"));
}

#[tokio::test]
async fn test_404_page_renders() {
    let (_temp_dir, server) = setup_test_server().await;

    // Create 404 template
    let templates_dir = _temp_dir.path().join("templates");
    let not_found_content = r#"{% assign page_title = "Not Found" %}
{% include "_header.html.liquid" %}
<h1>404 - Page Not Found</h1>
{% include "_footer.html.liquid" %}"#;
    fs::write(
        templates_dir.join("pages/404.html.liquid"),
        not_found_content,
    )
    .unwrap();

    let response = server.get("/nonexistent").await;

    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    let html = response.text();
    assert!(html.contains("404"));
}

#[tokio::test]
async fn test_template_with_missing_include_fails_gracefully() {
    let (_temp_dir, server) = setup_test_server().await;

    // Create a template with a bad include
    let templates_dir = _temp_dir.path().join("templates");
    let bad_content = r#"{% include "_nonexistent.html.liquid" %}"#;
    fs::write(templates_dir.join("pages/bad.html.liquid"), bad_content).unwrap();

    let response = server.get("/bad").await;

    // Should return 500 since template fails to render with a missing include
    assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}
