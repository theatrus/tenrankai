use super::types::ImageSize;
use super::{GalleryQuery, NavigationImage};
use crate::{ApiResponse, site::ResolvedState};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tracing::{debug, error};

// Named gallery handlers for multiple gallery support
#[axum::debug_handler(state = crate::AppState)]
pub async fn gallery_root_handler_for_named(
    ResolvedState(app_state): ResolvedState,
    Path(gallery_name): Path<String>,
    Query(query): Query<GalleryQuery>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    gallery_handler_for_named(
        ResolvedState(app_state),
        Path((gallery_name, "".to_string())),
        Query(query),
        auth,
    )
    .await
}

#[axum::debug_handler(state = crate::AppState)]
pub async fn gallery_handler_for_named(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<GalleryQuery>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let template_engine = app_state.template_engine();

    let gallery = match app_state.galleries().get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return ApiResponse::GalleryNotFound.into_response();
        }
    };

    // Resolve permissions for this path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        &path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    };

    // Check if user can view this path
    if !user_permissions.permissions.can_view {
        // If user is not authenticated and public role is "none", redirect to login
        if !auth.is_authenticated() {
            let return_url = if path.is_empty() {
                format!("/{}", gallery_name)
            } else {
                format!("/{}/{}", gallery_name, path)
            };

            let login_url = format!("/_login?return={}", urlencoding::encode(&return_url));
            return axum::response::Redirect::temporary(&login_url).into_response();
        } else {
            // User is authenticated but doesn't have access, return 403
            return ApiResponse::AccessDenied.into_response();
        }
    }

    let page = query.page.unwrap_or(0);
    let (directories, images, total_pages) = match gallery
        .list_directory_with_user(&path, page, auth.username())
        .await
    {
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
            return ApiResponse::DirectoryNotFound.into_response();
        }
    };

    // Get the JSON data from the API endpoint to ensure consistency
    let gallery_api_response = crate::api::gallery_api_handler_for_named(
        ResolvedState(app_state.clone()),
        axum::extract::Path((gallery_name.clone(), path.clone())),
        axum::extract::Query(query.clone()),
        auth.clone(),
    )
    .await;

    let gallery_data_json = match gallery_api_response {
        Ok(axum::Json(data)) => serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
        Err(_) => {
            // If API call fails, provide empty object
            "{}".to_string()
        }
    };

    // Check if this is the root path
    let is_root = path.is_empty() || path == "/";

    // Convert images to JSON for client-side rendering (legacy)
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());

    // Get breadcrumbs and folder metadata
    let breadcrumbs = gallery.build_breadcrumbs(&path).await;
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    // Resolve permissions for this path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        &path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => {
            // Fall back to default permissions on error
            crate::permissions::UserPermissions::new(None::<String>, Default::default())
        }
    };

    // Combine directories and images for the template's items array
    let mut items = directories.clone();
    items.extend(images.clone());

    let gallery_config = gallery.get_config();

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
        "gallery_data_json": gallery_data_json,
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
        "app_name": app_state.app_name(),
        "copyright_holder": gallery.config.copyright_holder,
        "base_url": app_state.base_url(),
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
        // Authentication info
        "is_authenticated": auth.is_authenticated(),
        "current_user": auth.username().unwrap_or_default().to_string(),
        // Add permissions for template use - serialize to JSON to avoid recursion limit
        "permissions": serde_json::to_value(&user_permissions.permissions).unwrap_or_else(|_| serde_json::json!({})),
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

