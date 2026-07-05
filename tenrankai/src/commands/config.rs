use std::path::Path;
use tenrankai_config_storage::{
    ConfigStorageUrl, StoredGalleryConfig, StoredImageSizeConfig, StoredPostsConfig,
    StoredPreviewConfig, StoredSiteConfig, create_config_storage,
};
use tokio::fs;
use tracing::info;

pub async fn handle_init_command(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let base_path = Path::new(path);

    // Create directory structure
    let sites_path = base_path.join("sites").join("default");
    let galleries_path = sites_path.join("galleries");
    let posts_path = sites_path.join("posts");

    fs::create_dir_all(&galleries_path).await?;
    fs::create_dir_all(&posts_path).await?;

    // Create default site.toml
    let site_config = r#"# Default Site Configuration
#
# This site handles any hostname not matched by other sites.

# Hostnames that route to this site
# "*" is the catch-all default
hostnames = ["*"]

# Base URL for this site (used for login links, OpenGraph tags)
# base_url = "https://example.com"

# Template directories (first match wins)
templates = ["templates"]

# Static file directories (first match wins)
static_files = ["static"]

# User database path/URL (optional)
# user_database = "users.toml"

# Storage prefix for gallery/posts source paths
# All paths in gallery configs are relative to this prefix
# storage_prefix = "/data/sites/default"
"#;
    fs::write(sites_path.join("site.toml"), site_config).await?;

    // Create default gallery config
    let gallery_config = r#"# Main Gallery Configuration
name = "main"
url_prefix = "/gallery"

# Source and cache directories
# If storage_prefix is set in site.toml, these are relative to it
source_directory = "photos"
cache_directory = "cache/main"

# Templates
gallery_template = "modules/gallery.html.liquid"
image_detail_template = "modules/image_detail.html.liquid"

# Image quality
jpeg_quality = 85
webp_quality = 85.0

# Image sizes
[thumbnail]
width = 300
height = 300

[gallery_size]
width = 800
height = 800

[medium]
width = 1200
height = 1200

[large]
width = 1600
height = 1600

# Preview configuration
[preview]
max_images = 6
max_depth = 3
max_per_folder = 3
"#;
    fs::write(galleries_path.join("main.toml"), gallery_config).await?;

    // Create default permissions.toml
    let permissions_config = r#"# Site Permissions Configuration
#
# These permissions apply to all galleries in this site.

# Role assigned to unauthenticated users
# Use "none" to require authentication
public_role = "viewer"

# Role assigned to authenticated users without specific role assignments
default_authenticated_role = "viewer"

# Role definitions
[roles.viewer]
[roles.viewer.permissions]
can_view = true
can_download_medium = true

[roles.admin]
[roles.admin.permissions]
owner_access = true

# User role assignments (optional)
# [[user_roles]]
# username = "alice"
# roles = ["admin"]
"#;
    fs::write(sites_path.join("permissions.toml"), permissions_config).await?;

    info!("Initialized ConfigStorage at: {}", path);
    println!("Created ConfigStorage directory structure at: {}", path);
    println!();
    println!("Structure:");
    println!("  {}/", path);
    println!("    sites/");
    println!("      default/");
    println!("        site.toml          # Site configuration");
    println!("        permissions.toml   # Permissions configuration");
    println!("        galleries/");
    println!("          main.toml        # Gallery configuration");
    println!("        posts/             # Posts configurations (empty)");
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {}/sites/default/site.toml to set your base_url",
        path
    );
    println!("  2. Create your photos directory");
    println!("  3. Run: cargo run -- serve");

    Ok(())
}

pub async fn handle_list_sites_command(
    config_storage_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    let sites = storage.list_sites().await?;

    if sites.is_empty() {
        println!("No sites found in ConfigStorage.");
        println!("Use 'tenrankai config add-site <name>' to create a site.");
    } else {
        println!("Sites:");
        for site in sites {
            println!("  - {}", site);
        }
    }

    Ok(())
}

pub async fn handle_add_site_command(
    config_storage_url: &str,
    name: &str,
    hostnames: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    // Check if site already exists
    if storage.get_site_config(name).await?.is_some() {
        return Err(format!("Site '{}' already exists", name).into());
    }

    let hostnames = if hostnames.is_empty() {
        vec![format!("{}.localhost", name)]
    } else {
        hostnames
    };

    let site_config = StoredSiteConfig {
        hostnames: hostnames.clone(),
        templates: vec!["templates".to_string()],
        static_files: vec!["static".to_string()],
        static_use_redirects: false,
        user_database: None,
        base_url: None,
        cookie_secret: None,
        storage_prefix: None,
        cache_prefix: None,
        email: None,
        theme: None,
        site_title: None,
        copyright_holder: None,
    };

    storage.set_site_config(name, &site_config, "cli").await?;

    println!("Created site: {}", name);
    println!("  Hostnames: {}", hostnames.join(", "));
    println!();
    println!("Next steps:");
    println!(
        "  1. Add a gallery: tenrankai config add-gallery --site {} <gallery-name>",
        name
    );
    println!("  2. Edit the site configuration in ConfigStorage");

    Ok(())
}

