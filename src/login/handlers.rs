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

use super::{LoginError, LoginRequest, LoginResponse};

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

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or_else(|| LoginError::DatabaseError("User database not configured".to_string()))?;

    // Check if user exists by username or email
    let user_with_username = {
        let db = db_manager.database().read().await;
        db.get_user_by_username_or_email_with_username(&identifier)
    };

    if let Some((username, user)) = user_with_username {
        // Create login token using the actual username
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
        let login_url = format!("{}/_login/verify?token={}", base_url, token);

        // Send email if provider is configured, otherwise log the URL
        if let Some(email_provider) = &app_state.email_provider {
            if let Some(email_config) = &app_state.config.email {
                // Create the email message
                let mut email_message = crate::email::EmailMessage::new(
                    &user.email,
                    email_config.format_from(),
                    format!("Login to {}", app_state.config.app.name),
                );

                if let Some(reply_to) = &email_config.reply_to {
                    email_message = email_message.with_reply_to(reply_to);
                }

                email_message = email_message.with_both(
                    format!(
                        "Click this link to login to {}:\n\n{}\n\nThis link will expire in 10 minutes.\n\nIf you did not request this login, please ignore this email.",
                        app_state.config.app.name, login_url
                    ),
                    format!(
                        r#"<p>Click this link to login to {}:</p>
<p><a href="{}">{}</a></p>
<p>This link will expire in 10 minutes.</p>
<p>If you did not request this login, please ignore this email.</p>"#,
                        app_state.config.app.name, login_url, login_url
                    ),
                );

                // Send the email
                match email_provider.send_email(email_message).await {
                    Ok(_) => {
                        info!("Login email sent to {}", user.email);
                    }
                    Err(e) => {
                        error!("Failed to send login email to {}: {}", user.email, e);
                        // Continue anyway - user experience shouldn't reveal email failures
                    }
                }
            }
        } else {
            // Fallback to logging if no email provider is configured
            info!("Login URL for {}: {}", user.email, login_url);
        }
    } else {
        // Log that no user was found, but don't reveal this to the client
        info!("Login attempt for non-existent user/email: {}", identifier);
    }

    // Always return success to avoid revealing user existence
    Ok(Json(LoginResponse {
        success: true,
        message: "If your account is registered, you will receive a login link via email."
            .to_string(),
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
        .map_err(LoginError::InternalError)?;

    let cookie = format!(
        "auth={}; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax",
        signed_value
    );

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    info!("User {} logged in successfully", username);

    // Check if WebAuthn is available and if user has passkeys
    let should_enroll = if app_state.webauthn.is_some() {
        // Check if user has passkeys
        let db_manager = app_state.user_database_manager.as_ref();
        if let Some(manager) = db_manager {
            let db = manager.database().read().await;
            if let Some(user) = db.get_user(&username) {
                !user.has_passkeys()
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    // Redirect to enrollment page if user has no passkeys, otherwise to gallery
    let redirect_url = if should_enroll {
        "/passkey-enrollment"
    } else {
        "/gallery"
    };
    
    Ok((headers, Redirect::to(redirect_url)))
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

    let username =
        crate::login::get_authenticated_user(&headers, &app_state.config.app.cookie_secret);

    Json(AuthStatusResponse {
        authorized: username.is_some(),
        username,
    })
}

pub async fn passkey_enrollment_page(
    State(app_state): State<AppState>,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    // Check if user is authenticated
    let username = crate::login::get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let globals = liquid::object!({
        "base_url": app_state.config.app.base_url.as_deref().unwrap_or(""),
        "username": username,
        "redirect_url": "/gallery",
    });

    match app_state
        .template_engine
        .render_template("passkey_enrollment.html.liquid", globals)
        .await
    {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            error!("Failed to render passkey enrollment page: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