#[axum::debug_handler(state = crate::AppState)]
pub async fn image_detail_handler_for_named(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let template_engine = app_state.template_engine();

    let gallery = match app_state.galleries().get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return ApiResponse::GalleryNotFound.into_response();
        }
    };

    // Resolve the path from the indexer (path might be an index or the actual path)
    let resolved_path = {
        let indexer = gallery.image_indexer.read().await;
        if let Some(actual_path) = indexer.get_path(&path) {
            actual_path.to_string()
        } else {
            // Fall back to treating it as a direct path (for backward compatibility)
            path.clone()
        }
    };

    // Extract the parent folder path from the resolved image path
    let parent_path = if let Some(last_slash) = resolved_path.rfind('/') {
        &resolved_path[..last_slash]
    } else {
        "" // Image is in root folder
    };

    // Resolve permissions for the parent path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        parent_path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    };

    // Check if user can view this path
    if !user_permissions.permissions.can_view {
        // If user is not authenticated and public role is "none", redirect to login
        if !auth.is_authenticated() {
            let return_url = format!("/{}/detail/{}", gallery_name, path);
            let login_url = format!("/_login?return={}", urlencoding::encode(&return_url));
            return axum::response::Redirect::temporary(&login_url).into_response();
        } else {
            // User is authenticated but doesn't have access, return 403
            return ApiResponse::AccessDenied.into_response();
        }
    }

    let mut image_info = match gallery
        .get_image_info_with_user(&resolved_path, auth.username())
        .await
    {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to get image info: {}", e);
            return (StatusCode::NOT_FOUND, "Image not found").into_response();
        }
    };

    // Update the name to use the display name from the indexer
    {
        let indexer = gallery.image_indexer.read().await;
        image_info.name = indexer.get_display_name(&resolved_path);
    }

    // Get permission resolver for the image path
    let parent_path_for_perms = std::path::Path::new(&resolved_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Get folder metadata to check permissions
    let folder_metadata = gallery
        .read_folder_metadata_full(&parent_path_for_perms)
        .await;

    // Create permission resolver
    let resolver = crate::permissions::PermissionResolver::new(
        &gallery.get_config().permissions,
        folder_metadata.as_ref().map(|m| &m.config.permissions),
    );

    // Resolve permissions for the user
    let user = auth.user().map(|u| u.username.as_str());
    let permissions = resolver.resolve_user_permissions(user).unwrap_or_default();

    // If user doesn't have exact date permission, modify the capture date
    if !permissions.can_see_exact_dates
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
    let parent_path = std::path::Path::new(&resolved_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    // Get all images in the parent directory for navigation
    let (_, images, _) = gallery
        .list_directory(parent_path, 0)
        .await
        .unwrap_or_default();

    // Find current image index and get prev/next
    // We need to compare against the original path (indexed identifier) not the resolved path
    let current_index = images.iter().position(|img| img.path == path);

    // Debug logging to understand the issue
    if current_index.is_none() {
        debug!(
            "Could not find current image in navigation. Looking for '{}', available paths: {:?}",
            path,
            images.iter().map(|i| &i.path).collect::<Vec<_>>()
        );
    }

    let (prev_image, next_image, prev_images, next_images) = if let Some(index) = current_index {
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

        // Extended navigation: multiple images in each direction (closest first)
        let prev_imgs: Vec<NavigationImage> = (0..index)
            .rev()
            .take(8)
            .map(|i| {
                let item = &images[i];
                NavigationImage {
                    path: item.path.clone(),
                    name: item.name.clone(),
                    thumbnail_url: item.thumbnail_url.clone().unwrap_or_default(),
                }
            })
            .collect();

        let next_imgs: Vec<NavigationImage> = ((index + 1)..images.len())
            .take(8)
            .map(|i| {
                let item = &images[i];
                NavigationImage {
                    path: item.path.clone(),
                    name: item.name.clone(),
                    thumbnail_url: item.thumbnail_url.clone().unwrap_or_default(),
                }
            })
            .collect();

        (prev, next, prev_imgs, next_imgs)
    } else {
        (None, None, Vec::new(), Vec::new())
    };

    // Build breadcrumbs for the parent directory, not including the image filename
    let breadcrumbs = gallery.build_breadcrumbs_with_mode(parent_path, true).await;
    let gallery_config = gallery.get_config();

    // Technical details are now controlled by permissions, not a separate flag

    // Get the JSON data from the API endpoint to ensure consistency
    let image_detail_response = crate::api::image_detail_api_handler_for_named(
        ResolvedState(app_state.clone()),
        axum::extract::Path((gallery_name.clone(), path.clone())), // Use original path for API consistency
        auth.clone(),
    )
    .await;

    let image_detail_json = match image_detail_response {
        Ok(axum::Json(data)) => serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
        Err(_) => {
            // If API call fails, provide empty object
            "{}".to_string()
        }
    };

    // Get authenticated user info from extractor
    let is_authenticated = auth.is_authenticated();
    let current_user = auth.username().unwrap_or_default().to_string();

    // Resolve permissions for this path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        parent_path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => {
            // Fall back to default permissions on error
            crate::permissions::UserPermissions::new(None::<String>, Default::default())
        }
    };

    let liquid_context = liquid::object!({
        "gallery_name": gallery_name,
        "gallery_url": gallery_config.url_prefix,
        "image": image_info,
        "breadcrumbs": breadcrumbs,
        "prev_image": prev_image,
        "next_image": next_image,
        "prev_images": prev_images,
        "next_images": next_images,
        "image_detail_json": image_detail_json,
        "page_title": format!("{} - Photo Gallery", image_info.name),
        "meta_description": format!("View {} in our photo gallery", image_info.name),
        "app_name": app_state.app_name(),
        "copyright_holder": gallery.config.copyright_holder,
        "base_url": app_state.base_url(),
        "og_title": image_info.name,
        "og_description": format!("Photo: {}", image_info.name),
        "og_image": format!("{}{}", app_state.base_url().unwrap_or(""), image_info.medium_url),
        "og_image_width": image_info.dimensions.0,
        "og_image_height": image_info.dimensions.1,
        "twitter_card_type": "summary_large_image",
        "is_authenticated": is_authenticated,
        "current_user": current_user,
        // Add permissions for template use - serialize to JSON to avoid recursion limit
        "permissions": serde_json::to_value(&user_permissions.permissions).unwrap_or_else(|_| serde_json::json!({})),
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
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<GalleryQuery>,
    headers: axum::http::HeaderMap,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let gallery = match app_state.galleries().get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return ApiResponse::GalleryNotFound.into_response();
        }
    };

    // Resolve the path from the indexer (path might be an index or the actual path)
    let resolved_path = {
        let indexer = gallery.image_indexer.read().await;
        if let Some(actual_path) = indexer.get_path(&path) {
            debug!("Resolved path '{}' to actual path '{}'", path, actual_path);
            actual_path.to_string()
        } else {
            // Fall back to treating it as a direct path (for backward compatibility)
            debug!("Could not resolve path '{}', using as-is", path);
            path.clone()
        }
    };

    // Extract the parent folder path from the resolved image path
    let parent_path = if let Some(last_slash) = resolved_path.rfind('/') {
        resolved_path[..last_slash].to_string()
    } else {
        String::new() // Image is in root folder
    };

    // Resolve permissions for this path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        &parent_path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    };

    // Check if user can view this path
    if !user_permissions.permissions.can_view {
        return ApiResponse::NotFound.into_response(); // Hide existence
    }

    // Validate size parameter and check permissions
    if let Some(ref size_str) = query.size {
        match ImageSize::parse(size_str) {
            Some(size) => {
                // Map size to required permission
                let has_permission = match size {
                    ImageSize::Thumbnail
                    | ImageSize::ThumbnailRetina
                    | ImageSize::Gallery
                    | ImageSize::GalleryRetina => {
                        // These sizes only require view permission
                        user_permissions.permissions.can_view
                    }
                    ImageSize::Medium | ImageSize::MediumRetina => {
                        user_permissions.permissions.can_download_medium
                    }
                    ImageSize::Large | ImageSize::LargeRetina => {
                        user_permissions.permissions.can_download_large
                    }
                    ImageSize::Tile(_, _) => {
                        // Tiles require can_use_tile_zoom permission
                        gallery.config.tiles.is_some()
                            && user_permissions.permissions.can_use_tile_zoom
                    }
                };

                if !has_permission {
                    tracing::warn!(path = %path, "Permission denied for size: {}", size.as_str());
                    return (StatusCode::FORBIDDEN, "Download permission required").into_response();
                }
            }
            None => {
                // Invalid size parameter
                tracing::warn!(path = %path, size = %size_str, "Invalid size parameter requested");
                let valid_sizes = ImageSize::base_size_names().join(", ");
                return (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Invalid size parameter. Valid sizes: {} (with optional @2x suffix)",
                        valid_sizes
                    ),
                )
                    .into_response();
            }
        }
    } else {
        // No size parameter means full-size original image
        if !user_permissions.permissions.can_download_original {
            tracing::warn!(path = %path, "Full-size image request denied - no permission");
            return (
                StatusCode::FORBIDDEN,
                "Original download permission required",
            )
                .into_response();
        }
    }

    // Extract Accept header for format negotiation
    let accept_header = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    debug!(
        "Serving image with resolved_path: {}, size: {:?}",
        resolved_path, query.size
    );
    gallery
        .serve_image(&resolved_path, query.size, accept_header)
        .await
}

