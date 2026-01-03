use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod api;
pub mod commands;
pub mod composite;
pub mod copyright;
pub mod email;
pub mod favicon;
pub mod gallery;
pub mod login;
pub mod posts;
pub mod robots;
pub mod startup_checks;
pub mod static_files;
pub mod templating;
pub mod webp_encoder;

/// Template types with path resolution and categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateType {
    // Pages - Static page templates
    Index,
    About,
    Contact,
    NotFound,

    // Modules - Feature-specific templates
    Gallery,
    ImageDetail,
    PostsIndex,
    PostDetail,
    Login,
    LoginSuccess,
    PasskeyEnrollment,
    Profile,

    // Partials - Reusable components
    Header,
    Footer,
    GalleryPreview,
    UserMenu,
}

/// Template path that can be either a type-safe enum or a dynamic string
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TemplatePath {
    /// Type-safe template reference
    Typed(TemplateType),
    /// Dynamic template path (for runtime-determined templates)
    Dynamic(String),
}

impl TemplateType {
    /// Get the template file path
    pub fn path(&self) -> &'static str {
        match self {
            // Pages
            TemplateType::Index => "pages/index.html.liquid",
            TemplateType::About => "pages/about.html.liquid",
            TemplateType::Contact => "pages/contact.html.liquid",
            TemplateType::NotFound => "pages/404.html.liquid",

            // Modules
            TemplateType::Gallery => "modules/gallery.html.liquid",
            TemplateType::ImageDetail => "modules/image_detail.html.liquid",
            TemplateType::PostsIndex => "modules/posts_index.html.liquid",
            TemplateType::PostDetail => "modules/post_detail.html.liquid",
            TemplateType::Login => "modules/login.html.liquid",
            TemplateType::LoginSuccess => "modules/login_success.html.liquid",
            TemplateType::PasskeyEnrollment => "modules/passkey_enrollment.html.liquid",
            TemplateType::Profile => "modules/profile.html.liquid",

            // Partials
            TemplateType::Header => "partials/_header.html.liquid",
            TemplateType::Footer => "partials/_footer.html.liquid",
            TemplateType::GalleryPreview => "partials/_gallery_preview.html.liquid",
            TemplateType::UserMenu => "partials/_user_menu.html.liquid",
        }
    }

    /// Get the template category
    pub fn category(&self) -> TemplateCategory {
        match self {
            TemplateType::Index
            | TemplateType::About
            | TemplateType::Contact
            | TemplateType::NotFound => TemplateCategory::Page,

            TemplateType::Gallery
            | TemplateType::ImageDetail
            | TemplateType::PostsIndex
            | TemplateType::PostDetail
            | TemplateType::Login
            | TemplateType::LoginSuccess
            | TemplateType::PasskeyEnrollment
            | TemplateType::Profile => TemplateCategory::Module,

            TemplateType::Header
            | TemplateType::Footer
            | TemplateType::GalleryPreview
            | TemplateType::UserMenu => TemplateCategory::Partial,
        }
    }

    /// Check if this is a partial template
    pub fn is_partial(&self) -> bool {
        matches!(self.category(), TemplateCategory::Partial)
    }

    /// Get all standard page templates
    pub const PAGES: &'static [TemplateType] = &[
        TemplateType::Index,
        TemplateType::About,
        TemplateType::Contact,
        TemplateType::NotFound,
    ];

    /// Get all module templates  
    pub const MODULES: &'static [TemplateType] = &[
        TemplateType::Gallery,
        TemplateType::ImageDetail,
        TemplateType::PostsIndex,
        TemplateType::PostDetail,
        TemplateType::Login,
        TemplateType::LoginSuccess,
        TemplateType::PasskeyEnrollment,
        TemplateType::Profile,
    ];

    /// Get all partial templates
    pub const PARTIALS: &'static [TemplateType] = &[
        TemplateType::Header,
        TemplateType::Footer,
        TemplateType::GalleryPreview,
        TemplateType::UserMenu,
    ];

    /// Get all standard templates (excludes dynamic pages)
    pub const ALL_STANDARD: &'static [TemplateType] = &[
        // Pages
        TemplateType::Index,
        TemplateType::About,
        TemplateType::Contact,
        TemplateType::NotFound,
        // Modules
        TemplateType::Gallery,
        TemplateType::ImageDetail,
        TemplateType::PostsIndex,
        TemplateType::PostDetail,
        TemplateType::Login,
        TemplateType::LoginSuccess,
        TemplateType::PasskeyEnrollment,
        TemplateType::Profile,
        // Partials
        TemplateType::Header,
        TemplateType::Footer,
        TemplateType::GalleryPreview,
        TemplateType::UserMenu,
    ];

    /// Parse from path string (e.g., "pages/index.html.liquid")
    pub fn parse_from_path(path: &str) -> Option<TemplateType> {
        match path {
            // Pages
            "pages/index.html.liquid" => Some(TemplateType::Index),
            "pages/about.html.liquid" => Some(TemplateType::About),
            "pages/contact.html.liquid" => Some(TemplateType::Contact),
            "pages/404.html.liquid" => Some(TemplateType::NotFound),

            // Modules
            "modules/gallery.html.liquid" => Some(TemplateType::Gallery),
            "modules/image_detail.html.liquid" => Some(TemplateType::ImageDetail),
            "modules/posts_index.html.liquid" => Some(TemplateType::PostsIndex),
            "modules/post_detail.html.liquid" => Some(TemplateType::PostDetail),
            "modules/login.html.liquid" => Some(TemplateType::Login),
            "modules/login_success.html.liquid" => Some(TemplateType::LoginSuccess),
            "modules/passkey_enrollment.html.liquid" => Some(TemplateType::PasskeyEnrollment),
            "modules/profile.html.liquid" => Some(TemplateType::Profile),

            // Partials
            "partials/_header.html.liquid" => Some(TemplateType::Header),
            "partials/_footer.html.liquid" => Some(TemplateType::Footer),
            "partials/_gallery_preview.html.liquid" => Some(TemplateType::GalleryPreview),
            "partials/_user_menu.html.liquid" => Some(TemplateType::UserMenu),

            _ => None,
        }
    }

    /// Create a dynamic page path for runtime-determined pages
    pub fn dynamic_page_path(name: &str) -> String {
        format!("pages/{}.html.liquid", name)
    }
}

