use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use tenrankai::{
    Config, LogLevel, commands, create_app, create_app_with_site_manager,
    config::MultiSiteConfig,
    gallery::Gallery,
    login::{User, UserDatabase},
    openai, posts, site::{ConfigReloader, SiteBuilder, SiteManager}, startup_checks, storage,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Global options that apply to all commands
    #[arg(short, long, default_value = "config.toml", global = true)]
    config: PathBuf,

    #[arg(short, long, default_value = "info", global = true)]
    log_level: LogLevel,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the web server (default if no command specified)
    Serve {
        #[arg(short, long)]
        port: Option<u16>,

        #[arg(long)]
        host: Option<String>,

        /// Automatically quit after specified number of seconds (useful for testing)
        #[arg(long)]
        quit_after: Option<u64>,
    },

    /// Manage users
    #[command(subcommand)]
    User(UserCommands),

    /// Debug AVIF image metadata and color properties
    #[cfg(feature = "avif")]
    AvifDebug {
        /// Path to the AVIF file to analyze
        image_path: PathBuf,

        /// Show detailed technical information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Cache management commands
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Analyze images using OpenAI Vision API to generate keywords and alt-text
    AnalyzeImages {
        /// Gallery name to analyze
        #[arg(short, long)]
        gallery: String,

        /// Specific folder within the gallery (optional)
        #[arg(short, long)]
        folder: Option<String>,

        /// Maximum number of images to analyze
        #[arg(long)]
        limit: Option<usize>,

        /// Force re-analysis of images that already have AI data
        #[arg(long)]
        force: bool,

        /// Dry run - show what would be analyzed without making API calls
        #[arg(long)]
        dry_run: bool,
    },

    /// Clear AI analysis data from images
    ClearAnalysis {
        /// Gallery name
        #[arg(short, long)]
        gallery: String,

        /// Specific folder within the gallery (optional)
        #[arg(short, long)]
        folder: Option<String>,

        /// Dry run - show what would be cleared without making changes
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum UserCommands {
    /// List all users
    List {
        /// Path to users database file
        #[arg(short, long, default_value = "users.toml")]
        database: String,
    },
    /// Add a new user
    Add {
        /// Username (will be converted to lowercase)
        username: String,
        /// Email address
        email: String,
        /// Path to users database file
        #[arg(short, long, default_value = "users.toml")]
        database: String,
    },
    /// Remove a user
    Remove {
        /// Username to remove
        username: String,
        /// Path to users database file
        #[arg(short, long, default_value = "users.toml")]
        database: String,
    },
    /// Update a user's email
    Update {
        /// Username to update
        username: String,
        /// New email address
        email: String,
        /// Path to users database file
        #[arg(short, long, default_value = "users.toml")]
        database: String,
    },
}

#[derive(Subcommand, Debug)]
enum CacheCommands {
    /// Report format coverage for a gallery's image cache
    Report {
        /// Gallery name to analyze
        #[arg(short, long)]
        gallery: String,
    },
    /// Validate and clean up outdated cache entries
    Cleanup {
        /// Gallery name to clean up
        #[arg(short, long)]
        gallery: String,
    },
    /// Invalidate cached files (removes from cache to force regeneration)
    Invalidate {
        /// Gallery name
        #[arg(short, long)]
        gallery: String,

        /// Type of cache to invalidate: "composite" or "image"
        #[arg(short = 't', long, default_value = "composite")]
        cache_type: String,

        /// Path within the gallery (e.g., "2026-01-lake-natoma" for composite, or image filename)
        #[arg(short, long)]
        path: String,

        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// List cached composite images for a gallery
    ListComposites {
        /// Gallery name
        #[arg(short, long)]
        gallery: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Load config to get default log level, but allow CLI to override
    // Parse as MultiSiteConfig which supports both legacy and multi-site formats
    let (mut config, multi_site_config) = if cli.config.exists() {
        let config_content = std::fs::read_to_string(&cli.config)?;

        // Parse as MultiSiteConfig (supports both legacy and multi-site formats)
        let multi_site_config: MultiSiteConfig = toml_edit::de::from_str(&config_content)?;

        // Convert to legacy Config for global settings, routes, etc.
        let config: Config = multi_site_config.clone().into();

        (config, Some(multi_site_config))
    } else {
        (Config::default(), None)
    };

    // Override config log level with CLI arg (CLI takes precedence)
    config.app.log_level = cli.log_level;

    // Set up logging using the final log level
    let app_level = config.app.log_level;
    let aws_level = config.app.aws_log_level;

    // Build filter with separate levels for app and AWS SDK
    // Format: "default_level,crate1=level,crate2=level"
    let filter_str = format!(
        "{},aws_smithy_runtime={},aws_smithy_runtime_api={},aws_config={},aws_sdk_s3={},aws_sdk_ses={},aws_credential_types={},aws_sigv4={}",
        app_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
        aws_level.as_str(),
    );

    let filter = EnvFilter::builder()
        .with_default_directive(app_level.to_tracing_filter().into())
        .parse_lossy(&filter_str);

    // Allow RUST_LOG to override
    let filter = EnvFilter::try_from_default_env().unwrap_or(filter);

    let subscriber = fmt::Subscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Handle commands
    match cli.command {
        Some(Commands::User(user_cmd)) => handle_user_command(user_cmd).await,
        Some(Commands::Cache(cache_cmd)) => handle_cache_command(cache_cmd, config).await,
        #[cfg(feature = "avif")]
        Some(Commands::AvifDebug {
            image_path,
            verbose,
        }) => commands::avif_debug::handle_avif_debug_command(image_path, verbose).await,
        Some(Commands::AnalyzeImages {
            gallery,
            folder,
            limit,
            force,
            dry_run,
        }) => {
            commands::analyze::handle_analyze_command(
                config, gallery, folder, limit, force, dry_run,
            )
            .await
        }
        Some(Commands::ClearAnalysis {
            gallery,
            folder,
            dry_run,
        }) => {
            commands::clear_analysis::handle_clear_analysis_command(
                config, gallery, folder, dry_run,
            )
            .await
        }
        Some(Commands::Serve {
            port,
            host,
            quit_after,
        }) => run_server(config, multi_site_config, cli.config.clone(), port, host, quit_after).await,
        None => {
            // Default to serve command if no subcommand specified
            run_server(config, multi_site_config, cli.config.clone(), None, None, None).await
        }
    }
}

async fn handle_user_command(cmd: UserCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        UserCommands::List { database } => {
            let db_path = std::path::Path::new(&database);
            let db = if db_path.exists() {
                UserDatabase::load_from_file(db_path).await?
            } else {
                println!("No user database found at: {}", database);
                return Ok(());
            };

            if db.users.is_empty() {
                println!("No users in database");
            } else {
                println!("Users in database:");
                for (username, user) in &db.users {
                    println!("  {} <{}>", username, user.email);
                }
            }
        }
        UserCommands::Add {
            username,
            email,
            database,
        } => {
            let db_path = std::path::Path::new(&database);
            let mut db = if db_path.exists() {
                UserDatabase::load_from_file(db_path).await?
            } else {
                println!("Creating new user database at: {}", database);
                UserDatabase::new()
            };

            let username = username.trim().to_lowercase();
            if db.get_user(&username).is_some() {
                eprintln!("Error: User '{}' already exists", username);
                std::process::exit(1);
            }

            let user = User {
                email: email.trim().to_string(),
                passkeys: Vec::new(),
            };

            db.add_user(username.clone(), user);
            db.save_to_file(db_path).await?;
            println!("Added user '{}' with email '{}'", username, email);
        }
        UserCommands::Remove { username, database } => {
            let db_path = std::path::Path::new(&database);
            let mut db = if db_path.exists() {
                UserDatabase::load_from_file(db_path).await?
            } else {
                eprintln!("Error: No user database found at: {}", database);
                std::process::exit(1);
            };

            let username = username.trim().to_lowercase();
            if db.remove_user(&username).is_some() {
                db.save_to_file(db_path).await?;
                println!("Removed user '{}'", username);
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
        UserCommands::Update {
            username,
            email,
            database,
        } => {
            let db_path = std::path::Path::new(&database);
            let mut db = if db_path.exists() {
                UserDatabase::load_from_file(db_path).await?
            } else {
                eprintln!("Error: No user database found at: {}", database);
                std::process::exit(1);
            };

            let username = username.trim().to_lowercase();
            if let Some(user) = db.users.get_mut(&username) {
                user.email = email.trim().to_string();
                db.save_to_file(db_path).await?;
                println!("Updated email for user '{}' to '{}'", username, email);
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn handle_cache_command(
    cmd: CacheCommands,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Find the gallery configuration
    let gallery_configs = config.galleries.as_ref().ok_or("No galleries configured")?;

    match cmd {
        CacheCommands::Report {
            gallery: gallery_name,
        } => {
            let gallery_config = gallery_configs
                .iter()
                .find(|g| g.name == gallery_name)
                .ok_or_else(|| format!("Gallery '{}' not found in configuration", gallery_name))?;

            commands::cache::report(gallery_config).await?;
        }
        CacheCommands::Cleanup {
            gallery: gallery_name,
        } => {
            let gallery_config = gallery_configs
                .iter()
                .find(|g| g.name == gallery_name)
                .ok_or_else(|| format!("Gallery '{}' not found in configuration", gallery_name))?;

            commands::cache::cleanup(gallery_config).await?;
        }
        CacheCommands::Invalidate {
            gallery: gallery_name,
            cache_type,
            path,
            dry_run,
        } => {
            let gallery_config = gallery_configs
                .iter()
                .find(|g| g.name == gallery_name)
                .ok_or_else(|| format!("Gallery '{}' not found in configuration", gallery_name))?;

            match cache_type.as_str() {
                "composite" => {
                    commands::cache::invalidate_composite(gallery_config, &path, dry_run).await?;
                }
                "image" => {
                    commands::cache::invalidate_image(gallery_config, &path, dry_run).await?;
                }
                _ => {
                    eprintln!(
                        "Unknown cache type '{}'. Valid types: composite, image",
                        cache_type
                    );
                    std::process::exit(1);
                }
            }
        }
        CacheCommands::ListComposites {
            gallery: gallery_name,
        } => {
            let gallery_config = gallery_configs
                .iter()
                .find(|g| g.name == gallery_name)
                .ok_or_else(|| format!("Gallery '{}' not found in configuration", gallery_name))?;

            commands::cache::list_composites(gallery_config).await?;
        }
    }

    Ok(())
}

async fn run_server(
    config: Config,
    multi_site_config: Option<MultiSiteConfig>,
    config_path: PathBuf,
    port: Option<u16>,
    host: Option<String>,
    quit_after: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = host.unwrap_or(config.server.host.clone());
    let port = port.unwrap_or(config.server.port);

    info!("Starting {} server", config.app.name);
    info!("Log level: {}", config.app.log_level);

    // Check if we're in multi-site mode
    let is_multi_site = multi_site_config
        .as_ref()
        .is_some_and(|m| m.is_multi_site());

    if is_multi_site {
        info!("Running in multi-site mode");
    } else {
        info!("Template directories: {:?}", config.templates.directories);
        info!(
            "Static files directories: {:?}",
            config.static_files.directories
        );
        if let Some(galleries) = &config.galleries {
            for gallery in galleries {
                info!(
                    "Gallery '{}' source directory: {:?}",
                    gallery.name, gallery.source_directory
                );
                info!(
                    "Gallery '{}' cache directory: {:?}",
                    gallery.name, gallery.cache_directory
                );
            }
        }
    }

    // Perform startup checks (only for legacy mode, multi-site does checks per-site)
    if !is_multi_site {
        match startup_checks::perform_startup_checks(&config).await {
            Ok(()) => info!("All startup checks passed"),
            Err(errors) => {
                for error in &errors {
                    tracing::error!("Startup check failed: {}", error);
                }
                let critical_error = errors.iter().any(|e| {
                    matches!(
                        e,
                        startup_checks::StartupCheckError::GallerySourceDirectoryMissing(_)
                            | startup_checks::StartupCheckError::CacheDirectoryCreationFailed(_)
                    )
                });

                if critical_error {
                    tracing::error!("Critical startup check failed, exiting");
                    return Err("Critical startup check failed".into());
                } else {
                    tracing::warn!("Non-critical startup checks failed, continuing");
                }
            }
        }
    }

    // Track all galleries for shutdown across all sites
    let mut galleries_for_shutdown: Vec<Arc<Gallery>> = Vec::new();
    // Track all galleries for background analysis (combined from all sites)
    let mut all_galleries_map: HashMap<String, Arc<Gallery>> = HashMap::new();

    // Build the app differently based on mode
    let (app, site_manager) = if is_multi_site {
        // Multi-site mode: Build sites using SiteBuilder and create SiteManager
        let multi_config = multi_site_config.as_ref().unwrap();
        let site_manager = Arc::new(SiteManager::new());

        for (site_name, site_section) in multi_config.get_site_configs() {
            info!("Building site '{}'...", site_name);

            // Convert site section to SiteConfig
            let site_config = site_section.to_site_config(&site_name, &multi_config.app);

            // Build the site
            let site_builder = SiteBuilder::new(site_config);
            let site = match site_builder.build().await {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    tracing::error!("Failed to build site '{}': {}", site_name, e);
                    continue;
                }
            };

            // Initialize galleries for this site (additional setup not done by SiteBuilder)
            for (gallery_name, gallery) in site.galleries().iter() {
                let gallery_config = gallery.get_config();

                // Initialize gallery and check for version changes
                if let Err(e) = gallery.initialize_and_check_version().await {
                    tracing::warn!(
                        "Failed to initialize gallery '{}' metadata cache: {}",
                        gallery_name,
                        e
                    );
                }

                // Trigger refresh and/or pre-generation on startup
                let metadata_empty = gallery.is_metadata_cache_empty().await;
                let pregenerate = gallery_config.pregenerate.is_some();

                if metadata_empty {
                    info!(
                        "Metadata cache for gallery '{}' (site '{}') is empty, triggering initial refresh",
                        gallery_name, site_name
                    );
                }

                if metadata_empty || pregenerate {
                    if pregenerate {
                        info!(
                            "Cache pre-generation enabled for gallery '{}' (site '{}')",
                            gallery_name, site_name
                        );
                    }
                    if let Err(e) = gallery
                        .clone()
                        .refresh_metadata_and_pregenerate_cache(pregenerate)
                        .await
                    {
                        tracing::error!(
                            "Failed to refresh metadata for gallery '{}' (site '{}'): {}",
                            gallery_name,
                            site_name,
                            e
                        );
                    }
                }

                // Start background cache refresh if configured
                if let Some(interval_minutes) = gallery_config.cache_refresh_interval_minutes
                    && interval_minutes > 0
                {
                    info!(
                        "Starting background cache refresh for gallery '{}' (site '{}') every {} minutes",
                        gallery_name, site_name, interval_minutes
                    );
                    Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
                }

                // Start periodic cache save (every 5 minutes)
                info!(
                    "Starting periodic cache save for gallery '{}' (site '{}')",
                    gallery_name, site_name
                );
                Gallery::start_periodic_cache_save(gallery.clone(), 5);

                // Track gallery for shutdown
                galleries_for_shutdown.push(gallery.clone());

                // Add to combined galleries map (use site_name prefix for uniqueness)
                all_galleries_map.insert(
                    format!("{}:{}", site_name, gallery_name),
                    gallery.clone(),
                );
            }

            // Add site to manager with its hostnames
            site_manager
                .add_site(site, site_section.hostnames.clone())
                .await;
            info!("Site '{}' ready with hostnames: {:?}", site_name, site_section.hostnames);
        }

        // Create app with site manager
        let app = create_app_with_site_manager(config.clone(), site_manager.clone()).await;
        (app, Some(site_manager))
    } else {
        // Legacy single-site mode: Initialize galleries manually
        let mut galleries_map = HashMap::new();

        if let Some(gallery_configs) = &config.galleries {
            for gallery_config in gallery_configs {
                // Create source storage backend from source_directory URL
                let source_storage =
                    match storage::create_storage_from_url(&gallery_config.source_directory).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(
                                "Failed to create source storage for gallery '{}': {}",
                                gallery_config.name,
                                e
                            );
                            continue;
                        }
                    };

                // Create cache storage backend from cache_directory URL
                let cache_storage =
                    match storage::create_storage_from_url(&gallery_config.cache_directory).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(
                                "Failed to create cache storage for gallery '{}': {}",
                                gallery_config.name,
                                e
                            );
                            continue;
                        }
                    };

                let gallery = std::sync::Arc::new(Gallery::new(
                    gallery_config.clone(),
                    source_storage,
                    cache_storage,
                ));

                // Initialize gallery and check for version changes
                if let Err(e) = gallery.initialize_and_check_version().await {
                    tracing::warn!(
                        "Failed to initialize gallery '{}' metadata cache: {}",
                        gallery_config.name,
                        e
                    );
                }

                // Trigger refresh and/or pre-generation on startup
                let metadata_empty = gallery.is_metadata_cache_empty().await;
                let pregenerate = gallery_config.pregenerate.is_some();

                if metadata_empty {
                    info!(
                        "Metadata cache for gallery '{}' is empty, triggering initial refresh",
                        gallery_config.name
                    );
                }

                // Run refresh if metadata is empty, or pregenerate if configured
                if metadata_empty || pregenerate {
                    if pregenerate {
                        info!(
                            "Cache pre-generation enabled for gallery '{}', will generate missing cache entries",
                            gallery_config.name
                        );
                    }
                    if let Err(e) = gallery
                        .clone()
                        .refresh_metadata_and_pregenerate_cache(pregenerate)
                        .await
                    {
                        tracing::error!(
                            "Failed to refresh metadata and pre-generate cache for gallery '{}': {}",
                            gallery_config.name,
                            e
                        );
                    }
                }

                // Start background cache refresh if configured
                if let Some(interval_minutes) = gallery_config.cache_refresh_interval_minutes
                    && interval_minutes > 0
                {
                    info!(
                        "Starting background metadata cache refresh for gallery '{}' every {} minutes",
                        gallery_config.name, interval_minutes
                    );
                    Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
                }

                // Start periodic cache save (every 5 minutes)
                info!(
                    "Starting periodic metadata cache save for gallery '{}' every 5 minutes",
                    gallery_config.name
                );
                Gallery::start_periodic_cache_save(gallery.clone(), 5);

                // Store gallery in map and for shutdown handler
                galleries_map.insert(gallery_config.name.clone(), gallery.clone());
                galleries_for_shutdown.push(gallery.clone());
                all_galleries_map.insert(gallery_config.name.clone(), gallery);
            }
        }

        // Create Arc for galleries - used by both app and background analysis
        let galleries_arc = Arc::new(galleries_map);

        // Create the app with the initialized galleries
        let app = create_app(config.clone(), Some(galleries_arc)).await;
        (app, None)
    };

    // Convert all_galleries_map to Arc for background analysis
    let galleries_arc = Arc::new(all_galleries_map);

    // Initialize posts background refresh
    // We need to recreate posts managers here for background tasks
    // This is not ideal but avoids circular dependencies
    if let Some(posts_configs) = &config.posts {
        for posts_config in posts_configs {
            if let Some(interval_minutes) = posts_config.refresh_interval_minutes
                && interval_minutes > 0
            {
                // Create storage backend from source_directory URL
                let posts_storage =
                    match storage::create_storage_from_url(&posts_config.source_directory).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(
                                "Failed to create posts storage for '{}': {}",
                                posts_config.name,
                                e
                            );
                            continue;
                        }
                    };

                info!(
                    "Starting background posts refresh for '{}' every {} minutes (storage: {})",
                    posts_config.name,
                    interval_minutes,
                    posts_storage.storage_type()
                );

                // Create a new posts manager for background refresh
                let posts_manager = std::sync::Arc::new(posts::PostsManager::new(
                    posts::PostsConfig::from(posts_config),
                    posts_storage,
                ));

                // Initial refresh
                if let Err(e) = posts_manager.refresh_posts().await {
                    tracing::error!(
                        "Failed to initialize posts for '{}': {}",
                        posts_config.name,
                        e
                    );
                }

                posts::PostsManager::start_background_refresh(posts_manager, interval_minutes);
            }
        }
    }

    // Start background image analysis if configured
    let background_analysis_token = tokio_util::sync::CancellationToken::new();
    if let Some(openai_config) = &config.openai
        && openai_config.enable_background_analysis
    {
        // We need to create a client here for the background task
        match openai::OpenAIClient::new(openai_config.clone()) {
            Ok(client) => {
                openai::start_background_analysis(
                    Arc::new(client),
                    galleries_arc.clone(),
                    openai_config.background_interval_minutes,
                    openai_config.background_batch_size,
                    background_analysis_token.clone(),
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to create OpenAI client for background analysis: {}",
                    e
                );
            }
        }
    }

    // Set up SIGHUP handler for config reload (Unix only, multi-site mode only)
    #[cfg(unix)]
    let reload_token = tokio_util::sync::CancellationToken::new();
    #[cfg(unix)]
    if let Some(ref manager) = site_manager {
        let config_reloader = Arc::new(ConfigReloader::new(&config_path));
        let manager_clone = manager.clone();
        let reload_token_clone = reload_token.clone();

        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sighup = match signal(SignalKind::hangup()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to install SIGHUP handler: {}", e);
                    return;
                }
            };

            info!("SIGHUP handler installed - send SIGHUP to reload configuration");

            loop {
                tokio::select! {
                    _ = sighup.recv() => {
                        info!("Received SIGHUP - reloading configuration...");
                        let result = config_reloader.reload(&manager_clone).await;
                        if result.is_success() {
                            info!("Configuration reloaded successfully: {}", result.summary());
                        } else {
                            tracing::warn!("Configuration reload had failures: {}", result.summary());
                        }
                    }
                    _ = reload_token_clone.cancelled() => {
                        info!("SIGHUP handler shutting down");
                        break;
                    }
                }
            }
        });
    }

    let addr = SocketAddr::from((host.parse::<std::net::IpAddr>()?, port));
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Add ConnectInfo layer to track client IPs
    let app = app.into_make_service_with_connect_info::<SocketAddr>();

    // Set up graceful shutdown
    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(shutdown_signal(quit_after));

    // Start the server
    if let Err(e) = graceful.await {
        tracing::error!("Server error: {}", e);
    }

    // Shutdown galleries and save caches
    info!("Shutting down - stopping background tasks and saving metadata caches...");

    // Cancel background image analysis
    background_analysis_token.cancel();

    // Cancel SIGHUP handler
    #[cfg(unix)]
    reload_token.cancel();

    for gallery in galleries_for_shutdown {
        // Trigger shutdown of background tasks (cancels both shutdown_token and pregeneration_token)
        gallery.shutdown().await;

        // Save caches
        if let Err(e) = gallery.save_caches().await {
            tracing::error!("Failed to save metadata cache on shutdown: {}", e);
        } else {
            info!(
                "Metadata cache saved successfully for gallery '{}'",
                gallery.get_config().name
            );
        }
    }

    info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal(quit_after: Option<u64>) {
    use tokio::signal;
    use tokio::time::{Duration, sleep};

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

    let quit_timer = async {
        if let Some(seconds) = quit_after {
            info!(
                "Server will automatically shut down after {} seconds",
                seconds
            );
            sleep(Duration::from_secs(seconds)).await;
            info!("Quit timer expired, shutting down");
        } else {
            std::future::pending::<()>().await
        }
    };

    tokio::select! {
        _ = ctrl_c => {
            info!("Shutdown signal received (Ctrl+C)");
        },
        _ = terminate => {
            info!("Shutdown signal received (SIGTERM)");
        },
        _ = quit_timer => {},
    }
}
