use crate::api::{get_cookie_value, verify_signed_cookie};
use axum::http::HeaderMap;

/// Check if the user is authenticated and return their username
pub fn get_authenticated_user(headers: &HeaderMap, secret: &str) -> Option<String> {
    get_cookie_value(headers, "auth").and_then(|signed_value| {
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
