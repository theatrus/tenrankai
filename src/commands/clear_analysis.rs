use crate::Config;
use crate::gallery::Gallery;
use crate::metadata_storage::{MetadataStorage, SidecarMetadataStorage};
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

/// Handle the clear-analysis CLI command
pub async fn handle_clear_analysis_command(
    config: Config,
    gallery_name: String,
    folder: Option<String>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Find the gallery
    let gallery_configs = config.galleries.as_ref().ok_or("No galleries configured")?;

    let gallery_config = gallery_configs
        .iter()
        .find(|g| g.name == gallery_name)
        .ok_or_else(|| format!("Gallery '{}' not found", gallery_name))?;

    let gallery = Arc::new(Gallery::new(gallery_config.clone()));

    // Create metadata storage
    let metadata_storage = SidecarMetadataStorage::new();

    // Get list of images with AI analysis
    let images = collect_images_with_analysis(
        &gallery,
        &metadata_storage,
        &gallery_config.source_directory,
        folder.as_deref(),
    )
    .await?;

    if images.is_empty() {
        println!("No images with AI analysis found.");
        return Ok(());
    }

    println!(
        "Found {} images with AI analysis{}",
        images.len(),
        if dry_run { " (dry run)" } else { "" }
    );

    if dry_run {
        println!("\nImages that would have AI analysis cleared:");
        for (i, image_path) in images.iter().enumerate() {
            println!("  {}. {}", i + 1, image_path);
        }
        return Ok(());
    }

    let mut success_count = 0;
    let mut error_count = 0;

    for (i, relative_path) in images.iter().enumerate() {
        let full_path = gallery_config.source_directory.join(relative_path);

        println!("[{}/{}] Clearing: {}", i + 1, images.len(), relative_path);

        match clear_single_image_analysis(&metadata_storage, &full_path).await {
            Ok(()) => {
                success_count += 1;
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
                error_count += 1;
            }
        }
    }

    println!("\nClear complete:");
    println!("  Successful: {}", success_count);
    println!("  Errors: {}", error_count);

    Ok(())
}

/// Collect list of images that have AI analysis
async fn collect_images_with_analysis(
    gallery: &Gallery,
    metadata_storage: &SidecarMetadataStorage,
    source_directory: &Path,
    folder: Option<&str>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut images = Vec::new();

    // Scan the gallery directory
    let scan_path = folder.unwrap_or("");
    let items = gallery.scan_directory(scan_path).await?;

    // Recursively collect image files with AI analysis
    collect_analyzed_images_recursive(
        gallery,
        metadata_storage,
        source_directory,
        &items,
        scan_path,
        &mut images,
    )
    .await?;

    Ok(images)
}

/// Recursively collect images that have AI analysis
async fn collect_analyzed_images_recursive(
    gallery: &Gallery,
    metadata_storage: &SidecarMetadataStorage,
    source_directory: &Path,
    items: &[crate::gallery::GalleryItem],
    current_path: &str,
    images: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for item in items {
        if item.is_directory {
            // This is a folder - recursively scan it
            let folder_path = if current_path.is_empty() {
                item.name.clone()
            } else {
                format!("{}/{}", current_path, item.name)
            };

            // Recursively scan subfolder
            if let Ok(sub_items) = gallery.scan_directory(&folder_path).await {
                Box::pin(collect_analyzed_images_recursive(
                    gallery,
                    metadata_storage,
                    source_directory,
                    &sub_items,
                    &folder_path,
                    images,
                ))
                .await?;
            }
        } else {
            // This is an image
            let relative_path = if current_path.is_empty() {
                item.name.clone()
            } else {
                format!("{}/{}", current_path, item.name)
            };

            // Check if it has AI analysis
            let full_path = source_directory.join(&relative_path);
            if let Ok(Some(metadata)) = metadata_storage.load(&full_path).await
                && metadata.has_ai_analysis()
            {
                debug!("Found analyzed image: {}", relative_path);
                images.push(relative_path);
            }
        }
    }

    Ok(())
}

/// Clear AI analysis from a single image
async fn clear_single_image_analysis(
    metadata_storage: &SidecarMetadataStorage,
    full_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load existing metadata
    let mut metadata = metadata_storage
        .load(full_path)
        .await?
        .ok_or("No metadata found for image")?;

    // Clear AI analysis
    metadata.clear_ai_analysis();

    // Save metadata (or delete if empty)
    if metadata.is_empty() {
        metadata_storage.delete(full_path).await?;
    } else {
        metadata_storage.save(full_path, &metadata).await?;
    }

    Ok(())
}
