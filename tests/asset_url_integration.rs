use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_asset_url_filter_in_templates() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let template_dir = temp_dir.path().join("templates");
    let static_dir = temp_dir.path().join("static");

    fs::create_dir_all(&template_dir).await.unwrap();
    fs::create_dir_all(&static_dir).await.unwrap();

    // Create test CSS and JS files
    fs::write(static_dir.join("test.css"), "/* test css */")
        .await
        .unwrap();
    fs::write(static_dir.join("test.js"), "// test js")
        .await
        .unwrap();

    // Create a test template using asset_url filter
    let template_content = r#"<!DOCTYPE html>
<html>
<head>
    <link rel="stylesheet" href="{{ 'test.css' | asset_url }}">
</head>
<body>
    <h1>Test Page</h1>
    <script src="{{ 'test.js' | asset_url }}"></script>
</body>
</html>"#;

    fs::write(template_dir.join("test.html.liquid"), template_content)
        .await
        .unwrap();

    // Create template engine and static handler
    let mut template_engine = tenrankai::templating::TemplateEngine::new(template_dir);
    let static_handler = tenrankai::static_files::StaticFileHandler::new(vec![static_dir]);

    // Refresh file versions to pick up our test files
    static_handler.refresh_file_versions().await;

    // Set static handler and update versions
    template_engine.set_static_handler(static_handler);
    template_engine.update_file_versions().await;

    // Render the template
    let globals = liquid::object!({});
    let result = template_engine
        .render_template("test.html.liquid", globals)
        .await
        .unwrap();

    // Verify the output contains versioned URLs
    assert!(
        result.contains("test.css?v="),
        "CSS should have version parameter"
    );
    assert!(
        result.contains("test.js?v="),
        "JS should have version parameter"
    );
    assert!(
        result.contains("/static/test.css?v="),
        "CSS should have full path with version"
    );
    assert!(
        result.contains("/static/test.js?v="),
        "JS should have full path with version"
    );
}

#[tokio::test]
async fn test_asset_url_filter_with_page_css_and_js() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let template_dir = temp_dir.path().join("templates");
    let static_dir = temp_dir.path().join("static");

    fs::create_dir_all(&template_dir).await.unwrap();
    fs::create_dir_all(&static_dir).await.unwrap();

    // Create test CSS and JS files
    fs::write(static_dir.join("page.css"), "/* page css */")
        .await
        .unwrap();
    fs::write(static_dir.join("app.js"), "// app js")
        .await
        .unwrap();

    // Create a template that uses page_css and page_js variables
    let template_content = r#"{% for css in page_css %}
<link rel="stylesheet" href="{{ css | asset_url }}">
{% endfor %}
{% for js in page_js %}
<script src="{{ js | asset_url }}"></script>
{% endfor %}"#;

    fs::write(template_dir.join("page.html.liquid"), template_content)
        .await
        .unwrap();

    // Create template engine and static handler
    let mut template_engine = tenrankai::templating::TemplateEngine::new(template_dir);
    let static_handler = tenrankai::static_files::StaticFileHandler::new(vec![static_dir]);

    // Refresh file versions
    static_handler.refresh_file_versions().await;

    // Set static handler and update versions
    template_engine.set_static_handler(static_handler);
    template_engine.update_file_versions().await;

    // Render with page_css and page_js arrays
    let globals = liquid::object!({
        "page_css": vec!["page.css"],
        "page_js": vec!["app.js"],
    });

    let result = template_engine
        .render_template("page.html.liquid", globals)
        .await
        .unwrap();

    // Verify the output
    assert!(
        result.contains("/static/page.css?v="),
        "page.css should have versioned URL"
    );
    assert!(
        result.contains("/static/app.js?v="),
        "app.js should have versioned URL"
    );
}
