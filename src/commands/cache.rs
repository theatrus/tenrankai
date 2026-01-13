use std::sync::Arc;

use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use crate::storage;

/// Report format coverage for a gallery's image cache
pub async fn report(
    gallery_config: &GallerySystemConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(gallery_config.clone(), cache_storage));

    // Initialize gallery to load metadata cache
    if let Err(e) = gallery.initialize_and_check_version().await {
        eprintln!("Warning: Failed to initialize gallery metadata: {}", e);
    }

    // If cache is empty, refresh metadata first
    if gallery.is_metadata_cache_empty().await {
        println!("Metadata cache is empty, refreshing...");
        gallery.clone().refresh_all_metadata().await?;
    }

    // Run the coverage report
    gallery.report_format_coverage().await?;

    Ok(())
}

/// Validate and clean up outdated cache entries
pub async fn cleanup(
    gallery_config: &GallerySystemConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(gallery_config.clone(), cache_storage));

    // Initialize gallery to load metadata cache
    if let Err(e) = gallery.initialize_and_check_version().await {
        eprintln!("Warning: Failed to initialize gallery metadata: {}", e);
    }

    // If cache is empty, refresh metadata first
    if gallery.is_metadata_cache_empty().await {
        println!("Metadata cache is empty, refreshing...");
        gallery.clone().refresh_all_metadata().await?;
    }

    // Run cache validation and cleanup
    gallery.validate_and_cleanup_cache().await?;

    Ok(())
}

/// Invalidate composite cache files for a gallery path
pub async fn invalidate_composite(
    gallery_config: &GallerySystemConfig,
    path: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(gallery_config.clone(), cache_storage.clone()));

    // Use the gallery's method to generate the composite cache key prefix
    let composite_key = gallery.generate_composite_cache_key_with_context(path);

    println!(
        "Looking for composite cache files matching gallery '{}' path '{}'...",
        gallery_config.name, path
    );
    println!("  Pattern: {}*.jpg", composite_key);

    let mut deleted_count = 0;
    let mut found_count = 0;

    // List files from storage
    let entries = cache_storage.list("").await?;
    for entry in entries {
        if entry.path.starts_with(&composite_key) && entry.path.ends_with(".jpg") {
            found_count += 1;
            if dry_run {
                println!("  Would delete: {}", entry.path);
            } else {
                match cache_storage.delete(&entry.path).await {
                    Ok(_) => {
                        println!("  Deleted: {}", entry.path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("  Failed to delete {}: {}", entry.path, e);
                    }
                }
            }
        }
    }

    if dry_run {
        if found_count == 0 {
            println!("No matching composite cache files found");
        } else {
            println!(
                "Dry run complete - {} file(s) would be deleted",
                found_count
            );
        }
    } else if deleted_count == 0 {
        println!("No matching composite cache files found");
    } else {
        println!("Deleted {} composite cache file(s)", deleted_count);
    }

    Ok(())
}

/// Invalidate image cache files for a specific image
pub async fn invalidate_image(
    gallery_config: &GallerySystemConfig,
    path: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(gallery_config.clone(), cache_storage.clone()));

    // Generate the hash prefix for this image path
    let hash = gallery.generate_cache_key(path, "");

    println!(
        "Looking for image cache files for '{}' (hash prefix: {})...",
        path,
        &hash[..8.min(hash.len())]
    );

    let mut deleted_count = 0;
    let mut found_count = 0;

    // List files from storage
    let entries = cache_storage.list("").await?;
    for entry in entries {
        // Check if file starts with the hash (handles all formats and watermarked variants)
        if entry.path.starts_with(&hash) {
            found_count += 1;
            if dry_run {
                println!("  Would delete: {}", entry.path);
            } else {
                match cache_storage.delete(&entry.path).await {
                    Ok(_) => {
                        println!("  Deleted: {}", entry.path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("  Failed to delete {}: {}", entry.path, e);
                    }
                }
            }
        }
    }

    if dry_run {
        if found_count == 0 {
            println!("No matching image cache files found");
        } else {
            println!(
                "Dry run complete - {} file(s) would be deleted",
                found_count
            );
        }
    } else if deleted_count == 0 {
        println!("No matching image cache files found");
    } else {
        println!("Deleted {} image cache file(s)", deleted_count);
    }

    Ok(())
}

/// List all composite cache files for a gallery
pub async fn list_composites(
    gallery_config: &GallerySystemConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery_name = &gallery_config.name;
    let pattern = format!("composite_{}_", gallery_name);

    println!("Composite cache files for gallery '{}':", gallery_name);

    let mut count = 0;
    // List files from storage
    let entries = cache_storage.list("").await?;
    for entry in entries {
        if entry.path.starts_with(&pattern) && entry.path.ends_with(".jpg") {
            // Try to decode the path from the filename
            let path_info = decode_composite_filename(&entry.path, gallery_name);
            println!("  {} {}", entry.path, path_info);
            count += 1;
        }
    }

    if count == 0 {
        println!("  (none)");
    } else {
        println!("\nTotal: {} composite file(s)", count);
    }

    Ok(())
}

/// Try to decode the gallery path from a composite filename
fn decode_composite_filename(filename: &str, gallery_name: &str) -> String {
    use base64::{Engine as _, engine::general_purpose};

    let prefix = format!("composite_{}_", gallery_name);
    if let Some(rest) = filename.strip_prefix(&prefix) {
        // Remove the hash suffix and .jpg extension
        // Format: {path_key}_{hash}.jpg
        if let Some(last_underscore) = rest.rfind('_') {
            let path_key = &rest[..last_underscore];
            if path_key == "root" {
                return "(root)".to_string();
            }
            // Try to decode base64
            if let Ok(decoded) = general_purpose::URL_SAFE_NO_PAD.decode(path_key)
                && let Ok(path) = String::from_utf8(decoded)
            {
                return format!("-> {}", path);
            }
        }
    }
    String::new()
}
