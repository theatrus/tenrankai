use std::sync::Arc;

use crate::GallerySystemConfig;
use crate::admin::format_size;
use crate::gallery::Gallery;
use crate::gallery::ImageSize;
use crate::gallery::{CacheFile, CacheFileKind};
use crate::storage;

async fn open_gallery(
    gallery_config: &GallerySystemConfig,
) -> Result<Arc<Gallery>, Box<dyn std::error::Error>> {
    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let mut gallery = Gallery::new(gallery_config.clone(), source_storage, cache_storage);

    gallery.load_image_watermark().await?;
    if let Some(watermark) = &gallery_config.image_watermark
        && gallery.get_image_watermark_hash().is_none()
    {
        return Err(format!(
            "Image watermark '{}' could not be loaded, so cache filenames cannot be computed",
            watermark.image
        )
        .into());
    }

    Ok(Arc::new(gallery))
}

async fn load_metadata(gallery: &Arc<Gallery>) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize gallery to load metadata cache
    if let Err(e) = gallery.initialize_and_check_version().await {
        eprintln!("Warning: Failed to initialize gallery metadata: {}", e);
    }

    // If cache is empty, refresh metadata first
    if gallery.is_metadata_cache_empty().await {
        println!("Metadata cache is empty, refreshing...");
        gallery.clone().refresh_all_metadata().await?;
    }

    Ok(())
}

fn print_cache_files(files: &[CacheFile], dry_run: bool) {
    if dry_run {
        for file in files {
            println!(
                "  Would delete: {} ({})",
                file.path,
                format_size(file.size.unwrap_or(0))
            );
        }
    }

    let (tiles, resized): (Vec<&CacheFile>, Vec<&CacheFile>) = files
        .iter()
        .partition(|file| file.kind == CacheFileKind::Tile);
    let bytes = |files: &[&CacheFile]| files.iter().filter_map(|file| file.size).sum();

    println!(
        "{} {} cache file(s), {}",
        if dry_run { "Would delete" } else { "Deleted" },
        files.len(),
        format_size(bytes(&resized) + bytes(&tiles))
    );
    println!(
        "  Resized images: {} ({})",
        resized.len(),
        format_size(bytes(&resized))
    );
    println!(
        "  Zoom tiles: {} ({})",
        tiles.len(),
        format_size(bytes(&tiles))
    );
}

async fn invalidate_images(
    gallery: &Gallery,
    image_paths: &[String],
    size_filter: Option<&[ImageSize]>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = gallery.list_cache_entries().await?;
    let (matched, _) = gallery.partition_cache_files(
        entries,
        image_paths,
        size_filter.unwrap_or(ImageSize::ALL),
        size_filter.is_none(),
    );

    let files = if dry_run {
        matched
    } else {
        gallery.delete_cache_files(matched).await
    };
    print_cache_files(&files, dry_run);

    Ok(())
}

/// Report format coverage for a gallery's image cache
pub async fn report(
    gallery_config: &GallerySystemConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let gallery = open_gallery(gallery_config).await?;
    load_metadata(&gallery).await?;

    // Run the coverage report
    gallery.report_format_coverage().await?;

    Ok(())
}

/// Validate and clean up outdated cache entries
pub async fn cleanup(
    gallery_config: &GallerySystemConfig,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let gallery = open_gallery(gallery_config).await?;
    load_metadata(&gallery).await?;

    if gallery.is_metadata_cache_empty().await {
        return Err(
            "No images found in the gallery source; refusing to treat the whole cache as orphaned"
                .into(),
        );
    }

    let files = gallery.remove_orphaned_cache_files(dry_run).await?;
    print_cache_files(&files, dry_run);

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
    size_filter: Option<&[ImageSize]>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let gallery = open_gallery(gallery_config).await?;

    if let Some(filter) = size_filter {
        let names: Vec<_> = filter.iter().map(|s| s.as_str()).collect();
        println!(
            "Looking for image cache files for '{}' (sizes: {})...",
            path,
            names.join(", ")
        );
    } else {
        println!("Looking for image cache files for '{}'...", path);
    }

    invalidate_images(&gallery, &[path.to_string()], size_filter, dry_run).await
}

/// Invalidate all image cache files for a folder in the gallery
pub async fn invalidate_folder(
    gallery_config: &GallerySystemConfig,
    folder_path: &str,
    size_filter: Option<&[ImageSize]>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let gallery = open_gallery(gallery_config).await?;

    // Normalize folder path (remove leading/trailing slashes)
    let folder_path = folder_path.trim_matches('/');
    let folder_label = if folder_path.is_empty() {
        "(root)"
    } else {
        folder_path
    };

    println!(
        "Looking for images in folder '{}' for gallery '{}'...",
        folder_label, gallery_config.name
    );

    // List all files recursively in the source folder (paths are relative to the folder)
    let image_paths: Vec<String> = gallery
        .source_storage()
        .list_recursive(folder_path)
        .await?
        .into_iter()
        .filter(|entry| !entry.is_dir && gallery.is_media(&entry.path))
        .map(|entry| {
            if folder_path.is_empty() {
                entry.path
            } else {
                format!("{}/{}", folder_path, entry.path)
            }
        })
        .collect();

    if image_paths.is_empty() {
        println!("No images found in '{}'", folder_label);
        return Ok(());
    }

    if let Some(filter) = size_filter {
        let names: Vec<_> = filter.iter().map(|s| s.as_str()).collect();
        println!(
            "Found {} image(s) in folder (filtering sizes: {})",
            image_paths.len(),
            names.join(", ")
        );
    } else {
        println!("Found {} image(s) in folder", image_paths.len());
    }

    invalidate_images(&gallery, &image_paths, size_filter, dry_run).await
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
