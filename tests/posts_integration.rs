use axum::http::StatusCode;
use axum_test::TestServer;
use std::fs;
use tempfile::TempDir;
use tenrankai::{PostsSystemConfig, StaticConfig, TemplateConfig, create_app, site::SiteConfig};

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
{% if active_category_description %}<p class="category-description">{{ active_category_description }}</p>{% endif %}
<nav class="category-bar">
    {% for category in categories %}
    <span class="chip{% if category.archive %} archive{% endif %}">{{ category.name }}={{ category.count }}</span>
    {% endfor %}
</nav>
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
    // Create modules directory for module templates
    let modules_dir = templates_dir.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    fs::write(
        modules_dir.join("posts_index.html.liquid"),
        posts_index_content,
    )
    .unwrap();

    // Create post detail template
    let post_detail_content = r#"{% assign page_title = post.title %}
{% assign meta_description = post.summary %}
{% include "_header.html.liquid" %}

<article>
    <h1>{{ post.title }}</h1>
    <time>{{ post.date_formatted }}</time>
    {% if post.hero_image and post.hero_image_explicit %}
    {% if post.hero_image_link %}
    <a href="{{ post.hero_image_link }}" class="post-hero-link"><img src="{{ post.hero_image }}"></a>
    {% else %}
    <img class="post-hero-plain" src="{{ post.hero_image }}">
    {% endif %}
    {% endif %}
    <div class="content">{{ post.html_content }}</div>
</article>

{% include "_footer.html.liquid" %}"#;
    fs::write(
        modules_dir.join("post_detail.html.liquid"),
        post_detail_content,
    )
    .unwrap();

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
    let config = SiteConfig {
        name: "TestServer".to_string(),
        base_url: Some("http://localhost:3000".to_string()),
        cookie_secret: "test-cookie-secret".to_string(),
        templates: TemplateConfig {
            directories: vec![templates_dir.to_string_lossy().to_string()],
        },
        static_files: StaticConfig {
            directories: vec![static_dir.to_string_lossy().to_string()],
            use_redirects: false,
        },
        galleries: Some(vec![tenrankai::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: gallery_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html.liquid".to_string(),
            image_detail_template: "image_detail.html.liquid".to_string(),
            ..Default::default()
        }]),
        posts: Some(vec![PostsSystemConfig {
            name: "blog".to_string(),
            source_directory: blog_dir.to_string_lossy().to_string(),
            url_prefix: "/blog".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        }]),
        user_database: None,
        email: None,
        config_storage: None,
        site_admins: Vec::new(),
        hosted_mode: false,
        site_title: None,
        copyright_holder: None,
    };

    let app = create_app(config, None).await;
    let server = TestServer::new(app.into_make_service());

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
    let tutorials_dir = _temp_dir
        .path()
        .join("posts")
        .join("blog")
        .join("tutorials");
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

#[tokio::test]
async fn test_posts_rss_feed() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let response = server.get("/blog/feed.xml").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/rss+xml")
    );

    let xml = response.text();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<rss version=\"2.0\""));
    assert!(xml.contains("<title>First Test Post</title>"));
    assert!(xml.contains("<title>Second Test Post</title>"));
    assert!(xml.contains("<link>http://localhost:3000/blog/first-post</link>"));
    assert!(xml.contains("<description>This is the first test post</description>"));
    assert!(xml.contains("<content:encoded><![CDATA["));
    // Newest first
    let first = xml.find("Second Test Post").unwrap();
    let second = xml.find("First Test Post").unwrap();
    assert!(first < second);

    // Auth-varying responses must not be mixed by shared caches
    assert_eq!(response.headers().get("vary").unwrap(), "Cookie");
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=300"
    );
}

#[tokio::test]
async fn test_category_pages_and_feeds() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    // Add a categorized post and refresh
    let blog_dir = _temp_dir.path().join("posts").join("blog");
    let post_content = r#"+++
title = "Rust Deep Dive"
summary = "A post about Rust"
date = "2024-03-01"
categories = ["Rust & Systems"]
+++

