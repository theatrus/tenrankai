use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{Html, IntoResponse, Redirect},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{error, info};

use crate::{AppState, api::create_signed_cookie};

use super::{LoginError, LoginRequest, LoginResponse, UserDatabase};

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    token: String,
}

pub async fn login_page(State(app_state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let globals = liquid::object!({
        "base_url": app_state.config.app.base_url.as_deref().unwrap_or(""),
    });

    match app_state
        .template_engine
        .render_template("login.html.liquid", globals)
        .await
    {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            error!("Failed to render login page: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn login_request(
    State(app_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, LoginError> {
    let identifier = request.username.trim().to_lowercase();
    let client_ip = addr.ip().to_string();
    
    // Check rate limit
    {
        let mut login_state = app_state.login_state.write().await;
        if let Err(msg) = login_state.check_rate_limit(&client_ip) {
            return Ok(Json(LoginResponse {
                success: false,
                message: msg.to_string(),
            }));
        }
    }

    // Load user database from configured path
    let db_path = app_state.config.app.user_database.as_ref()
        .ok_or_else(|| LoginError::DatabaseError("User database not configured".to_string()))?;
    let user_db = UserDatabase::load_from_file(db_path)
        .await
        .map_err(|e| LoginError::DatabaseError(e.to_string()))?;

    // Check if user exists by username or email
    if let Some(user) = user_db.get_user_by_username_or_email(&identifier) {
        // Create login token using the actual username
        let token = {
            let mut login_state = app_state.login_state.write().await;
            login_state.create_token(user.username.clone())
        };
        
        // Build login URL
        let base_url = app_state
            .config
            .app
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:8080");
        let login_url = format!("{}/_login/verify?token={}", base_url, token);
        
        // For now, just log the URL instead of sending email
        info!("Login URL for {}: {}", user.email, login_url);
    } else {
        // Log that no user was found, but don't reveal this to the client
        info!("Login attempt for non-existent user/email: {}", identifier);
    }
    
    // Always return success to avoid revealing user existence
    Ok(Json(LoginResponse {
        success: true,
        message: "If your account is registered, you will receive a login link via email.".to_string(),
    }))
}

pub async fn verify_login(
    State(app_state): State<AppState>,
    Query(query): Query<VerifyQuery>,
) -> Result<impl IntoResponse, LoginError> {
    // Verify token
    let username = {
        let mut login_state = app_state.login_state.write().await;
        login_state
            .verify_token(&query.token)
            .ok_or(LoginError::TokenInvalid)?
    };

    // Create secure session cookie
    let signed_value = create_signed_cookie(&app_state.config.app.cookie_secret, &username)
        .map_err(|e| LoginError::InternalError(e))?;

    let cookie = format!(
        "auth={}; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax",
        signed_value
    );

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    info!("User {} logged in successfully", username);

    // Redirect to gallery
    Ok((headers, Redirect::to("/gallery")))
}

pub async fn logout() -> impl IntoResponse {
    let cookie = "auth=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax";

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    (headers, Redirect::to("/"))
}

pub async fn login_success(State(app_state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let globals = liquid::object!({
        "base_url": app_state.config.app.base_url.as_deref().unwrap_or(""),
    });

    match app_state
        .template_engine
        .render_template("login_success.html.liquid", globals)
        .await
    {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            error!("Failed to render login success page: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub authorized: bool,
    pub username: Option<String>,
}

pub async fn check_auth_status(
    State(app_state): State<AppState>,
    headers: HeaderMap,
) -> Json<AuthStatusResponse> {
    // If no user database is configured, return not authorized
    if app_state.config.app.user_database.is_none() {
        return Json(AuthStatusResponse {
            authorized: false,
            username: None,
        });
    }
    
    let username = crate::login::get_authenticated_user(&headers, &app_state.config.app.cookie_secret);
    
    Json(AuthStatusResponse {
        authorized: username.is_some(),
        username,
    })
}
