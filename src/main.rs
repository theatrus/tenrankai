use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use tenrankai::{
    Config, create_app,
    gallery::Gallery,
    login::{User, UserDatabase},
    posts, startup_checks,
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
    log_level: String,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Set up logging first
    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Handle commands
    match cli.command {
        Some(Commands::User(user_cmd)) => handle_user_command(user_cmd).await,
        Some(Commands::Serve {
            port,
            host,
            quit_after,
        }) => run_server(cli.config, port, host, quit_after).await,
        None => {
            // Default to serve command if no subcommand specified
            run_server(cli.config, None, None, None).await
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

async fn run_server(
    config_path: PathBuf,
    port: Option<u16>,
    host: Option<String>,
    quit_after: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = if config_path.exists() {
        let config_content = std::fs::read_to_string(&config_path)?;
        toml_edit::de::from_str::<Config>(&config_content)?
    } else {
        info!("Config file not found at {:?}, using defaults", config_path);
        Config::default()
    };

    let host = host.unwrap_or(config.server.host.clone());
    let port = port.unwrap_or(config.server.port);

    info!("Starting {} server", config.app.name);
    info!("Configuration loaded from: {:?}", config_path);
    info!("Template directory: {:?}", config.templates.directory);
    info!(
        "Static files directory: {:?}",
        config.static_files.directory
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

    // Perform startup checks
    match startup_checks::perform_startup_checks(&config).await {
        Ok(()) => info!("All startup checks passed"),
        Err(errors) => {
            for error in &errors {
                tracing::error!("Startup check failed: {}", error);
            }
            // Decide whether to continue or exit based on severity
            // For now, we'll continue with warnings for non-critical errors
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

    let app = create_app(config.clone()).await;

    // Initialize galleries and set up background tasks
    let mut galleries_for_shutdown = Vec::new();

    if let Some(gallery_configs) = &config.galleries {
        for gallery_config in gallery_configs {
            let gallery =
                std::sync::Arc::new(Gallery::new(gallery_config.clone(), config.app.clone()));

            // Initialize gallery and check for version changes
            if let Err(e) = gallery.initialize_and_check_version().await {
                tracing::warn!(
                    "Failed to initialize gallery '{}' metadata cache: {}",
                    gallery_config.name,
                    e
                );
            }

            // Trigger refresh with pre-generation if configured
            if gallery.is_metadata_cache_empty().await {
                info!(
                    "Metadata cache for gallery '{}' is empty, triggering initial refresh",
                    gallery_config.name
                );
                let pregenerate = gallery_config.pregenerate_cache;
                if pregenerate {
                    info!(
                        "Cache pre-generation is enabled for gallery '{}'",
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

            // Store gallery for shutdown handler
            galleries_for_shutdown.push(gallery);
        }
    }

    // Initialize posts background refresh
    // We need to recreate posts managers here for background tasks
    // This is not ideal but avoids circular dependencies
    if let Some(posts_configs) = &config.posts {
        for posts_config in posts_configs {
            if let Some(interval_minutes) = posts_config.refresh_interval_minutes
                && interval_minutes > 0
            {
                info!(
                    "Starting background posts refresh for '{}' every {} minutes",
                    posts_config.name, interval_minutes
                );

                // Create a new posts manager for background refresh
                let posts_manager =
                    std::sync::Arc::new(posts::PostsManager::new(posts::PostsConfig {
                        source_directory: posts_config.source_directory.clone(),
                        url_prefix: posts_config.url_prefix.clone(),
                        index_template: posts_config.index_template.clone(),
                        post_template: posts_config.post_template.clone(),
                        posts_per_page: posts_config.posts_per_page,
                        refresh_interval_minutes: posts_config.refresh_interval_minutes,
                    }));

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

    // Save caches on shutdown
    info!("Shutting down - saving metadata caches...");
    for gallery in galleries_for_shutdown {
        if let Err(e) = gallery.save_caches().await {
            tracing::error!("Failed to save metadata cache on shutdown: {}", e);
        } else {
            info!("Metadata cache saved successfully");
        }
    }

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
