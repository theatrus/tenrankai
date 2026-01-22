use super::GalleryQuery;
use super::types::ImageSize;
use crate::{ApiResponse, site::ResolvedState};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::time::SystemTime;
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
        Err(e) => {
            error!(
                "Failed to resolve permissions for gallery '{}' path '{}': {:?}",
                gallery_name, path, e
            );
            return ApiResponse::InternalServerError.into_response();
        }
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

    // Get the JSON data from the API endpoint - this is the single source of truth
    // The API endpoint already fetches breadcrumbs, folder metadata, and permissions
    let gallery_api_response = crate::api::gallery_api_handler_for_named(
        ResolvedState(app_state.clone()),
        axum::extract::Path((gallery_name.clone(), path.clone())),
        axum::extract::Query(query.clone()),
        auth.clone(),
    )
    .await;

    let (gallery_data_json, api_data) = match gallery_api_response {
        Ok(axum::Json(data)) => {
            let json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
            (json, Some(data))
        }
        Err(_) => ("{}".to_string(), None),
    };

    // Extract data from API response (avoiding duplicate fetches)
    let breadcrumbs = api_data
        .as_ref()
        .map(|d| d.breadcrumbs.clone())
        .unwrap_or_default();
    let folder_title = api_data.as_ref().and_then(|d| d.folder_title.clone());
    let folder_description = api_data.as_ref().and_then(|d| d.folder_description.clone());
    let folder_description_markdown = api_data
        .as_ref()
        .and_then(|d| d.folder_description_markdown.clone());
    let user_permissions = api_data
        .as_ref()
        .map(|d| {
            crate::permissions::UserPermissions::new(
                auth.username().map(String::from),
                d.permissions.clone(),
            )
        })
        .unwrap_or_else(|| {
            crate::permissions::UserPermissions::new(None::<String>, Default::default())
        });

    // Check if this is the root path
    let is_root = path.is_empty() || path == "/";

    // Convert images to JSON for client-side rendering (legacy)
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());

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
            app_state.base_url().unwrap_or(""),
            gallery_name,
            composite_path
        );
        (Some(og_image_url), Some(1210), Some(1210))
    } else if let Some(first_image) = images.first() {
        // Use the first image if we only have one
        let og_image_url = format!(
            "{}{}",
            app_state.base_url().unwrap_or(""),
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
        "breadcrumbs": breadcrumbs,
        "images": images,
        "items": items,
        "images_json": images_json,
        "gallery_data_json": gallery_data_json,
        "current_page": page,
        "total_pages": total_pages,
        "folder_title": folder_title,
        "folder_description": folder_description,
        "folder_description_markdown": folder_description_markdown,
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
        "is_authenticated": auth.is_authenticated(),
        "current_user": auth.username().unwrap_or_default().to_string(),
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

    let liquid_context = liquid::object!({
        "gallery_url": gallery_config.url_prefix,
        "image": image_info,
        "image_detail_json": image_detail_json,
        "base_url": app_state.base_url(),
        "is_authenticated": auth.is_authenticated(),
        "current_user": auth.username().unwrap_or_default().to_string(),
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
        .serve_image(&resolved_path, query.size, accept_header, &headers)
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

/// Convert SystemTime to zip::DateTime for file timestamps in zip archives
fn systemtime_to_zip_datetime(time: SystemTime) -> Option<zip::DateTime> {
    use chrono::{Datelike, Timelike};
    let datetime: chrono::DateTime<chrono::Utc> = time.into();
    zip::DateTime::from_date_and_time(
        datetime.year() as u16,
        datetime.month() as u8,
        datetime.day() as u8,
        datetime.hour() as u8,
        datetime.minute() as u8,
        datetime.second() as u8,
    )
    .ok()
}

/// Query parameters for download folder handler
#[derive(Debug, Deserialize)]
pub struct DownloadFolderQuery {
    /// Include all versions of images (default: false, only primary images)
    #[serde(default)]
    pub include_versions: bool,
}

pub async fn download_folder_handler(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<DownloadFolderQuery>,
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
            let return_url = format!("/{}/_download/{}", gallery_name, path);
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

    // Recursively collect images in the directory and subdirectories
    // Tuple: (file_path, display_name, capture_date)
    let mut all_images: Vec<(String, String, Option<SystemTime>)> = Vec::new();
    let include_versions = query.include_versions;

    // Helper to recursively collect images with their capture dates
    async fn collect_images_recursive(
        gallery: &super::SharedGallery,
        folder_path: &str,
        images: &mut Vec<(String, String, Option<SystemTime>)>,
        include_versions: bool,
    ) {
        if let Some(cached) = gallery.get_cached_folder_data(folder_path).await {
            // Read the image metadata cache once for this folder's images
            let metadata_cache = gallery.image_cache.read_all().await;

            if include_versions {
                // Include all images (primary + versions)
                for image_path in &cached.images {
                    // Get base display name and append version suffix if present
                    let display_name = {
                        let indexer = gallery.image_indexer.read().await;
                        let base_name = indexer.get_display_name(image_path);
                        // If this is a version file, append the version suffix
                        let filename = image_path.rsplit('/').next().unwrap_or(image_path);
                        if let Some(version_num) = super::grouping::extract_version_number(filename)
                        {
                            format!("{}_v{}", base_name, version_num)
                        } else {
                            base_name
                        }
                    };
                    let capture_date = metadata_cache.get(image_path).and_then(|m| m.capture_date);
                    images.push((image_path.clone(), display_name, capture_date));
                }
            } else {
                // Only include primary images from each group
                if !cached.image_groups.is_empty() {
                    for group in &cached.image_groups {
                        let display_name = {
                            let indexer = gallery.image_indexer.read().await;
                            indexer.get_display_name(&group.primary_path)
                        };
                        let capture_date = metadata_cache
                            .get(&group.primary_path)
                            .and_then(|m| m.capture_date);
                        images.push((group.primary_path.clone(), display_name, capture_date));
                    }
                } else {
                    // Fallback: filter out version files manually
                    for image_path in &cached.images {
                        if !super::grouping::is_version_file(image_path) {
                            let display_name = {
                                let indexer = gallery.image_indexer.read().await;
                                indexer.get_display_name(image_path)
                            };
                            let capture_date =
                                metadata_cache.get(image_path).and_then(|m| m.capture_date);
                            images.push((image_path.clone(), display_name, capture_date));
                        }
                    }
                }
            }

            // Recursively process subdirectories
            for subdir in &cached.subdirectories {
                let subdir_path = if folder_path.is_empty() {
                    subdir.clone()
                } else {
                    format!("{}/{}", folder_path, subdir)
                };
                Box::pin(collect_images_recursive(
                    gallery,
                    &subdir_path,
                    images,
                    include_versions,
                ))
                .await;
            }
        }
    }

    collect_images_recursive(gallery, &path, &mut all_images, include_versions).await;

    if all_images.is_empty() {
        return (StatusCode::NOT_FOUND, "No images in this folder").into_response();
    }

    // Generate filename for the zip
    let folder_name = if path.is_empty() {
        gallery_name.clone()
    } else {
        path.rsplit('/').next().unwrap_or(&gallery_name).to_string()
    };
    let zip_filename = format!("{}.zip", folder_name);

    // Create a channel for streaming the zip
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);

    // Spawn a task to build and stream the zip
    let storage = gallery.source_storage().clone();
    let download_root = path.clone();
    tokio::spawn(async move {
        let mut zip_buffer = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut zip_buffer);
            // Base options: no compression for faster streaming
            let base_options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            for (image_path, display_name, capture_date) in &all_images {
                // Create options with the file's capture date if available
                let options = if let Some(date) = capture_date {
                    if let Some(zip_datetime) = systemtime_to_zip_datetime(*date) {
                        base_options.last_modified_time(zip_datetime)
                    } else {
                        base_options
                    }
                } else {
                    base_options
                };
                // Calculate relative path from download root for folder structure in zip
                let relative_path = if download_root.is_empty() {
                    image_path.clone()
                } else if let Some(stripped) = image_path.strip_prefix(&download_root) {
                    stripped.trim_start_matches('/').to_string()
                } else {
                    image_path.clone()
                };

                // Get folder prefix (if image is in a subfolder)
                let folder_prefix = relative_path
                    .rfind('/')
                    .map(|pos| &relative_path[..pos + 1])
                    .unwrap_or("");

                // Use display name but preserve the original file extension
                let original_filename = image_path.rsplit('/').next().unwrap_or(image_path);
                let extension = original_filename
                    .rfind('.')
                    .map(|pos| &original_filename[pos..])
                    .unwrap_or("");
                let filename = format!("{}{}{}", folder_prefix, display_name, extension);

                match storage.read(image_path).await {
                    Ok(data) => {
                        if let Err(e) = zip.start_file(&filename, options) {
                            tracing::error!("Failed to start zip file entry '{}': {}", filename, e);
                            continue;
                        }
                        if let Err(e) = std::io::Write::write_all(&mut zip, &data) {
                            tracing::error!("Failed to write image '{}' to zip: {}", filename, e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to read image '{}' for zip: {}", image_path, e);
                    }
                }
            }

            if let Err(e) = zip.finish() {
                tracing::error!("Failed to finish zip file: {}", e);
            }
        }

        // Send the completed zip in chunks
        let data = zip_buffer.into_inner();
        for chunk in data.chunks(64 * 1024) {
            if tx.send(Ok(chunk.to_vec())).await.is_err() {
                break; // Client disconnected
            }
        }
    });

    // Convert channel receiver to a stream
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = axum::body::Body::from_stream(stream);

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{}\"", zip_filename),
            ),
        ],
        body,
    )
        .into_response()
}