/// New image handler that uses path-based URLs instead of query parameters
/// URL format: /gallery/_image/{path}/{size}
/// Example: /gallery/_image/vacation/photo.jpg/medium
pub async fn image_handler_for_named_v2(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, full_path)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    auth: crate::login::OptionalAuth,
) -> Response {
    // Parse the path to extract identifier and size
    // The full_path is like "vacation/photo.jpg/medium" or "vacation/abc123/thumbnail"
    let parts: Vec<&str> = full_path.split('/').collect();

    if parts.is_empty() {
        return (StatusCode::BAD_REQUEST, "Invalid image path").into_response();
    }

    // The last part is the size, everything before is the identifier
    let size = parts.last().copied();
    let identifier = if parts.len() > 1 {
        parts[..parts.len() - 1].join("/")
    } else {
        // No size specified, treat entire path as identifier
        full_path.clone()
    };

    // If size is actually part of the filename (e.g., "photo.jpg"), then there's no size
    let (actual_identifier, size_param) = if let Some(s) = size {
        if s.contains('.') || ImageSize::parse(s).is_none() {
            // The "size" is actually part of the filename
            (full_path.clone(), None)
        } else {
            (identifier, size)
        }
    } else {
        (identifier, None)
    };

    // Create a query struct with the size parameter
    let query = GalleryQuery {
        page: None,
        size: size_param.map(String::from),
    };

    // Call the original handler with the parsed parameters
    image_handler_for_named(
        ResolvedState(app_state),
        Path((gallery_name, actual_identifier)),
        Query(query),
        headers,
        auth,
    )
    .await
    .into_response()
}

