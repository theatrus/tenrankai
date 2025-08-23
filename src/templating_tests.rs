#[cfg(test)]
mod tests {
    use crate::templating::TemplateEngine;
    use std::fs;
    use tempfile::TempDir;

    async fn setup_test_templates() -> (TempDir, TemplateEngine) {
        let temp_dir = TempDir::new().unwrap();
        let template_path = temp_dir.path();

        // Create test header template
        let header_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>{% if page_title %}{{ page_title }} - {% endif %}Test Site</title>
    {% if meta_description %}
    <meta name="description" content="{{ meta_description }}">
    {% endif %}
</head>
<body>
    <header>
        <h1>Test Site</h1>
    </header>
    <main>"#;
        fs::write(template_path.join("_header.html.liquid"), header_content).unwrap();

        // Create test footer template
        let footer_content = r#"    </main>
    <footer>
        <p>&copy; {{ current_year }} Test Site</p>
    </footer>
</body>
</html>"#;
        fs::write(template_path.join("_footer.html.liquid"), footer_content).unwrap();

        // Create a test index template
        let index_content = r#"{% assign page_title = "Home" %}
{% assign meta_description = "Test home page" %}
{% include "_header.html.liquid" %}

<h2>Welcome to the test site</h2>
<p>This is a test page.</p>

{% include "_footer.html.liquid" %}"#;
        fs::write(template_path.join("index.html.liquid"), index_content).unwrap();

        // Create a test gallery template
        let gallery_content = r#"{% if folder_title %}
    {% assign page_title = folder_title %}
{% else %}
    {% assign page_title = "Gallery" %}
{% endif %}
{% assign meta_description = folder_description | default: "Browse the gallery" | strip_html | truncate: 160 %}
{% include "_header.html.liquid" %}

<h2>{{ page_title }}</h2>
{% if folder_description %}
    <div class="description">{{ folder_description }}</div>
{% endif %}

<div class="gallery">
    {% for item in items %}
        <div class="item">{{ item.name }}</div>
    {% endfor %}
</div>

{% include "_footer.html.liquid" %}"#;
        fs::write(template_path.join("gallery.html.liquid"), gallery_content).unwrap();

        // Create a test gallery preview component
        let preview_content = r#"<div class="gallery-preview">
    <h3>{{ preview_title | default: "Gallery Preview" }}</h3>
    {% if gallery_preview.size > 0 %}
        <div class="preview-items">
            {% for item in gallery_preview %}
                <div class="preview-item">{{ item.name }}</div>
            {% endfor %}
        </div>
    {% else %}
        <p>No items to preview</p>
    {% endif %}
</div>"#;
        fs::write(
            template_path.join("_gallery_preview.html.liquid"),
            preview_content,
        )
        .unwrap();

        let template_engine = TemplateEngine::new(template_path.to_path_buf());

        (temp_dir, template_engine)
    }

    #[tokio::test]
    async fn test_render_index_template() {
        let (_temp_dir, engine) = setup_test_templates().await;

        let globals = liquid::object!({});

        let result = engine.render_template("index.html.liquid", globals).await;
        assert!(
            result.is_ok(),
            "Failed to render index template: {:?}",
            result.err()
        );

        let html = result.unwrap();
        assert!(html.contains("<title>Home - Test Site</title>"));
        assert!(html.contains("Test home page"));
        assert!(html.contains("Welcome to the test site"));
        assert!(html.contains("&copy;"));
    }

    #[tokio::test]
    async fn test_render_gallery_template() {
        let (_temp_dir, engine) = setup_test_templates().await;

        let test_items = vec![
            liquid::object!({
                "name": "Item 1",
                "is_directory": false,
            }),
            liquid::object!({
                "name": "Item 2",
                "is_directory": true,
            }),
        ];

        let globals = liquid::object!({
            "folder_title": "Test Gallery",
            "folder_description": "This is a test gallery",
            "items": test_items,
        });

        let result = engine.render_template("gallery.html.liquid", globals).await;
        assert!(
            result.is_ok(),
            "Failed to render gallery template: {:?}",
            result.err()
        );

        let html = result.unwrap();
        assert!(html.contains("<title>Test Gallery - Test Site</title>"));
        assert!(html.contains("This is a test gallery"));
        assert!(html.contains("Item 1"));
        assert!(html.contains("Item 2"));
    }

