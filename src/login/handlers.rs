use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
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
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, LoginError> {
    let username = request.username.trim().to_lowercase();

    // Load user database
    let db_path = std::path::Path::new("users.toml");
    let user_db = UserDatabase::load_from_file(db_path)
        .await
        .map_err(|e| LoginError::DatabaseError(e.to_string()))?;

    // Check if user exists
    let user = user_db
        .get_user(&username)
        .ok_or(LoginError::UserNotFound)?;

    // Create login token
    let token = {
        let mut login_state = app_state.login_state.write().await;
        login_state.create_token(username.clone())
    };

    // Build login URL
    let base_url = app_state
        .config
        .app
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8080");
    let login_url = format!("{}/login/verify?token={}", base_url, token);

    // For now, just log the URL instead of sending email
    info!("Login URL for {}: {}", user.email, login_url);

    Ok(Json(LoginResponse {
        success: true,
        message: format!("Login link sent to {}", user.email),
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
    let signed_value = create_signed_cookie(&app_state.config.app.download_secret, &username)
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