/// Download a gallery folder as a zip file
/// URL format: /gallery/download/{path}
#[axum::debug_handler]
pub async fn download_folder_handler(
    State(app_state): State<AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> Response {
    let gallery = match app_state.galleries().get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return ApiResponse::GalleryNotFound.into_response();
        }
    };

    // Resolve permissions for this path
    let user_permissions = match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        &path,
        auth.username(),
    )
    .await
    {
        Ok(perms) => perms,
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    };

    // Check if user can view and download gallery
    if !user_permissions.permissions.can_view {
        if !auth.is_authenticated() {
            let return_url = format!("/{}/download/{}", gallery_name, path);
            let login_url = format!("/_login?return={}", urlencoding::encode(&return_url));
            return axum::response::Redirect::temporary(&login_url).into_response();
        }
        return ApiResponse::AccessDenied.into_response();
    }

    if !user_permissions.permissions.can_download_gallery {
        return (
            StatusCode::FORBIDDEN,
            "Gallery download permission required",
        )
            .into_response();
    }

    // Get all images in the directory (not paginated)
    let items = match gallery
        .scan_directory_with_user(&path, auth.username())
        .await
    {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to scan directory for download: {}", e);
            return ApiResponse::DirectoryNotFound.into_response();
        }
    };

    // Filter to only images
    let images: Vec<_> = items
        .into_iter()
        .filter(|item| !item.is_directory)
        .collect();

    if images.is_empty() {
        return (StatusCode::NOT_FOUND, "No images in this folder").into_response();
    }

    // Create zip file in memory
    let mut zip_buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for image in &images {
            // Read the original image from storage
            let image_path = if path.is_empty() {
                image.name.clone()
            } else {
                format!("{}/{}", path, image.name)
            };

            match gallery.source_storage().read(&image_path).await {
                Ok(data) => {
                    if let Err(e) = zip.start_file(&image.name, options) {
                        error!("Failed to start zip file entry '{}': {}", image.name, e);
                        continue;
                    }
                    if let Err(e) = std::io::Write::write_all(&mut zip, &data) {
                        error!("Failed to write image '{}' to zip: {}", image.name, e);
                    }
                }
                Err(e) => {
                    error!("Failed to read image '{}' for zip: {}", image_path, e);
                }
            }
        }

        if let Err(e) = zip.finish() {
            error!("Failed to finish zip file: {}", e);
            return ApiResponse::InternalServerError.into_response();
        }
    }

    // Generate filename for the zip
    let folder_name = if path.is_empty() {
        gallery_name.clone()
    } else {
        path.rsplit('/').next().unwrap_or(&gallery_name).to_string()
    };
    let zip_filename = format!("{}.zip", folder_name);

    // Return the zip file
    let body = zip_buffer.into_inner();
    (
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/zip".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", zip_filename),
            ),
        ],
        body,
    )
        .into_response()
}