impl TemplatePath {
    /// Get the template path string
    pub fn path(&self) -> String {
        match self {
            TemplatePath::Typed(template_type) => template_type.path().to_string(),
            TemplatePath::Dynamic(path) => path.clone(),
        }
    }

    /// Create a typed template path
    pub fn typed(template_type: TemplateType) -> Self {
        TemplatePath::Typed(template_type)
    }

    /// Create a dynamic template path
    pub fn dynamic(path: String) -> Self {
        TemplatePath::Dynamic(path)
    }

    /// Create a dynamic page path
    pub fn dynamic_page(name: &str) -> Self {
        TemplatePath::Dynamic(TemplateType::dynamic_page_path(name))
    }
}

/// Template category classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateCategory {
    Page,
    Module,
    Partial,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path())
    }
}

impl std::fmt::Display for TemplatePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path())
    }
}

/// HTTP Status Response System for consistent API responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiResponse {
    // Success responses
    Ok,

    // Client errors (4xx)
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,

    // Specific error cases
    GalleryNotFound,
    ImageNotFound,
    DirectoryNotFound,
    CacheEntryNotFound,
    InvalidSizeParameter,
    InvalidCredentials,
    AccessDenied,
    TemplateNotFound,
    PostNotFound,
    UserNotFound,

    // Server errors (5xx)
    InternalServerError,
    NotImplemented,
    ProcessingError,
    DatabaseError,
    TemplateRenderError,
    FileSystemError,
}

impl ApiResponse {
    /// Get the HTTP status code for this response
    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            // Success responses
            ApiResponse::Ok => StatusCode::OK,

            // Client errors
            ApiResponse::BadRequest | ApiResponse::InvalidSizeParameter => StatusCode::BAD_REQUEST,
            ApiResponse::Unauthorized | ApiResponse::InvalidCredentials => StatusCode::UNAUTHORIZED,
            ApiResponse::Forbidden | ApiResponse::AccessDenied => StatusCode::FORBIDDEN,
            ApiResponse::NotFound
            | ApiResponse::GalleryNotFound
            | ApiResponse::ImageNotFound
            | ApiResponse::DirectoryNotFound
            | ApiResponse::CacheEntryNotFound
            | ApiResponse::TemplateNotFound
            | ApiResponse::PostNotFound
            | ApiResponse::UserNotFound => StatusCode::NOT_FOUND,
            ApiResponse::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,

