use axum::http::StatusCode;
use axum_test::TestServer;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use tenrankai::{
    GallerySystemConfig, ImageIndexingMode, PostsSystemConfig, StaticConfig, TemplateConfig,
    create_app, site::SiteConfig,
};

fn write_image(path: &Path) {
    let img = image::ImageBuffer::from_fn(64, 64, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 4) as u8, 128u8])
    });
    img.save(path).unwrap();
}

async fn setup_test_server() -> (TempDir, TestServer) {
    let temp_dir = TempDir::new().unwrap();

    // Reuse the real templates directory so the dynamic pages (`about`,
    // `contact`, ...) are discoverable. CARGO_MANIFEST_DIR is the tenrankai
    // package directory; the templates live at the workspace root.
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let templates_dir = workspace_root.join("templates");

    let static_dir = temp_dir.path().join("static");
    let photos_dir = temp_dir.path().join("photos");
    let cache_dir = temp_dir.path().join("cache");
    let posts_dir = temp_dir.path().join("posts");
    fs::create_dir_all(&static_dir).unwrap();
    fs::create_dir_all(&photos_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();
    fs::create_dir_all(&posts_dir).unwrap();

    // Images directly in the gallery root.
    write_image(&photos_dir.join("alpha.jpg"));
    write_image(&photos_dir.join("beta.jpg"));

    // A publicly visible subfolder.
    let vacation_dir = photos_dir.join("vacation");
    fs::create_dir_all(&vacation_dir).unwrap();
    write_image(&vacation_dir.join("gamma.jpg"));

    // A subfolder restricted to authenticated users only.
    let secret_dir = photos_dir.join("secret");
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(
        secret_dir.join("_folder.md"),
        "+++\n[permissions]\npublic_role = \"none\"\n+++\nSecret stuff\n",
    )
    .unwrap();
    write_image(&secret_dir.join("delta.jpg"));

    // A public subfolder whose name contains a space, to exercise URL encoding
    // of both folder and image-detail sitemap URLs.
    let spaced_dir = photos_dir.join("big sur");
    fs::create_dir_all(&spaced_dir).unwrap();
    write_image(&spaced_dir.join("epsilon.jpg"));

    // A blog post.
    fs::write(
        posts_dir.join("hello-world.md"),
        "+++\ntitle = \"Hello World\"\nsummary = \"First post\"\ndate = \"2024-03-04\"\n+++\n\n# Hello\n",
    )
    .unwrap();

    // Unique per test so the process-wide sitemap cache never bleeds between
    // tests running in parallel.
    let site_name = format!(
        "sitemap-test-{}",
        temp_dir.path().file_name().unwrap().to_string_lossy()
    );

    let config = SiteConfig {
        name: site_name,
        base_url: Some("https://example.com".to_string()),
        cookie_secret: "test-cookie-secret".to_string(),
        templates: TemplateConfig {
            directories: vec![templates_dir.to_string_lossy().to_string()],
        },
        static_files: StaticConfig {
            directories: vec![static_dir.to_string_lossy().to_string()],
            use_redirects: false,
        },
        galleries: Some(vec![GallerySystemConfig {
            name: "main".to_string(),
            source_directory: photos_dir.to_string_lossy().to_string(),
            cache_directory: cache_dir.to_string_lossy().to_string(),
            gallery_template: "gallery.html.liquid".to_string(),
            image_detail_template: "image_detail.html.liquid".to_string(),
            image_indexing: ImageIndexingMode::Filename,
            ..Default::default()
        }]),
        posts: Some(vec![PostsSystemConfig {
            name: "blog".to_string(),
            source_directory: posts_dir.to_string_lossy().to_string(),
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
async fn sitemap_lists_public_resources() {
    let (_temp_dir, server) = setup_test_server().await;

    let response = server.get("/sitemap.xml").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/xml")
    );

    let xml = response.text();
    assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));

    // Static pages discovered from templates/pages/.
    assert!(xml.contains("<loc>https://example.com/</loc>"));
    assert!(xml.contains("<loc>https://example.com/about</loc>"));
    assert!(xml.contains("<loc>https://example.com/contact</loc>"));

    // Gallery root and the publicly visible subfolder.
    assert!(xml.contains("<loc>https://example.com/gallery</loc>"));
    assert!(xml.contains("<loc>https://example.com/gallery/vacation</loc>"));

    // Image detail pages.
    assert!(xml.contains("https://example.com/gallery/detail/"));

    // Posts index and the post itself (with its publish date as lastmod).
    assert!(xml.contains("<loc>https://example.com/blog</loc>"));
    assert!(xml.contains("<loc>https://example.com/blog/hello-world</loc>"));
    assert!(xml.contains("<lastmod>2024-03-04T00:00:00+00:00</lastmod>"));

    // The restricted folder and its images must not be exposed.
    assert!(!xml.contains("/gallery/secret"));
    assert!(!xml.contains("delta.jpg"));
}

#[tokio::test]
async fn sitemap_percent_encodes_urls_with_spaces() {
    let (_temp_dir, server) = setup_test_server().await;

    let response = server.get("/sitemap.xml").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let xml = response.text();

    // Folder URLs are percent-encoded.
    assert!(
        xml.contains("<loc>https://example.com/gallery/big%20sur</loc>"),
        "folder URL with a space should be percent-encoded:\n{xml}"
    );
    // Regression: image-detail URLs must be percent-encoded too, matching folders.
    assert!(
        xml.contains("https://example.com/gallery/detail/big%20sur/"),
        "image-detail URL with a space should be percent-encoded:\n{xml}"
    );
    // No <loc> may contain a raw space, which is invalid in a sitemap and 404s.
    assert!(
        !xml.contains("/gallery/detail/big sur/"),
        "image-detail URL must not contain a raw space:\n{xml}"
    );
}

#[tokio::test]
async fn robots_txt_references_sitemap() {
    let (_temp_dir, server) = setup_test_server().await;

    let response = server.get("/robots.txt").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(
        response
            .text()
            .contains("Sitemap: https://example.com/sitemap.xml")
    );
}

#[tokio::test]
async fn sitemap_chunk_is_not_found_for_small_site() {
    let (_temp_dir, server) = setup_test_server().await;

    // A small site serves a single <urlset> at /sitemap.xml, so no chunk files
    // exist.
    let response = server.get("/sitemap/pages-1.xml").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);

    let response = server.get("/sitemap/nope.xml").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}
