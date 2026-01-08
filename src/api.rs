use crate::{ApiResponse, login::AuthScope};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Json, Response},
};
use base64::{Engine, engine::general_purpose};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{error, info};

type HmacSha256 = Hmac<Sha256>;

pub fn create_signed_cookie(secret: &str, value: &str) -> Result<String, String> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| "Invalid secret key")?;
    mac.update(value.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = general_purpose::URL_SAFE_NO_PAD.encode(signature);
    Ok(format!("{}:{}", value, signature_b64))
}

pub fn verify_signed_cookie(secret: &str, signed_value: &str) -> bool {
    if let Some((value, signature_b64)) = signed_value.split_once(':')
        && let Ok(signature) = general_purpose::URL_SAFE_NO_PAD.decode(signature_b64)
        && let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes())
    {
        mac.update(value.as_bytes());
        return mac.verify_slice(&signature).is_ok();
    }
    false
}

pub fn get_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            if let Some((key, value)) = cookie.split_once('=') {
                if key.trim() == name {
                    Some(value.trim().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
}

/// Get cookie value using AuthScope (type-safe version)
pub fn get_scoped_cookie_value(headers: &HeaderMap, scope: AuthScope) -> Option<String> {
    get_cookie_value(headers, scope.cookie_name())
}

#[derive(Deserialize)]
pub struct GalleryPreviewQuery {
    count: Option<usize>,
}

#[derive(Serialize)]
pub struct GalleryPreviewResponse {
    images: Vec<crate::gallery::GalleryItem>,
}

#[derive(Serialize, Debug)]
pub struct GalleryApiResponse {
    pub gallery_name: String,
    pub gallery_path: String,
    pub is_root: bool,
    pub breadcrumbs: Vec<crate::gallery::BreadcrumbItem>,
    pub directories: Vec<crate::gallery::GalleryItem>,
    pub images: Vec<crate::gallery::GalleryItem>,
    pub page: usize,
    pub total_pages: usize,
    pub folder_title: Option<String>,
    pub folder_description: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct ImageDetailApiResponse {
    pub gallery_name: String,
    pub image: crate::gallery::ImageInfo,
    pub breadcrumbs: Vec<crate::gallery::BreadcrumbItem>,
    pub prev_image: Option<crate::gallery::NavigationImage>,
    pub next_image: Option<crate::gallery::NavigationImage>,
}

// Named gallery API handlers for multiple gallery support
pub async fn gallery_preview_handler_for_named(
    State(app_state): State<crate::AppState>,
    Path(gallery_name): Path<String>,
    Query(query): Query<GalleryPreviewQuery>,
) -> Result<Json<GalleryPreviewResponse>, StatusCode> {
    let gallery = app_state.galleries.get(&gallery_name).ok_or_else(|| {
        tracing::error!("Gallery '{}' not found", gallery_name);
        StatusCode::NOT_FOUND
    })?;

    let count = query.count.unwrap_or(6).min(20); // Cap at 20 for performance
    match gallery.get_gallery_preview(count).await {
        Ok(images) => Ok(Json(GalleryPreviewResponse { images })),
        Err(e) => {
            tracing::error!("Failed to get gallery preview: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn gallery_composite_preview_handler_for_named(
    State(app_state): State<crate::AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let gallery = app_state.galleries.get(&gallery_name).ok_or_else(|| {
        tracing::error!("Gallery '{}' not found", gallery_name);
        StatusCode::NOT_FOUND
    })?;

    // Handle special case for root gallery
    let gallery_path = if path == "_root" { String::new() } else { path };

    // Generate composite cache key and filename using the new enhanced system
    let composite_cache_key = gallery.generate_composite_cache_key_with_context(&gallery_path);
    let cache_filename = gallery.generate_composite_cache_filename(&gallery_path);

    // Try to serve from cache first
    if let Ok(cached_response) = gallery
        .serve_cached_image(&cache_filename, "composite", "")
        .await
    {
        // Only return if it's not a 404 (i.e., cache exists)
        if cached_response.status() != StatusCode::NOT_FOUND {
            return Ok(cached_response);
        }
        // Otherwise, fall through to generate the composite
    }

    // Not in cache, need to generate it
    // List directory to get images
    let (_, images, _) = gallery
        .list_directory(&gallery_path, 0)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list directory: {}", e);
            StatusCode::NOT_FOUND
        })?;

    // Take up to 4 images for a 2x2 grid
    let preview_images: Vec<_> = images.into_iter().take(4).collect();

    if preview_images.is_empty() {
        return Err(ApiResponse::NotFound.status_code());
    }

    // Create composite image in a blocking task
    let source_dir = gallery.source_directory().to_path_buf();
    let composite_result = tokio::task::spawn_blocking(move || {
        crate::composite::create_composite_preview(source_dir, preview_images)
    })
    .await
    .map_err(|e| {
        tracing::error!("Failed to spawn blocking task: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let composite_image = composite_result.map_err(|e| {
        tracing::error!("Failed to create composite preview: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Store in cache and serve
    gallery
        .store_and_serve_composite(&composite_cache_key, composite_image)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store composite in cache: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub success: bool,
    pub message: String,
}

pub async fn refresh_static_versions(
    State(app_state): State<crate::AppState>,
    headers: HeaderMap,
) -> Result<Json<RefreshResponse>, StatusCode> {
    // If no user database is configured, deny access
    if app_state.config.app.user_database.is_none() {
        return Ok(Json(RefreshResponse {
            success: false,
            message: "Authentication not configured".to_string(),
        }));
    }

    // Check if user is authenticated
    if !crate::login::is_authenticated(&headers, &app_state.config.app.cookie_secret) {
        return Ok(Json(RefreshResponse {
            success: false,
            message: "Authentication required".to_string(),
        }));
    }

    // Refresh static file versions
    app_state.static_handler.refresh_file_versions().await;

    info!("Static file versions refreshed");

    Ok(Json(RefreshResponse {
        success: true,
        message: "Static file versions refreshed successfully".to_string(),
    }))
}

pub async fn gallery_api_handler_for_named(
    State(app_state): State<crate::AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    Query(query): Query<crate::gallery::GalleryQuery>,
    headers: HeaderMap,
) -> Result<Json<GalleryApiResponse>, StatusCode> {
    let gallery = app_state.galleries.get(&gallery_name).ok_or_else(|| {
        error!("Gallery '{}' not found", gallery_name);
        StatusCode::NOT_FOUND
    })?;

    // Check authentication
    let user = crate::login::get_authenticated_user_for_app(&app_state, &headers);

    // Check if the user has access to this path
    if !gallery.check_path_access(&path, user.as_deref()).await {
        return Err(StatusCode::FORBIDDEN);
    }

    let page = query.page.unwrap_or(0);
    let (directories, images, total_pages) = gallery
        .list_directory_with_user(&path, page, user.as_deref())
        .await
        .map_err(|e| {
            error!("Failed to list directory: {}", e);
            StatusCode::NOT_FOUND
        })?;

    // Check if this is the root path
    let is_root = path.is_empty() || path == "/";

    // Get breadcrumbs and folder metadata
    let breadcrumbs = gallery.build_breadcrumbs(&path).await;
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    Ok(Json(GalleryApiResponse {
        gallery_name,
        gallery_path: path,
        is_root,
        breadcrumbs,
        directories,
        images,
        page,
        total_pages,
        folder_title,
        folder_description,
    }))
}

pub async fn image_detail_api_handler_for_named(
    State(app_state): State<crate::AppState>,
    Path((gallery_name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ImageDetailApiResponse>, StatusCode> {
    let gallery = app_state.galleries.get(&gallery_name).ok_or_else(|| {
        error!("Gallery '{}' not found", gallery_name);
        StatusCode::NOT_FOUND
    })?;

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

    // Check authentication
    let user = crate::login::get_authenticated_user_for_app(&app_state, &headers);

    // Extract the parent folder path from the resolved image path
    let parent_path = if let Some(last_slash) = resolved_path.rfind('/') {
        &resolved_path[..last_slash]
    } else {
        "" // Image is in root folder
    };

    // Check if the user has access to the folder containing this image
    if !gallery
        .check_path_access(parent_path, user.as_deref())
        .await
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Get image info (this function already handles authentication logic internally)
    let mut image_info = gallery
        .get_image_info_with_user(&resolved_path, user.as_deref())
        .await
        .map_err(|e| {
            error!("Failed to get image info: {}", e);
            StatusCode::NOT_FOUND
        })?;

    // Update the name to use the display name from the indexer
    {
        let indexer = gallery.image_indexer.read().await;
        image_info.name = indexer.get_display_name(&resolved_path);
    }

    // Check if user has download permission
    let has_permission = app_state.config.app.user_database.is_none() || user.is_some();

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
            Some(crate::gallery::NavigationImage {
                path: prev_item.path.clone(),
                name: prev_item.name.clone(),
                thumbnail_url: prev_item.thumbnail_url.clone().unwrap_or_default(),
            })
        } else {
            None
        };

        let next = if index + 1 < images.len() {
            let next_item = &images[index + 1];
            Some(crate::gallery::NavigationImage {
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

    Ok(Json(ImageDetailApiResponse {
        gallery_name,
        image: image_info,
        breadcrumbs,
        prev_image,
        next_image,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Config, GallerySystemConfig};
    use axum::http::{HeaderMap, HeaderValue};
    use std::{collections::HashMap, sync::Arc};
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_app_state() -> (AppState, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let gallery_dir = temp_dir.path().join("gallery");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&gallery_dir).await.unwrap();
        fs::create_dir_all(&cache_dir).await.unwrap();

        // Create a test image file
        let test_image_path = gallery_dir.join("test.jpg");
        fs::write(&test_image_path, &[0xFF, 0xD8, 0xFF, 0xE0])
            .await
            .unwrap(); // Minimal JPEG header

        // Create folder metadata with privacy settings
        let folder_md_path = gallery_dir.join("_folder.md");
        fs::write(&folder_md_path, "+++\nhide_technical_details = true\nhide_location_from_public = true\nrequire_auth = false\n+++\n# Test Folder").await.unwrap();

        // Create a private folder that requires auth
        let private_dir = gallery_dir.join("private");
        fs::create_dir_all(&private_dir).await.unwrap();
        let private_image_path = private_dir.join("private.jpg");
        fs::write(&private_image_path, &[0xFF, 0xD8, 0xFF, 0xE0])
            .await
            .unwrap();

        let private_folder_md_path = private_dir.join("_folder.md");
        fs::write(
            &private_folder_md_path,
            "+++\nrequire_auth = true\nallowed_users = [\"testuser\"]\n+++\n# Private Folder",
        )
        .await
        .unwrap();

        let gallery_config = GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/test".to_string(),
            source_directory: gallery_dir,
            cache_directory: cache_dir,
            gallery_template: "test.html".to_string(),
            image_detail_template: "test.html".to_string(),
            images_per_page: 50,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            new_threshold_days: Some(7),
            pregenerate_cache: false,
            approximate_dates_for_public: true,
            copyright_holder: Some("Test".to_string()),
            hide_location_from_public: false, // Gallery-level default
            cache_refresh_interval_minutes: Some(60),
            thumbnail: crate::ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: crate::ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: crate::ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: crate::ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: crate::PreviewConfig {
                max_images: 6,
                max_depth: 3,
                max_per_folder: 3,
            },
            image_indexing: crate::config::ImageIndexingMode::Filename,
        };

        let gallery = Arc::new(crate::gallery::Gallery::new(gallery_config));
        let mut galleries = HashMap::new();
        galleries.insert("test".to_string(), gallery);

        let config = Config {
            app: crate::AppConfig {
                name: "Test".to_string(),
                base_url: Some("http://test.com".to_string()),
                user_database: Some("users.toml".into()),
                cookie_secret: "test-secret".to_string(),
                log_level: crate::LogLevel::Info,
            },
            server: crate::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            static_files: crate::StaticConfig {
                directories: vec!["static".into()],
            },
            templates: crate::TemplateConfig {
                directories: vec!["templates".into()],
            },
            galleries: Some(vec![]),
            posts: None,
            email: None,
        };

        let app_state = AppState {
            template_engine: Arc::new(crate::templating::TemplateEngine::new(
                config.templates.directories.clone(),
            )),
            static_handler: crate::static_files::StaticFileHandler::new(
                config.static_files.directories.clone(),
            ),
            galleries: Arc::new(galleries),
            favicon_renderer: crate::favicon::FaviconRenderer::new(
                config.static_files.directories.clone(),
            ),
            posts_managers: Arc::new(HashMap::new()),
            login_state: Arc::new(tokio::sync::RwLock::new(crate::login::LoginState::new())),
            user_database_manager: None,
            email_provider: None,
            webauthn: None,
            config,
        };

        (app_state, temp_dir)
    }

    fn create_auth_headers(username: &str, secret: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let signed_cookie = create_signed_cookie(secret, username).unwrap();
        let cookie_value = format!("auth={}", signed_cookie);
        headers.insert("cookie", HeaderValue::from_str(&cookie_value).unwrap());
        headers
    }

    #[tokio::test]
    async fn test_gallery_api_unauthenticated_access() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        let result = gallery_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "".to_string())),
            axum::extract::Query(crate::gallery::GalleryQuery::default()),
            headers,
        )
        .await;

        assert!(
            result.is_ok(),
            "Gallery API should work for unauthenticated users"
        );
        let response = result.unwrap();
        assert_eq!(response.0.gallery_name, "test");
        assert!(response.0.is_root);
    }

    #[tokio::test]
    async fn test_gallery_api_private_folder_access_denied() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        let result = gallery_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "private".to_string())),
            axum::extract::Query(crate::gallery::GalleryQuery::default()),
            headers,
        )
        .await;

        assert!(
            result.is_err(),
            "Private folder should deny access to unauthenticated users"
        );
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_gallery_api_private_folder_access_with_auth() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = create_auth_headers("testuser", "test-secret");

        let result = gallery_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "private".to_string())),
            axum::extract::Query(crate::gallery::GalleryQuery::default()),
            headers,
        )
        .await;

        match result {
            Ok(response) => {
                assert_eq!(response.0.gallery_name, "test");
                assert_eq!(response.0.gallery_path, "private");
                assert!(!response.0.is_root);
            }
            Err(status) => {
                panic!(
                    "Private folder should allow access to authorized users, but got status: {:?}",
                    status
                );
            }
        }
    }

    #[tokio::test]
    async fn test_gallery_api_private_folder_wrong_user() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = create_auth_headers("wronguser", "test-secret");

        let result = gallery_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "private".to_string())),
            axum::extract::Query(crate::gallery::GalleryQuery::default()),
            headers,
        )
        .await;

        assert!(
            result.is_err(),
            "Private folder should deny access to unauthorized users"
        );
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_image_detail_api_privacy_filtering() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        // Add metadata to the test image
        {
            let gallery = app_state.galleries.get("test").unwrap();
            let mut cache = gallery.metadata_cache.write().await;
            cache.insert(
                "test.jpg".to_string(),
                crate::gallery::ImageMetadata {
                    dimensions: (1920, 1080),
                    capture_date: None,
                    camera_info: Some(crate::gallery::CameraInfo {
                        camera_make: Some("Canon".to_string()),
                        camera_model: Some("EOS R5".to_string()),
                        lens_model: Some("RF 24-70mm".to_string()),
                        iso: Some(800),
                        aperture: Some("f/2.8".to_string()),
                        shutter_speed: Some("1/60s".to_string()),
                        focal_length: Some("50mm".to_string()),
                    }),
                    location_info: Some(crate::gallery::LocationInfo {
                        latitude: 37.7749,
                        longitude: -122.4194,
                        google_maps_url: "https://maps.google.com/...".to_string(),
                        apple_maps_url: "https://maps.apple.com/...".to_string(),
                    }),
                    modification_date: None,
                    color_profile: Some("sRGB".to_string()),
                },
            );
        }

        let result = image_detail_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "test.jpg".to_string())),
            headers,
        )
        .await;

        assert!(
            result.is_ok(),
            "Image detail API should work for unauthenticated users"
        );
        let response = result.unwrap();

        // Technical details should be hidden due to folder setting
        assert!(
            response.0.image.camera_info.is_none(),
            "Camera info should be hidden due to hide_technical_details"
        );
        assert!(
            response.0.image.color_profile.is_none(),
            "Color profile should be hidden due to hide_technical_details"
        );

        // Location should be hidden due to folder setting
        assert!(
            response.0.image.location_info.is_none(),
            "Location should be hidden due to hide_location_from_public"
        );
    }

    #[tokio::test]
    async fn test_image_detail_api_with_auth_shows_all_data() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = create_auth_headers("testuser", "test-secret");

        // Add metadata to the test image
        {
            let gallery = app_state.galleries.get("test").unwrap();
            let mut cache = gallery.metadata_cache.write().await;
            cache.insert(
                "test.jpg".to_string(),
                crate::gallery::ImageMetadata {
                    dimensions: (1920, 1080),
                    capture_date: None,
                    camera_info: Some(crate::gallery::CameraInfo {
                        camera_make: Some("Canon".to_string()),
                        camera_model: Some("EOS R5".to_string()),
                        lens_model: Some("RF 24-70mm".to_string()),
                        iso: Some(800),
                        aperture: Some("f/2.8".to_string()),
                        shutter_speed: Some("1/60s".to_string()),
                        focal_length: Some("50mm".to_string()),
                    }),
                    location_info: Some(crate::gallery::LocationInfo {
                        latitude: 37.7749,
                        longitude: -122.4194,
                        google_maps_url: "https://maps.google.com/...".to_string(),
                        apple_maps_url: "https://maps.apple.com/...".to_string(),
                    }),
                    modification_date: None,
                    color_profile: Some("sRGB".to_string()),
                },
            );
        }

        let result = image_detail_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "test.jpg".to_string())),
            headers,
        )
        .await;

        assert!(
            result.is_ok(),
            "Image detail API should work for authenticated users"
        );
        let response = result.unwrap();

        // For authenticated users with hide_technical_details=true, technical details should still be hidden
        assert!(
            response.0.image.camera_info.is_none(),
            "Camera info should still be hidden due to hide_technical_details"
        );
        assert!(
            response.0.image.color_profile.is_none(),
            "Color profile should still be hidden due to hide_technical_details"
        );

        // But location should be visible to authenticated users
        assert!(
            response.0.image.location_info.is_some(),
            "Location should be visible to authenticated users"
        );
    }

    #[tokio::test]
    async fn test_image_detail_api_private_image_access_denied() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        let result = image_detail_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("test".to_string(), "private/private.jpg".to_string())),
            headers,
        )
        .await;

        assert!(
            result.is_err(),
            "Private image should deny access to unauthenticated users"
        );
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_image_detail_api_nonexistent_gallery() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        let result = image_detail_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("nonexistent".to_string(), "test.jpg".to_string())),
            headers,
        )
        .await;

        assert!(result.is_err(), "Nonexistent gallery should return 404");
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_gallery_api_nonexistent_gallery() {
        let (app_state, _temp_dir) = create_test_app_state().await;
        let headers = HeaderMap::new();

        let result = gallery_api_handler_for_named(
            axum::extract::State(app_state),
            axum::extract::Path(("nonexistent".to_string(), "".to_string())),
            axum::extract::Query(crate::gallery::GalleryQuery::default()),
            headers,
        )
        .await;

        assert!(result.is_err(), "Nonexistent gallery should return 404");
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
