use crate::Config;
use crate::gallery::Gallery;
use crate::metadata_storage::{MetadataStorage, SidecarMetadataStorage};
use crate::openai::{OpenAIClient, OpenAIError};
use base64::{Engine as _, engine::general_purpose};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Handle the analyze-images CLI command
pub async fn handle_analyze_command(
    config: Config,
    gallery_name: String,
    folder: Option<String>,
    limit: Option<usize>,
    force: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if OpenAI is configured
    let openai_config = config
        .openai
        .ok_or("OpenAI not configured. Add [openai] section to config.toml with api_key.")?;

    // Find the gallery
    let gallery_configs = config.galleries.as_ref().ok_or("No galleries configured")?;

    let gallery_config = gallery_configs
        .iter()
        .find(|g| g.name == gallery_name)
        .ok_or_else(|| format!("Gallery '{}' not found", gallery_name))?;

    let gallery = Arc::new(Gallery::new(gallery_config.clone()));

    // Create OpenAI client (unless dry run)
    let client = if !dry_run {
        Some(OpenAIClient::new(openai_config)?)
    } else {
        None
    };

    // Create metadata storage
    let metadata_storage = SidecarMetadataStorage::new();

    // Get list of images to analyze
    let images = collect_images_to_analyze(
        &gallery,
        &metadata_storage,
        &gallery_config.source_directory,
        folder.as_deref(),
        limit,
        force,
    )
    .await?;

    if images.is_empty() {
        println!("No images to analyze.");
        return Ok(());
    }

    println!(
        "Found {} images to analyze{}",
        images.len(),
        if dry_run { " (dry run)" } else { "" }
    );

    if dry_run {
        println!("\nImages that would be analyzed:");
        for (i, image_path) in images.iter().enumerate() {
            println!("  {}. {}", i + 1, image_path);
        }
        return Ok(());
    }

    let client = client.unwrap();
    let mut success_count = 0;
    let mut error_count = 0;

    for (i, relative_path) in images.iter().enumerate() {
        let full_path = gallery_config.source_directory.join(relative_path);

        println!("[{}/{}] Analyzing: {}", i + 1, images.len(), relative_path);

        match analyze_single_image(
            &client,
            &gallery,
            &metadata_storage,
            &full_path,
            relative_path,
        )
        .await
        {
            Ok(result) => {
                println!("  Keywords: {}", result.keywords.join(", "));
                println!("  Alt-text: {}", truncate_string(&result.alt_text, 80));
                success_count += 1;
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
                error_count += 1;

                // If rate limited, wait and potentially retry
                if let Some(OpenAIError::RateLimited { retry_after_ms }) =
                    e.downcast_ref::<OpenAIError>()
                {
                    warn!(
                        "Rate limited, waiting {}ms before continuing...",
                        retry_after_ms
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(*retry_after_ms)).await;
                }
            }
        }
    }

    println!("\nAnalysis complete:");
    println!("  Successful: {}", success_count);
    println!("  Errors: {}", error_count);

    Ok(())
}

/// Collect list of images that need analysis
async fn collect_images_to_analyze(
    gallery: &Gallery,
    metadata_storage: &SidecarMetadataStorage,
    source_directory: &Path,
    folder: Option<&str>,
    limit: Option<usize>,
    force: bool,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut images = Vec::new();

    // Scan the gallery directory
    let scan_path = folder.unwrap_or("");
    let items = gallery.scan_directory(scan_path).await?;

    // Recursively collect image files
    collect_images_recursive(
        gallery,
        metadata_storage,
        source_directory,
        &items,
        scan_path,
        &mut images,
        force,
    )
    .await?;

    // Apply limit if specified
    if let Some(max) = limit {
        images.truncate(max);
    }

    Ok(images)
}

/// Recursively collect images from gallery items
async fn collect_images_recursive(
    gallery: &Gallery,
    metadata_storage: &SidecarMetadataStorage,
    source_directory: &Path,
    items: &[crate::gallery::GalleryItem],
    current_path: &str,
    images: &mut Vec<String>,
    force: bool,
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
                Box::pin(collect_images_recursive(
                    gallery,
                    metadata_storage,
                    source_directory,
                    &sub_items,
                    &folder_path,
                    images,
                    force,
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

            // Check if already analyzed (unless force is set)
            if !force {
                let full_path = source_directory.join(&relative_path);
                if let Ok(Some(metadata)) = metadata_storage.load(&full_path).await
                    && metadata.has_ai_analysis()
                {
                    debug!("Skipping {} - already analyzed", relative_path);
                    continue;
                }
            }

            images.push(relative_path);
        }
    }

    Ok(())
}

/// Analyze a single image and save the results
async fn analyze_single_image(
    client: &OpenAIClient,
    gallery: &Gallery,
    metadata_storage: &SidecarMetadataStorage,
    full_path: &Path,
    relative_path: &str,
) -> Result<crate::openai::ImageAnalysisResult, Box<dyn std::error::Error>> {
    // Get or generate a medium-sized version of the image for API efficiency
    let image_data = get_image_for_analysis(gallery, full_path, relative_path).await?;

    // Encode as base64
    let base64_image = general_purpose::STANDARD.encode(&image_data);

    // Call OpenAI API
    let result = client
        .analyze_image_data(&base64_image, relative_path)
        .await?;

    // Load existing metadata or create new
    let mut metadata = metadata_storage.load(full_path).await?.unwrap_or_default();

    // Update with AI analysis results
    metadata.set_ai_analysis(result.keywords.clone(), result.alt_text.clone());

    // Save metadata
    metadata_storage.save(full_path, &metadata).await?;

    info!(
        "Saved AI analysis for {}: {} keywords",
        relative_path,
        result.keywords.len()
    );

    Ok(result)
}

/// Get image data for analysis (preferring cached medium size)
async fn get_image_for_analysis(
    gallery: &Gallery,
    full_path: &Path,
    relative_path: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Try to get cached medium-sized image first (more efficient for API)
    let cache_filename = gallery.generate_cache_filename(relative_path, "medium", "jpg", false);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);

    if cache_path.exists() {
        debug!("Using cached medium image for analysis");
        return Ok(tokio::fs::read(&cache_path).await?);
    }

    // Fall back to original image
    debug!("Using original image for analysis");
    Ok(tokio::fs::read(full_path).await?)
}

/// Truncate a string for display
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
