use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum UploadError {
    MissingHeader(&'static str),
    InvalidHeader(&'static str),
    GalleryNotFound(String),
    UploadNotFound(String),
    OffsetMismatch { expected: u64, actual: u64 },
    UploadExpired,
    StorageError(String),
    Forbidden,
    UnsupportedVersion,
    UnsupportedMethod,
    FileTooLarge { size: u64, max: u64 },
    InvalidFilename(&'static str),
    InvalidFileType(String),
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadError::MissingHeader(h) => write!(f, "Missing required header: {}", h),
            UploadError::InvalidHeader(h) => write!(f, "Invalid header: {}", h),
            UploadError::GalleryNotFound(g) => write!(f, "Gallery not found: {}", g),
            UploadError::UploadNotFound(id) => write!(f, "Upload not found: {}", id),
            UploadError::OffsetMismatch { expected, actual } => {
                write!(f, "Offset mismatch: expected {}, got {}", expected, actual)
            }
            UploadError::UploadExpired => write!(f, "Upload has expired"),
            UploadError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            UploadError::Forbidden => write!(f, "Access denied"),
            UploadError::UnsupportedVersion => write!(f, "Unsupported Tus version"),
            UploadError::UnsupportedMethod => write!(f, "Unsupported method"),
            UploadError::FileTooLarge { size, max } => {
                write!(f, "File too large: {} bytes (max: {} bytes)", size, max)
            }
            UploadError::InvalidFilename(msg) => write!(f, "Invalid filename: {}", msg),
            UploadError::InvalidFileType(ext) => write!(f, "File type not allowed: {}", ext),
        }
    }
}

impl std::error::Error for UploadError {}

impl IntoResponse for UploadError {
    fn into_response(self) -> Response {
        let status = match &self {
            UploadError::MissingHeader(_) => StatusCode::BAD_REQUEST,
            UploadError::InvalidHeader(_) => StatusCode::BAD_REQUEST,
            UploadError::GalleryNotFound(_) => StatusCode::NOT_FOUND,
            UploadError::UploadNotFound(_) => StatusCode::NOT_FOUND,
            UploadError::OffsetMismatch { .. } => StatusCode::CONFLICT,
            UploadError::UploadExpired => StatusCode::GONE,
            UploadError::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UploadError::Forbidden => StatusCode::FORBIDDEN,
            UploadError::UnsupportedVersion => StatusCode::PRECONDITION_FAILED,
            UploadError::UnsupportedMethod => StatusCode::METHOD_NOT_ALLOWED,
            UploadError::FileTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            UploadError::InvalidFilename(_) => StatusCode::BAD_REQUEST,
            UploadError::InvalidFileType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        };

        (status, self.to_string()).into_response()
    }
}

impl From<tenrankai_storage::StorageError> for UploadError {
    fn from(err: tenrankai_storage::StorageError) -> Self {
        match err {
            tenrankai_storage::StorageError::NotFound(path) => UploadError::UploadNotFound(path),
            tenrankai_storage::StorageError::UploadNotFound(id) => UploadError::UploadNotFound(id),
            tenrankai_storage::StorageError::OffsetMismatch { expected, actual } => {
                UploadError::OffsetMismatch { expected, actual }
            }
            tenrankai_storage::StorageError::UploadExpired(_) => UploadError::UploadExpired,
            tenrankai_storage::StorageError::PermissionDenied(_) => UploadError::Forbidden,
            other => UploadError::StorageError(other.to_string()),
        }
    }
}