            // Server errors
            ApiResponse::InternalServerError
            | ApiResponse::ProcessingError
            | ApiResponse::DatabaseError
            | ApiResponse::TemplateRenderError
            | ApiResponse::FileSystemError => StatusCode::INTERNAL_SERVER_ERROR,
            ApiResponse::NotImplemented => StatusCode::NOT_IMPLEMENTED,
        }
    }

    /// Get the error message for this response
    pub fn message(&self) -> &'static str {
        match self {
            // Success responses
            ApiResponse::Ok => "Success",

            // Client errors
            ApiResponse::BadRequest => "Bad request",
            ApiResponse::Unauthorized => "Unauthorized",
            ApiResponse::Forbidden => "Forbidden",
            ApiResponse::NotFound => "Not found",
            ApiResponse::MethodNotAllowed => "Method not allowed",

            // Specific error cases
            ApiResponse::GalleryNotFound => "Gallery not found",
            ApiResponse::ImageNotFound => "Image not found",
            ApiResponse::DirectoryNotFound => "Directory not found",
            ApiResponse::CacheEntryNotFound => "Cache entry not found",
            ApiResponse::InvalidSizeParameter => "Invalid size parameter",
            ApiResponse::InvalidCredentials => "Invalid credentials",
            ApiResponse::AccessDenied => "Access denied",
            ApiResponse::TemplateNotFound => "Template not found",
            ApiResponse::PostNotFound => "Post not found",
            ApiResponse::UserNotFound => "User not found",

            // Server errors
            ApiResponse::InternalServerError => "Internal server error",
            ApiResponse::NotImplemented => "Not implemented",
            ApiResponse::ProcessingError => "Processing error",
            ApiResponse::DatabaseError => "Database error",
            ApiResponse::TemplateRenderError => "Template rendering error",
            ApiResponse::FileSystemError => "File system error",
        }
    }

    /// Create an HTTP response for this API response
    pub fn into_response(self) -> axum::response::Response {
        use axum::response::IntoResponse;
        (self.status_code(), self.message()).into_response()
    }

    /// Create an HTTP response with custom message
    pub fn with_message(self, message: String) -> axum::response::Response {
        use axum::response::IntoResponse;
        (self.status_code(), message).into_response()
    }

    /// Create an HTML response with custom content and proper status
    pub fn with_html(self, html: String) -> axum::response::Response {
        use axum::response::{Html, IntoResponse};
        (self.status_code(), Html(html)).into_response()
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        matches!(self.status_code().as_u16(), 400..=499)
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        matches!(self.status_code().as_u16(), 500..=599)
    }

    /// Check if this is a success response (2xx)
    pub fn is_success(&self) -> bool {
        matches!(self.status_code().as_u16(), 200..=299)
    }

    /// Get all API responses that are client errors
    pub const CLIENT_ERRORS: &'static [ApiResponse] = &[
        ApiResponse::BadRequest,
        ApiResponse::Unauthorized,
        ApiResponse::Forbidden,
        ApiResponse::NotFound,
        ApiResponse::MethodNotAllowed,
        ApiResponse::GalleryNotFound,
        ApiResponse::ImageNotFound,
        ApiResponse::DirectoryNotFound,
        ApiResponse::CacheEntryNotFound,
        ApiResponse::InvalidSizeParameter,
        ApiResponse::InvalidCredentials,
        ApiResponse::AccessDenied,
        ApiResponse::TemplateNotFound,
        ApiResponse::PostNotFound,
        ApiResponse::UserNotFound,
    ];

    /// Get all API responses that are server errors
    pub const SERVER_ERRORS: &'static [ApiResponse] = &[
        ApiResponse::InternalServerError,
        ApiResponse::NotImplemented,
        ApiResponse::ProcessingError,
        ApiResponse::DatabaseError,
        ApiResponse::TemplateRenderError,
        ApiResponse::FileSystemError,
    ];
}

