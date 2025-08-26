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
    
    /// Debug image metadata and color properties
    Debug {
        /// Path to the image file to analyze
        image_path: PathBuf,
        
        /// Show detailed technical information
        #[arg(short, long)]
        verbose: bool,
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
        Some(Commands::Debug { image_path, verbose }) => handle_debug_command(image_path, verbose).await,
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

async fn handle_debug_command(image_path: PathBuf, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    use tenrankai::gallery::image_processing::formats;
    
    if !image_path.exists() {
        eprintln!("Error: Image file not found: {:?}", image_path);
        std::process::exit(1);
    }
    
    println!("=== Image Debug Information ===");
    println!("File: {:?}", image_path);
    
    // Get file size
    let metadata = std::fs::metadata(&image_path)?;
    println!("Size: {} bytes ({:.2} MB)", metadata.len(), metadata.len() as f64 / 1_048_576.0);
    
    // Detect file type by extension
    let extension = image_path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());
    
    println!("Extension: {}", extension.as_deref().unwrap_or("unknown"));
    
    // Try to get basic dimensions using image crate first
    match image::image_dimensions(&image_path) {
        Ok((width, height)) => {
            println!("Dimensions (image crate): {}x{}", width, height);
        },
        Err(e) => {
            println!("Dimensions (image crate): Failed - {}", e);
        }
    }
    
    println!();
    
    // Handle different formats
    match extension.as_deref() {
        Some("avif") => {
            println!("=== AVIF Analysis ===");
            
            // Try our custom AVIF reader
            match formats::avif::read_avif_info(&image_path) {
                Ok((image, info)) => {
                    println!("✓ Successfully decoded with custom AVIF reader");
                    println!();
                    
                    println!("Image Properties:");
                    println!("  Dimensions: {}x{}", image.width(), image.height());
                    println!("  Color type: {:?}", image.color());
                    println!("  In-memory format: {}", format_description(&image));
                    println!();
                    
                    println!("AVIF Metadata:");
                    println!("  Bit depth: {} bits", info.bit_depth);
                    println!("  Has alpha: {}", info.has_alpha);
                    println!("  Detected as HDR: {}", info.is_hdr);
                    
                    // Color space details
                    println!();
                    println!("Color Space Properties:");
                    let primaries_name = color_primaries_name(info.color_primaries);
                    let transfer_name = transfer_characteristics_name(info.transfer_characteristics);
                    let matrix_name = matrix_coefficients_name(info.matrix_coefficients);
                    
                    println!("  Color primaries: {} ({})", info.color_primaries, primaries_name);
                    println!("  Transfer characteristics: {} ({})", info.transfer_characteristics, transfer_name);
                    println!("  Matrix coefficients: {} ({})", info.matrix_coefficients, matrix_name);
                    
                    // HDR analysis
                    println!();
                    println!("HDR Analysis:");
                    let is_display_p3 = info.color_primaries == 12;
                    let is_bt2020 = info.color_primaries == 9;
                    let has_pq_transfer = info.transfer_characteristics == 16;
                    let has_hlg_transfer = info.transfer_characteristics == 18;
                    let has_hdr_transfer = has_pq_transfer || has_hlg_transfer;
                    let has_clli = info.max_cll > 0 || info.max_pall > 0;
                    
                    println!("  Wide gamut (BT.2020 or Display P3): {}", is_bt2020 || is_display_p3);
                    println!("  HDR transfer function (PQ/HLG): {}", has_hdr_transfer);
                    println!("  High bit depth (>8 bits): {}", info.bit_depth > 8);
                    println!("  Content Light Level Info (CLLI): {}", has_clli);
                    if has_clli {
                        println!("    Max Content Light Level: {} cd/m²", info.max_cll);
                        println!("    Max Picture Average Light Level: {} cd/m²", info.max_pall);
                    }
                    println!("  Current HDR detection result: {}", info.is_hdr);
                    
                    // ICC profile
                    if let Some(ref icc) = info.icc_profile {
                        println!("  ICC profile: {} bytes", icc.len());
                    } else {
                        println!("  ICC profile: None");
                    }
                    
                    if verbose {
                        println!();
                        println!("=== Technical Details ===");
                        println!("HDR Detection Logic:");
                        println!("  Current logic: bit_depth > 8 AND (");
                        println!("    (BT.2020 primaries AND (PQ OR HLG transfer)) OR");
                        println!("    (Display P3 primaries AND bit_depth >= 10) OR");
                        println!("    (bit_depth > 8 AND (PQ OR HLG transfer)) OR");
                        println!("    (CLLI data present)");
                        println!("  )");
                        println!();
                        
                        let traditional_hdr = is_bt2020 && has_hdr_transfer;
                        let wide_gamut_hdr = is_display_p3 && info.bit_depth >= 10;
                        let hdr_transfer_any = info.bit_depth > 8 && has_hdr_transfer;
                        let clli_hdr = has_clli;
                        
                        println!("  Traditional HDR (BT.2020 + PQ/HLG): {}", traditional_hdr);
                        println!("  Wide gamut HDR (Display P3 + ≥10-bit): {}", wide_gamut_hdr);
                        println!("  HDR transfer + high bit depth: {}", hdr_transfer_any);
                        println!("  CLLI metadata present: {}", clli_hdr);
                        println!("  Final result: {}", traditional_hdr || wide_gamut_hdr || hdr_transfer_any || clli_hdr);
                    }
                },
                Err(e) => {
                    println!("✗ Failed to decode with custom AVIF reader: {}", e);
                    
                    // Try fallback with image crate
                    match image::open(&image_path) {
                        Ok(img) => {
                            println!("✓ Decoded with image crate fallback");
                            println!("  Dimensions: {}x{}", img.width(), img.height());
                            println!("  Color type: {:?}", img.color());
                            println!("  Format: {}", format_description(&img));
                            println!("  Note: HDR and color space information not available with fallback");
                        },
                        Err(e2) => {
                            println!("✗ Also failed with image crate: {}", e2);
                        }
                    }
                }
            }
        },
        Some("jpg") | Some("jpeg") => {
            println!("=== JPEG Analysis ===");
            if let Some(icc) = formats::jpeg::extract_icc_profile(&image_path) {
                println!("ICC profile: {} bytes", icc.len());
            } else {
                println!("ICC profile: None");
            }
            
            match image::open(&image_path) {
                Ok(img) => {
                    println!("Decoded successfully");
                    println!("  Dimensions: {}x{}", img.width(), img.height());
                    println!("  Color type: {:?}", img.color());
                    println!("  Format: {}", format_description(&img));
                },
                Err(e) => println!("Decode error: {}", e),
            }
        },
        Some("png") => {
            println!("=== PNG Analysis ===");
            if let Some(icc) = formats::png::extract_icc_profile(&image_path) {
                println!("ICC profile: {} bytes", icc.len());
            } else {
                println!("ICC profile: None");
            }
            
            match image::open(&image_path) {
                Ok(img) => {
                    println!("Decoded successfully");
                    println!("  Dimensions: {}x{}", img.width(), img.height());
                    println!("  Color type: {:?}", img.color());
                    println!("  Format: {}", format_description(&img));
                },
                Err(e) => println!("Decode error: {}", e),
            }
        },
        Some("webp") => {
            println!("=== WebP Analysis ===");
            match image::open(&image_path) {
                Ok(img) => {
                    println!("Decoded successfully");
                    println!("  Dimensions: {}x{}", img.width(), img.height());
                    println!("  Color type: {:?}", img.color());
                    println!("  Format: {}", format_description(&img));
                },
                Err(e) => println!("Decode error: {}", e),
            }
        },
        _ => {
            println!("=== Generic Image Analysis ===");
            match image::open(&image_path) {
                Ok(img) => {
                    println!("Decoded successfully");
                    println!("  Dimensions: {}x{}", img.width(), img.height());
                    println!("  Color type: {:?}", img.color());
                    println!("  Format: {}", format_description(&img));
                },
                Err(e) => println!("Decode error: {}", e),
            }
        }
    }
    
    Ok(())
}

