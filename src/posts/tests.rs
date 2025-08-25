#[cfg(test)]
mod posts_tests {
    use super::super::*;
    use std::fs;
    use tempfile::TempDir;

    async fn setup_test_posts_dir() -> (TempDir, PostsConfig) {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create test markdown files
        let post1_content = r#"+++
title = "First Post"
summary = "This is the first test post"
date = "2024-01-01"
+++

# First Post

This is the content of the first post.

It has multiple paragraphs."#;

        let post2_content = r#"+++
title = "Second Post"
summary = "This is the second test post"
date = "2024-01-02"
+++

# Second Post

This is the content of the second post.

## With a subsection

And some more content."#;

        fs::write(posts_dir.join("first-post.md"), post1_content).unwrap();
        fs::write(posts_dir.join("second-post.md"), post2_content).unwrap();

        // Create a subdirectory with another post
        let subdir = posts_dir.join("tutorials");
        fs::create_dir(&subdir).unwrap();

        let post3_content = r#"+++
title = "Tutorial Post"
summary = "This is a tutorial"
date = "2024-01-03"
+++

# Tutorial

This is a tutorial post in a subdirectory."#;

        fs::write(subdir.join("tutorial.md"), post3_content).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        (temp_dir, config)
    }

    #[tokio::test]
    async fn test_posts_manager_creation() {
        let (_temp_dir, config) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config.clone());
        assert_eq!(manager.get_config().url_prefix, "/posts");
    }

    #[tokio::test]
    async fn test_refresh_posts() {
        let (_temp_dir, config) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config);

        let result = manager.refresh_posts().await;
        assert!(result.is_ok());

        let posts = manager.get_posts_page(0).await;
        assert_eq!(posts.len(), 3);

        // Check that posts are sorted by date (newest first)
        assert_eq!(posts[0].title, "Tutorial Post");
        assert_eq!(posts[1].title, "Second Post");
        assert_eq!(posts[2].title, "First Post");
    }

    #[tokio::test]
    async fn test_get_post() {
        let (_temp_dir, config) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("first-post").await;
        assert!(post.is_some());
        let post = post.unwrap();
        assert_eq!(post.title, "First Post");
        assert_eq!(post.summary, "This is the first test post");
        assert!(post.html_content.contains("<h1>First Post</h1>"));
    }

    #[tokio::test]
    async fn test_get_post_from_subdirectory() {
        let (_temp_dir, config) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("tutorials/tutorial").await;
        assert!(post.is_some());
        let post = post.unwrap();
        assert_eq!(post.title, "Tutorial Post");
        assert_eq!(post.slug, "tutorials/tutorial");
    }

    #[tokio::test]
    async fn test_pagination() {
        let (_temp_dir, config_orig) = setup_test_posts_dir().await;
        let mut config = config_orig;
        config.posts_per_page = 2;

        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        let page1 = manager.get_posts_page(0).await;
        assert_eq!(page1.len(), 2);

        let page2 = manager.get_posts_page(1).await;
        assert_eq!(page2.len(), 1);

        let total_pages = manager.get_total_pages().await;
        assert_eq!(total_pages, 2);
    }

    #[tokio::test]
    async fn test_invalid_front_matter() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create post with invalid front matter
        let invalid_content = r#"This post has no front matter

Just content."#;

        fs::write(posts_dir.join("invalid.md"), invalid_content).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        let manager = PostsManager::new(config);
        let result = manager.refresh_posts().await;
        assert!(result.is_ok()); // Should not fail completely

        let posts = manager.get_posts_page(0).await;
        assert_eq!(posts.len(), 0); // Invalid post should be skipped
    }

    #[tokio::test]
    async fn test_date_formats() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Test different date formats
        let post_with_full_date = r#"+++
title = "Full Date Post"
summary = "Post with full RFC3339 date"
date = "2024-01-15T10:30:00Z"
+++

Content"#;

        let post_with_simple_date = r#"+++
title = "Simple Date Post"
summary = "Post with simple date"
date = "2024-01-16"
+++