/// Log level type system for configuration and tracing integration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Convert to tracing::Level
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }

    /// Convert to tracing filter level
    pub fn to_tracing_filter(&self) -> tracing::metadata::LevelFilter {
        match self {
            LogLevel::Trace => tracing::metadata::LevelFilter::TRACE,
            LogLevel::Debug => tracing::metadata::LevelFilter::DEBUG,
            LogLevel::Info => tracing::metadata::LevelFilter::INFO,
            LogLevel::Warn => tracing::metadata::LevelFilter::WARN,
            LogLevel::Error => tracing::metadata::LevelFilter::ERROR,
        }
    }

    /// Check if this is a verbose log level (trace or debug)
    pub fn is_verbose(&self) -> bool {
        matches!(self, LogLevel::Trace | LogLevel::Debug)
    }

    /// Check if this level would log messages at the given level
    pub fn would_log(&self, other: LogLevel) -> bool {
        match self {
            LogLevel::Trace => true,
            LogLevel::Debug => !matches!(other, LogLevel::Trace),
            LogLevel::Info => matches!(other, LogLevel::Info | LogLevel::Warn | LogLevel::Error),
            LogLevel::Warn => matches!(other, LogLevel::Warn | LogLevel::Error),
            LogLevel::Error => matches!(other, LogLevel::Error),
        }
    }

    /// Get all log levels
    pub const ALL: &'static [LogLevel] = &[
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ];

    /// Parse from string
    pub fn parse(s: &str) -> Option<LogLevel> {
        match s.to_lowercase().as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "Invalid log level '{}'. Valid levels: trace, debug, info, warn, error",
                s
            )
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub app: AppConfig,
    pub templates: TemplateConfig,
    pub static_files: StaticConfig,
    #[serde(default)]
    pub galleries: Option<Vec<GallerySystemConfig>>,
    #[serde(default)]
    pub posts: Option<Vec<PostsSystemConfig>>,
    #[serde(default)]
    pub email: Option<email::EmailConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub log_level: LogLevel,
    pub cookie_secret: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub user_database: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateConfig {
    #[serde(
        deserialize_with = "deserialize_template_directories",
        serialize_with = "serialize_template_directories"
    )]
    pub directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticConfig {
    #[serde(
        deserialize_with = "deserialize_static_directories",
        serialize_with = "serialize_static_directories"
    )]
    pub directories: Vec<PathBuf>,
}

fn deserialize_static_directories<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct StaticDirectoriesVisitor;

    impl<'de> Visitor<'de> for StaticDirectoriesVisitor {
        type Value = Vec<PathBuf>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a path string or an array of path strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![PathBuf::from(value)])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut dirs = Vec::new();
            while let Some(dir) = seq.next_element::<String>()? {
                dirs.push(PathBuf::from(dir));
            }
            Ok(dirs)
        }
    }

    deserializer.deserialize_any(StaticDirectoriesVisitor)
}

fn serialize_static_directories<S>(dirs: &Vec<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    if dirs.len() == 1 {
        serializer.serialize_str(dirs[0].to_str().unwrap_or(""))
    } else {
        let mut seq = serializer.serialize_seq(Some(dirs.len()))?;
        for dir in dirs {
            seq.serialize_element(dir.to_str().unwrap_or(""))?;
        }
        seq.end()
    }
}

fn deserialize_template_directories<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TemplateDirectoriesVisitor;

    impl<'de> Visitor<'de> for TemplateDirectoriesVisitor {
        type Value = Vec<PathBuf>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a path string or an array of path strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![PathBuf::from(value)])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut dirs = Vec::new();
            while let Some(dir) = seq.next_element::<String>()? {
                dirs.push(PathBuf::from(dir));
            }
            Ok(dirs)
        }
    }

    deserializer.deserialize_any(TemplateDirectoriesVisitor)
}

fn serialize_template_directories<S>(dirs: &Vec<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    if dirs.len() == 1 {
        serializer.serialize_str(dirs[0].to_str().unwrap_or(""))
    } else {
        let mut seq = serializer.serialize_seq(Some(dirs.len()))?;
        for dir in dirs {
            seq.serialize_element(dir.to_str().unwrap_or(""))?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GallerySystemConfig {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: PathBuf,
    pub cache_directory: PathBuf,
    #[serde(default = "default_gallery_template")]
    pub gallery_template: String,
    #[serde(default = "default_image_detail_template")]
    pub image_detail_template: String,
    #[serde(default = "default_images_per_page")]
    pub images_per_page: usize,
    #[serde(default = "default_thumbnail_size")]
    pub thumbnail: ImageSizeConfig,
    #[serde(default = "default_gallery_size")]
    pub gallery_size: ImageSizeConfig,
    #[serde(default = "default_medium_size")]
    pub medium: ImageSizeConfig,
    #[serde(default = "default_large_size")]
    pub large: ImageSizeConfig,
    #[serde(default = "default_preview_config")]
    pub preview: PreviewConfig,
    pub cache_refresh_interval_minutes: Option<u64>,
    pub jpeg_quality: Option<u8>,
    pub webp_quality: Option<f32>,
    #[serde(default)]
    pub pregenerate_cache: bool,
    /// Number of days to consider an image as "new" (based on file modification date)
    pub new_threshold_days: Option<u32>,
    /// When true, show only approximate capture dates (month/year) to non-authenticated users
    #[serde(default = "default_false")]
    pub approximate_dates_for_public: bool,
    /// Copyright holder name for watermarking medium-sized images
    #[serde(default)]
    pub copyright_holder: Option<String>,
    /// When true, hide location/GPS information from non-authenticated users
    #[serde(default = "default_false")]
    pub hide_location_from_public: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageSizeConfig {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewConfig {
    pub max_images: usize,
    pub max_depth: usize,
    pub max_per_folder: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PostsSystemConfig {
    pub name: String,
    pub source_directory: PathBuf,
    pub url_prefix: String,
    #[serde(default = "default_posts_index_template")]
    pub index_template: String,
    #[serde(default = "default_posts_detail_template")]
    pub post_template: String,
    #[serde(default = "default_posts_per_page")]
    pub posts_per_page: usize,
    pub refresh_interval_minutes: Option<u64>,
}

fn default_posts_index_template() -> String {
    "modules/posts_index.html.liquid".to_string()
}

fn default_posts_detail_template() -> String {
    "modules/post_detail.html.liquid".to_string()
}

fn default_posts_per_page() -> usize {
    20
}

fn default_false() -> bool {
    false
}

fn default_gallery_template() -> String {
    "modules/gallery.html.liquid".to_string()
}

fn default_image_detail_template() -> String {
    "modules/image_detail.html.liquid".to_string()
}

fn default_images_per_page() -> usize {
    50
}

fn default_thumbnail_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 300,
        height: 300,
    }
}

fn default_gallery_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 800,
        height: 800,
    }
}

fn default_medium_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 1200,
        height: 1200,
    }
}

