//! Background image analysis task
//!
//! Periodically scans galleries for images without AI analysis and processes them.

use super::{ImageContext, LocationContext, OpenAIClient};
use crate::gallery::Gallery;
use base64::{Engine as _, engine::general_purpose};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Start background image analysis for all galleries
pub fn start_background_analysis(
    openai_client: Arc<OpenAIClient>,
    galleries: Arc<std::collections::HashMap<String, Arc<Gallery>>>,
    interval_minutes: u64,
    batch_size: usize,
    shutdown_token: CancellationToken,
) {
    info!(
        "Starting background image analysis (interval: {} minutes, batch size: {})",
        interval_minutes, batch_size
    );

    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(interval_minutes * 60);

        // Initial delay to let server fully start
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Background image analysis shutting down");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    info!("Running background image analysis");
                    if let Err(e) = run_analysis_batch(&openai_client, &galleries, batch_size).await {
                        error!("Background analysis error: {}", e);
                    }
                }
            }
        }
    });
}

/// Run a single batch of image analysis across all galleries
async fn run_analysis_batch(
    openai_client: &OpenAIClient,
    galleries: &std::collections::HashMap<String, Arc<Gallery>>,
    batch_size: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut total_analyzed = 0;

    for (gallery_name, gallery) in galleries.iter() {
        if total_analyzed >= batch_size {
            break;
        }

        let remaining = batch_size - total_analyzed;
        match analyze_gallery_images(openai_client, gallery, gallery_name, remaining).await {
            Ok(count) => {
                total_analyzed += count;
                if count > 0 {
                    info!(
                        "Background analysis: processed {} images in gallery '{}'",
                        count, gallery_name
                    );
                }
            }
            Err(e) => {
                error!(
                    "Background analysis error for gallery '{}': {}",
                    gallery_name, e
                );
            }
        }
    }

    if total_analyzed > 0 {
        info!(
            "Background analysis batch complete: {} images analyzed",
            total_analyzed
        );
    } else {
        debug!("Background analysis: no images needed analysis");
    }

    Ok(())
}

/// Analyze images in a single gallery that don't have AI metadata
async fn analyze_gallery_images(
    openai_client: &OpenAIClient,
    gallery: &Gallery,
    gallery_name: &str,
    limit: usize,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Collect images that need analysis
    let images_to_analyze = collect_images_needing_analysis(gallery, "", limit).await?;

    if images_to_analyze.is_empty() {
        return Ok(0);
    }

    debug!(
        "Found {} images needing analysis in gallery '{}'",
        images_to_analyze.len(),
        gallery_name
    );

    let mut analyzed_count = 0;

    for relative_path in images_to_analyze {
        let full_path = gallery.source_directory().join(&relative_path);

        // Get image data for analysis
        let image_data = match get_image_for_analysis(gallery, &full_path, &relative_path).await {
            Ok(data) => data,
            Err(e) => {
                warn!(
                    "Failed to read image {} for background analysis: {}",
                    relative_path, e
                );
                continue;
            }
        };

        // Encode as base64
        let base64_image = general_purpose::STANDARD.encode(&image_data);

        // Build context from available metadata
        let context = build_image_context(gallery, &relative_path).await;

        // Call OpenAI API
        let result = match openai_client
            .analyze_image_data_with_context(&base64_image, &relative_path, context)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                // Check if rate limited - stop processing if so
                if matches!(e, super::OpenAIError::RateLimited { .. }) {
                    warn!("Rate limited during background analysis, stopping batch");
                    break;
                }
                warn!(
                    "Failed to analyze image {} in background: {}",
                    relative_path, e
                );
                continue;
            }
        };

        // Load existing metadata or create new
        let mut metadata = gallery
            .user_metadata_storage
            .load(&full_path)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        // Update with AI analysis results
        metadata.set_ai_analysis(result.keywords.clone(), result.alt_text.clone());

        // Save metadata
        if let Err(e) = gallery
            .user_metadata_storage
            .save(&full_path, &metadata)
            .await
        {
            error!(
                "Failed to save AI analysis metadata for {}: {}",
                relative_path, e
            );
        } else {
            debug!(
                "Background analysis: saved {} keywords for {}",
                result.keywords.len(),
                relative_path
            );
            analyzed_count += 1;
        }
    }

    Ok(analyzed_count)
}

/// Recursively collect images that need AI analysis
async fn collect_images_needing_analysis(
    gallery: &Gallery,
    path: &str,
    limit: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut images = Vec::new();

    // Scan the directory
    let items = match gallery.scan_directory(path).await {
        Ok(items) => items,
        Err(e) => {
            debug!("Could not scan directory '{}': {}", path, e);
            return Ok(images);
        }
    };

    for item in items {
        if images.len() >= limit {
            break;
        }

        if item.is_directory {
            // Recursively scan subdirectories
            let subpath = if path.is_empty() {
                item.name.clone()
            } else {
                format!("{}/{}", path, item.name)
            };

            let sub_images = Box::pin(collect_images_needing_analysis(
                gallery,
                &subpath,
                limit - images.len(),
            ))
            .await?;
            images.extend(sub_images);
        } else {
            // Check if this image needs analysis
            let relative_path = if path.is_empty() {
                item.name.clone()
            } else {
                format!("{}/{}", path, item.name)
            };

            let full_path = gallery.source_directory().join(&relative_path);

            // Check if already has AI analysis
            if let Ok(Some(metadata)) = gallery.user_metadata_storage.load(&full_path).await
                && metadata.has_ai_analysis()
            {
                continue;
            }

            images.push(relative_path);
        }
    }

    Ok(images)
}

/// Get image data for analysis (preferring cached medium size)
async fn get_image_for_analysis(
    gallery: &Gallery,
    full_path: &std::path::Path,
    relative_path: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Try to get cached medium-sized image first (more efficient for API)
    let cache_filename = gallery.generate_cache_filename(relative_path, "medium", "jpg", false);
    let cache_path = gallery.config.cache_directory.join(&cache_filename);

    if cache_path.exists() {
        return Ok(tokio::fs::read(&cache_path).await?);
    }

    // Fall back to original image
    Ok(tokio::fs::read(full_path).await?)
}

/// Build context for image analysis from available metadata
async fn build_image_context(gallery: &Gallery, relative_path: &str) -> ImageContext {
    use crate::gallery::metadata_sources::read_image_markdown_metadata;

    let mut context = ImageContext::default();

    // Get image metadata (location, camera info, capture date from EXIF)
    if let Ok(cached_meta) = gallery.get_image_metadata_cached(relative_path).await {
        if let Some(loc) = cached_meta.location_info {
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

    // Get title and description from image markdown
    let full_path = gallery.source_directory().join(relative_path);
    if let Some(md_meta) = read_image_markdown_metadata(&full_path).await {
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
