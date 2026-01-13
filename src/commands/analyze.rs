use crate::Config;
use crate::gallery::Gallery;
use crate::openai::{ImageContext, LocationContext, OpenAIClient, OpenAIError};
use crate::storage;
use base64::{Engine as _, engine::general_purpose};
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

    let source_storage =
        storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(
        gallery_config.clone(),
        source_storage,
        cache_storage,
    ));

    // Create OpenAI client (unless dry run)
    let client = if !dry_run {
        Some(OpenAIClient::new(openai_config)?)
    } else {
        None
    };

    // Get list of images to analyze (uses gallery's storage-backed metadata)
    let images = collect_images_to_analyze(&gallery, folder.as_deref(), limit, force).await?;

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
        println!("[{}/{}] Analyzing: {}", i + 1, images.len(), relative_path);

        match analyze_single_image(&client, &gallery, relative_path).await {
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
    folder: Option<&str>,
    limit: Option<usize>,
    force: bool,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut images = Vec::new();

    // Scan the gallery directory
    let scan_path = folder.unwrap_or("");
    let items = gallery.scan_directory(scan_path).await?;

    // Recursively collect image files
    collect_images_recursive(gallery, &items, scan_path, &mut images, force).await?;

    // Apply limit if specified
    if let Some(max) = limit {
        images.truncate(max);
    }

    Ok(images)
}

/// Recursively collect images from gallery items
async fn collect_images_recursive(
    gallery: &Gallery,
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
            if !force
                && let Ok(Some(metadata)) = gallery.user_metadata_storage.load(&relative_path).await
                && metadata.has_ai_analysis()
            {
                debug!("Skipping {} - already analyzed", relative_path);
                continue;
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
    relative_path: &str,
) -> Result<crate::openai::ImageAnalysisResult, Box<dyn std::error::Error>> {
    // Get or generate a medium-sized version of the image for API efficiency
    let image_data = get_image_for_analysis(gallery, relative_path).await?;

    // Encode as base64
    let base64_image = general_purpose::STANDARD.encode(&image_data);

    // Build context from available metadata
    let context = build_image_context(gallery, relative_path).await;

    // Call OpenAI API with context
    let result = client
        .analyze_image_data_with_context(&base64_image, relative_path, context)
        .await?;

    // Load existing metadata or create new
    let mut metadata = gallery
        .user_metadata_storage
        .load(relative_path)
        .await?
        .unwrap_or_default();

    // Update with AI analysis results
    metadata.set_ai_analysis(result.keywords.clone(), result.alt_text.clone());

    // Save metadata
    gallery
        .user_metadata_storage
        .save(relative_path, &metadata)
        .await?;

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
    relative_path: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Try to get cached medium-sized image first (more efficient for API)
    let cache_filename = gallery.generate_cache_filename(relative_path, "medium", "jpg", false);

    if gallery
        .cache_storage()
        .exists(&cache_filename)
        .await
        .unwrap_or(false)
    {
        debug!("Using cached medium image for analysis");
        return Ok(gallery
            .cache_storage()
            .read(&cache_filename)
            .await?
            .to_vec());
    }

    // Fall back to original image using source storage
    debug!("Using original image for analysis");
    Ok(gallery.source_storage().read(relative_path).await?.to_vec())
}

/// Truncate a string for display
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Build context for image analysis from available metadata
async fn build_image_context(gallery: &Gallery, relative_path: &str) -> ImageContext {
    use crate::gallery::metadata_sources::read_image_markdown_metadata_from_storage;

    let mut context = ImageContext::default();

    // Get image metadata (location, camera info, capture date from EXIF)
    if let Ok(cached_meta) = gallery.get_image_metadata_cached(relative_path).await {
        if let Some(loc) = cached_meta.location_info {
            debug!(
                "Using GPS coordinates for {}: {:.6}, {:.6}",
                relative_path, loc.latitude, loc.longitude
            );
            context.location = Some(LocationContext {
                latitude: loc.latitude,
                longitude: loc.longitude,
            });
        }

        // Camera settings
        if let Some(ref camera_info) = cached_meta.camera_info {
            context.focal_length = camera_info.focal_length.clone();
            context.aperture = camera_info.aperture.clone();
        }

        // Capture date and time (EXIF stores local time, which is preserved here)
        if let Some(capture_date) = cached_meta.capture_date
            && let Ok(duration) = capture_date.duration_since(std::time::UNIX_EPOCH)
            && let Some(dt) =
                chrono::DateTime::<chrono::Utc>::from_timestamp(duration.as_secs() as i64, 0)
        {
            context.capture_date = Some(dt.format("%B %d, %Y at %H:%M").to_string());
        }
    }

    // Get title and description from image markdown using storage abstraction
    if let Some(md_meta) =
        read_image_markdown_metadata_from_storage(gallery.source_storage(), relative_path).await
    {
        context.title = md_meta.config.title;
        if !md_meta.description_markdown.is_empty() {
            context.description = Some(md_meta.description_markdown);
        }
    }

    // Get folder metadata for additional context
    let parent_path = std::path::Path::new(relative_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let (folder_title, _) = gallery.read_folder_metadata(&parent_path).await;
    context.folder_title = folder_title;

    context
}