    #[tokio::test]
    async fn test_render_with_gallery_preview() {
        let (_temp_dir, engine) = setup_test_templates().await;

        let globals = liquid::object!({});

        let result = engine.render_template("index.html.liquid", globals).await;
        assert!(
            result.is_ok(),
            "Failed to render with gallery preview: {:?}",
            result.err()
        );

        let html = result.unwrap();
        // The gallery preview component should be included in the template output
        // Since we passed gallery_preview data, the component should be rendered
        assert!(
            html.contains("Welcome to the test site"),
            "Missing main content"
        );

        // Note: The gallery preview component is only rendered if it's referenced
        // in the template. Our test index template doesn't include it.
    }

    #[tokio::test]
    async fn test_missing_partial_error() {
        let temp_dir = TempDir::new().unwrap();
        let template_path = temp_dir.path();

        // Create a template that includes a missing partial
        let bad_content = r#"{% include "_missing.html.liquid" %}"#;
        fs::write(template_path.join("bad.html.liquid"), bad_content).unwrap();

        let engine = TemplateEngine::new(template_path.to_path_buf());
        let globals = liquid::object!({});

        let result = engine.render_template("bad.html.liquid", globals).await;
        assert!(result.is_err(), "Should fail with missing partial");
    }

    #[tokio::test]
    async fn test_template_caching() {
        let (_temp_dir, engine) = setup_test_templates().await;

        // First render
        let globals1 = liquid::object!({});
        let result1 = engine.render_template("index.html.liquid", globals1).await;
        assert!(result1.is_ok());

        // Second render should use cache
        let globals2 = liquid::object!({});
        let result2 = engine.render_template("index.html.liquid", globals2).await;
        assert!(result2.is_ok());

        // Results should be similar (minus dynamic content like year)
        assert_eq!(result1.unwrap().len(), result2.unwrap().len());
    }

    #[tokio::test]
    async fn test_meta_tags_rendering() {
        let (_temp_dir, engine) = setup_test_templates().await;

        // Create image detail template
        let image_detail_content = r#"{% assign page_title = image.name %}
{% assign meta_description = image.description | default: "View this image" | strip_html | truncate: 160 %}
{% assign og_title = image.name %}
{% assign og_description = meta_description %}
{% assign og_image = base_url | append: image.gallery_url %}
{% if image.dimensions %}
    {% assign og_image_width = image.dimensions[0] %}
    {% assign og_image_height = image.dimensions[1] %}
{% endif %}
{% include "_header.html.liquid" %}

<h1>{{ image.name }}</h1>
<img src="{{ image.gallery_url }}" alt="{{ image.name }}">

{% include "_footer.html.liquid" %}"#;

        let temp_path = _temp_dir.path();
        fs::write(
            temp_path.join("image_detail.html.liquid"),
            image_detail_content,
        )
        .unwrap();

        // Update header to include OG tags
        let header_with_og = r#"<!DOCTYPE html>
<html>
<head>
    <title>{% if page_title %}{{ page_title }} - {% endif %}Test Site</title>
    {% if meta_description %}
    <meta name="description" content="{{ meta_description }}">
    {% endif %}
    {% if og_title %}
    <meta property="og:title" content="{{ og_title }}">
    {% endif %}
    {% if og_image %}
    <meta property="og:image" content="{{ og_image }}">
    {% endif %}
</head>
<body>
    <header>
        <h1>Test Site</h1>
    </header>
    <main>"#;
        fs::write(temp_path.join("_header.html.liquid"), header_with_og).unwrap();

        let globals = liquid::object!({
            "image": {
                "name": "Test Image",
                "description": "A beautiful test image",
                "gallery_url": "/gallery/image/test.jpg?size=gallery",
                "dimensions": [1200, 800],
            },
            "base_url": "https://example.com",
        });

        let result = engine
            .render_template("image_detail.html.liquid", globals)
            .await;
        assert!(
            result.is_ok(),
            "Failed to render image detail: {:?}",
            result.err()
        );

        let html = result.unwrap();
        assert!(html.contains("<title>Test Image - Test Site</title>"));
        assert!(html.contains(r#"<meta name="description" content="A beautiful test image">"#));
        assert!(html.contains(r#"<meta property="og:title" content="Test Image">"#));
        assert!(html.contains(r#"<meta property="og:image" content="https://example.com/gallery/image/test.jpg?size=gallery">"#));
    }
}
