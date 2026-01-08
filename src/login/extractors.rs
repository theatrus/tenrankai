#![allow(clippy::manual_async_fn)] // Required for axum 0.8 FromRequestParts trait

use crate::AppState;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use std::future::Future;

/// Authenticated user information extracted from request
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub username: String,
}

/// Optional authentication - extracts user if authenticated, None otherwise
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<AuthUser>);

/// Required authentication - returns 401 if not authenticated
#[derive(Debug, Clone)]
pub struct RequireAuth(pub AuthUser);

// Implement extraction for OptionalAuth
impl<S> FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let app_state = AppState::from_ref(state);

            // Extract headers
            let headers = &parts.headers;

            // Get authenticated user using existing function
            let username = crate::login::get_authenticated_user_for_app(&app_state, headers);

            Ok(OptionalAuth(username.map(|u| AuthUser { username: u })))
        }
    }
}

// Implement extraction for RequireAuth
impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let OptionalAuth(auth) = OptionalAuth::from_request_parts(parts, state).await?;

            match auth {
                Some(user) => Ok(RequireAuth(user)),
                None => Err((StatusCode::UNAUTHORIZED, "Authentication required")),
            }
        }
    }
}

impl OptionalAuth {
    /// Create a new OptionalAuth for testing
    #[cfg(test)]
    pub fn new(username: Option<String>) -> Self {
        OptionalAuth(username.map(|u| AuthUser { username: u }))
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.0.is_some()
    }

    /// Get the username if authenticated
    pub fn username(&self) -> Option<&str> {
        self.0.as_ref().map(|u| u.username.as_str())
    }

    /// Get the user if authenticated
    pub fn user(&self) -> Option<&AuthUser> {
        self.0.as_ref()
    }
}

impl RequireAuth {
    /// Get the username
    pub fn username(&self) -> &str {
        &self.0.username
    }

    /// Get the user
    pub fn user(&self) -> &AuthUser {
        &self.0
    }
}

/// Helper for handlers that need download permission
#[derive(Debug, Clone)]
pub struct RequireDownloadPermission(pub AuthUser);

impl<S> FromRequestParts<S> for RequireDownloadPermission
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // For now, download permission is the same as being authenticated
            // In the future, this could check additional permissions
            let RequireAuth(user) = RequireAuth::from_request_parts(parts, state).await?;

            Ok(RequireDownloadPermission(user))
        }
    }
}

impl RequireDownloadPermission {
    /// Get the username
    pub fn username(&self) -> &str {
        &self.0.username
    }

    /// Get the user
    pub fn user(&self) -> &AuthUser {
        &self.0
    }
}

// Re-export for convenience
pub use self::{
    OptionalAuth as OptAuth, RequireAuth as Auth, RequireDownloadPermission as DownloadAuth,
};
