use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_complete_template_with_asset_filter() {
    let temp_dir = TempDir::new().unwrap();
    let template_dir = temp_dir.path().join("templates");
    let static_dir = temp_dir.path().join("static");

    fs::create_dir_all(&template_dir.join("partials"))
        .await
        .unwrap();
    fs::create_dir_all(&static_dir).await.unwrap();

    // Create static files
    fs::write(static_dir.join("style.css"), "body { color: red; }")
        .await
        .unwrap();
    fs::write(static_dir.join("login.css"), ".login { width: 100%; }")
        .await
        .unwrap();
    fs::write(static_dir.join("login.js"), "console.log('login');")
        .await
        .unwrap();

    // Create header partial
    let header_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>{{ page_title | default: "Test" }}</title>
    <link rel="stylesheet" href="{{ 'style.css' | asset_url }}">
    {% if page_css %}
        {% for css_file in page_css %}
        <link rel="stylesheet" href="{{ css_file | asset_url }}">
        {% endfor %}
    {% endif %}
</head>
<body>"#;

    // Create footer partial
    let footer_content = r#"    {% if page_js %}
        {% for js_file in page_js %}
        <script src="{{ js_file | asset_url }}"></script>
        {% endfor %}
    {% endif %}
</body>
</html>"#;

    // Create main template
    let main_template = r#"{% assign page_title = "Login Page" %}
{% assign page_css = "login.css" | split: "," %}
{% assign page_js = "login.js" | split: "," %}
{% include "_header.html.liquid" %}
    <h1>Login</h1>
{% include "_footer.html.liquid" %}"#;

    fs::write(
        template_dir.join("partials").join("_header.html.liquid"),
        header_content,
    )
    .await
    .unwrap();
    fs::write(
        template_dir.join("partials").join("_footer.html.liquid"),
        footer_content,
    )
    .await
    .unwrap();
    fs::write(template_dir.join("login.html.liquid"), main_template)
        .await
        .unwrap();

    // Create and configure template engine
    let mut template_engine = tenrankai::templating::TemplateEngine::new(template_dir);
    let static_handler = tenrankai::static_files::StaticFileHandler::new(vec![static_dir]);

    static_handler.refresh_file_versions().await;
    template_engine.set_static_handler(static_handler);
    template_engine.update_file_versions().await;

    // Render the template
    let globals = liquid::object!({});
    let result = template_engine
        .render_template("login.html.liquid", globals)
        .await
        .unwrap();

    // Verify all assets have version parameters
    assert!(
        result.contains("/static/style.css?v="),
        "style.css should be versioned"
    );
    assert!(
        result.contains("/static/login.css?v="),
        "login.css should be versioned"
    );
    assert!(
        result.contains("/static/login.js?v="),
        "login.js should be versioned"
    );

    // Verify the structure is correct
    assert!(
        result.contains("<title>Login Page</title>"),
        "Title should be set"
    );
    assert!(
        result.contains("<h1>Login</h1>"),
        "Content should be included"
    );

    println!("Rendered template:\n{}", result);
}
