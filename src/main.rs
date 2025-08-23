use axum::{
    Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod api;
mod copyright;
mod favicon;
mod gallery;
mod static_files;
mod templating;

use favicon::{
    FaviconRenderer, favicon_ico_handler, favicon_png_16_handler, favicon_png_32_handler,
    favicon_png_48_handler,
};
use gallery::{Gallery, SharedGallery, gallery_handler, gallery_root_handler, image_detail_handler, image_handler};
use static_files::StaticFileHandler;
use templating::{TemplateEngine, template_with_gallery_handler};

#[derive(Clone)]
pub struct AppState {
    pub template_engine: Arc<TemplateEngine>,
    pub static_handler: StaticFileHandler,
    pub gallery: SharedGallery,
    pub favicon_renderer: FaviconRenderer,
    pub config: Config,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    #[arg(short, long)]
    port: Option<u16>,

    #[arg(long)]
    host: Option<String>,

    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    server: ServerConfig,
    app: AppConfig,
    templates: TemplateConfig,
    static_files: StaticConfig,
    gallery: GalleryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    name: String,
    log_level: String,
    download_secret: String,
    download_password: String,
    #[serde(default)]
    copyright_holder: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateConfig {
    directory: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticConfig {
    directory: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GalleryConfig {
    path_prefix: String,
    source_directory: PathBuf,
    cache_directory: PathBuf,
    images_per_page: usize,
    thumbnail: ImageSizeConfig,
    gallery_size: ImageSizeConfig,
    medium: ImageSizeConfig,
    large: ImageSizeConfig,
    preview: PreviewConfig,
    cache_refresh_interval_minutes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageSizeConfig {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewConfig {
    max_images: usize,
    max_depth: usize,
    max_per_folder: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            app: AppConfig {
                name: "DynServer".to_string(),
                log_level: "info".to_string(),
                download_secret: "your-secret-key-change-this".to_string(),
                download_password: "gallery2024".to_string(),
                copyright_holder: None,
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
                cache_directory: PathBuf::from("cache/photos"),
                images_per_page: 20,
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
                    max_images: 4,
                    max_depth: 3,
                    max_per_folder: 3,
                },
                cache_refresh_interval_minutes: Some(60), // Default to 1 hour
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let config = if args.config.exists() {
        let config_content = std::fs::read_to_string(&args.config)?;
        toml::from_str::<Config>(&config_content)?
    } else {
        info!("Config file not found at {:?}, using defaults", args.config);
        Config::default()
    };

    let host = args.host.unwrap_or(config.server.host.clone());
    let port = args.port.unwrap_or(config.server.port);
    let log_level = args.log_level;
    let config_clone = config.clone();

    let level = match log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting {} server", config.app.name);
    info!("Configuration loaded from: {:?}", args.config);
    info!("Template directory: {:?}", config.templates.directory);
    info!(
        "Static files directory: {:?}",
        config.static_files.directory
    );
    info!(
        "Gallery source directory: {:?}",
        config.gallery.source_directory
    );
    info!(
        "Gallery cache directory: {:?}",
        config.gallery.cache_directory
    );

    let template_engine = Arc::new(TemplateEngine::new(
        config_clone.templates.directory.clone(),
    ));
    let static_handler = StaticFileHandler::new(config_clone.static_files.directory.clone());
    let favicon_renderer = FaviconRenderer::new(config_clone.static_files.directory.clone());
    let gallery: SharedGallery = Arc::new(Gallery::new(
        config_clone.gallery.clone(),
        config_clone.app.clone(),
    ));

    // Initialize gallery and check for version changes
    if let Err(e) = gallery.initialize_and_check_version().await {
        tracing::warn!("Failed to initialize gallery metadata cache: {}", e);
    }

    // Start background cache refresh if configured
    if let Some(interval_minutes) = config_clone.gallery.cache_refresh_interval_minutes
        && interval_minutes > 0
    {
        info!(
            "Starting background metadata cache refresh every {} minutes",
            interval_minutes
        );
        Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
    }

    // Clone gallery for shutdown handler before moving it into router state
    let gallery_for_shutdown = gallery.clone();

    let app_state = AppState {
        template_engine,
        static_handler,
        gallery,
        favicon_renderer,
        config: config_clone,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/echo", post(echo))
        .route("/api/auth", post(api::authenticate_handler))
        .route("/api/verify", get(api::verify_handler))
        .route("/static/{*path}", get(static_file_handler))
        .route("/favicon.ico", get(favicon_ico_handler))
        .route("/favicon-16.png", get(favicon_png_16_handler))
        .route("/favicon-32.png", get(favicon_png_32_handler))
        .route("/favicon-48.png", get(favicon_png_48_handler))
        .route("/robots.txt", get(robots_txt_handler))
        .route("/gallery", get(gallery_root_handler))
        .route("/gallery/", get(gallery_root_handler))
        .route("/gallery/detail/{*path}", get(image_detail_handler))
        .route("/gallery/image/{*path}", get(image_handler))
        .route("/gallery/{*path}", get(gallery_handler))
        .route("/", get(template_with_gallery_handler))
        .route("/{*path}", get(template_with_gallery_handler))
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(
                    tower_http::trace::DefaultMakeSpan::new()
                        .level(tracing::Level::INFO)
                        .include_headers(true),
                )
                .on_request(tower_http::trace::DefaultOnRequest::new().level(tracing::Level::INFO))
                .on_response(
                    tower_http::trace::DefaultOnResponse::new()
                        .level(tracing::Level::INFO)
                        .latency_unit(tower_http::LatencyUnit::Micros),
                ),
        )
        .with_state(app_state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Server listening on {}", addr);
    info!("HTTP request logging enabled - set RUST_LOG environment variable to control verbosity");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Set up graceful shutdown
    let shutdown_signal = async move {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        info!("Received shutdown signal, saving cache...");

        // Save metadata cache before shutting down
        let _ = gallery_for_shutdown.save_caches().await;
        info!("Cache saved successfully");
    };

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn echo(body: String) -> String {
    body
}

async fn static_file_handler(
    State(app_state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    app_state.static_handler.serve(&path).await
}

async fn robots_txt_handler() -> impl IntoResponse {
    use axum::http::header;

    let robots_content = r#"User-agent: *
Allow: /

# Allow crawling of the gallery
Allow: /gallery

# Allow common files
Allow: /favicon.ico
Allow: /robots.txt
Allow: /static/

# Generated sitemap (if you add one later)
# Sitemap: https://your-domain.com/sitemap.xml
"#;

    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"), // Cache for 24 hours
        ],
        robots_content,
    )
}

