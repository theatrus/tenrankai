#![allow(clippy::manual_async_fn)] // Required for axum 0.8 FromRequestParts trait

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use std::future::Future;

use crate::AppState;
use crate::login::AuthUser;
use crate::login::extractors::RequireAuth;

/// Extractor that requires the user to have owner_access permission.
/// This is used to gate access to admin endpoints.
pub struct RequireAdmin(pub AuthUser);

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // First, require authentication
            let RequireAuth(auth_user) = RequireAuth::from_request_parts(parts, state)
                .await
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

            // Check if user is an admin (site-level or gallery-level)
            let app_state = AppState::from_ref(state);
            if app_state.is_admin(&auth_user.username) {
                Ok(RequireAdmin(auth_user))
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
    }
}
