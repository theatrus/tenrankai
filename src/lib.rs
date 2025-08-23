use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod api;
pub mod composite;
pub mod copyright;
pub mod favicon;
pub mod gallery;
pub mod robots;
pub mod startup_checks;
pub mod static_files;
pub mod templating;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub app: AppConfig,
    pub templates: TemplateConfig,
    pub static_files: StaticConfig,
    pub gallery: GalleryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub name: String,
    pub log_level: String,
    pub download_secret: String,
    pub download_password: String,
    #[serde(default)]
    pub copyright_holder: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateConfig {
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticConfig {
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GalleryConfig {
    pub path_prefix: String,
    pub source_directory: PathBuf,
    pub cache_directory: PathBuf,
    pub images_per_page: usize,
    pub thumbnail: ImageSizeConfig,
    pub gallery_size: ImageSizeConfig,
    pub medium: ImageSizeConfig,
    pub large: ImageSizeConfig,
    pub preview: PreviewConfig,
    pub cache_refresh_interval_minutes: Option<u64>,
    pub jpeg_quality: Option<u8>,
    pub webp_quality: Option<f32>,
    #[serde(default)]
    pub pregenerate_cache: bool,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            app: AppConfig {
                name: "Tenrankai".to_string(),
                log_level: "info".to_string(),
                download_secret: "change-me-in-production".to_string(),
                download_password: "password".to_string(),
                copyright_holder: None,
                base_url: None,
            },
            templates: TemplateConfig {
                directory: PathBuf::from("templates"),
            },
            static_files: StaticConfig {
                directory: PathBuf::from("static"),
            },
            gallery: GalleryConfig {
                path_prefix: "gallery".to_string(),
                source_directory: PathBuf::from("photos"),
                cache_directory: PathBuf::from("cache"),
                images_per_page: 50,
                thumbnail: ImageSizeConfig {
                    width: 300,
                    height: 300,
                },
                gallery_size: ImageSizeConfig {
                    width: 800,
                    height: 800,
                },
                medium: ImageSizeConfig {
                    width: 1200,
                    height: 1200,
                },
                large: ImageSizeConfig {
                    width: 1600,
                    height: 1600,
                },
                preview: PreviewConfig {
                    max_images: 6,
                    max_depth: 3,
                    max_per_folder: 3,
                },
                cache_refresh_interval_minutes: Some(60),
                jpeg_quality: Some(85),
                webp_quality: Some(85.0),
                pregenerate_cache: false,
            },
        }
    }
}

use axum::{
    Router,
    extract::{Path, State},
    response::IntoResponse,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub template_engine: Arc<templating::TemplateEngine>,
    pub static_handler: static_files::StaticFileHandler,
    pub gallery: gallery::SharedGallery,
    pub favicon_renderer: favicon::FaviconRenderer,
    pub config: Config,
}

async fn static_file_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    app_state.static_handler.serve(&path).await
}

pub async fn create_app(config: Config) -> Router {
    let template_engine = Arc::new(templating::TemplateEngine::new(
        config.templates.directory.clone(),
    ));

    let static_handler =
        static_files::StaticFileHandler::new(config.static_files.directory.clone());

    let favicon_renderer = favicon::FaviconRenderer::new(config.static_files.directory.clone());

    let gallery = Arc::new(gallery::Gallery::new(
        config.gallery.clone(),
        config.app.clone(),
    ));

    let app_state = AppState {
        template_engine,
        static_handler,
        gallery,
        favicon_renderer,
        config: config.clone(),
    };

    Router::new()
        .route(
            "/",
            axum::routing::get(templating::template_with_gallery_handler),
        )
        .route(
            "/gallery",
            axum::routing::get(gallery::gallery_root_handler),
        )
        .route(
            "/gallery/{*path}",
            axum::routing::get(gallery::gallery_handler),
        )
        .route(
            "/gallery/image/{*path}",
            axum::routing::get(gallery::image_handler),
        )
        .route(
            "/gallery/detail/{*path}",
            axum::routing::get(gallery::image_detail_handler),
        )
        .route("/api/auth", axum::routing::post(api::authenticate_handler))
        .route("/api/verify", axum::routing::get(api::verify_handler))
        .route(
            "/api/gallery/preview",
            axum::routing::get(api::gallery_preview_handler),
        )
        .route(
            "/api/gallery/composite/{*path}",
            axum::routing::get(api::gallery_composite_preview_handler),
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
        .route("/static/{*path}", axum::routing::get(static_file_handler))
        .route(
            "/{*path}",
            axum::routing::get(templating::template_with_gallery_handler),
        )
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