pub async fn handle_list_galleries_command(
    config_storage_url: &str,
    site: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    let galleries = storage.list_galleries(site).await?;

    if galleries.is_empty() {
        println!("No galleries found for site '{}'.", site);
        println!(
            "Use 'tenrankai config add-gallery --site {} <name>' to create a gallery.",
            site
        );
    } else {
        println!("Galleries for site '{}':", site);
        for gallery in galleries {
            if let Ok(Some(config)) = storage.get_gallery_full_config(site, &gallery).await {
                println!(
                    "  - {} (url: {}, source: {})",
                    gallery, config.url_prefix, config.source_directory
                );
            } else {
                println!("  - {}", gallery);
            }
        }
    }

    Ok(())
}

pub async fn handle_add_gallery_command(
    config_storage_url: &str,
    site: &str,
    name: &str,
    source_directory: Option<String>,
    url_prefix: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    // Check if gallery already exists
    if storage.get_gallery_full_config(site, name).await?.is_some() {
        return Err(format!("Gallery '{}' already exists in site '{}'", name, site).into());
    }

    let source_dir = source_directory.unwrap_or_else(|| name.to_string());
    let url_prefix = url_prefix.unwrap_or_else(|| format!("/{}", name));
    let cache_dir = format!("cache/{}", name);

    let gallery_config = StoredGalleryConfig {
        name: name.to_string(),
        url_prefix: url_prefix.clone(),
        source_directory: source_dir.clone(),
        cache_directory: cache_dir.clone(),
        gallery_template: "modules/gallery.html.liquid".to_string(),
        image_detail_template: "modules/image_detail.html.liquid".to_string(),
        jpeg_quality: Some(85),
        webp_quality: Some(85.0),
        thumbnail: StoredImageSizeConfig {
            width: 300,
            height: 300,
        },
        gallery_size: StoredImageSizeConfig {
            width: 800,
            height: 800,
        },
        medium: StoredImageSizeConfig {
            width: 1200,
            height: 1200,
        },
        large: StoredImageSizeConfig {
            width: 1600,
            height: 1600,
        },
        preview: Some(StoredPreviewConfig {
            max_images: 6,
            max_depth: 3,
            max_per_folder: 3,
        }),
        cache_refresh_interval_minutes: None,
        new_threshold_days: None,
        copyright_holder: None,
        image_watermark: None,
        image_indexing: "filename".to_string(),
        metadata_cache_size: 1000,
        tiles: None,
        pregenerate: None,
        grid_mode: Default::default(),
        max_columns: None,
    };

    storage
        .set_gallery_full_config(site, name, &gallery_config, "cli")
        .await?;

    println!("Created gallery: {}", name);
    println!("  Site: {}", site);
    println!("  URL prefix: {}", url_prefix);
    println!("  Source directory: {}", source_dir);
    println!("  Cache directory: {}", cache_dir);

    Ok(())
}

pub async fn handle_list_posts_command(
    config_storage_url: &str,
    site: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    let posts = storage.list_posts(site).await?;

    if posts.is_empty() {
        println!("No posts configurations found for site '{}'.", site);
        println!(
            "Use 'tenrankai config add-posts --site {} <name>' to create a posts configuration.",
            site
        );
    } else {
        println!("Posts for site '{}':", site);
        for post in posts {
            if let Ok(Some(config)) = storage.get_posts_config(site, &post).await {
                println!(
                    "  - {} (url: {}, source: {})",
                    post, config.url_prefix, config.source_directory
                );
            } else {
                println!("  - {}", post);
            }
        }
    }

    Ok(())
}

pub async fn handle_add_posts_command(
    config_storage_url: &str,
    site: &str,
    name: &str,
    source_directory: Option<String>,
    url_prefix: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = ConfigStorageUrl::parse(config_storage_url)?;
    let storage = create_config_storage(&url).await?;

    // Check if posts config already exists
    if storage.get_posts_config(site, name).await?.is_some() {
        return Err(format!(
            "Posts configuration '{}' already exists in site '{}'",
            name, site
        )
        .into());
    }

    let source_dir = source_directory.unwrap_or_else(|| format!("posts/{}", name));
    let url_prefix = url_prefix.unwrap_or_else(|| format!("/{}", name));

    let posts_config = StoredPostsConfig {
        name: name.to_string(),
        source_directory: source_dir.clone(),
        url_prefix: url_prefix.clone(),
        index_template: "modules/posts_index.html.liquid".to_string(),
        post_template: "modules/post_detail.html.liquid".to_string(),
        posts_per_page: 20,
        refresh_interval_minutes: None,
    };

    storage
        .set_posts_config(site, name, &posts_config, "cli")
        .await?;

    println!("Created posts configuration: {}", name);
    println!("  Site: {}", site);
    println!("  URL prefix: {}", url_prefix);
    println!("  Source directory: {}", source_dir);

    Ok(())
}
