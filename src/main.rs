use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod gallery;
mod static_files;
mod templating;

use gallery::{Gallery, SharedGallery, gallery_handler, image_detail_handler, image_handler};
use static_files::StaticFileHandler;
use templating::{TemplateEngine, template_with_gallery_handler};

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

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    server: ServerConfig,
    app: AppConfig,
    templates: TemplateConfig,
    static_files: StaticConfig,
    gallery: GalleryConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
struct AppConfig {
    name: String,
    log_level: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TemplateConfig {
    directory: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct StaticConfig {
    directory: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GalleryConfig {
    path_prefix: String,
    source_directory: PathBuf,
    cache_directory: PathBuf,
    images_per_page: usize,
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

    let host = args.host.unwrap_or(config.server.host);
    let port = args.port.unwrap_or(config.server.port);
    let log_level = args.log_level;

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

    let template_engine = Arc::new(TemplateEngine::new(config.templates.directory));
    let static_handler = StaticFileHandler::new(config.static_files.directory);
    let gallery: SharedGallery = Arc::new(Gallery::new(config.gallery.clone()));

    let app = Router::new()
        .route("/health", get(health))
        .route("/echo", post(echo))
        .route("/static/{*path}", get(static_file_handler))
        .route("/gallery", get(gallery_root_handler))
        .route("/gallery/", get(gallery_root_handler))
        .route("/gallery/detail/{*path}", get(image_detail_handler))
        .route("/gallery/image/{*path}", get(image_handler))
        .route("/gallery/{*path}", get(gallery_handler))
        .route("/", get(template_with_gallery_handler))
        .route("/{*path}", get(template_with_gallery_handler))
        .with_state((template_engine, static_handler, gallery));

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn echo(body: String) -> String {
    body
}

async fn static_file_handler(
    axum::extract::State((_, static_handler, _)): axum::extract::State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    static_handler.serve(&path).await
}


async fn gallery_root_handler(
    axum::extract::State((template_engine, static_handler, gallery)): axum::extract::State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    axum::extract::Query(query): axum::extract::Query<gallery::GalleryQuery>,
) -> impl axum::response::IntoResponse {
    gallery_handler(
        axum::extract::State((template_engine, static_handler, gallery)),
        axum::extract::Path("".to_string()),
        axum::extract::Query(query),
    )
    .await
}
