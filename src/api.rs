use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Json, Response},
};
use base64::{Engine, engine::general_purpose};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize)]
pub struct AuthRequest {
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    success: bool,
    message: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    authorized: bool,
}

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

pub async fn authenticate_handler(
    State(app_state): State<crate::AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    tracing::info!("Authentication attempt received");
    let config = &app_state.config;

    if payload.password == config.app.download_password {
        tracing::info!("Authentication successful");
        match create_signed_cookie(&config.app.download_secret, "true") {
            Ok(signed_value) => {
                let cookie = format!(
                    "download_allowed={}; Path=/; Max-Age=86400; HttpOnly; SameSite=Lax",
                    signed_value
                );

                let mut headers = HeaderMap::new();
                headers.insert(SET_COOKIE, cookie.parse().unwrap());

                let response = AuthResponse {
                    success: true,
                    message: "Authentication successful".to_string(),
                };

                Ok((headers, Json(response)))
            }
            Err(_) => {
                let response = AuthResponse {
                    success: false,
                    message: "Server error".to_string(),
                };
                Ok((HeaderMap::new(), Json(response)))
            }
        }
    } else {
        tracing::warn!("Authentication failed - invalid password");
        let response = AuthResponse {
            success: false,
            message: "Invalid password".to_string(),
        };
        Ok((HeaderMap::new(), Json(response)))
    }
}

pub async fn verify_handler(
    State(app_state): State<crate::AppState>,
    headers: HeaderMap,
) -> Json<VerifyResponse> {
    let authorized = get_cookie_value(&headers, "download_allowed")
        .map(|signed_value| {
            verify_signed_cookie(&app_state.config.app.download_secret, &signed_value)
        })
        .unwrap_or(false);

    Json(VerifyResponse { authorized })
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

pub fn is_authenticated(headers: &HeaderMap, secret: &str) -> Option<String> {
    get_cookie_value(headers, "auth")
        .and_then(|signed_value| {
            if verify_signed_cookie(secret, &signed_value) {
                // Extract username from signed value
                signed_value.split(':').next().map(|s| s.to_string())
            } else {
                None
            }
        })
}

#[derive(Deserialize)]
pub struct GalleryPreviewQuery {
    count: Option<usize>,
}

#[derive(Serialize)]
pub struct GalleryPreviewResponse {
    images: Vec<crate::gallery::GalleryItem>,
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

    // Generate a cache key for the composite
    let composite_cache_key = crate::gallery::Gallery::generate_composite_cache_key(&gallery_path);

    // Try to serve from cache first
    if let Ok(cached_response) = gallery
        .serve_cached_image(&composite_cache_key, "composite", "")
        .await
    {
        return Ok(cached_response);
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
        return Err(StatusCode::NOT_FOUND);
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
