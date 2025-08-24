use super::{GalleryQuery, NavigationImage};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use tracing::error;

fn has_download_permission(headers: &HeaderMap, secret: &str) -> bool {
    // Check if user is authenticated with the login system
    crate::login::is_authenticated(headers, secret)
}

// Named gallery handlers for multiple gallery support
#[axum::debug_handler]
pub async fn gallery_root_handler_for_named(
    State(app_state): State<AppState>,
    Path(gallery_name): Path<String>,
    Query(query): Query<GalleryQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    gallery_handler_for_named(
        State(app_state),
        Path((gallery_name, "".to_string())),
        Query(query),
        headers,
    )
    .await
}

#[axum::debug_handler]
pub async fn gallery_handler_for_named(
    State(app_state): State<AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<GalleryQuery>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;

    let gallery = match app_state.galleries.get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
        }
    };

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
    let gallery_config = gallery.get_config();

    // Render the template
    let breadcrumbs = gallery.build_breadcrumbs(&path).await;
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    // Determine OpenGraph image - use composite if we have 2+ images, otherwise use first image
    let (og_image, og_image_width, og_image_height) = if images.len() >= 2 {
        // Use composite image for galleries with multiple images
        let composite_path = if path.is_empty() { "_root" } else { &path };
        let og_image_url = format!(
            "{}/api/gallery/{}/composite/{}",
            app_state
                .config
                .app
                .base_url
                .as_ref()
                .unwrap_or(&String::new()),
            gallery_name,
            composite_path
        );
        (Some(og_image_url), Some(1210), Some(1210))
    } else if let Some(first_image) = images.first() {
        // Use the first image if we only have one
        let og_image_url = format!(
            "{}{}",
            app_state
                .config
                .app
                .base_url
                .as_ref()
                .unwrap_or(&String::new()),
            first_image.gallery_url.as_ref().unwrap_or(&String::new())
        );
        (
            Some(og_image_url),
            first_image.dimensions.map(|d| d.0),
            first_image.dimensions.map(|d| d.1),
        )
    } else {
        (None, None, None)
    };

    let liquid_context = liquid::object!({
        "gallery_name": gallery_name,
        "gallery_url": gallery_config.url_prefix,
        "gallery_path": path,
        "is_root": is_root,
        "breadcrumbs": breadcrumbs,
        "directories": directories,
        "images": images,
        "items": items,
        "images_json": images_json,
        "page": page,
        "current_page": page,
        "total_pages": total_pages,
        "folder_title": folder_title,
        "folder_description": folder_description,
        "page_title": if is_root { "Gallery".to_string() } else {
            folder_title.clone().unwrap_or_else(|| breadcrumbs.last().map(|b| b.name.clone()).unwrap_or_else(|| "Gallery".to_string()))
        },
        "meta_description": folder_description.as_ref().map(|desc_html| {
            // Strip HTML tags from the description
            let stripped = desc_html
                .replace("<p>", "")
                .replace("</p>", " ")
                .replace("<br>", " ")
                .replace("<br/>", " ")
                .replace("<br />", " ")
                .split('<')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if stripped.is_empty() {
                "".to_string()
            } else {
                // Limit length for meta description
                if stripped.len() > 160 {
                    format!("{}...", &stripped[..157])
                } else {
                    stripped
                }
            }
        }).unwrap_or_else(|| "".to_string()),
        "app_name": app_state.config.app.name,
        "copyright_holder": app_state.config.app.copyright_holder,
        "base_url": app_state.config.app.base_url,
        "og_title": folder_title.clone().unwrap_or_else(|| {
            if is_root {
                "Photo Gallery".to_string()
            } else {
                format!("{} - Photo Gallery", breadcrumbs.last().map(|b| &b.display_name).unwrap_or(&"Gallery".to_string()))
            }
        }),
        "og_description": folder_description.as_ref().map(|desc_html| {
            // Strip HTML tags from the description for OpenGraph
            let stripped = desc_html
                .replace("<p>", "")
                .replace("</p>", " ")
                .replace("<br>", " ")
                .replace("<br/>", " ")
                .replace("<br />", " ")
                .split('<')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if stripped.is_empty() {
                "".to_string()
            } else {
                // Limit length for social media
                if stripped.len() > 160 {
                    format!("{}...", &stripped[..157])
                } else {
                    stripped
                }
            }
        }).unwrap_or_else(|| "".to_string()),
        "og_image": og_image,
        "og_image_width": og_image_width,
        "og_image_height": og_image_height,
        "twitter_card_type": "summary_large_image",
    });

    match template_engine
        .render_template(&gallery_config.gallery_template, liquid_context)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

#[axum::debug_handler]
pub async fn image_detail_handler_for_named(
    State(app_state): State<AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;

    let gallery = match app_state.galleries.get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
        }
    };

    let mut image_info = match gallery.get_image_info(&path).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to get image info: {}", e);
            return (StatusCode::NOT_FOUND, "Image not found").into_response();
        }
    };

    // Check if user has download permission
    let has_permission = has_download_permission(&headers, &app_state.config.app.download_secret);

    // If approximate dates are enabled and user doesn't have permission, modify the capture date
    if gallery.get_config().approximate_dates_for_public
        && !has_permission
        && let Some(ref capture_date_str) = image_info.capture_date
    {
        // Parse the existing date and reformat to show only month and year
        if let Ok(datetime) =
            chrono::DateTime::parse_from_str(capture_date_str, "%B %d, %Y at %H:%M:%S")
        {
            image_info.capture_date = Some(datetime.format("%B %Y").to_string());
        } else if let Ok(datetime) =
            chrono::NaiveDateTime::parse_from_str(capture_date_str, "%B %d, %Y at %H:%M:%S")
        {
            image_info.capture_date = Some(datetime.format("%B %Y").to_string());
        }
    }

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

    // Build breadcrumbs for the parent directory, not including the image filename
    let breadcrumbs = gallery.build_breadcrumbs_with_mode(parent_path, true).await;
    let gallery_config = gallery.get_config();

    let liquid_context = liquid::object!({
        "gallery_name": gallery_name,
        "gallery_url": gallery_config.url_prefix,
        "image": image_info,
        "breadcrumbs": breadcrumbs,
        "prev_image": prev_image,
        "next_image": next_image,
        "page_title": format!("{} - Photo Gallery", image_info.name),
        "meta_description": format!("View {} in our photo gallery", image_info.name),
        "app_name": app_state.config.app.name,
        "copyright_holder": app_state.config.app.copyright_holder,
        "base_url": app_state.config.app.base_url,
        "og_title": image_info.name,
        "og_description": format!("Photo: {}", image_info.name),
        "og_image": format!("{}{}", app_state.config.app.base_url.as_ref().unwrap_or(&String::new()), image_info.medium_url),
        "og_image_width": image_info.dimensions.0,
        "og_image_height": image_info.dimensions.1,
        "twitter_card_type": "summary_large_image",
    });

    match template_engine
        .render_template(&gallery_config.image_detail_template, liquid_context)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub async fn image_handler_for_named(
    State(app_state): State<AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<GalleryQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let gallery = match app_state.galleries.get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
        }
    };

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

    gallery.serve_image(&path, query.size, accept_header).await
}
