use std::sync::Arc;

use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use crate::gallery::ImageSize;
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
    size_filter: Option<&[ImageSize]>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::gallery::generate_tile_cache_filename;
    use std::collections::HashSet;

    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage.clone(),
        cache_storage.clone(),
    ));

    let sizes: &[ImageSize] = size_filter.unwrap_or(ImageSize::ALL);

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

    // Generate all possible cache hashes for this image
    // Use the same method as the serving code (generate_image_cache_key)
    let mut hashes_to_delete: HashSet<String> = HashSet::new();
    let mut tile_filenames_to_delete: HashSet<String> = HashSet::new();

    // Standard sizes (thumbnail, gallery, medium, large + retina variants)
    for size in sizes {
        for format in &["jpg", "webp", "png", "avif"] {
            // Non-watermarked variant
            let hash = gallery.generate_image_cache_key(path, &size.as_str(), format, false);
            hashes_to_delete.insert(hash);

            // Watermarked variant (only for sizes that support it)
            if size.supports_watermark() {
                let hash_wm = gallery.generate_image_cache_key(path, &size.as_str(), format, true);
                hashes_to_delete.insert(hash_wm);
            }
        }
    }

    // Handle tiles if configured (only when no size filter, since tiles are their own size class)
    if size_filter.is_none()
        && let Some(ref tile_config) = gallery_config.tiles
    {
        let tile_size = tile_config.tile_size;

        // Try to get image dimensions from source to calculate tile coordinates
        // Read the image to get dimensions (needed to know how many tiles exist)
        match source_storage.read(path).await {
            Ok(data) => {
                if let Ok(reader) =
                    image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format()
                    && let Ok((width, height)) = reader.into_dimensions()
                {
                    let max_tile_x = width.div_ceil(tile_size);
                    let max_tile_y = height.div_ceil(tile_size);

                    println!(
                        "Image {}x{}, tile size {}, generating tile patterns for {}x{} grid",
                        width, height, tile_size, max_tile_x, max_tile_y
                    );

                    for tile_y in 0..max_tile_y {
                        for tile_x in 0..max_tile_x {
                            for format in &["jpg", "webp", "png", "avif"] {
                                // Standard tile
                                let tile_filename = generate_tile_cache_filename(
                                    path, tile_x, tile_y, tile_size, false, format,
                                );
                                tile_filenames_to_delete.insert(tile_filename);

                                // Retina tile
                                let tile_filename_retina = generate_tile_cache_filename(
                                    path, tile_x, tile_y, tile_size, true, format,
                                );
                                tile_filenames_to_delete.insert(tile_filename_retina);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Could not read source image to determine tile count: {}",
                    e
                );
                eprintln!("Tile cache files will not be invalidated for this image.");
            }
        }
    }

    println!(
        "Generated {} standard cache keys and {} tile filenames to search for",
        hashes_to_delete.len(),
        tile_filenames_to_delete.len()
    );

    let mut deleted_count = 0;
    let mut found_count = 0;

    // List files from storage
    let entries = cache_storage.list("").await?;
    for entry in entries {
        // Skip metadata files and composites
        if entry.path.starts_with("composite_")
            || entry.path == "metadata_cache.json"
            || entry.path == "cache_metadata.json"
        {
            continue;
        }

        // Check for tile files (exact filename match)
        if entry.path.starts_with("tile_") && tile_filenames_to_delete.contains(&entry.path) {
            found_count += 1;
            if dry_run {
                println!("  Would delete tile: {}", entry.path);
            } else {
                match cache_storage.delete(&entry.path).await {
                    Ok(_) => {
                        println!("  Deleted tile: {}", entry.path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("  Failed to delete {}: {}", entry.path, e);
                    }
                }
            }
            continue;
        }

        // Check if the cache filename (without extension) matches any of our hashes
        // Cache files are named: {hash}.{extension} or {hash}_watermarked.{extension}
        let filename_without_ext = entry
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
    size_filter: Option<&[ImageSize]>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::gallery::generate_tile_cache_filename;
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

    let sizes: &[ImageSize] = size_filter.unwrap_or(ImageSize::ALL);

    if let Some(filter) = size_filter {
        let names: Vec<_> = filter.iter().map(|s| s.as_str()).collect();
        println!(
            "Found {} image(s) in folder (filtering sizes: {})",
            image_files.len(),
            names.join(", ")
        );
    } else {
        println!("Found {} image(s) in folder", image_files.len());
    }

    // Generate all possible cache hashes for each image
    let mut hashes_to_delete: HashSet<String> = HashSet::new();
    let mut tile_filenames_to_delete: HashSet<String> = HashSet::new();

    // Get tile config if present
    let tile_config = gallery_config.tiles.as_ref();

    for entry in &image_files {
        // Construct full path (folder_path + filename)
        // Storage.list() returns just filenames, not full paths
        let full_path = if folder_path.is_empty() {
            entry.path.clone()
        } else {
            format!("{}/{}", folder_path, entry.path)
        };

        // Generate hashes for all size/format/watermark combinations
        // Use the same method as the serving code (generate_image_cache_key)
        for size in sizes {
            for format in &["jpg", "webp", "png", "avif"] {
                // Non-watermarked variant
                let hash =
                    gallery.generate_image_cache_key(&full_path, &size.as_str(), format, false);
                hashes_to_delete.insert(hash);

                // Watermarked variant (only for sizes that support it)
                if size.supports_watermark() {
                    let hash_wm =
                        gallery.generate_image_cache_key(&full_path, &size.as_str(), format, true);
                    hashes_to_delete.insert(hash_wm);
                }
            }
        }

        // Handle tiles if configured (only when no size filter)
        if size_filter.is_none()
            && let Some(tc) = tile_config
        {
            let tile_size = tc.tile_size;

            // Try to get image dimensions from source to calculate tile coordinates
            match source_storage.read(&full_path).await {
                Ok(data) => {
                    if let Ok(reader) =
                        image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format()
                        && let Ok((width, height)) = reader.into_dimensions()
                    {
                        let max_tile_x = width.div_ceil(tile_size);
                        let max_tile_y = height.div_ceil(tile_size);

                        for tile_y in 0..max_tile_y {
                            for tile_x in 0..max_tile_x {
                                for format in &["jpg", "webp", "png", "avif"] {
                                    // Standard tile
                                    let tile_filename = generate_tile_cache_filename(
                                        &full_path, tile_x, tile_y, tile_size, false, format,
                                    );
                                    tile_filenames_to_delete.insert(tile_filename);

                                    // Retina tile
                                    let tile_filename_retina = generate_tile_cache_filename(
                                        &full_path, tile_x, tile_y, tile_size, true, format,
                                    );
                                    tile_filenames_to_delete.insert(tile_filename_retina);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not read '{}' for tile calculation: {}",
                        full_path, e
                    );
                }
            }
        }
    }

    println!(
        "Generated {} standard cache keys and {} tile filenames to search for",
        hashes_to_delete.len(),
        tile_filenames_to_delete.len()
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

        // Check for tile files (exact filename match)
        if cache_entry.path.starts_with("tile_")
            && tile_filenames_to_delete.contains(&cache_entry.path)
        {
            found_count += 1;
            if dry_run {
                println!("  Would delete tile: {}", cache_entry.path);
            } else {
                match cache_storage.delete(&cache_entry.path).await {
                    Ok(_) => {
                        println!("  Deleted tile: {}", cache_entry.path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("  Failed to delete {}: {}", cache_entry.path, e);
                    }
                }
            }
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