fn format_description(img: &image::DynamicImage) -> &'static str {
    match img {
        image::DynamicImage::ImageLuma8(_) => "8-bit Grayscale",
        image::DynamicImage::ImageLumaA8(_) => "8-bit Grayscale + Alpha",
        image::DynamicImage::ImageRgb8(_) => "8-bit RGB",
        image::DynamicImage::ImageRgba8(_) => "8-bit RGBA",
        image::DynamicImage::ImageLuma16(_) => "16-bit Grayscale",
        image::DynamicImage::ImageLumaA16(_) => "16-bit Grayscale + Alpha",
        image::DynamicImage::ImageRgb16(_) => "16-bit RGB (HDR)",
        image::DynamicImage::ImageRgba16(_) => "16-bit RGBA (HDR)",
        image::DynamicImage::ImageRgb32F(_) => "32-bit Float RGB",
        image::DynamicImage::ImageRgba32F(_) => "32-bit Float RGBA",
        _ => "Unknown format",
    }
}

fn color_primaries_name(value: u16) -> &'static str {
    match value {
        0 => "Reserved(0)",
        1 => "BT.709",
        2 => "Unspecified",
        3 => "Reserved(3)", 
        4 => "BT.470M",
        5 => "BT.470BG", 
        6 => "BT.601",
        7 => "SMPTE-240M",
        8 => "Generic film",
        9 => "BT.2020",
        10 => "SMPTE-428",
        11 => "SMPTE-431",
        12 => "SMPTE-432 (Display P3)",
        22 => "EBU Tech 3213-E",
        _ => "Unknown"
    }
}

fn transfer_characteristics_name(value: u16) -> &'static str {
    match value {
        0 => "Reserved(0)",
        1 => "BT.709",
        2 => "Unspecified", 
        3 => "Reserved(3)",
        4 => "Gamma 2.2 (BT.470M)",
        5 => "Gamma 2.8 (BT.470BG)",
        6 => "BT.601",
        7 => "SMPTE-240M",
        8 => "Linear",
        9 => "Log 100:1",
        10 => "Log 316:1 (Log sqrt(10))",
        11 => "IEC 61966-2-1 (xvYCC)",
        12 => "BT.1361",
        13 => "sRGB",
        14 => "BT.2020 (10-bit) **HDR**",
        15 => "BT.2020 (12-bit) **HDR**",
        16 => "SMPTE-2084 (PQ) **HDR**",
        17 => "SMPTE-428",
        18 => "HLG **HDR**",
        _ => "Unknown"
    }
}

fn matrix_coefficients_name(value: u16) -> &'static str {
    match value {
        0 => "Identity",
        1 => "BT.709",
        2 => "Unspecified",
        3 => "Reserved(3)",
        4 => "FCC",
        5 => "BT.470BG",
        6 => "BT.601",
        7 => "SMPTE-240M",
        8 => "YCgCo",
        9 => "BT.2020 NCL",
        10 => "BT.2020 CL",
        11 => "SMPTE-2085",
        12 => "Chroma NCL",
        13 => "Chroma CL",
        14 => "ICtCp",
        _ => "Unknown"
    }
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
            let gallery = std::sync::Arc::new(Gallery::new(gallery_config.clone()));

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