fn default_large_size() -> ImageSizeConfig {
    ImageSizeConfig {
        width: 1600,
        height: 1600,
    }
}

fn default_preview_config() -> PreviewConfig {
    PreviewConfig {
        max_images: 4,
        max_depth: 3,
        max_per_folder: 3,
    }
}

impl Default for GallerySystemConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: PathBuf::from("photos"),
            cache_directory: PathBuf::from("cache"),
            gallery_template: default_gallery_template(),
            image_detail_template: default_image_detail_template(),
            images_per_page: default_images_per_page(),
            thumbnail: default_thumbnail_size(),
            gallery_size: default_gallery_size(),
            medium: default_medium_size(),
            large: default_large_size(),
            preview: default_preview_config(),
            cache_refresh_interval_minutes: Some(60),
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
            copyright_holder: None,
            hide_location_from_public: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            app: AppConfig {
                name: "Tenrankai".to_string(),
                log_level: LogLevel::Info,
                cookie_secret: "change-me-in-production-use-a-long-random-string".to_string(),
                base_url: None,
                user_database: None,
            },
            templates: TemplateConfig {
                directories: vec![PathBuf::from("templates")],
            },
            static_files: StaticConfig {
                directories: vec![PathBuf::from("static")],
            },
            galleries: Some(vec![GallerySystemConfig {
                name: "default".to_string(),
                url_prefix: "/gallery".to_string(),
                source_directory: PathBuf::from("photos"),
                cache_directory: PathBuf::from("cache"),
                gallery_template: default_gallery_template(),
                image_detail_template: default_image_detail_template(),
                images_per_page: default_images_per_page(),
                thumbnail: default_thumbnail_size(),
                gallery_size: default_gallery_size(),
                medium: default_medium_size(),
                large: default_large_size(),
                preview: default_preview_config(),
                cache_refresh_interval_minutes: Some(60),
                jpeg_quality: Some(85),
                webp_quality: Some(85.0),
                pregenerate_cache: false,
                new_threshold_days: None,
                approximate_dates_for_public: false,
                copyright_holder: None,
                hide_location_from_public: false,
            }]),
            posts: None,
            email: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_enum_functionality() {
        // Test parsing
        assert_eq!(LogLevel::parse("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::parse("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::parse("invalid"), None);

        // Test string conversion
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Error.as_str(), "error");

        // Test is_verbose
        assert!(LogLevel::Trace.is_verbose());
        assert!(LogLevel::Debug.is_verbose());
        assert!(!LogLevel::Info.is_verbose());
        assert!(!LogLevel::Warn.is_verbose());
        assert!(!LogLevel::Error.is_verbose());

        // Test would_log
        assert!(LogLevel::Debug.would_log(LogLevel::Error));
        assert!(LogLevel::Debug.would_log(LogLevel::Warn));
        assert!(LogLevel::Debug.would_log(LogLevel::Info));
        assert!(LogLevel::Debug.would_log(LogLevel::Debug));
        assert!(!LogLevel::Debug.would_log(LogLevel::Trace));

        assert!(!LogLevel::Error.would_log(LogLevel::Debug));
        assert!(!LogLevel::Error.would_log(LogLevel::Info));
        assert!(LogLevel::Error.would_log(LogLevel::Error));

        // Test tracing level conversion
        assert_eq!(LogLevel::Info.to_tracing_level(), tracing::Level::INFO);
        assert_eq!(LogLevel::Debug.to_tracing_level(), tracing::Level::DEBUG);

        // Test tracing filter conversion
        assert_eq!(
            LogLevel::Info.to_tracing_filter(),
            tracing::metadata::LevelFilter::INFO
        );

        // Test default
        assert_eq!(LogLevel::default(), LogLevel::Info);

        // Test display
        assert_eq!(format!("{}", LogLevel::Warn), "warn");

        // Test FromStr
        let level: LogLevel = "trace".parse().unwrap();
        assert_eq!(level, LogLevel::Trace);

        let result: Result<LogLevel, _> = "invalid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_log_level_serde() {
        // Test serialization/deserialization
        let config = AppConfig {
            name: "Test".to_string(),
            log_level: LogLevel::Debug,
            cookie_secret: "secret".to_string(),
            base_url: None,
            user_database: None,
        };

        let toml = toml_edit::ser::to_string_pretty(&config).unwrap();
        assert!(toml.contains("log_level = \"debug\""));

        let parsed: AppConfig = toml_edit::de::from_str(&toml).unwrap();
        assert_eq!(parsed.log_level, LogLevel::Debug);
    }
}

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderValue, Request},
    middleware::{self, Next},
    response::IntoResponse,
};
use std::{collections::HashMap, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

#[derive(Clone)]
pub struct AppState {
    pub template_engine: Arc<templating::TemplateEngine>,
    pub static_handler: static_files::StaticFileHandler,
    pub galleries: Arc<HashMap<String, gallery::SharedGallery>>,
    pub favicon_renderer: favicon::FaviconRenderer,
    pub posts_managers: Arc<HashMap<String, Arc<posts::PostsManager>>>,
    pub login_state: Arc<tokio::sync::RwLock<login::LoginState>>,
    pub user_database_manager: Option<login::types::UserDatabaseManager>,
    pub email_provider: Option<email::DynEmailProvider>,
    pub webauthn: Option<Arc<webauthn_rs::Webauthn>>,
    pub config: Config,
}

async fn static_file_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // Check if request has version parameter
    let has_version = params.contains_key("v");
    app_state.static_handler.serve(&path, has_version).await
}

