#[cfg(test)]
mod multi_dir_tests {
    use crate::templating::TemplateEngine;
    use std::fs;
    use tempfile::TempDir;

    async fn setup_multi_dir_test() -> (TempDir, TempDir, TemplateEngine) {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let template_path1 = temp_dir1.path();
        let template_path2 = temp_dir2.path();

        // Create directory structures in both locations
        fs::create_dir_all(template_path1.join("pages")).unwrap();
        fs::create_dir_all(template_path1.join("partials")).unwrap();
        fs::create_dir_all(template_path2.join("pages")).unwrap();
        fs::create_dir_all(template_path2.join("partials")).unwrap();

        // Create a partial in BOTH directories with different content
        let header_content_dir1 = r#"<!-- Header from Directory 1 -->
<header>
    <h1>Site from Dir1</h1>
</header>"#;
        fs::write(
            template_path1.join("partials/_header.html.liquid"),
            header_content_dir1,
        )
        .unwrap();

        let header_content_dir2 = r#"<!-- Header from Directory 2 -->
<header>
    <h1>Site from Dir2</h1>
</header>"#;
        fs::write(
            template_path2.join("partials/_header.html.liquid"),
            header_content_dir2,
        )
        .unwrap();

        // Create footer partial only in dir2
        let footer_content = r#"<footer>
    <p>Footer content from Dir2</p>
</footer>"#;
        fs::write(
            template_path2.join("partials/_footer.html.liquid"),
            footer_content,
        )
        .unwrap();

        // Create gallery preview partial only in dir1
        let gallery_preview_content = r#"<div class="gallery-preview">
    <h2>Gallery Preview from Dir1</h2>
</div>"#;
        fs::write(
            template_path1.join("partials/_gallery_preview.html.liquid"),
            gallery_preview_content,
        )
        .unwrap();

        // Create a page template only in dir2 that uses the partials
        let page_content = r#"{% include '_header.html.liquid' %}
<main>
    <h2>Main Content</h2>
    <p>This page is from directory 2</p>
    {% include '_gallery_preview.html.liquid' %}
</main>
{% include '_footer.html.liquid' %}"#;
        fs::write(template_path2.join("pages/test.html.liquid"), page_content).unwrap();

        // Create template engine with dir1 first (higher priority)
        let template_engine = TemplateEngine::new(vec![
            template_path1.to_path_buf(),
            template_path2.to_path_buf(),
        ]);

        (temp_dir1, temp_dir2, template_engine)
    }

    #[tokio::test]
    async fn test_partial_precedence_from_multiple_directories() {
        let (_temp_dir1, _temp_dir2, template_engine) = setup_multi_dir_test().await;

        // Load the header partial directly - should get from dir1
        let header_result = template_engine
            .load_template("partials/_header.html.liquid")
            .await;
        assert!(header_result.is_ok());
        let header_content = header_result.unwrap();
        assert!(header_content.contains("Site from Dir1"));
        assert!(!header_content.contains("Site from Dir2"));

        // Load the footer partial directly - should get from dir2 (only exists there)
        let footer_result = template_engine
            .load_template("partials/_footer.html.liquid")
            .await;
        assert!(footer_result.is_ok());
        let footer_content = footer_result.unwrap();
        assert!(footer_content.contains("Footer content from Dir2"));

        // Load the gallery preview partial directly - should get from dir1 (only exists there)
        let gallery_result = template_engine
            .load_template("partials/_gallery_preview.html.liquid")
            .await;
        assert!(gallery_result.is_ok());
        let gallery_content = gallery_result.unwrap();
        assert!(gallery_content.contains("Gallery Preview from Dir1"));
    }

    #[tokio::test]
    async fn test_template_with_mixed_partial_sources() {
        let (_temp_dir1, _temp_dir2, mut template_engine) = setup_multi_dir_test().await;

        // Set has_user_auth to false to avoid loading user menu partial
        template_engine.set_has_user_auth(false);

        // Render the template from dir2, which should use:
        // - Header from dir1 (exists in both, dir1 has precedence)
        // - Gallery preview from dir1 (only exists there)
        // - Footer from dir2 (only exists there)
        let result = template_engine
            .render_template("pages/test.html.liquid", Default::default())
            .await;

        assert!(result.is_ok());
        let html_content = result.unwrap();

        // Verify we got the header from dir1 (higher priority)
        assert!(html_content.contains("Site from Dir1"));
        assert!(!html_content.contains("Site from Dir2"));

        // Verify we got the gallery preview from dir1
        assert!(html_content.contains("Gallery Preview from Dir1"));

        // Verify we got the footer from dir2
        assert!(html_content.contains("Footer content from Dir2"));

        // Verify we got the main content from dir2
        assert!(html_content.contains("This page is from directory 2"));
    }

    #[tokio::test]
    async fn test_template_override_behavior() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let template_path1 = temp_dir1.path();
        let template_path2 = temp_dir2.path();

        // Create directory structures
        fs::create_dir_all(template_path1.join("pages")).unwrap();
        fs::create_dir_all(template_path2.join("pages")).unwrap();

        // Create same template in both directories with different content
        fs::write(
            template_path1.join("pages/override.html.liquid"),
            "<h1>Override from Dir1</h1>",
        )
        .unwrap();

        fs::write(
            template_path2.join("pages/override.html.liquid"),
            "<h1>Override from Dir2</h1>",
        )
        .unwrap();

        let template_engine = TemplateEngine::new(vec![
            template_path1.to_path_buf(),
            template_path2.to_path_buf(),
        ]);

        // Should load from dir1 (first in the list)
        let result = template_engine
            .load_template("pages/override.html.liquid")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("Override from Dir1"));
        assert!(!content.contains("Override from Dir2"));
    }

    #[tokio::test]
    async fn test_missing_template_error_message() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let template_path1 = temp_dir1.path();
        let template_path2 = temp_dir2.path();

        let template_engine = TemplateEngine::new(vec![
            template_path1.to_path_buf(),
            template_path2.to_path_buf(),
        ]);

        // Try to load a non-existent template
        let result = template_engine
            .load_template("pages/nonexistent.html.liquid")
            .await;
        assert!(result.is_err());

        let error = result.unwrap_err();

        // Check that the error message contains the expected parts
        assert!(error.contains("Template pages/nonexistent.html.liquid not found"));
        assert!(error.contains("configured directories:"));

        // The error should show it searched in multiple directories (formatted as a Vec)
        assert!(
            error.contains("["),
            "Error should contain opening bracket from Vec formatting"
        );
        assert!(
            error.contains("]"),
            "Error should contain closing bracket from Vec formatting"
        );

        // Basic sanity check - the error should be reasonably sized and informative
        assert!(error.len() > 50, "Error message should be detailed");
    }
}
