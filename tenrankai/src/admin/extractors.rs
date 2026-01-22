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

            // Get the app state
            let app_state = AppState::from_ref(state);

            // Check if user has owner_access in any gallery
            for (_, gallery) in app_state.galleries().iter() {
                let permissions = &gallery.get_config().permissions;

                // Check user_roles for this user
                for user_role in &permissions.user_roles {
                    if user_role.username.eq_ignore_ascii_case(&auth_user.username) {
                        // Check each role for owner_access
                        for role_name in &user_role.roles {
                            if let Some(role) = permissions.roles.get(role_name)
                                && role.permissions.owner_access
                            {
                                return Ok(RequireAdmin(auth_user));
                            }
                            // Check built-in admin role
                            if role_name == "admin" {
                                return Ok(RequireAdmin(auth_user));
                            }
                        }
                    }
                }
            }

            Err(StatusCode::FORBIDDEN)
        }
    }
}