async fn server_header_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Add server header with version
    let server_value = format!("Tenrankai/{}", env!("CARGO_PKG_VERSION"));
    if let Ok(header_value) = HeaderValue::from_str(&server_value) {
        headers.insert("Server", header_value);
    }

    response
}

pub async fn create_app(config: Config) -> axum::Router {
    let mut template_engine = templating::TemplateEngine::new(config.templates.directories.clone());

    let static_handler =
        static_files::StaticFileHandler::new(config.static_files.directories.clone());

    // Ensure file versions are loaded before proceeding
    static_handler.refresh_file_versions().await;

    // Set the static handler on the template engine for cache busting
    template_engine.set_static_handler(static_handler.clone());

    // Set whether user auth is enabled
    template_engine.set_has_user_auth(config.app.user_database.is_some());

    // Update file versions for the template engine
    template_engine.update_file_versions().await;

    let template_engine = Arc::new(template_engine);

    let favicon_renderer = favicon::FaviconRenderer::new(config.static_files.directories.clone());

    // Initialize galleries
    let mut galleries = HashMap::new();
    if let Some(gallery_configs) = &config.galleries {
        for gallery_config in gallery_configs {
            let gallery = Arc::new(gallery::Gallery::new(gallery_config.clone()));
            galleries.insert(gallery_config.name.clone(), gallery);
        }
    }

    // Initialize posts managers
    let galleries_arc = Arc::new(galleries);
    let mut posts_managers = HashMap::new();
    if let Some(posts_configs) = &config.posts {
        for posts_config in posts_configs {
            let mut posts_manager = posts::PostsManager::new(posts::PostsConfig {
                source_directory: posts_config.source_directory.clone(),
                url_prefix: posts_config.url_prefix.clone(),
                index_template: posts_config.index_template.clone(),
                post_template: posts_config.post_template.clone(),
                posts_per_page: posts_config.posts_per_page,
                refresh_interval_minutes: posts_config.refresh_interval_minutes,
            });

            // Set galleries reference
            posts_manager.set_galleries(galleries_arc.clone());

            let posts_manager = Arc::new(posts_manager);

            // Initialize posts on startup
            info!(
                "Initializing posts for '{}' from {:?}",
                posts_config.name, posts_config.source_directory
            );
            if let Err(e) = posts_manager.refresh_posts().await {
                error!(
                    "Failed to initialize posts for '{}': {}",
                    posts_config.name, e
                );
            }

            posts_managers.insert(posts_config.name.clone(), posts_manager);
        }
    }

    let posts_managers_arc = Arc::new(posts_managers);

    // Initialize login state and user database only if user database is configured
    let (login_state, user_database_manager) =
        if let Some(db_path) = config.app.user_database.as_ref() {
            let state = Arc::new(tokio::sync::RwLock::new(login::LoginState::new()));
            // Start periodic cleanup for login tokens and rate limits
            login::start_periodic_cleanup(state.clone());

            // Initialize user database manager
            let db_manager = match login::types::UserDatabaseManager::new(db_path.clone()).await {
                Ok(manager) => {
                    info!("User database initialized from {:?}", db_path);
                    Some(manager)
                }
                Err(e) => {
                    error!("Failed to initialize user database: {}", e);
                    None
                }
            };

            (state, db_manager)
        } else {
            // Create an empty login state for consistency
            (
                Arc::new(tokio::sync::RwLock::new(login::LoginState::new())),
                None,
            )
        };

    // Initialize email provider if configured
    let email_provider = if let Some(email_config) = &config.email {
        match email::create_provider(&email_config.provider).await {
            Ok(provider) => {
                info!("Email provider initialized: {}", provider.name());
                Some(provider)
            }
            Err(e) => {
                error!("Failed to initialize email provider: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Initialize WebAuthn if base_url is configured
    let webauthn = if config.app.base_url.is_some() {
        match login::webauthn::create_webauthn(&config) {
            Ok(wa) => {
                info!("WebAuthn initialized");
                Some(wa)
            }
            Err(e) => {
                error!("Failed to initialize WebAuthn: {}", e);
                None
            }
        }
    } else {
        None
    };

    let app_state = AppState {
        template_engine,
        static_handler,
        galleries: galleries_arc,
        favicon_renderer,
        posts_managers: posts_managers_arc.clone(),
        login_state,
        user_database_manager,
        email_provider,
        webauthn,
        config: config.clone(),
    };

    let mut router = Router::new()
        .route(
            "/",
            axum::routing::get(templating::template_with_gallery_handler),
        )
        .route(
            "/favicon.ico",
            axum::routing::get(favicon::favicon_ico_handler),
        )
        .route(
            "/favicon-16x16.png",
            axum::routing::get(favicon::favicon_png_16_handler),
        )
        .route(
            "/favicon-32x32.png",
            axum::routing::get(favicon::favicon_png_32_handler),
        )
        .route(
            "/favicon-48x48.png",
            axum::routing::get(favicon::favicon_png_48_handler),
        )
        .route(
            "/robots.txt",
            axum::routing::get(robots::robots_txt_handler),
        )
        .route("/static/{*path}", axum::routing::get(static_file_handler));

    // Add login routes only if user database is configured
    if config.app.user_database.is_some() {
        router = router
            .route("/_login", axum::routing::get(login::login_page))
            .route("/_login/request", axum::routing::post(login::login_request))
            .route("/_login/verify", axum::routing::get(login::verify_login))
            .route("/_login/logout", axum::routing::get(login::logout))
            .route(
                "/_login/passkeys",
                axum::routing::get(templating::template_with_gallery_handler),
            )
            .route(
                "/_login/passkey-enrollment",
                axum::routing::get(login::passkey_enrollment_page),
            )
            .route("/_login/profile", axum::routing::get(login::profile_page))
            .route("/api/verify", axum::routing::get(login::check_auth_status))
            .route(
                "/api/refresh-static-versions",
                axum::routing::post(api::refresh_static_versions),
            );

        // Add WebAuthn routes if available
        if app_state.webauthn.is_some() {
            router = router
                .route(
                    "/api/webauthn/check-passkeys",
                    axum::routing::post(login::webauthn::check_user_has_passkeys),
                )
                .route(
                    "/api/webauthn/register/start",
                    axum::routing::post(login::webauthn::start_passkey_registration),
                )
                .route(
                    "/api/webauthn/register/finish/{reg_id}",
                    axum::routing::post(login::webauthn::finish_passkey_registration),
                )
                .route(
                    "/api/webauthn/authenticate/start",
                    axum::routing::post(login::webauthn::start_passkey_authentication),
                )
                .route(
                    "/api/webauthn/authenticate/finish/{auth_id}",
                    axum::routing::post(login::webauthn::finish_passkey_authentication),
                )
                .route(
                    "/api/webauthn/passkeys",
                    axum::routing::get(login::webauthn::list_passkeys),
                )
                .route(
                    "/api/webauthn/passkeys/{passkey_id}",
                    axum::routing::delete(login::webauthn::delete_passkey),
                )
                .route(
                    "/api/webauthn/passkeys/{passkey_id}/name",
                    axum::routing::put(login::webauthn::update_passkey_name),
                );
        }
    }

    // Add gallery routes dynamically based on configuration
    if let Some(gallery_configs) = &config.galleries {
        for gallery_config in gallery_configs {
            let prefix = &gallery_config.url_prefix;
            let name = gallery_config.name.clone();

            // Root route for gallery
            router = router.route(
                prefix,
                axum::routing::get({
                    let name = name.clone();
                    move |state, query, headers| {
                        gallery::gallery_root_handler_for_named(state, Path(name), query, headers)
                    }
                }),
            );

            // Gallery folder browsing
            router = router.route(
                &format!("{}/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, query, headers| {
                        let gallery_path = path.0;
                        gallery::gallery_handler_for_named(
                            state,
                            Path((name, gallery_path)),
                            query,
                            headers,
                        )
                    }
                }),
            );

            // Image serving
            router = router.route(
                &format!("{}/image/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, query, headers| {
                        let image_path = path.0;
                        gallery::image_handler_for_named(
                            state,
                            Path((name, image_path)),
                            query,
                            headers,
                        )
                    }
                }),
            );

            // Image detail view
            router = router.route(
                &format!("{}/detail/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, headers| {
                        let detail_path = path.0;
                        gallery::image_detail_handler_for_named(
                            state,
                            Path((name, detail_path)),
                            headers,
                        )
                    }
                }),
            );

            // API routes for gallery
            router = router.route(
                &format!("/api/gallery/{}/preview", name),
                axum::routing::get({
                    let name = name.clone();
                    move |state, query| {
                        api::gallery_preview_handler_for_named(state, Path(name), query)
                    }
                }),
            );

            router = router.route(
                &format!("/api/gallery/{}/composite/{{*path}}", name),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>| {
                        let composite_path = path.0;
                        api::gallery_composite_preview_handler_for_named(
                            state,
                            Path((name, composite_path)),
                        )
                    }
                }),
            );
        }
    }

    // Add posts routes dynamically based on configuration
    if let Some(posts_configs) = &config.posts {
        for posts_config in posts_configs {
            let prefix = &posts_config.url_prefix;
            let name = posts_config.name.clone();

            // Index route for posts listing
            router = router.route(
                prefix,
                axum::routing::get({
                    let name = name.clone();
                    move |state, query| {
                        posts::handlers::posts_index_handler(state, Path(name), query)
                    }
                }),
            );

            // Detail route for individual posts
            router = router.route(
                &format!("{}/{{*slug}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>| {
                        let slug = path.0;
                        posts::handlers::post_detail_handler(state, Path((name, slug)))
                    }
                }),
            );

            // Refresh route for posts
            router = router.route(
                &format!("/api/posts/{}/refresh", name),
                axum::routing::post({
                    let name = name.clone();
                    move |state| posts::handlers::refresh_posts_handler(state, Path(name))
                }),
            );
        }
    }

    // Add catch-all route for templates
    router = router.route(
        "/{*path}",
        axum::routing::get(templating::template_with_gallery_handler),
    );

    router
        .layer(middleware::from_fn(server_header_middleware))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let method = request.method();
                    let uri = request.uri();
                    let matched_path = request
                        .extensions()
                        .get::<axum::extract::MatchedPath>()
                        .map(|matched_path| matched_path.as_str());

                    tracing::info_span!(
                        "http_request",
                        method = %method,
                        uri = %uri,
                        matched_path,
                    )
                })
                .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                    let method = request.method();
                    let uri = request.uri();
                    let headers = request.headers();
                    let user_agent = headers
                        .get("user-agent")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("-");
                    let referer = headers
                        .get("referer")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("-");

                    tracing::info!(
                        target: "access_log",
                        method = %method,
                        path = %uri.path(),
                        query = ?uri.query(),
                        user_agent = %user_agent,
                        referer = %referer,
                        "request"
                    );
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     _span: &tracing::Span| {
                        let status = response.status();
                        let size = response
                            .headers()
                            .get("content-length")
                            .and_then(|h| h.to_str().ok())
                            .unwrap_or("-");

                        tracing::info!(
                            target: "access_log",
                            status = %status,
                            size = %size,
                            latency_ms = %latency.as_millis(),
                            "response"
                        );
                    },
                ),
        )
        .with_state(app_state)
}