Rust content."#;
    fs::write(blog_dir.join("rust-deep-dive.md"), post_content).unwrap();
    server.post("/api/posts/blog/refresh").await;

    // Category index page shows only matching posts
    let response = server.get("/blog/category/rust-systems").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let html = response.text();
    assert!(html.contains("Rust Deep Dive"));
    assert!(!html.contains("First Test Post"));

    // Legacy query-parameter URLs redirect permanently to the path form
    let response = server
        .get("/blog")
        .add_query_param("category", "Rust & Systems")
        .await;
    assert_eq!(response.status_code(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/blog/category/rust-systems"
    );

    // Unrelated query parameters survive the redirect
    let response = server
        .get("/blog")
        .add_query_param("utm_source", "newsletter")
        .add_query_param("category", "Rust & Systems")
        .await;
    assert_eq!(response.status_code(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/blog/category/rust-systems?utm_source=newsletter"
    );

    // Unknown categories 404 on the index page
    let response = server.get("/blog/category/nonexistent").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);

    // Per-category feed contains only matching posts
    let response = server.get("/blog/category/rust-systems/feed.xml").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let xml = response.text();
    assert!(xml.contains("<title>Rust Deep Dive</title>"));
    assert!(!xml.contains("First Test Post"));
    assert!(xml.contains("<category>Rust &amp; Systems</category>"));

    // Unknown category feeds are 404
    let response = server.get("/blog/category/nonexistent/feed.xml").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_category_options_and_archive() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let blog_dir = _temp_dir.path().join("posts").join("blog");
    fs::write(
        blog_dir.join("_categories.md"),
        r#"+++
[categories.legacy]
name = "The Archive"
description = "Older posts kept for reference"
archive = true

[categories.travel]
weight = 1

[categories.gear]
weight = 2
+++
"#,
    )
    .unwrap();

    let old_news = r#"+++
title = "Old News"
summary = "An archived post"
date = "2024-02-01"
categories = ["Legacy"]
+++

Old content."#;
    fs::write(blog_dir.join("old-news.md"), old_news).unwrap();

    let travel_memories = r#"+++
title = "Travel Memories"
summary = "Archived but still a travel post"
date = "2024-02-02"
categories = ["Travel", "Legacy"]
+++

Travel content."#;
    fs::write(blog_dir.join("travel-memories.md"), travel_memories).unwrap();

    let new_trip = r#"+++
title = "New Trip"
summary = "A current travel post"
date = "2024-02-03"
categories = ["Travel"]
+++

Trip content."#;
    fs::write(blog_dir.join("new-trip.md"), new_trip).unwrap();

    let gear_post = r#"+++
title = "Gear Notes"
summary = "A gear post"
date = "2024-02-04"
categories = ["Gear"]
+++

Gear content."#;
    fs::write(blog_dir.join("gear-notes.md"), gear_post).unwrap();

    server.post("/api/posts/blog/refresh").await;

    // Archived posts are hidden from the unfiltered index
    let html = server.get("/blog").await.text();
    assert!(html.contains("New Trip"));
    assert!(!html.contains("Old News"));
    assert!(!html.contains("Travel Memories"));

    // Chips: declared weights order Travel before Gear, archive category
    // last with its declared display name; counts include archived posts
    let travel = html.find("Travel=2").expect("travel chip with count");
    let gear = html.find("Gear=1").expect("gear chip with count");
    let archive = html
        .find(r#"<span class="chip archive">The Archive=2</span>"#)
        .expect("archive chip with declared name");
    assert!(travel < gear && gear < archive);

    // ...and from the main feed
    let xml = server.get("/blog/feed.xml").await.text();
    assert!(xml.contains("<title>New Trip</title>"));
    assert!(!xml.contains("Old News"));

    // Category pages list everything in the category, archived or not
    let html = server.get("/blog/category/travel").await.text();
    assert!(html.contains("New Trip"));
    assert!(html.contains("Travel Memories"));

    // The archive category page is the archive view, with the declared
    // name and description
    let html = server.get("/blog/category/legacy").await.text();
    assert!(html.contains("Old News"));
    assert!(html.contains("Travel Memories"));
    assert!(html.contains("Older posts kept for reference"));

    // Archived posts stay reachable at their permalinks
    let response = server.get("/blog/old-news").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // The archive category feed uses the declared name and description
    let xml = server.get("/blog/category/legacy/feed.xml").await.text();
    assert!(xml.contains("The Archive"));
    assert!(xml.contains("<description>Older posts kept for reference</description>"));
    assert!(xml.contains("<title>Old News</title>"));

    // Archived permalinks and the archive category page stay in the sitemap
    let xml = server.get("/sitemap.xml").await.text();
    assert!(xml.contains("/blog/old-news"));
    assert!(xml.contains("/blog/travel-memories"));
    assert!(xml.contains("/blog/category/legacy"));
}

#[tokio::test]
async fn test_gallery_links_use_index_identifiers() {
    // A gallery with sequence indexing addresses images by index id, not
    // filename; post-generated links must use the id or detail-page
    // navigation breaks
    let temp_dir = TempDir::new().unwrap();
    let templates_dir = temp_dir.path().join("templates");
    let modules_dir = templates_dir.join("modules");
    let partials_dir = templates_dir.join("partials");
    let gallery_dir = temp_dir.path().join("gallery");
    let blog_dir = temp_dir.path().join("blog");
    for dir in [&modules_dir, &partials_dir, &gallery_dir, &blog_dir] {
        fs::create_dir_all(dir).unwrap();
    }

    fs::write(partials_dir.join("_header.html.liquid"), "<html><body>").unwrap();
    fs::write(partials_dir.join("_footer.html.liquid"), "</body></html>").unwrap();
    fs::write(
        modules_dir.join("posts_index.html.liquid"),
        r#"{% include "_header.html.liquid" %}{% include "_footer.html.liquid" %}"#,
    )
    .unwrap();
    fs::write(
        modules_dir.join("post_detail.html.liquid"),
        r#"{% include "_header.html.liquid" %}
{% if post.hero_image_link %}<a class="post-hero-link" href="{{ post.hero_image_link }}"></a>{% endif %}
<div class="content">{{ post.html_content }}</div>
{% include "_footer.html.liquid" %}"#,
    )
    .unwrap();

    // Minimal valid 1x1 PNG
    const PNG: [u8; 69] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    fs::write(gallery_dir.join("only-image.png"), PNG).unwrap();

    fs::write(
        blog_dir.join("seq-post.md"),
        r#"+++
title = "Seq Post"
summary = "Post referencing a sequence-indexed gallery"
date = "2024-05-01"
hero_image = "gallery:seq:only-image.png"
+++

![gallery:seq:only-image.png](medium,details)
"#,
    )
    .unwrap();

    let config = SiteConfig {
        name: "TestServer".to_string(),
        base_url: Some("http://localhost:3000".to_string()),
        cookie_secret: "test-cookie-secret".to_string(),
        templates: TemplateConfig {
            directories: vec![templates_dir.to_string_lossy().to_string()],
        },
        static_files: StaticConfig {
            directories: vec![temp_dir.path().to_string_lossy().to_string()],
            use_redirects: false,
        },
        galleries: Some(vec![tenrankai::GallerySystemConfig {
            name: "seq".to_string(),
            source_directory: gallery_dir.to_string_lossy().to_string(),
            cache_directory: temp_dir.path().join("cache").to_string_lossy().to_string(),
            gallery_template: "gallery.html.liquid".to_string(),
            image_detail_template: "image_detail.html.liquid".to_string(),
            image_indexing: tenrankai::ImageIndexingMode::Sequence,
            ..Default::default()
        }]),
        posts: Some(vec![PostsSystemConfig {
            name: "blog".to_string(),
            source_directory: blog_dir.to_string_lossy().to_string(),
            url_prefix: "/blog".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        }]),
        user_database: None,
        email: None,
        config_storage: None,
        site_admins: Vec::new(),
        hosted_mode: false,
        site_title: None,
        copyright_holder: None,
    };
    let app = create_app(config, None).await;
    let server = TestServer::new(app.into_make_service());

    // Populate the gallery index, then re-render the post against it
    server.get("/api/gallery/seq/data").await;
    server.post("/api/posts/blog/refresh").await;

    let html = server.get("/blog/seq-post").await.text();

    // Sequence galleries address the only image as "1": links, image URLs,
    // and the hover-card path all use the identifier, not the filename
    assert!(html.contains(r#"href="/gallery/detail/1""#), "{html}");
    assert!(html.contains("/gallery/_image/1/medium"));
    assert!(html.contains(r#"data-image-path="1""#));
    assert!(html.contains(r#"class="post-hero-link" href="/gallery/detail/1""#));
    assert!(!html.contains("detail/only-image.png"));
}

#[tokio::test]
async fn test_gallery_embeds_details_and_hero_link() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let blog_dir = _temp_dir.path().join("posts").join("blog");
    fs::write(
        blog_dir.join("gallery-post.md"),
        r#"+++
title = "Gallery Post"
summary = "A post with gallery embeds"
date = "2024-04-01"
hero_image = "gallery:test:vacation/beach.jpg"
+++

![gallery:test:vacation/sunset.jpg](medium,details)

![gallery:test:vacation/plain.jpg](gallery)
"#,
    )
    .unwrap();
    fs::write(
        blog_dir.join("url-hero.md"),
        r#"+++
title = "URL Hero"
summary = "A post with a plain URL hero"
date = "2024-04-02"
hero_image = "https://example.com/hero.jpg"
+++

Content."#,
    )
    .unwrap();
    server.post("/api/posts/blog/refresh").await;

    let html = server.get("/blog/gallery-post").await.text();

    // A gallery hero links to its gallery detail page
    assert!(html.contains(r#"class="post-hero-link""#));
    assert!(html.contains(r#"href="/gallery/detail/vacation/beach.jpg""#));

    // The details option adds hover-card data attributes; plain embeds don't
    assert!(html.contains("gallery-image-details"));
    assert!(html.contains(r#"data-gallery="test""#));
    assert!(html.contains(r#"data-image-path="vacation/sunset.jpg""#));
    assert!(html.contains("gallery-image-medium"));
    assert!(html.contains("gallery-image-gallery"));
    let details_count = html.matches("data-gallery-details").count();
    assert_eq!(details_count, 1);

    // Plain URL heroes render without a gallery link
    let html = server.get("/blog/url-hero").await.text();
    assert!(html.contains(r#"class="post-hero-plain" src="https://example.com/hero.jpg""#));
    assert!(!html.contains("post-hero-link"));
}

#[tokio::test]
async fn test_posts_preview_api() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let blog_dir = _temp_dir.path().join("posts").join("blog");
    fs::write(
        blog_dir.join("_categories.md"),
        r#"+++
[categories.legacy]
archive = true
+++
"#,
    )
    .unwrap();
    fs::write(
        blog_dir.join("rust-post.md"),
        r#"+++
title = "Rust Post"
summary = "About Rust"
date = "2024-03-01"
categories = ["Rust & Systems"]
+++

Rust content."#,
    )
    .unwrap();
    fs::write(
        blog_dir.join("old-news.md"),
        r#"+++
title = "Old News"
summary = "Archived"
date = "2024-03-02"
categories = ["Legacy"]
+++

Old content."#,
    )
    .unwrap();
    server.post("/api/posts/blog/refresh").await;

    // Default preview: newest first, archived posts excluded
    let response = server.get("/api/posts/blog/preview").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 3);
    assert_eq!(posts[0]["title"], "Rust Post");
    assert_eq!(posts[0]["url"], "/blog/rust-post");
    assert_eq!(posts[0]["date_formatted"], "March 1, 2024");
    assert!(posts[0]["reading_time_minutes"].as_u64().unwrap() >= 1);
    assert_eq!(posts[0]["categories"][0]["name"], "Rust & Systems");
    assert_eq!(
        posts[0]["categories"][0]["url"],
        "/blog/category/rust-systems"
    );
    assert!(!json.to_string().contains("Old News"));

    // count is honored
    let response = server
        .get("/api/posts/blog/preview")
        .add_query_param("count", "1")
        .await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"].as_array().unwrap().len(), 1);

    // Category filtering by slug, which includes archived posts
    let response = server
        .get("/api/posts/blog/preview")
        .add_query_param("category", "rust-systems")
        .await;
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"], "Rust Post");
    let response = server
        .get("/api/posts/blog/preview")
        .add_query_param("category", "legacy")
        .await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"][0]["title"], "Old News");

    // Unknown posts systems 404
    let response = server.get("/api/posts/nonexistent/preview").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_feed_absolutizes_content_urls() {
    let (_temp_dir, server) = setup_test_server_with_posts().await;

    let blog_dir = _temp_dir.path().join("posts").join("blog");
    let post_content = r#"+++
title = "Relative Links"
summary = "Post with root-relative content URLs"
date = "2024-03-05"
+++

![local image](/static/photo.jpg)

[a page](/about) and [protocol-relative](//example.com/x) and [absolute](https://example.com/y)."#;
    fs::write(blog_dir.join("relative-links.md"), post_content).unwrap();
    server.post("/api/posts/blog/refresh").await;

    let response = server.get("/blog/feed.xml").await;
    let xml = response.text();

    // Root-relative URLs become absolute inside content:encoded
    assert!(xml.contains(r#"src="http://localhost:3000/static/photo.jpg""#));
    assert!(xml.contains(r#"href="http://localhost:3000/about""#));
    // Protocol-relative and absolute URLs are untouched
    assert!(xml.contains(r#"href="//example.com/x""#));
    assert!(xml.contains(r#"href="https://example.com/y""#));
    // The HTML page still serves the original relative URLs
    let page = server.get("/blog/relative-links").await.text();
    assert!(page.contains(r#"src="/static/photo.jpg""#));
}
