use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use dynserver::{Config, create_app, gallery::Gallery};

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

    let app = create_app(config.clone()).await;

    // Get the gallery from the app state to set up background tasks
    let gallery = std::sync::Arc::new(Gallery::new(config.gallery.clone(), config.app.clone()));

    // Initialize gallery and check for version changes
    if let Err(e) = gallery.initialize_and_check_version().await {
        tracing::warn!("Failed to initialize gallery metadata cache: {}", e);
    }
    
    // Trigger refresh with pre-generation if configured
    if gallery.is_metadata_cache_empty().await {
        info!("Metadata cache is empty, triggering initial refresh");
        let pregenerate = config.gallery.pregenerate_cache;
        if pregenerate {
            info!("Cache pre-generation is enabled");
        }
        if let Err(e) = gallery.clone().refresh_metadata_and_pregenerate_cache(pregenerate).await {
            tracing::error!("Failed to refresh metadata and pre-generate cache: {}", e);
        }
    }

    // Start background cache refresh if configured
    if let Some(interval_minutes) = config.gallery.cache_refresh_interval_minutes
        && interval_minutes > 0
    {
        info!(
            "Starting background metadata cache refresh every {} minutes",
            interval_minutes
        );
        Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
    }

    // Start periodic cache save (every 5 minutes)
    info!("Starting periodic metadata cache save every 5 minutes");
    Gallery::start_periodic_cache_save(gallery.clone(), 5);

    // Clone gallery for shutdown handler before moving it into router state
    let gallery_for_shutdown = gallery.clone();

    let addr = SocketAddr::from((host.parse::<std::net::IpAddr>()?, port));
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Set up graceful shutdown
    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(shutdown_signal());

    // Start the server
    if let Err(e) = graceful.await {
        tracing::error!("Server error: {}", e);
    }

    // Save cache on shutdown
    info!("Shutting down - saving metadata cache...");
    if let Err(e) = gallery_for_shutdown.save_caches().await {
        tracing::error!("Failed to save metadata cache on shutdown: {}", e);
    } else {
        info!("Metadata cache saved successfully");
    }

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received");
}
