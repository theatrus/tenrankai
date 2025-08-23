use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Json},
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

#[derive(Deserialize)]
pub struct GalleryPreviewQuery {
    count: Option<usize>,
}

#[derive(Serialize)]
pub struct GalleryPreviewResponse {
    images: Vec<crate::gallery::GalleryItem>,
}

pub async fn gallery_preview_handler(
    State(app_state): State<crate::AppState>,
    Query(query): Query<GalleryPreviewQuery>,
) -> Result<Json<GalleryPreviewResponse>, StatusCode> {
    let count = query.count.unwrap_or(6).min(20); // Cap at 20 for performance
    match app_state.gallery.get_gallery_preview(count).await {
        Ok(images) => Ok(Json(GalleryPreviewResponse { images })),
        Err(e) => {
            tracing::error!("Failed to get gallery preview: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
