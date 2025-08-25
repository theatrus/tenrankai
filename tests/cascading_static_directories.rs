use axum_test::TestServer;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_cascading_static_directories() {
    // Create temporary directories for testing
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();

    // Create multiple static directories
    let static_dir1 = base_dir.join("static1");
    let static_dir2 = base_dir.join("static2");
    let templates_dir = base_dir.join("templates");

    fs::create_dir_all(&static_dir1).await.unwrap();
    fs::create_dir_all(&static_dir2).await.unwrap();
    fs::create_dir_all(&templates_dir).await.unwrap();

    // Create files in both directories
    fs::write(static_dir1.join("shared.css"), "/* from static1 */")
        .await
        .unwrap();
    fs::write(static_dir1.join("file1.js"), "// file1 from static1")
        .await
        .unwrap();

    fs::write(static_dir2.join("shared.css"), "/* from static2 */")
        .await
        .unwrap();
    fs::write(static_dir2.join("file2.js"), "// file2 from static2")
        .await
        .unwrap();

    // Create a minimal template
    fs::write(templates_dir.join("index.html.liquid"), "<h1>Test</h1>")
        .await
        .unwrap();

    // Configure with cascading directories (static_dir1 has precedence)
    let mut config = tenrankai::Config::default();
    config.static_files.directories = vec![static_dir1, static_dir2];
    config.templates.directory = templates_dir;

    // Create the app
    let app = tenrankai::create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test that shared.css comes from static_dir1 (first in list)
    let shared_response = server.get("/static/shared.css").await;
    shared_response.assert_status_ok();
    let shared_content = shared_response.text();
    assert_eq!(
        shared_content, "/* from static1 */",
        "Should serve from first directory"
    );

    // Test that file1.js is served (only exists in static_dir1)
    let file1_response = server.get("/static/file1.js").await;
    file1_response.assert_status_ok();
    let file1_content = file1_response.text();
    assert_eq!(file1_content, "// file1 from static1");

    // Test that file2.js is served (only exists in static_dir2)
    let file2_response = server.get("/static/file2.js").await;
    file2_response.assert_status_ok();
    let file2_content = file2_response.text();
    assert_eq!(file2_content, "// file2 from static2");

    // Test that non-existent file returns 404
    let missing_response = server.get("/static/missing.css").await;
    missing_response.assert_status_not_found();
}

#[tokio::test]
async fn test_favicon_cascading_directories() {
    // Create temporary directories for testing
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();

    // Create multiple static directories
    let static_dir1 = base_dir.join("static1");
    let static_dir2 = base_dir.join("static2");
    let templates_dir = base_dir.join("templates");

    fs::create_dir_all(&static_dir1).await.unwrap();
    fs::create_dir_all(&static_dir2).await.unwrap();
    fs::create_dir_all(&templates_dir).await.unwrap();

    // Create favicon.svg only in the second directory
    let favicon_content = r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">
        <circle cx="16" cy="16" r="14" fill="blue"/>
        <text x="16" y="21" text-anchor="middle" fill="white" font-family="Arial" font-size="18" font-weight="bold">T</text>
    </svg>"#;
    fs::write(static_dir2.join("favicon.svg"), favicon_content)
        .await
        .unwrap();

    // Create a minimal template
    fs::write(templates_dir.join("index.html.liquid"), "<h1>Test</h1>")
        .await
        .unwrap();

    // Configure with cascading directories
    let mut config = tenrankai::Config::default();
    config.static_files.directories = vec![static_dir1, static_dir2];
    config.templates.directory = templates_dir;

    // Create the app
    let app = tenrankai::create_app(config).await;
    let server = TestServer::new(app).unwrap();

    // Test that favicon.ico can be generated from the favicon.svg in static_dir2
    let favicon_response = server.get("/favicon.ico").await;
    favicon_response.assert_status_ok();
    assert_eq!(
        favicon_response.header("content-type"),
        "image/vnd.microsoft.icon"
    );

    // Test that PNG favicons can be generated
    let png16_response = server.get("/favicon-16x16.png").await;
    png16_response.assert_status_ok();
    assert_eq!(png16_response.header("content-type"), "image/png");
}
