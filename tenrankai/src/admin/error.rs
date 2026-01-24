use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub enum AdminError {
    Unauthorized,
    Forbidden,
    NotFound(String),
    AlreadyExists(String),
    BadRequest(String),
    Internal(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AdminError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AdminError::Forbidden => (
                StatusCode::FORBIDDEN,
                "Access denied. Owner permission required.".to_string(),
            ),
            AdminError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AdminError::AlreadyExists(msg) => (StatusCode::CONFLICT, msg),
            AdminError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AdminError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<tenrankai_users::UserStorageError> for AdminError {
    fn from(err: tenrankai_users::UserStorageError) -> Self {
        match err {
            tenrankai_users::UserStorageError::UserNotFound(u) => {
                AdminError::NotFound(format!("User not found: {}", u))
            }
            tenrankai_users::UserStorageError::UserAlreadyExists(u) => {
                AdminError::AlreadyExists(format!("User already exists: {}", u))
            }
            _ => AdminError::Internal(err.to_string()),
        }
    }
}
