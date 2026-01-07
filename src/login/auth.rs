use super::AuthScope;
use crate::api::{get_scoped_cookie_value, verify_signed_cookie};
use axum::http::HeaderMap;

/// Check if the user is authenticated and return their username
pub fn get_authenticated_user(headers: &HeaderMap, secret: &str) -> Option<String> {
    get_scoped_cookie_value(headers, AuthScope::Session).and_then(|signed_value| {
        if verify_signed_cookie(secret, &signed_value) {
            // Extract username from signed value (it's before the signature)
            signed_value.split(':').next().map(|s| s.to_string())
        } else {
            None
        }
    })
}

/// Check if the user is authenticated (returns true/false)
pub fn is_authenticated(headers: &HeaderMap, secret: &str) -> bool {
    get_authenticated_user(headers, secret).is_some()
}

/// Get authenticated user for app state (handles no auth config case)
pub fn get_authenticated_user_for_app(app_state: &crate::AppState, headers: &HeaderMap) -> Option<String> {
    // If no user database is configured, return None (no authentication)
    #[allow(clippy::question_mark)]
    if app_state.config.app.user_database.is_none() {
        return None;
    }

    // Get authenticated user from headers
    get_authenticated_user(headers, &app_state.config.app.cookie_secret)
}

/// Check if user has download permission
pub fn has_download_permission(app_state: &crate::AppState, headers: &HeaderMap) -> bool {
    // If no user database is configured, allow all downloads
    if app_state.config.app.user_database.is_none() {
        return true;
    }

    // Check if user is authenticated with the login system
    is_authenticated(headers, &app_state.config.app.cookie_secret)
}

/// Create a login redirect URL with return path
pub fn login_redirect_url(return_path: &str) -> String {
    if return_path.starts_with('/')
        && !return_path.starts_with("//")
        && !return_path.contains("://")
    {
        format!("/_login?return={}", urlencoding::encode(return_path))
    } else {
        "/_login".to_string()
    }
}