Content"#;

        fs::write(posts_dir.join("full-date.md"), post_with_full_date).unwrap();
        fs::write(posts_dir.join("simple-date.md"), post_with_simple_date).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        let manager = PostsManager::new(config);
        let result = manager.refresh_posts().await;
        assert!(result.is_ok());

        let posts = manager.get_posts_page(0).await;
        assert_eq!(posts.len(), 2);
    }

    #[tokio::test]
    async fn test_markdown_rendering() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let markdown_content = r#"+++
title = "Markdown Test"
summary = "Testing markdown features"
date = "2024-01-01"
+++

# Heading 1

## Heading 2

This is a paragraph with **bold** and *italic* text.

- List item 1
- List item 2

1. Numbered item 1
2. Numbered item 2

```rust
fn main() {
    println!("Hello, world!");
}
```

> This is a blockquote

[Link to example](https://example.com)

| Column 1 | Column 2 |
|----------|----------|
| Cell 1   | Cell 2   |

~~Strikethrough text~~

Footnote[^1]

[^1]: This is a footnote
"#;

        fs::write(posts_dir.join("markdown-test.md"), markdown_content).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("markdown-test").await.unwrap();

        // Check various markdown features are rendered
        assert!(post.html_content.contains("<h1>Heading 1</h1>"));
        assert!(post.html_content.contains("<h2>Heading 2</h2>"));
        assert!(post.html_content.contains("<strong>bold</strong>"));
        assert!(post.html_content.contains("<em>italic</em>"));
        assert!(post.html_content.contains("<ul>"));
        assert!(post.html_content.contains("<ol>"));
        assert!(post.html_content.contains("<pre><code"));
        assert!(post.html_content.contains("<blockquote>"));
        assert!(
            post.html_content
                .contains("<a href=\"https://example.com\"")
        );
        assert!(post.html_content.contains("<table>"));
        assert!(post.html_content.contains("<del>Strikethrough text</del>"));
        assert!(post.html_content.contains("sup")); // Footnote reference
    }

    #[tokio::test]
    async fn test_gallery_image_references() {
        use crate::gallery::Gallery;
        use crate::{GallerySystemConfig, ImageSizeConfig, PreviewConfig};
        use std::collections::HashMap;
        use std::sync::Arc;

        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create a post with gallery references
        let post_content = r#"+++
title = "Gallery Test Post"
summary = "Testing gallery image references"
date = "2024-01-15"
+++

# Gallery Test Post

Here's a thumbnail from the main gallery:
![gallery:main:vacation/beach.jpg](thumbnail)

And here's a gallery-sized image:
![gallery:main:vacation/sunset.jpg](gallery)

And a medium-sized image with a link:
![gallery:portfolio:projects/app-screenshot.png](medium)

Regular markdown image (not a gallery reference):
![Regular Image](https://example.com/image.jpg)
"#;

        fs::write(posts_dir.join("gallery-test.md"), post_content).unwrap();

        // Set up mock galleries
        let mut galleries = HashMap::new();

        // Create main gallery
        let main_gallery_config = GallerySystemConfig {
            name: "main".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: temp_dir.path().join("photos"),
            cache_directory: temp_dir.path().join("cache/main"),
            gallery_template: "modules/gallery.html.liquid".to_string(),
            image_detail_template: "modules/image_detail.html.liquid".to_string(),
            images_per_page: 50,
            thumbnail: ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: PreviewConfig {
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
            copyright_holder: None,
        };

        let main_gallery = Arc::new(Gallery::new(main_gallery_config.clone()));
        galleries.insert("main".to_string(), main_gallery);

        // Create portfolio gallery
        let portfolio_gallery_config = GallerySystemConfig {
            name: "portfolio".to_string(),
            url_prefix: "/my-portfolio".to_string(),
            source_directory: temp_dir.path().join("portfolio"),
            cache_directory: temp_dir.path().join("cache/portfolio"),
            gallery_template: "modules/gallery.html.liquid".to_string(),
            image_detail_template: "modules/image_detail.html.liquid".to_string(),
            images_per_page: 20,
            thumbnail: ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: PreviewConfig {
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(90),
            webp_quality: Some(90.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
            copyright_holder: None,
        };

        let portfolio_gallery = Arc::new(Gallery::new(portfolio_gallery_config));
        galleries.insert("portfolio".to_string(), portfolio_gallery);

        // Create posts config
        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        // Create posts manager with galleries
        let mut posts_manager = PostsManager::new(config);
        posts_manager.set_galleries(Arc::new(galleries));

        // Load posts
        posts_manager.refresh_posts().await.unwrap();

        // Get the gallery test post
        let post = posts_manager.get_post("gallery-test").await.unwrap();

        // Check that gallery references were converted to HTML
        assert!(post.html_content.contains(
            r#"<a href="/gallery/detail/vacation%2Fbeach.jpg" class="gallery-image-link">"#
        ));
        assert!(
            post.html_content
                .contains(r#"<img src="/gallery/image/vacation%2Fbeach.jpg?size=thumbnail""#)
        );
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-thumbnail""#)
        );

        assert!(post.html_content.contains(
            r#"<a href="/gallery/detail/vacation%2Fsunset.jpg" class="gallery-image-link">"#
        ));
        assert!(
            post.html_content
                .contains(r#"<img src="/gallery/image/vacation%2Fsunset.jpg?size=gallery""#)
        );
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-gallery""#)
        );

        assert!(post.html_content.contains(r#"<a href="/my-portfolio/detail/projects%2Fapp-screenshot.png" class="gallery-image-link">"#));
        assert!(post.html_content.contains(
            r#"<img src="/my-portfolio/image/projects%2Fapp-screenshot.png?size=medium""#
        ));
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-medium""#)
        );

        // Check that regular images are not converted
        assert!(
            post.html_content
                .contains(r#"<img src="https://example.com/image.jpg" alt="Regular Image""#)
        );
    }

    #[tokio::test]
    async fn test_post_reload_on_change() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create initial post
        let initial_content = r#"+++
title = "Test Post"
summary = "Initial summary"
date = "2024-01-01"
+++

# Initial Content

This is the initial content."#;

        let post_path = posts_dir.join("test-post.md");
        fs::write(&post_path, initial_content).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        // Get the initial post
        let post1 = manager.get_post("test-post").await.unwrap();
        assert_eq!(post1.title, "Test Post");
        assert_eq!(post1.summary, "Initial summary");
        assert!(post1.html_content.contains("Initial Content"));

        // Sleep briefly to ensure file modification time differs
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Modify the post
        let updated_content = r#"+++
title = "Updated Test Post"
summary = "Updated summary"
date = "2024-01-01"
+++

# Updated Content

This is the updated content with **bold text**."#;

        fs::write(&post_path, updated_content).unwrap();

        // Get the post again - it should automatically reload
        let post2 = manager.get_post("test-post").await.unwrap();
        assert_eq!(post2.title, "Updated Test Post");
        assert_eq!(post2.summary, "Updated summary");
        assert!(post2.html_content.contains("Updated Content"));
        assert!(post2.html_content.contains("<strong>bold text</strong>"));
    }

    #[tokio::test]
    async fn test_post_not_reloaded_when_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let content = r#"+++
title = "Stable Post"
summary = "This post won't change"
date = "2024-01-01"
+++

# Stable Content"#;

        let post_path = posts_dir.join("stable-post.md");
        fs::write(&post_path, content).unwrap();

        let config = PostsConfig {
            source_directory: posts_dir.to_path_buf(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
        };

        let manager = PostsManager::new(config);
        manager.refresh_posts().await.unwrap();

        // Get the post twice without modifications
        let post1 = manager.get_post("stable-post").await.unwrap();
        let post2 = manager.get_post("stable-post").await.unwrap();

        // The modification times should be the same (not reloaded)
        assert_eq!(post1.last_modified, post2.last_modified);
        assert_eq!(post1.title, post2.title);
    }
}