/// Handler for downloading RAW files associated with images.
/// URL format: /gallery/_raw/{path}
/// The path is the actual path to the RAW file (e.g., "photos/IMG_0001.dng")
#[axum::debug_handler(state = crate::AppState)]
pub async fn raw_download_handler(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    auth: crate::login::OptionalAuth,
) -> Response {
    let gallery = match app_state.galleries().get(&gallery_name) {
        Some(g) => g,
        None => {
            error!("Gallery '{}' not found", gallery_name);
            return ApiResponse::GalleryNotFound.into_response();
        }
    };

    // Security check - prevent path traversal
    if path.contains("..") || path.starts_with('/') {
        return ApiResponse::Forbidden.into_response();
    }

    // Verify this is a RAW file extension
    let ext = super::grouping::get_extension(&path).map(|e| e.to_lowercase());
    if !ext
        .as_ref()
        .map(|e| super::grouping::is_raw_extension(e))
        .unwrap_or(false)
    {
        return (StatusCode::BAD_REQUEST, "Not a RAW file format").into_response();
    }

    // Extract the parent folder path from the file path
    let parent_path = if let Some(last_slash) = path.rfind('/') {
        path[..last_slash].to_string()
    } else {
        String::new() // File is in root folder
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
        if !auth.is_authenticated() {
            let return_url = format!("/{}/_raw/{}", gallery_name, path);
            let login_url = format!("/_login?return={}", urlencoding::encode(&return_url));
            return axum::response::Redirect::temporary(&login_url).into_response();
        }
        return ApiResponse::AccessDenied.into_response();
    }

    // Check if user can download RAW files
    if !user_permissions.permissions.can_download_raw {
        return (StatusCode::FORBIDDEN, "RAW download permission required").into_response();
    }

    // Verify the file exists before serving
    if !gallery
        .source_storage()
        .exists(&path)
        .await
        .unwrap_or(false)
    {
        return ApiResponse::NotFound.into_response();
    }

    // Get the filename for Content-Disposition header
    let filename = path.rsplit('/').next().unwrap_or(&path);

    // Serve the RAW file from source storage with download disposition
    let response = gallery.serve_from_source_storage(&path, &headers).await;

    // Add Content-Disposition header to prompt download
    if response.status().is_success() {
        let (parts, body) = response.into_parts();
        let mut response = Response::from_parts(parts, body);
        response.headers_mut().insert(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename)
                .parse()
                .unwrap(),
        );
        response
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemtime_to_zip_datetime_converts_correctly() {
        // Create a known date: 2024-06-15 14:30:44 UTC
        // Note: ZIP datetime has 2-second resolution, so we use even seconds
        let datetime = chrono::NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(14, 30, 44)
            .unwrap();
        let system_time: SystemTime =
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc)
                .into();

        let zip_dt = systemtime_to_zip_datetime(system_time).expect("Should convert successfully");

        assert_eq!(zip_dt.year(), 2024);
        assert_eq!(zip_dt.month(), 6);
        assert_eq!(zip_dt.day(), 15);
        assert_eq!(zip_dt.hour(), 14);
        assert_eq!(zip_dt.minute(), 30);
        assert_eq!(zip_dt.second(), 44);
    }

    #[test]
    fn test_systemtime_to_zip_datetime_rejects_pre_1980() {
        // Unix epoch is 1970-01-01 00:00:00, which is before ZIP's 1980 minimum
        let epoch = SystemTime::UNIX_EPOCH;
        let result = systemtime_to_zip_datetime(epoch);

        // The zip crate rejects dates before 1980
        assert!(result.is_none(), "Dates before 1980 should return None");
    }

    #[test]
    fn test_systemtime_to_zip_datetime_handles_recent_date() {
        // Test with current time
        let now = SystemTime::now();
        let zip_dt = systemtime_to_zip_datetime(now).expect("Should convert current time");

        // Should be a reasonable recent year
        assert!(zip_dt.year() >= 2020, "Year should be recent");
        assert!(
            zip_dt.year() <= 2100,
            "Year should not be too far in future"
        );
    }

    #[test]
    fn test_systemtime_to_zip_datetime_preserves_date_components() {
        // Test boundary values (using even seconds due to ZIP's 2-second resolution)
        let test_cases = [
            (2000, 1, 1, 0, 0, 0),      // Y2K
            (2023, 12, 31, 23, 59, 58), // End of year (58 not 59 due to 2-sec resolution)
            (2024, 2, 29, 12, 0, 0),    // Leap day
            (1980, 1, 1, 0, 0, 0),      // ZIP minimum date
        ];

        for (year, month, day, hour, minute, second) in test_cases {
            let datetime = chrono::NaiveDate::from_ymd_opt(year, month, day)
                .unwrap()
                .and_hms_opt(hour, minute, second)
                .unwrap();
            let system_time: SystemTime =
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc)
                    .into();

            let zip_dt =
                systemtime_to_zip_datetime(system_time).expect("Should convert successfully");

            assert_eq!(
                zip_dt.year(),
                year as u16,
                "Year mismatch for {:?}",
                datetime
            );
            assert_eq!(
                zip_dt.month(),
                month as u8,
                "Month mismatch for {:?}",
                datetime
            );
            assert_eq!(zip_dt.day(), day as u8, "Day mismatch for {:?}", datetime);
            assert_eq!(
                zip_dt.hour(),
                hour as u8,
                "Hour mismatch for {:?}",
                datetime
            );
            assert_eq!(
                zip_dt.minute(),
                minute as u8,
                "Minute mismatch for {:?}",
                datetime
            );
            assert_eq!(
                zip_dt.second(),
                second as u8,
                "Second mismatch for {:?}",
                datetime
            );
        }
    }
}
