use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::fmt;

#[derive(Debug)]
pub enum LoginError {
    UserNotFound,
    TokenInvalid,
    TokenExpired,
    DatabaseError(String),
    InternalError(String),
}

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoginError::UserNotFound => write!(f, "User not found"),
            LoginError::TokenInvalid => write!(f, "Invalid login token"),
            LoginError::TokenExpired => write!(f, "Login token has expired"),
            LoginError::DatabaseError(e) => write!(f, "Database error: {}", e),
            LoginError::InternalError(e) => write!(f, "Internal error: {}", e),
        }
    }
}

impl std::error::Error for LoginError {}

impl IntoResponse for LoginError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            LoginError::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
            LoginError::TokenInvalid => (StatusCode::UNAUTHORIZED, "Invalid login token"),
            LoginError::TokenExpired => (StatusCode::UNAUTHORIZED, "Login token has expired"),
            LoginError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            LoginError::InternalError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        (status, message).into_response()
    }
}
