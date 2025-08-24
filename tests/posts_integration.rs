use axum::http::StatusCode;
use axum_test::TestServer;
use std::fs;
use tempfile::TempDir;
use tenrankai::{Config, PostsSystemConfig, create_app};

async fn setup_test_server_with_posts() -> (TempDir, TestServer) {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let templates_dir = temp_dir.path().join("templates");
    let pages_dir = templates_dir.join("pages");
    let partials_dir = templates_dir.join("partials");
    let static_dir = temp_dir.path().join("static");
    let gallery_dir = temp_dir.path().join("gallery");
    let cache_dir = temp_dir.path().join("cache");
    let posts_dir = temp_dir.path().join("posts");
    let blog_dir = posts_dir.join("blog");

    fs::create_dir_all(&pages_dir).unwrap();
    fs::create_dir_all(&partials_dir).unwrap();
    fs::create_dir_all(&static_dir).unwrap();
    fs::create_dir_all(&gallery_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();
    fs::create_dir_all(&blog_dir).unwrap();

    // Create test templates
    let header_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>{% if page_title %}{{ page_title }} - {% endif %}Test Site</title>
    {% if og_title %}<meta property="og:title" content="{{ og_title }}">{% endif %}
    {% if og_description %}<meta property="og:description" content="{{ og_description }}">{% endif %}
</head>
<body>
    <header><h1>Test Site</h1></header>
    <main>"#;
    fs::write(partials_dir.join("_header.html.liquid"), header_content).unwrap();

    let footer_content = r#"    </main>
    <footer><p>&copy; {{ current_year }} Test</p></footer>
</body>
</html>"#;
    fs::write(partials_dir.join("_footer.html.liquid"), footer_content).unwrap();

    // Create posts index template
    let posts_index_content = r#"{% assign page_title = posts_name | capitalize %}
{% assign meta_description = meta_description | default: "Browse " | append: posts_name | append: " posts" %}
{% assign og_title = og_title | default: page_title %}
{% assign og_description = og_description | default: meta_description %}
{% include "_header.html.liquid" %}

<h1>{{ posts_name | capitalize }}</h1>
<div class="posts-list">
    {% for post in posts %}
        <article>
            <h2><a href="{{ post.url }}">{{ post.title }}</a></h2>
            <time>{{ post.date_formatted }}</time>
            <p>{{ post.summary }}</p>
        </article>
    {% endfor %}
</div>

{% if total_pages > 1 %}
    <nav class="pagination">
        {% if has_prev %}<a href="?page={{ prev_page }}">Previous</a>{% endif %}
        <span>Page {{ current_page | plus: 1 }} of {{ total_pages }}</span>
        {% if has_next %}<a href="?page={{ next_page }}">Next</a>{% endif %}
    </nav>
{% endif %}

{% include "_footer.html.liquid" %}"#;
    fs::write(pages_dir.join("posts_index.html.liquid"), posts_index_content).unwrap();

    // Create post detail template
    let post_detail_content = r#"{% assign page_title = post.title %}
{% assign meta_description = post.summary %}
{% include "_header.html.liquid" %}

<article>
    <h1>{{ post.title }}</h1>
    <time>{{ post.date_formatted }}</time>
    <div class="content">{{ post.html_content }}</div>
</article>

{% include "_footer.html.liquid" %}"#;
    fs::write(pages_dir.join("post_detail.html.liquid"), post_detail_content).unwrap();

    // Create test posts
    let post1_content = r#"+++
title = "First Test Post"
summary = "This is the first test post"
date = "2024-01-01"
+++

# First Test Post

This is the content of the first test post."#;
    fs::write(blog_dir.join("first-post.md"), post1_content).unwrap();

    let post2_content = r#"+++
title = "Second Test Post"
summary = "This is the second test post"
date = "2024-01-02"
+++

# Second Test Post

This is the content of the second test post."#;
    fs::write(blog_dir.join("second-post.md"), post2_content).unwrap();

    // Create test config
    let config = Config {
        server: tenrankai::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        app: tenrankai::AppConfig {
            name: "TestServer".to_string(),
            log_level: "error".to_string(),
            download_secret: "test-secret".to_string(),
            download_password: "test-pass".to_string(),
            copyright_holder: None,
            base_url: Some("http://localhost:3000".to_string()),
        },
        templates: tenrankai::TemplateConfig {
            directory: templates_dir,
        },
        static_files: tenrankai::StaticConfig {
            directory: static_dir,
        },
        gallery: tenrankai::GalleryConfig {
            path_prefix: "gallery".to_string(),
            source_directory: gallery_dir,
            cache_directory: cache_dir,
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
        },
        posts: Some(vec![
            PostsSystemConfig {
                name: "blog".to_string(),
                source_directory: blog_dir,
                url_prefix: "/blog".to_string(),
                index_template: "pages/posts_index.html.liquid".to_string(),
                post_template: "pages/post_detail.html.liquid".to_string(),
                posts_per_page: 10,
            }
        ]),
    };

    let app = create_app(config).await;
    let server = TestServer::new(app.into_make_service()).unwrap();

    (temp_dir, server)
}

#[tokio::test]
async fn test_posts_index_renders() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let response = server.get("/blog").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    
    // Check that the page renders with proper title
    assert!(html.contains("<title>Blog - Test Site</title>"));
    assert!(html.contains("<h1>Blog</h1>"));
    
    // Check that posts are listed
    assert!(html.contains("Second Test Post")); // Should be first (newer)
    assert!(html.contains("First Test Post"));
    assert!(html.contains("This is the first test post"));
    assert!(html.contains("This is the second test post"));
    
    // Check dates are rendered with formatted dates
    assert!(html.contains("January 1, 2024"));
    assert!(html.contains("January 2, 2024"));
    
    // Check meta tags
    assert!(html.contains(r#"<meta property="og:title" content="Blog">"#));
    assert!(html.contains(r#"<meta property="og:description" content="Browse blog posts">"#));
}

#[tokio::test]
async fn test_post_detail_renders() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let response = server.get("/blog/first-post").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    
    // Check that the page renders with proper title
    assert!(html.contains("<title>First Test Post - Test Site</title>"));
    assert!(html.contains("<h1>First Test Post</h1>"));
    
    // Check content is rendered
    assert!(html.contains("This is the content of the first test post"));
    assert!(html.contains("January 1, 2024"));
}

#[tokio::test]
async fn test_posts_pagination() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    // Create more posts to test pagination
    let blog_dir = _temp_dir.path().join("posts").join("blog");
    for i in 3..=15 {
        let post_content = format!(
            r#"+++
title = "Test Post {}"
summary = "Summary for post {}"
date = "2024-01-{:02}"
+++

Content for post {}."#,
            i, i, i, i
        );
        fs::write(blog_dir.join(format!("post-{}.md", i)), post_content).unwrap();
    }

    // Refresh posts
    let refresh_response = server.post("/api/posts/blog/refresh").await;
    assert_eq!(refresh_response.status_code(), StatusCode::OK);

    // Test first page
    let response = server.get("/blog").await;
    let html = response.text();
    assert!(html.contains("Page 1 of 2"));
    assert!(html.contains(r#"<a href="?page=1">Next</a>"#));
    assert!(!html.contains("Previous"));

    // Test second page
    let response = server.get("/blog?page=1").await;
    let html = response.text();
    assert!(html.contains("Page 2 of 2"));
    assert!(html.contains(r#"<a href="?page=0">Previous</a>"#));
    assert!(!html.contains("Next"));
}

#[tokio::test]
async fn test_posts_not_found() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    // Test non-existent posts system
    let response = server.get("/stories").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);

    // Test non-existent post
    let response = server.get("/blog/non-existent-post").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_posts_refresh_api() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    // Add a new post
    let blog_dir = _temp_dir.path().join("posts").join("blog");
    let new_post_content = r#"+++
title = "New Post After Refresh"
summary = "This post was added after server start"
date = "2024-02-01"
+++

Content of the new post."#;
    fs::write(blog_dir.join("new-post.md"), new_post_content).unwrap();

    // Verify it's not visible yet
    let response = server.get("/blog").await;
    let html = response.text();
    assert!(!html.contains("New Post After Refresh"));

    // Refresh posts
    let refresh_response = server.post("/api/posts/blog/refresh").await;
    assert_eq!(refresh_response.status_code(), StatusCode::OK);

    // Verify it's now visible
    let response = server.get("/blog").await;
    let html = response.text();
    assert!(html.contains("New Post After Refresh"));
}

#[tokio::test]
async fn test_posts_subdirectory() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    // Create a post in a subdirectory
    let tutorials_dir = _temp_dir.path().join("posts").join("blog").join("tutorials");
    fs::create_dir_all(&tutorials_dir).unwrap();
    
    let tutorial_content = r#"+++
title = "Rust Tutorial"
summary = "Learn Rust basics"
date = "2024-03-01"
+++

# Rust Tutorial

This is a tutorial in a subdirectory."#;
    fs::write(tutorials_dir.join("rust-basics.md"), tutorial_content).unwrap();

    // Refresh posts
    server.post("/api/posts/blog/refresh").await;

    // Test that the post is accessible
    let response = server.get("/blog/tutorials/rust-basics").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    assert!(html.contains("Rust Tutorial"));
    assert!(html.contains("This is a tutorial in a subdirectory"));
}