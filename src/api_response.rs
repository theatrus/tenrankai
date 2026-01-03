use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    body::Body,
};

/// HTTP Status Response System for consistent API responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiResponse {
    // Success responses
    Ok,
    Created,
    NoContent,

    // Client errors (4xx)
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    Gone,
    UnprocessableEntity,
    TooManyRequests,

    // Specific error cases
    InvalidCredentials,
    ExpiredToken,
    MissingPermissions,
    ResourceNotFound,
    DuplicateResource,
    ValidationFailed,
    RateLimited,
    FileTooLarge,
    UnsupportedMediaType,

    // Server errors (5xx)
    InternalServerError,
    NotImplemented,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,

    // Application-specific errors
    DatabaseError,
    ExternalServiceError,
    ConfigurationError,
    TemplateRenderError,
    FileSystemError,
    
    // Gallery-specific errors
    GalleryNotFound,
    DirectoryNotFound,
    ImageNotFound,
    InvalidSizeParameter,
    ProcessingError,
    AccessDenied,
    CacheEntryNotFound,
    
    // Posts-specific errors
    PostNotFound,
    
    // Template-specific errors
    TemplateNotFound,
}

impl ApiResponse {
    /// Get the HTTP status code for this response
    pub fn status_code(self) -> StatusCode {
        match self {
            // Success responses
            Self::Ok => StatusCode::OK,
            Self::Created => StatusCode::CREATED,
            Self::NoContent => StatusCode::NO_CONTENT,

            // Client errors
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,

            // Specific error cases (mapped to appropriate 4xx codes)
            Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::ExpiredToken => StatusCode::UNAUTHORIZED,
            Self::MissingPermissions => StatusCode::FORBIDDEN,
            Self::ResourceNotFound => StatusCode::NOT_FOUND,
            Self::DuplicateResource => StatusCode::CONFLICT,
            Self::ValidationFailed => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::FileTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,

            // Server errors
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout => StatusCode::GATEWAY_TIMEOUT,

            // Application-specific errors (mapped to 500)
            Self::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ExternalServiceError => StatusCode::BAD_GATEWAY,
            Self::ConfigurationError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::TemplateRenderError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::FileSystemError => StatusCode::INTERNAL_SERVER_ERROR,
            
            // Gallery-specific errors
            Self::GalleryNotFound => StatusCode::NOT_FOUND,
            Self::DirectoryNotFound => StatusCode::NOT_FOUND,
            Self::ImageNotFound => StatusCode::NOT_FOUND,
            Self::InvalidSizeParameter => StatusCode::BAD_REQUEST,
            Self::ProcessingError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::AccessDenied => StatusCode::FORBIDDEN,
            Self::CacheEntryNotFound => StatusCode::NOT_FOUND,
            
            // Posts-specific errors
            Self::PostNotFound => StatusCode::NOT_FOUND,
            
            // Template-specific errors
            Self::TemplateNotFound => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get a human-readable message for this response
    pub fn message(self) -> &'static str {
        match self {
            // Success responses
            Self::Ok => "Request completed successfully",
            Self::Created => "Resource created successfully",
            Self::NoContent => "Request completed successfully",

            // Client errors
            Self::BadRequest => "Invalid request format",
            Self::Unauthorized => "Authentication required",
            Self::Forbidden => "Access denied",
            Self::NotFound => "Resource not found",
            Self::MethodNotAllowed => "HTTP method not allowed",
            Self::Conflict => "Resource conflict",
            Self::Gone => "Resource no longer available",
            Self::UnprocessableEntity => "Request data is invalid",
            Self::TooManyRequests => "Too many requests",

            // Specific error cases
            Self::InvalidCredentials => "Invalid username or password",
            Self::ExpiredToken => "Authentication token has expired",
            Self::MissingPermissions => "Insufficient permissions for this action",
            Self::ResourceNotFound => "The requested resource was not found",
            Self::DuplicateResource => "Resource already exists",
            Self::ValidationFailed => "Request validation failed",
            Self::RateLimited => "Rate limit exceeded",
            Self::FileTooLarge => "File size exceeds maximum limit",
            Self::UnsupportedMediaType => "Media type not supported",

            // Server errors
            Self::InternalServerError => "Internal server error",
            Self::NotImplemented => "Feature not implemented",
            Self::BadGateway => "Bad gateway",
            Self::ServiceUnavailable => "Service temporarily unavailable",
            Self::GatewayTimeout => "Gateway timeout",

            // Application-specific errors
            Self::DatabaseError => "Database operation failed",
            Self::ExternalServiceError => "External service error",
            Self::ConfigurationError => "Server configuration error",
            Self::TemplateRenderError => "Template rendering failed",
            Self::FileSystemError => "File system operation failed",
            
            // Gallery-specific errors
            Self::GalleryNotFound => "Gallery not found",
            Self::DirectoryNotFound => "Directory not found",
            Self::ImageNotFound => "Image not found",
            Self::InvalidSizeParameter => "Invalid size parameter",
            Self::ProcessingError => "Image processing error",
            Self::AccessDenied => "Access denied",
            Self::CacheEntryNotFound => "Cache entry not found",
            
            // Posts-specific errors
            Self::PostNotFound => "Post not found",
            
            // Template-specific errors
            Self::TemplateNotFound => "Template not found",
        }
    }

    /// Convert to an axum response
    pub fn into_response(self) -> Response<Body> {
        (self.status_code(), self.message()).into_response()
    }

    /// Create a response with a custom message
    pub fn with_message(self, message: &str) -> Response<Body> {
        (self.status_code(), message.to_string()).into_response()
    }

    /// Create an HTML response
    pub fn with_html(self, html: String) -> Response<Body> {
        (self.status_code(), Html(html)).into_response()
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(self) -> bool {
        matches!(self.status_code().as_u16(), 400..=499)
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(self) -> bool {
        matches!(self.status_code().as_u16(), 500..=599)
    }

    /// Check if this is a success response (2xx)
    pub fn is_success(self) -> bool {
        matches!(self.status_code().as_u16(), 200..=299)
    }

    /// All client error variants (for validation or testing)
    pub const CLIENT_ERRORS: &'static [ApiResponse] = &[
        Self::BadRequest,
        Self::Unauthorized,
        Self::Forbidden,
        Self::NotFound,
        Self::MethodNotAllowed,
        Self::Conflict,
        Self::Gone,
        Self::UnprocessableEntity,
        Self::TooManyRequests,
        Self::InvalidCredentials,
        Self::ExpiredToken,
        Self::MissingPermissions,
        Self::ResourceNotFound,
        Self::DuplicateResource,
        Self::ValidationFailed,
        Self::RateLimited,
        Self::FileTooLarge,
        Self::UnsupportedMediaType,
        Self::GalleryNotFound,
        Self::DirectoryNotFound,
        Self::ImageNotFound,
        Self::InvalidSizeParameter,
        Self::AccessDenied,
        Self::PostNotFound,
        Self::CacheEntryNotFound,
    ];

    /// All server error variants (for validation or testing)
    pub const SERVER_ERRORS: &'static [ApiResponse] = &[
        Self::InternalServerError,
        Self::NotImplemented,
        Self::BadGateway,
        Self::ServiceUnavailable,
        Self::GatewayTimeout,
        Self::DatabaseError,
        Self::ExternalServiceError,
        Self::ConfigurationError,
        Self::TemplateRenderError,
        Self::FileSystemError,
        Self::ProcessingError,
        Self::TemplateNotFound,
    ];
}