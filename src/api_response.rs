use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header::{CACHE_CONTROL, PRAGMA, EXPIRES}},
    response::{Html, IntoResponse, Response},
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
    FeatureDisabled,

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
            Self::FeatureDisabled => StatusCode::NOT_FOUND,

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
            Self::FeatureDisabled => "Feature not available",

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

/// Create headers that prevent caching entirely
/// Used for dynamic content that should never be cached
pub fn no_cache_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, "no-cache, no-store, must-revalidate".parse().unwrap());
    headers.insert(PRAGMA, "no-cache".parse().unwrap());
    headers.insert(EXPIRES, "0".parse().unwrap());
    headers
}

/// Create headers for short-term caching
/// Used for content that changes occasionally but can be cached briefly
pub fn short_cache_headers(seconds: u32) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL, 
        format!("public, max-age={}", seconds).parse().unwrap()
    );
    headers
}

/// Create headers for long-term caching with immutable flag
/// Used for content that never changes (like processed images with hash-based names)
pub fn long_cache_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        "public, max-age=31536000, immutable".parse().unwrap()
    );
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_status_codes() {
        // Test success responses
        assert_eq!(ApiResponse::Ok.status_code(), StatusCode::OK);
        assert_eq!(ApiResponse::Created.status_code(), StatusCode::CREATED);
        assert_eq!(ApiResponse::NoContent.status_code(), StatusCode::NO_CONTENT);

        // Test client errors
        assert_eq!(
            ApiResponse::BadRequest.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiResponse::Unauthorized.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(ApiResponse::Forbidden.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(ApiResponse::NotFound.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(
            ApiResponse::MethodNotAllowed.status_code(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(ApiResponse::Conflict.status_code(), StatusCode::CONFLICT);
        assert_eq!(ApiResponse::Gone.status_code(), StatusCode::GONE);
        assert_eq!(
            ApiResponse::UnprocessableEntity.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ApiResponse::TooManyRequests.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );

        // Test specific error cases (mapped to appropriate codes)
        assert_eq!(
            ApiResponse::InvalidCredentials.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ApiResponse::ExpiredToken.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ApiResponse::MissingPermissions.status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ApiResponse::ResourceNotFound.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiResponse::DuplicateResource.status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            ApiResponse::ValidationFailed.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ApiResponse::RateLimited.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ApiResponse::FileTooLarge.status_code(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            ApiResponse::UnsupportedMediaType.status_code(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );

        // Test server errors
        assert_eq!(
            ApiResponse::InternalServerError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiResponse::NotImplemented.status_code(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ApiResponse::BadGateway.status_code(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiResponse::ServiceUnavailable.status_code(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            ApiResponse::GatewayTimeout.status_code(),
            StatusCode::GATEWAY_TIMEOUT
        );

        // Test application-specific errors
        assert_eq!(
            ApiResponse::DatabaseError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiResponse::ExternalServiceError.status_code(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiResponse::ConfigurationError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiResponse::TemplateRenderError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiResponse::FileSystemError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        // Test gallery-specific errors
        assert_eq!(
            ApiResponse::GalleryNotFound.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiResponse::DirectoryNotFound.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiResponse::ImageNotFound.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiResponse::InvalidSizeParameter.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiResponse::ProcessingError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiResponse::AccessDenied.status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ApiResponse::CacheEntryNotFound.status_code(),
            StatusCode::NOT_FOUND
        );

        // Test posts-specific errors
        assert_eq!(
            ApiResponse::PostNotFound.status_code(),
            StatusCode::NOT_FOUND
        );

        // Test template-specific errors
        assert_eq!(
            ApiResponse::TemplateNotFound.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_api_response_messages() {
        // Test success messages
        assert_eq!(ApiResponse::Ok.message(), "Request completed successfully");
        assert_eq!(
            ApiResponse::Created.message(),
            "Resource created successfully"
        );
        assert_eq!(
            ApiResponse::NoContent.message(),
            "Request completed successfully"
        );

        // Test client error messages
        assert_eq!(ApiResponse::BadRequest.message(), "Invalid request format");
        assert_eq!(
            ApiResponse::Unauthorized.message(),
            "Authentication required"
        );
        assert_eq!(ApiResponse::Forbidden.message(), "Access denied");
        assert_eq!(ApiResponse::NotFound.message(), "Resource not found");

        // Test specific error messages
        assert_eq!(
            ApiResponse::InvalidCredentials.message(),
            "Invalid username or password"
        );
        assert_eq!(
            ApiResponse::ExpiredToken.message(),
            "Authentication token has expired"
        );
        assert_eq!(
            ApiResponse::MissingPermissions.message(),
            "Insufficient permissions for this action"
        );
        assert_eq!(
            ApiResponse::ResourceNotFound.message(),
            "The requested resource was not found"
        );

        // Test server error messages
        assert_eq!(
            ApiResponse::InternalServerError.message(),
            "Internal server error"
        );
        assert_eq!(
            ApiResponse::DatabaseError.message(),
            "Database operation failed"
        );
        assert_eq!(
            ApiResponse::TemplateRenderError.message(),
            "Template rendering failed"
        );

        // Test gallery-specific messages
        assert_eq!(ApiResponse::GalleryNotFound.message(), "Gallery not found");
        assert_eq!(ApiResponse::ImageNotFound.message(), "Image not found");
        assert_eq!(
            ApiResponse::ProcessingError.message(),
            "Image processing error"
        );

        // Test posts-specific messages
        assert_eq!(ApiResponse::PostNotFound.message(), "Post not found");
    }

    #[test]
    fn test_api_response_classification_methods() {
        // Test success responses
        assert!(ApiResponse::Ok.is_success());
        assert!(ApiResponse::Created.is_success());
        assert!(ApiResponse::NoContent.is_success());
        assert!(!ApiResponse::Ok.is_client_error());
        assert!(!ApiResponse::Ok.is_server_error());

        // Test client errors
        assert!(ApiResponse::BadRequest.is_client_error());
        assert!(ApiResponse::Unauthorized.is_client_error());
        assert!(ApiResponse::NotFound.is_client_error());
        assert!(ApiResponse::InvalidCredentials.is_client_error());
        assert!(ApiResponse::GalleryNotFound.is_client_error());
        assert!(!ApiResponse::BadRequest.is_success());
        assert!(!ApiResponse::BadRequest.is_server_error());

        // Test server errors
        assert!(ApiResponse::InternalServerError.is_server_error());
        assert!(ApiResponse::DatabaseError.is_server_error());
        assert!(ApiResponse::TemplateRenderError.is_server_error());
        assert!(ApiResponse::ProcessingError.is_server_error());
        assert!(!ApiResponse::InternalServerError.is_success());
        assert!(!ApiResponse::InternalServerError.is_client_error());
    }

    #[test]
    fn test_api_response_constants() {
        // Test CLIENT_ERRORS contains all client error variants
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::BadRequest));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::Unauthorized));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::Forbidden));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::NotFound));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::InvalidCredentials));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::GalleryNotFound));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::PostNotFound));
        assert!(ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::AccessDenied));

        // Ensure no server errors are in CLIENT_ERRORS
        assert!(!ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::InternalServerError));
        assert!(!ApiResponse::CLIENT_ERRORS.contains(&ApiResponse::DatabaseError));

        // Test SERVER_ERRORS contains all server error variants
        assert!(ApiResponse::SERVER_ERRORS.contains(&ApiResponse::InternalServerError));
        assert!(ApiResponse::SERVER_ERRORS.contains(&ApiResponse::NotImplemented));
        assert!(ApiResponse::SERVER_ERRORS.contains(&ApiResponse::DatabaseError));
        assert!(ApiResponse::SERVER_ERRORS.contains(&ApiResponse::TemplateRenderError));
        assert!(ApiResponse::SERVER_ERRORS.contains(&ApiResponse::ProcessingError));

        // Ensure no client errors are in SERVER_ERRORS
        assert!(!ApiResponse::SERVER_ERRORS.contains(&ApiResponse::BadRequest));
        assert!(!ApiResponse::SERVER_ERRORS.contains(&ApiResponse::NotFound));
    }

    #[test]
    fn test_api_response_into_response() {
        let response = ApiResponse::Ok.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let response = ApiResponse::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = ApiResponse::InternalServerError.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_api_response_with_message() {
        let response = ApiResponse::BadRequest.with_message("Custom error message");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = ApiResponse::Ok.with_message("Custom success message");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_api_response_with_html() {
        let html_content = "<h1>Error Page</h1>".to_string();
        let response = ApiResponse::NotFound.with_html(html_content);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_comprehensive_error_coverage() {
        // Ensure all defined variants have proper status codes and messages
        let all_variants = [
            // Success responses
            ApiResponse::Ok,
            ApiResponse::Created,
            ApiResponse::NoContent,
            // Client errors
            ApiResponse::BadRequest,
            ApiResponse::Unauthorized,
            ApiResponse::Forbidden,
            ApiResponse::NotFound,
            ApiResponse::MethodNotAllowed,
            ApiResponse::Conflict,
            ApiResponse::Gone,
            ApiResponse::UnprocessableEntity,
            ApiResponse::TooManyRequests,
            // Specific error cases
            ApiResponse::InvalidCredentials,
            ApiResponse::ExpiredToken,
            ApiResponse::MissingPermissions,
            ApiResponse::ResourceNotFound,
            ApiResponse::DuplicateResource,
            ApiResponse::ValidationFailed,
            ApiResponse::RateLimited,
            ApiResponse::FileTooLarge,
            ApiResponse::UnsupportedMediaType,
            // Server errors
            ApiResponse::InternalServerError,
            ApiResponse::NotImplemented,
            ApiResponse::BadGateway,
            ApiResponse::ServiceUnavailable,
            ApiResponse::GatewayTimeout,
            // Application-specific errors
            ApiResponse::DatabaseError,
            ApiResponse::ExternalServiceError,
            ApiResponse::ConfigurationError,
            ApiResponse::TemplateRenderError,
            ApiResponse::FileSystemError,
            // Gallery-specific errors
            ApiResponse::GalleryNotFound,
            ApiResponse::DirectoryNotFound,
            ApiResponse::ImageNotFound,
            ApiResponse::InvalidSizeParameter,
            ApiResponse::ProcessingError,
            ApiResponse::AccessDenied,
            ApiResponse::CacheEntryNotFound,
            // Posts-specific errors
            ApiResponse::PostNotFound,
            // Template-specific errors
            ApiResponse::TemplateNotFound,
        ];

        // Test that all variants have non-empty messages and valid status codes
        for variant in &all_variants {
            let status_code = variant.status_code();
            let message = variant.message();

            // Status code should be in valid HTTP range
            let code_num = status_code.as_u16();
            assert!(
                (200..600).contains(&code_num),
                "Invalid status code {} for variant {:?}",
                code_num,
                variant
            );

            // Message should not be empty
            assert!(
                !message.is_empty(),
                "Empty message for variant {:?}",
                variant
            );

            // Response creation should work
            let _response = variant.into_response();
        }

        // Verify we have expected number of variants
        assert_eq!(
            all_variants.len(),
            40,
            "Update test if new variants are added"
        );
    }

    #[test]
    fn test_status_code_groupings() {
        // Test 2xx success codes
        let success_codes = [
            ApiResponse::Ok,
            ApiResponse::Created,
            ApiResponse::NoContent,
        ];
        for response in &success_codes {
            let code = response.status_code().as_u16();
            assert!(
                (200..300).contains(&code),
                "Success response should have 2xx code"
            );
            assert!(response.is_success());
        }

        // Test 4xx client error codes
        let client_errors = [
            ApiResponse::BadRequest,
            ApiResponse::Unauthorized,
            ApiResponse::Forbidden,
            ApiResponse::NotFound,
            ApiResponse::GalleryNotFound,
            ApiResponse::PostNotFound,
        ];
        for response in &client_errors {
            let code = response.status_code().as_u16();
            assert!(
                (400..500).contains(&code),
                "Client error should have 4xx code"
            );
            assert!(response.is_client_error());
        }

        // Test 5xx server error codes
        let server_errors = [
            ApiResponse::InternalServerError,
            ApiResponse::DatabaseError,
            ApiResponse::TemplateRenderError,
            ApiResponse::ProcessingError,
        ];
        for response in &server_errors {
            let code = response.status_code().as_u16();
            assert!(
                (500..600).contains(&code),
                "Server error should have 5xx code"
            );
            assert!(response.is_server_error());
        }
    }
}
