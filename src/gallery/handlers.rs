use super::{GalleryQuery, NavigationImage};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use tracing::error;

#[axum::debug_handler]
pub async fn gallery_root_handler(
    State(app_state): State<AppState>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    gallery_handler(State(app_state), Path("".to_string()), Query(query)).await
}

#[axum::debug_handler]
pub async fn gallery_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;
    let gallery = &app_state.gallery;

    let page = query.page.unwrap_or(0);
    let (directories, images, total_pages) = match gallery.list_directory(&path, page).await {
        Ok(result) => {
            tracing::debug!(
                "Handler received: {} directories, {} images",
                result.0.len(),
                result.1.len()
            );
            result
        }
        Err(e) => {
            error!("Failed to list directory: {}", e);
            return (StatusCode::NOT_FOUND, "Directory not found").into_response();
        }
    };

    // Convert images to JSON for client-side rendering
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());

    // Combine directories and images for the template's items array
    let mut items = directories.clone();
    items.extend(images.clone());

    // Check if this is the root path
    let is_root = path.is_empty() || path == "/";

    // Read folder metadata
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    // Build breadcrumb data - all items should be clickable in gallery view
    let breadcrumbs = gallery.build_breadcrumbs_with_mode(&path, true).await;

    // Get base URL from config or use default
    let base_url = app_state
        .config
        .app
        .base_url
        .as_deref()
        .unwrap_or("https://theatr.us");

    // Prepare OpenGraph image - use composite if multiple images, otherwise use first image
    let (og_image, og_image_dimensions) = if images.len() >= 2 {
        // Use composite preview for multiple images
        let composite_path = if path.is_empty() {
            "/api/gallery/composite/_root"
        } else {
            &format!("/api/gallery/composite/{}", path)
        };
        (
            Some(format!("{}{}", base_url, composite_path)),
            Some((1210, 1210)), // 2x2 grid of 600px images + padding
        )
    } else if let Some(first_img) = images.first() {
        // Use single image for single image galleries
        (
            first_img
                .gallery_url
                .clone()
                .map(|url| format!("{}{}", base_url, url)),
            first_img.dimensions,
        )
    } else {
        (None, None)
    };

    // Build OpenGraph URL for this gallery page
    let og_url = if path.is_empty() {
        format!("{}/gallery", base_url)
    } else {
        format!("{}/gallery/{}", base_url, path)
    };

    // Create OpenGraph title - use folder title or generate from path
    let og_title = folder_title.clone().unwrap_or_else(|| {
        if path.is_empty() {
            "Gallery".to_string()
        } else {
            // Use the last part of the path as a display name
            path.split('/')
                .last()
                .unwrap_or(&path)
                .replace('-', " ")
                .replace('_', " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    });

    // Create a better description including folder count info
    let og_description = if let Some(desc) = &folder_description {
        // Strip HTML tags from description for OpenGraph
        let stripped = desc
            .replace("<p>", "")
            .replace("</p>", " ")
            .replace("<br>", " ")
            .replace("<br/>", " ")
            .replace("<br />", " ")
            .chars()
            .filter(|&c| c != '<' && c != '>')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        stripped
    } else {
        let folder_count = directories.len();
        let image_count = images.len();
        if folder_count > 0 && image_count > 0 {
            format!(
                "Browse {} folders and {} images in this gallery",
                folder_count, image_count
            )
        } else if folder_count > 0 {
            format!("Browse {} folders in this gallery", folder_count)
        } else if image_count > 0 {
            format!("Browse {} images in this gallery", image_count)
        } else {
            "Browse the photo gallery".to_string()
        }
    };

    let globals = liquid::object!({
        "items": items,
        "images": images,
        "images_json": images_json,
        "gallery_path": path,
        "current_page": page,
        "total_pages": total_pages,
        "has_prev": page > 0,
        "has_next": page + 1 < total_pages,
        "prev_page": if page > 0 { page - 1 } else { 0 },
        "next_page": page + 1,
        "is_root": is_root,
        "folder_title": folder_title,
        "folder_description": folder_description,
        "breadcrumbs": breadcrumbs,
        // OpenGraph meta tags
        "og_title": og_title,
        "og_description": og_description,
        "og_image": og_image,
        "og_image_width": og_image_dimensions.map(|(w, _)| w),
        "og_image_height": og_image_dimensions.map(|(_, h)| h),
        "og_url": og_url,
        "og_type": "website",
        // Twitter card
        "twitter_card_type": "summary_large_image",
        "twitter_title": og_title,
        "twitter_description": og_description,
        "twitter_image": og_image,
    });

    match template_engine
        .render_template("gallery.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

#[axum::debug_handler]
pub async fn image_detail_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;
    let gallery = &app_state.gallery;

    let image_info = match gallery.get_image_info(&path).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to get image info: {}", e);
            return (StatusCode::NOT_FOUND, "Image not found").into_response();
        }
    };

    // Get the parent directory for navigation
    let parent_path = std::path::Path::new(&path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    // Get all images in the parent directory for navigation
    let (_, images, _) = gallery
        .list_directory(parent_path, 0)
        .await
        .unwrap_or_default();

    // Find current image index and get prev/next
    let current_index = images.iter().position(|img| img.path == path);

    let (prev_image, next_image) = if let Some(index) = current_index {
        let prev = if index > 0 {
            let prev_item = &images[index - 1];
            Some(NavigationImage {
                path: prev_item.path.clone(),
                name: prev_item.name.clone(),
                thumbnail_url: prev_item.thumbnail_url.clone().unwrap_or_default(),
            })
        } else {
            None
        };

        let next = if index + 1 < images.len() {
            let next_item = &images[index + 1];
            Some(NavigationImage {
                path: next_item.path.clone(),
                name: next_item.name.clone(),
                thumbnail_url: next_item.thumbnail_url.clone().unwrap_or_default(),
            })
        } else {
            None
        };

        (prev, next)
    } else {
        (None, None)
    };

    // Build breadcrumb data for the image's parent directory - all items should be clickable
    let parent_path = std::path::Path::new(&path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let breadcrumbs = gallery.build_breadcrumbs_with_mode(parent_path, true).await;

    // Get base URL from config or use default
    let base_url = app_state
        .config
        .app
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8080");

    let globals = liquid::object!({
        "image": image_info,
        "prev_image": prev_image,
        "next_image": next_image,
        "breadcrumbs": breadcrumbs,
        "base_url": base_url,
    });

    match template_engine
        .render_template("image_detail.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

fn has_download_permission(headers: &HeaderMap, secret: &str) -> bool {
    crate::api::get_cookie_value(headers, "download_allowed")
        .map(|signed_value| crate::api::verify_signed_cookie(secret, &signed_value))
        .unwrap_or(false)
}

pub async fn image_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate size parameter if provided
    if let Some(ref size) = query.size {
        // Check if it's a @2x variant
        let (base_size, _is_2x) = if size.ends_with("@2x") {
            (size.trim_end_matches("@2x"), true)
        } else {
            (size.as_str(), false)
        };

        match base_size {
            "thumbnail" | "gallery" | "medium" => {
                // These sizes are allowed without authentication
            }
            "large" => {
                // Large size requires authentication
                if !has_download_permission(&headers, &app_state.config.app.download_secret) {
                    tracing::warn!(path = %path, "Large image request denied - authentication required");
                    return (StatusCode::FORBIDDEN, "Download permission required").into_response();
                }
            }
            _ => {
                // Invalid size parameter
                tracing::warn!(path = %path, size = %size, "Invalid size parameter requested");
                return (StatusCode::BAD_REQUEST, "Invalid size parameter. Valid sizes: thumbnail, gallery, medium, large (with optional @2x suffix)").into_response();
            }
        }
    } else {
        // No size parameter means full-size original image - requires authentication
        if !has_download_permission(&headers, &app_state.config.app.download_secret) {
            tracing::warn!(path = %path, "Full-size image request denied - authentication required");
            return (StatusCode::FORBIDDEN, "Download permission required").into_response();
        }
    }

    // Extract Accept header for format negotiation
    let accept_header = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    app_state
        .gallery
        .serve_image(&path, query.size, accept_header)
        .await
}
