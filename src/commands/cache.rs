use std::sync::Arc;

use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use crate::storage;

/// Report format coverage for a gallery's image cache
pub async fn report(
    gallery_config: &GallerySystemConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage,
        cache_storage,
    ));

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
    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage,
        cache_storage,
    ));

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
    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage,
        cache_storage.clone(),
    ));

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
    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage,
        cache_storage.clone(),
    ));

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

/// Invalidate all image cache files for a folder in the gallery
pub async fn invalidate_folder(
    gallery_config: &GallerySystemConfig,
    folder_path: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::gallery::ImageSize;
    use std::collections::HashSet;

    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage.clone(),
        cache_storage.clone(),
    ));

    // Normalize folder path (remove leading/trailing slashes)
    let folder_path = folder_path.trim_matches('/');

    println!(
        "Looking for images in folder '{}' for gallery '{}'...",
        if folder_path.is_empty() {
            "(root)"
        } else {
            folder_path
        },
        gallery_config.name
    );

    // List all files in the source folder
    let entries = source_storage.list(folder_path).await?;

    // Filter to image files only
    let image_extensions = ["jpg", "jpeg", "png", "webp", "avif", "gif", "heic", "heif"];
    let image_files: Vec<_> = entries
        .into_iter()
        .filter(|e| !e.is_dir)
        .filter(|e| {
            e.path
                .rsplit('.')
                .next()
                .map(|ext| image_extensions.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();

    if image_files.is_empty() {
        println!("No images found in folder '{}'", folder_path);
        return Ok(());
    }

    println!("Found {} image(s) in folder", image_files.len());

    // Generate all possible cache hashes for each image
    let mut hashes_to_delete: HashSet<String> = HashSet::new();

    for entry in &image_files {
        // Generate hashes for all size/format/watermark combinations
        for size in ImageSize::ALL {
            for format in &["jpg", "webp", "png", "avif"] {
                // Non-watermarked variant
                let hash = gallery
                    .generate_cache_key(&entry.path, &format!("{}_{}", size.as_str(), format));
                hashes_to_delete.insert(hash);

                // Watermarked variant (only for sizes that support it)
                if size.supports_watermark() {
                    let hash_wm = gallery.generate_cache_key(
                        &entry.path,
                        &format!("{}_{}_watermarked", size.as_str(), format),
                    );
                    hashes_to_delete.insert(hash_wm);
                }
            }
        }
    }

    println!(
        "Generated {} cache key patterns to search for",
        hashes_to_delete.len()
    );

    // List all cache files and delete matching ones
    let cache_entries = cache_storage.list("").await?;

    let mut deleted_count = 0;
    let mut found_count = 0;

    for cache_entry in cache_entries {
        // Skip metadata files and composites
        if cache_entry.path.starts_with("composite_")
            || cache_entry.path == "metadata_cache.json"
            || cache_entry.path == "cache_metadata.json"
        {
            continue;
        }

        // Check if the cache filename (without extension) matches any of our hashes
        // Cache files are named: {hash}.{extension} or {hash}_watermarked.{extension}
        let filename_without_ext = cache_entry
            .path
            .rsplit('.')
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(".");

        // The hash is the first part before any underscore suffix
        let base_filename =
            if let Some(stripped) = filename_without_ext.strip_suffix("_watermarked") {
                stripped
            } else {
                &filename_without_ext
            };

        // Check if this hash matches any of our target hashes
        if hashes_to_delete.contains(base_filename)
            || hashes_to_delete.contains(&filename_without_ext)
        {
            found_count += 1;
            if dry_run {
                println!("  Would delete: {}", cache_entry.path);
            } else {
                match cache_storage.delete(&cache_entry.path).await {
                    Ok(_) => {
                        println!("  Deleted: {}", cache_entry.path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("  Failed to delete {}: {}", cache_entry.path, e);
                    }
                }
            }
        }
    }

    if dry_run {
        if found_count == 0 {
            println!("No matching cache files found for images in folder");
        } else {
            println!(
                "Dry run complete - {} file(s) would be deleted for {} image(s)",
                found_count,
                image_files.len()
            );
        }
    } else if deleted_count == 0 {
        println!("No matching cache files found for images in folder");
    } else {
        println!(
            "Deleted {} cache file(s) for {} image(s)",
            deleted_count,
            image_files.len()
        );
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
