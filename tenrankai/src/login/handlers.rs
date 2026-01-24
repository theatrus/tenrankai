use axum::{
    Json,
    extract::{ConnectInfo, Query},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{Html, IntoResponse, Redirect},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{error, info};

use super::AuthScope;
use crate::{
    ApiResponse,
    api::{create_signed_cookie, get_scoped_cookie_value, is_https_request},
    api_response::no_cache_headers,
    site::ResolvedState,
};

use super::{LoginError, LoginRequest, LoginResponse};

/// Validates that a return URL is safe (relative path only, no external redirects)
fn is_safe_return_url(url: &str) -> bool {
    // Must start with / and not be a protocol URL
    url.starts_with('/') && !url.starts_with("//") && !url.contains("://")
}

/// Extracts the return URL from cookies using AuthScope
fn get_return_url_from_cookies(headers: &HeaderMap) -> Option<String> {
    get_scoped_cookie_value(headers, AuthScope::RedirectState)
        .and_then(|v| urlencoding::decode(&v).ok().map(|s| s.into_owned()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginQuery {
    #[serde(rename = "return")]
    return_url: Option<String>,
}

pub async fn login_page(
    ResolvedState(app_state): ResolvedState,
    Query(query): Query<LoginQuery>,
    request_headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let globals = liquid::object!({
        "base_url": app_state.base_url().unwrap_or(""),
    });

    let html = match app_state
        .template_engine()
        .render_template(crate::TemplateType::Login.path(), globals)
        .await
    {
        Ok(html) => html,
        Err(e) => {
            error!("Failed to render login page: {}", e);
            return Err(ApiResponse::TemplateRenderError.status_code());
        }
    };

    // Set return URL cookie if provided
    let is_https = is_https_request(&request_headers);
    let mut headers = HeaderMap::new();
    if let Some(return_url) = query.return_url {
        // Validate the return URL to prevent open redirects
        if is_safe_return_url(&return_url) {
            let cookie =
                AuthScope::RedirectState.format_cookie(&urlencoding::encode(&return_url), is_https);
            headers.insert(SET_COOKIE, cookie.parse().unwrap());
        }
    }

    // Add no-cache headers for security
    headers.extend(no_cache_headers());

    Ok((headers, Html(html)))
}

pub async fn login_request(
    ResolvedState(app_state): ResolvedState,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, LoginError> {
    let identifier = request.username.trim().to_lowercase();
    let client_ip = addr.ip().to_string();

    // Check rate limit
    {
        let mut login_state = app_state.login_state().write().await;
        if let Err(msg) = login_state.check_rate_limit(&client_ip) {
            return Ok(Json(LoginResponse {
                success: false,
                message: msg.to_string(),
            }));
        }
    }

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or_else(|| LoginError::DatabaseError("User database not configured".to_string()))?;

    // Check if user exists by username or email
    let user_with_username = match user_storage.get_user(&identifier).await {
        Ok(Some(user)) => Some((identifier.clone(), user)),
        Ok(None) => {
            // Try by email
            match user_storage.get_user_by_email(&identifier).await {
                Ok(Some((username, user))) => Some((username, user)),
                Ok(None) => None,
                Err(e) => {
                    error!("Failed to lookup user by email: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            error!("Failed to lookup user: {}", e);
            None
        }
    };

    if let Some((username, user)) = user_with_username {
        // Create login token using the actual username
        let token = {
            let mut login_state = app_state.login_state().write().await;
            login_state.create_token(username.clone())
        };

        // Build login URL
        let base_url = app_state.base_url().unwrap_or("http://localhost:8080");
        let login_url = format!("{}/_login/verify?token={}", base_url, token);

        // Send email if provider is configured, otherwise log the URL
        // Use site-specific email config for sender identity
        if let Some(email_provider) = &app_state.email_provider {
            if let Some(email_config) = app_state.email_config() {
                // Create the email message using site-specific sender settings
                let mut email_message = crate::email::EmailMessage::new(
                    &user.email,
                    email_config.format_from(),
                    format!("Login to {}", app_state.app_name()),
                );

                if let Some(reply_to) = &email_config.reply_to {
                    email_message = email_message.with_reply_to(reply_to);
                }

                email_message = email_message.with_both(
                    format!(
                        "Click this link to login to {}:\n\n{}\n\nThis link will expire in 10 minutes.\n\nIf you did not request this login, please ignore this email.",
                        app_state.app_name(), login_url
                    ),
                    format!(
                        r#"<p>Click this link to login to {}:</p>
<p><a href="{}">{}</a></p>
<p>This link will expire in 10 minutes.</p>
<p>If you did not request this login, please ignore this email.</p>"#,
                        app_state.app_name(), login_url, login_url
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
    ResolvedState(app_state): ResolvedState,
    Query(query): Query<VerifyQuery>,
    req_headers: HeaderMap,
) -> Result<impl IntoResponse, LoginError> {
    // Verify token
    let username = {
        let mut login_state = app_state.login_state().write().await;
        login_state
            .verify_token(&query.token)
            .ok_or(LoginError::TokenInvalid)?
    };

    // Get return URL from cookie
    let return_url = get_return_url_from_cookies(&req_headers);

    // Create secure session cookie
    let is_https = is_https_request(&req_headers);
    let signed_value = create_signed_cookie(app_state.cookie_secret(), &username)
        .map_err(LoginError::InternalError)?;

    let auth_cookie = AuthScope::Session.format_cookie(&signed_value, is_https);

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, auth_cookie.parse().unwrap());

    // Clear the return URL cookie
    let clear_cookie = AuthScope::RedirectState.clear_cookie(is_https);
    headers.append(SET_COOKIE, clear_cookie.parse().unwrap());

    info!("User {} logged in successfully", username);

    // Check if WebAuthn is available and if user has passkeys
    let should_enroll = if app_state.webauthn.is_some() {
        // Check if user has passkeys
        if let Some(user_storage) = app_state.user_storage().as_ref() {
            match user_storage.get_user(&username).await {
                Ok(Some(user)) => !user.has_passkeys(),
                Ok(None) => false,
                Err(e) => {
                    error!("Failed to get user for passkey check: {}", e);
                    false
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    // Determine where to redirect
    let redirect_url = if should_enroll {
        // If enrolling, we'll pass the return URL to the enrollment page
        if let Some(return_to) = return_url {
            format!(
                "/_login/passkey-enrollment?return={}",
                urlencoding::encode(&return_to)
            )
        } else {
            "/_login/passkey-enrollment".to_string()
        }
    } else {
        // Otherwise, use the return URL or default to gallery
        return_url.unwrap_or_else(|| "/gallery".to_string())
    };

    // Add no-cache headers for security
    headers.extend(no_cache_headers());

    Ok((headers, Redirect::to(&redirect_url)))
}

pub async fn logout(request_headers: HeaderMap) -> impl IntoResponse {
    let is_https = is_https_request(&request_headers);
    let cookie = AuthScope::Session.clear_cookie(is_https);

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    // Add no-cache headers for security
    headers.extend(no_cache_headers());

    (headers, Redirect::to("/"))
}

pub async fn login_success(
    ResolvedState(app_state): ResolvedState,
) -> Result<impl IntoResponse, StatusCode> {
    let globals = liquid::object!({
        "base_url": app_state.base_url().unwrap_or(""),
    });

    match app_state
        .template_engine()
        .render_template(crate::TemplateType::LoginSuccess.path(), globals)
        .await
    {
        Ok(html) => {
            let mut headers = HeaderMap::new();
            headers.extend(no_cache_headers());
            Ok((headers, Html(html)))
        }
        Err(e) => {
            error!("Failed to render login success page: {}", e);
            Err(ApiResponse::TemplateRenderError.status_code())
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub authorized: bool,
    pub username: Option<String>,
    pub is_admin: bool,
}

fn check_user_is_admin(app_state: &crate::AppState, username: &str) -> bool {
    for (_, gallery) in app_state.galleries().iter() {
        let permissions = &gallery.get_config().permissions;

        for user_role in &permissions.user_roles {
            if user_role.username.eq_ignore_ascii_case(username) {
                for role_name in &user_role.roles {
                    if let Some(role) = permissions.roles.get(role_name)
                        && role.permissions.owner_access
                    {
                        return true;
                    }
                    if role_name == "admin" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub async fn check_auth_status(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let response = if app_state.user_storage().is_none() {
        AuthStatusResponse {
            authorized: false,
            username: None,
            is_admin: false,
        }
    } else {
        let is_admin = auth
            .username()
            .map(|u| check_user_is_admin(&app_state, u))
            .unwrap_or(false);
        AuthStatusResponse {
            authorized: auth.is_authenticated(),
            username: auth.username().map(|s| s.to_string()),
            is_admin,
        }
    };

    (no_cache_headers(), Json(response))
}

pub async fn passkey_enrollment_page(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
    Query(query): Query<LoginQuery>,
) -> Result<Html<String>, StatusCode> {
    // Get authenticated username
    let username = auth.username().to_string();

    // Use return URL from query parameter or default to gallery
    let redirect_url = query
        .return_url
        .filter(|url| is_safe_return_url(url))
        .unwrap_or_else(|| "/gallery".to_string());

    let globals = liquid::object!({
        "base_url": app_state.base_url().unwrap_or(""),
        "username": username,
        "redirect_url": redirect_url,
    });

    match app_state
        .template_engine()
        .render_template(crate::TemplateType::PasskeyEnrollment.path(), globals)
        .await
    {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            error!("Failed to render passkey enrollment page: {}", e);
            Err(ApiResponse::TemplateRenderError.status_code())
        }
    }
}

/// Profile page handler - shows user information and passkey management
pub async fn profile_page(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
) -> Result<Html<String>, StatusCode> {
    // Get authenticated username
    let username = auth.username().to_string();

    // Get user information
    let user_info = if let Some(user_storage) = app_state.user_storage().as_ref() {
        match user_storage.get_user(&username).await {
            Ok(Some(user)) => Some((user.email.clone(), user.passkeys.len())),
            Ok(None) => None,
            Err(e) => {
                error!("Failed to get user info: {}", e);
                None
            }
        }
    } else {
        None
    };

    let (email, passkey_count) = user_info.unwrap_or(("Unknown".to_string(), 0));

    // Check if WebAuthn is available
    let webauthn_available = app_state.webauthn.is_some();

    let globals = liquid::object!({
        "username": username,
        "email": email,
        "passkey_count": passkey_count,
        "has_passkeys": passkey_count > 0,
        "webauthn_available": webauthn_available,
        "base_url": app_state.base_url().unwrap_or(""),
    });

    match app_state
        .template_engine()
        .render_template(crate::TemplateType::Profile.path(), globals)
        .await
    {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            error!("Failed to render profile page: {}", e);
            Err(ApiResponse::TemplateRenderError.status_code())
        }
    }
}
