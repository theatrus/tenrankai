use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use tenrankai::{
    Config, GallerySystemConfig, LogLevel, commands,
    config::{ConfigStorageLoader, MultiSiteConfig},
    create_app_with_site_manager,
    gallery::Gallery,
    openai, posts,
    site::{SiteBuilder, SiteManager},
    user_storage::{User, create_user_storage},
};
use tenrankai_config_storage::{ConfigStorageUrl, create_config_storage};

#[cfg(unix)]
use tenrankai::site::ConfigReloader;

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

        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,

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

        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,

        /// Specific folder within the gallery (optional)
        #[arg(short, long)]
        folder: Option<String>,

        /// Dry run - show what would be cleared without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Manage ConfigStorage (sites, galleries, posts)
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand, Debug)]
enum UserCommands {
    /// List all users
    List {
        /// User database URL (e.g., users.toml, postgresql://..., dynamodb://...)
        #[arg(short, long, default_value = "users.toml")]
        database: String,
        /// Site ID for multi-tenant databases
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Add a new user
    Add {
        /// Username (will be converted to lowercase)
        username: String,
        /// Email address
        email: String,
        /// User database URL (e.g., users.toml, postgresql://..., dynamodb://...)
        #[arg(short, long, default_value = "users.toml")]
        database: String,
        /// Site ID for multi-tenant databases
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Remove a user
    Remove {
        /// Username to remove
        username: String,
        /// User database URL (e.g., users.toml, postgresql://..., dynamodb://...)
        #[arg(short, long, default_value = "users.toml")]
        database: String,
        /// Site ID for multi-tenant databases
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Update a user's email
    Update {
        /// Username to update
        username: String,
        /// New email address
        email: String,
        /// User database URL (e.g., users.toml, postgresql://..., dynamodb://...)
        #[arg(short, long, default_value = "users.toml")]
        database: String,
        /// Site ID for multi-tenant databases
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Export all users to JSON (for migration)
    Export {
        /// Source database URL
        #[arg(short, long, default_value = "users.toml")]
        database: String,
        /// Site ID
        #[arg(short, long, default_value = "default")]
        site: String,
        /// Output file (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Import users from JSON (for migration)
    Import {
        /// Target database URL
        #[arg(short, long)]
        database: String,
        /// Site ID
        #[arg(short, long, default_value = "default")]
        site: String,
        /// Input file (defaults to stdin)
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Skip existing users instead of erroring
        #[arg(long)]
        skip_existing: bool,
    },
}

#[derive(Subcommand, Debug)]
enum CacheCommands {
    /// Report format coverage for a gallery's image cache
    Report {
        /// Gallery name to analyze
        #[arg(short, long)]
        gallery: String,
        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Validate and clean up outdated cache entries
    Cleanup {
        /// Gallery name to clean up
        #[arg(short, long)]
        gallery: String,
        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Invalidate cached files (removes from cache to force regeneration)
    Invalidate {
        /// Gallery name
        #[arg(short, long)]
        gallery: String,

        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,

        /// Type of cache to invalidate: "composite", "image", or "folder"
        #[arg(short = 't', long, default_value = "composite")]
        cache_type: String,

        /// Path within the gallery (folder path for composite/folder, image filename for image)
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
        /// Site name (for multi-site configurations)
        #[arg(short, long, default_value = "default")]
        site: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Initialize a new ConfigStorage directory structure
    Init {
        /// Path for the ConfigStorage directory
        #[arg(default_value = "config.d")]
        path: String,
    },
    /// List all sites in ConfigStorage
    ListSites,
    /// Add a new site
    AddSite {
        /// Site name (identifier)
        name: String,
        /// Hostnames for this site (can specify multiple)
        #[arg(short = 'H', long)]
        hostname: Vec<String>,
    },
    /// List galleries for a site
    ListGalleries {
        /// Site name
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Add a gallery to a site
    AddGallery {
        /// Gallery name
        name: String,
        /// Site name
        #[arg(short, long, default_value = "default")]
        site: String,
        /// Source directory (defaults to gallery name)
        #[arg(long)]
        source: Option<String>,
        /// URL prefix (defaults to /<name>)
        #[arg(long)]
        url_prefix: Option<String>,
    },
    /// List posts configurations for a site
    ListPosts {
        /// Site name
        #[arg(short, long, default_value = "default")]
        site: String,
    },
    /// Add a posts configuration to a site
    AddPosts {
        /// Posts name
        name: String,
        /// Site name
        #[arg(short, long, default_value = "default")]
        site: String,
        /// Source directory (defaults to posts/<name>)
        #[arg(long)]
        source: Option<String>,
        /// URL prefix (defaults to /<name>)
        #[arg(long)]
        url_prefix: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Load config to get default log level, but allow CLI to override
    let mut config = if cli.config.exists() {
        let config_content = std::fs::read_to_string(&cli.config)?;

        // Parse config - MultiSiteConfig handles legacy and modern formats
        let multi_site_config: MultiSiteConfig = toml_edit::de::from_str(&config_content)?;
        Config::from(multi_site_config)
    } else {
        Config::default()
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
            site,
            folder,
            limit,
            force,
            dry_run,
        }) => {
            commands::analyze::handle_analyze_command(
                config, site, gallery, folder, limit, force, dry_run,
            )
            .await
        }
        Some(Commands::ClearAnalysis {
            gallery,
            site,
            folder,
            dry_run,
        }) => {
            commands::clear_analysis::handle_clear_analysis_command(
                config, site, gallery, folder, dry_run,
            )
            .await
        }
        Some(Commands::Config(config_cmd)) => handle_config_command(config_cmd, config).await,
        Some(Commands::Serve {
            port,
            host,
            quit_after,
        }) => run_server(config, port, host, quit_after).await,
        None => {
            // Default to serve command if no subcommand specified
            run_server(config, None, None, None).await
        }
    }
}

async fn handle_user_command(cmd: UserCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        UserCommands::List { database, site } => {
            let storage = create_user_storage(&database, &site).await?;
            let users = storage.list_users().await?;

            if users.is_empty() {
                println!("No users in database (backend: {})", storage.backend_name());
            } else {
                println!(
                    "Users in database ({} users, backend: {}):",
                    users.len(),
                    storage.backend_name()
                );
                for (username, user) in &users {
                    let passkey_count = user.passkeys.len();
                    if passkey_count > 0 {
                        println!(
                            "  {} <{}> ({} passkey{})",
                            username,
                            user.email,
                            passkey_count,
                            if passkey_count == 1 { "" } else { "s" }
                        );
                    } else {
                        println!("  {} <{}>", username, user.email);
                    }
                }
            }
        }
        UserCommands::Add {
            username,
            email,
            database,
            site,
        } => {
            let storage = create_user_storage(&database, &site).await?;

            let username = username.trim().to_lowercase();
            let user = User {
                email: email.trim().to_string(),
                passkeys: Vec::new(),
            };

            match storage.add_user(&username, &user).await {
                Ok(()) => {
                    println!(
                        "Added user '{}' with email '{}' (backend: {})",
                        username,
                        email,
                        storage.backend_name()
                    );
                }
                Err(tenrankai::user_storage::UserStorageError::UserAlreadyExists(_)) => {
                    eprintln!("Error: User '{}' already exists", username);
                    std::process::exit(1);
                }
                Err(e) => return Err(e.into()),
            }
        }
        UserCommands::Remove {
            username,
            database,
            site,
        } => {
            let storage = create_user_storage(&database, &site).await?;

            let username = username.trim().to_lowercase();
            if storage.remove_user(&username).await? {
                println!(
                    "Removed user '{}' (backend: {})",
                    username,
                    storage.backend_name()
                );
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
        UserCommands::Update {
            username,
            email,
            database,
            site,
        } => {
            let storage = create_user_storage(&database, &site).await?;

            let username = username.trim().to_lowercase();

            // Get the existing user to preserve passkeys
            let existing = storage.get_user(&username).await?;
            if let Some(mut user) = existing {
                user.email = email.trim().to_string();
                storage.update_user(&username, &user).await?;
                println!(
                    "Updated email for user '{}' to '{}' (backend: {})",
                    username,
                    email,
                    storage.backend_name()
                );
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
        UserCommands::Export {
            database,
            site,
            output,
        } => {
            let storage = create_user_storage(&database, &site).await?;
            let users = storage.list_users().await?;

            let export_data = UserExportData {
                version: 1,
                site_id: site.clone(),
                exported_at: chrono::Utc::now(),
                users: users
                    .into_iter()
                    .map(|(username, user)| ExportedUser {
                        username,
                        email: user.email,
                        passkeys: user
                            .passkeys
                            .into_iter()
                            .map(|p| ExportedPasskey {
                                id: p.id,
                                name: p.name,
                                credential_json: serde_json::to_string(&p.credential)
                                    .unwrap_or_default(),
                                created_at: p.created_at,
                                last_used_at: p.last_used_at,
                            })
                            .collect(),
                    })
                    .collect(),
            };

            let json = serde_json::to_string_pretty(&export_data)?;

            if let Some(output_path) = output {
                std::fs::write(&output_path, &json)?;
                eprintln!(
                    "Exported {} users to {} (backend: {})",
                    export_data.users.len(),
                    output_path.display(),
                    storage.backend_name()
                );
            } else {
                println!("{}", json);
                eprintln!(
                    "Exported {} users (backend: {})",
                    export_data.users.len(),
                    storage.backend_name()
                );
            }
        }
        UserCommands::Import {
            database,
            site,
            input,
            skip_existing,
        } => {
            let storage = create_user_storage(&database, &site).await?;

            let json = if let Some(input_path) = input {
                std::fs::read_to_string(&input_path)?
            } else {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                buffer
            };

            let export_data: UserExportData = serde_json::from_str(&json)?;

            let mut imported = 0;
            let mut skipped = 0;

            for exported_user in export_data.users {
                let passkeys: Vec<tenrankai::user_storage::UserPasskey> = exported_user
                    .passkeys
                    .into_iter()
                    .filter_map(|p| {
                        let credential: webauthn_rs::prelude::Passkey =
                            serde_json::from_str(&p.credential_json).ok()?;
                        Some(tenrankai::user_storage::UserPasskey {
                            id: p.id,
                            name: p.name,
                            credential,
                            created_at: p.created_at,
                            last_used_at: p.last_used_at,
                        })
                    })
                    .collect();

                let user = User {
                    email: exported_user.email,
                    passkeys,
                };

                match storage.add_user(&exported_user.username, &user).await {
                    Ok(()) => {
                        imported += 1;
                    }
                    Err(tenrankai::user_storage::UserStorageError::UserAlreadyExists(_)) => {
                        if skip_existing {
                            skipped += 1;
                        } else {
                            eprintln!(
                                "Error: User '{}' already exists (use --skip-existing to skip)",
                                exported_user.username
                            );
                            std::process::exit(1);
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            eprintln!(
                "Imported {} users, skipped {} (backend: {})",
                imported,
                skipped,
                storage.backend_name()
            );
        }
    }

    Ok(())
}

/// Export data format for user migration.
#[derive(serde::Serialize, serde::Deserialize)]
struct UserExportData {
    version: u32,
    site_id: String,
    exported_at: chrono::DateTime<chrono::Utc>,
    users: Vec<ExportedUser>,
}

/// Exported user data.
#[derive(serde::Serialize, serde::Deserialize)]
struct ExportedUser {
    username: String,
    email: String,
    passkeys: Vec<ExportedPasskey>,
}

/// Exported passkey data.
#[derive(serde::Serialize, serde::Deserialize)]
struct ExportedPasskey {
    id: uuid::Uuid,
    name: String,
    credential_json: String,
    created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_used_at: Option<i64>,
}

async fn handle_cache_command(
    cmd: CacheCommands,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load gallery config from ConfigStorage
    let find_gallery = |site_name: &str,
                        gallery_name: &str|
     -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<GallerySystemConfig, String>> + Send>,
    > {
        let config_storage_url = config.app.config_storage.clone();
        let site = site_name.to_string();
        let gallery = gallery_name.to_string();

        Box::pin(async move {
            let url = config_storage_url.ok_or_else(|| {
                "config_storage is required. Add 'config_storage = \"config.d\"' to [app] section of config.toml.".to_string()
            })?;

            let storage_url = ConfigStorageUrl::parse(&url)
                .map_err(|e| format!("Invalid config_storage URL: {}", e))?;

            let storage = create_config_storage(&storage_url)
                .await
                .map_err(|e| format!("Failed to create config storage: {}", e))?;

            let loader = ConfigStorageLoader::new(storage, config.app.cookie_secret.clone());

            // Load site config to get storage_prefix
            let site_config = loader
                .load_site(&site)
                .await
                .map_err(|e| format!("Failed to load site '{}': {}", site, e))?
                .ok_or_else(|| format!("Site '{}' not found", site))?;

            // Find the gallery in the site config
            site_config
                .galleries
                .as_ref()
                .and_then(|galleries| galleries.iter().find(|g| g.name == gallery).cloned())
                .ok_or_else(|| format!("Gallery '{}' not found in site '{}'", gallery, site))
        })
    };

    match cmd {
        CacheCommands::Report {
            gallery: gallery_name,
            site,
        } => {
            let gallery_config = find_gallery(&site, &gallery_name).await?;
            commands::cache::report(&gallery_config).await?;
        }
        CacheCommands::Cleanup {
            gallery: gallery_name,
            site,
        } => {
            let gallery_config = find_gallery(&site, &gallery_name).await?;
            commands::cache::cleanup(&gallery_config).await?;
        }
        CacheCommands::Invalidate {
            gallery: gallery_name,
            site,
            cache_type,
            path,
            dry_run,
        } => {
            let gallery_config = find_gallery(&site, &gallery_name).await?;

            match cache_type.as_str() {
                "composite" => {
                    commands::cache::invalidate_composite(&gallery_config, &path, dry_run).await?;
                }
                "image" => {
                    commands::cache::invalidate_image(&gallery_config, &path, dry_run).await?;
                }
                "folder" => {
                    commands::cache::invalidate_folder(&gallery_config, &path, dry_run).await?;
                }
                _ => {
                    eprintln!(
                        "Unknown cache type '{}'. Valid types: composite, image, folder",
                        cache_type
                    );
                    std::process::exit(1);
                }
            }
        }
        CacheCommands::ListComposites {
            gallery: gallery_name,
            site,
        } => {
            let gallery_config = find_gallery(&site, &gallery_name).await?;
            commands::cache::list_composites(&gallery_config).await?;
        }
    }

    Ok(())
}

async fn handle_config_command(
    cmd: ConfigCommands,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get config_storage URL from config, or use default for init command
    let get_config_storage_url = || -> Result<String, Box<dyn std::error::Error>> {
        config.app.config_storage.clone().ok_or_else(|| {
            "config_storage is required. Add 'config_storage = \"config.d\"' to [app] section of config.toml.".into()
        })
    };

    match cmd {
        ConfigCommands::Init { path } => commands::config::handle_init_command(&path).await,
        ConfigCommands::ListSites => {
            let url = get_config_storage_url()?;
            commands::config::handle_list_sites_command(&url).await
        }
        ConfigCommands::AddSite { name, hostname } => {
            let url = get_config_storage_url()?;
            commands::config::handle_add_site_command(&url, &name, hostname).await
        }
        ConfigCommands::ListGalleries { site } => {
            let url = get_config_storage_url()?;
            commands::config::handle_list_galleries_command(&url, &site).await
        }
        ConfigCommands::AddGallery {
            name,
            site,
            source,
            url_prefix,
        } => {
            let url = get_config_storage_url()?;
            commands::config::handle_add_gallery_command(&url, &site, &name, source, url_prefix)
                .await
        }
        ConfigCommands::ListPosts { site } => {
            let url = get_config_storage_url()?;
            commands::config::handle_list_posts_command(&url, &site).await
        }
        ConfigCommands::AddPosts {
            name,
            site,
            source,
            url_prefix,
        } => {
            let url = get_config_storage_url()?;
            commands::config::handle_add_posts_command(&url, &site, &name, source, url_prefix).await
        }
    }
}

async fn run_server(
    config: Config,
    port: Option<u16>,
    host: Option<String>,
    quit_after: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = host.unwrap_or(config.server.host.clone());
    let port = port.unwrap_or(config.server.port);

    info!("Starting {} server", config.app.name);
    info!("Log level: {}", config.app.log_level);

    // ConfigStorage is required
    let config_storage_url = config.app.config_storage.as_ref().ok_or(
        "config_storage is required. Add 'config_storage = \"config.d\"' to [app] section of config.toml.\n\
         Use 'tenrankai config init' to create initial site configuration."
    )?;

    info!(
        "Loading site configuration from ConfigStorage: {}",
        config_storage_url
    );

    // Track all galleries for shutdown across all sites
    let mut galleries_for_shutdown: Vec<Arc<Gallery>> = Vec::new();
    // Track all galleries for background analysis (combined from all sites)
    let mut all_galleries_map: HashMap<String, Arc<Gallery>> = HashMap::new();

    // ConfigStorage for SIGHUP reload (captured after storage creation)
    #[cfg(unix)]
    let config_storage_for_reload: (tenrankai_config_storage::DynConfigStorage, String);

    // Parse and create ConfigStorage
    let url = match ConfigStorageUrl::parse(config_storage_url) {
        Ok(u) => u,
        Err(e) => {
            return Err(format!("Invalid config_storage URL: {}", e).into());
        }
    };

    let storage = match create_config_storage(&url).await {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("Failed to create config storage: {}", e).into());
        }
    };

    info!(
        "ConfigStorage initialized (backend: {})",
        storage.backend_name()
    );

    // Capture for SIGHUP reload
    #[cfg(unix)]
    {
        config_storage_for_reload = (storage.clone(), config_storage_url.clone());
    }

    let loader = ConfigStorageLoader::new(storage.clone(), config.app.cookie_secret.clone());
    let loaded_sites = match loader.load_all_sites().await {
        Ok(sites) => sites,
        Err(e) => {
            return Err(format!("Failed to load sites from ConfigStorage: {}", e).into());
        }
    };

    info!("Loaded {} site(s) from ConfigStorage", loaded_sites.len());

    let site_manager = Arc::new(SiteManager::new());

    for (site_name, mut site_config) in loaded_sites {
        info!("Building site '{}'...", site_name);

        // Set the config_storage URL so the site can use it for permission writes
        site_config.config_storage = Some(config_storage_url.clone());

        // Get hostnames from storage
        let hostnames = match storage.get_site_config(&site_name).await {
            Ok(Some(stored)) => stored.hostnames,
            _ => vec!["*".to_string()], // Default to catch-all
        };

        // Build the site
        let site_builder = SiteBuilder::new(site_config);
        let site = match site_builder.build().await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                tracing::error!("Failed to build site '{}': {}", site_name, e);
                continue;
            }
        };

        // Initialize galleries for this site
        for (gallery_name, gallery) in site.galleries().iter() {
            let gallery_config = gallery.get_config();

            if let Err(e) = gallery.initialize_and_check_version().await {
                tracing::warn!(
                    "Failed to initialize gallery '{}' metadata cache: {}",
                    gallery_name,
                    e
                );
            }

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

            if let Some(interval_minutes) = gallery_config.cache_refresh_interval_minutes
                && interval_minutes > 0
            {
                info!(
                    "Starting background cache refresh for gallery '{}' (site '{}') every {} minutes",
                    gallery_name, site_name, interval_minutes
                );
                Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
            }

            info!(
                "Starting periodic cache save for gallery '{}' (site '{}')",
                gallery_name, site_name
            );
            Gallery::start_periodic_cache_save(gallery.clone(), 5);

            galleries_for_shutdown.push(gallery.clone());
            all_galleries_map.insert(format!("{}:{}", site_name, gallery_name), gallery.clone());
        }

        // Initialize posts managers for this site
        for (posts_name, posts_manager) in site.posts_managers().iter() {
            let posts_config = posts_manager.get_config();

            // Initial refresh
            if let Err(e) = posts_manager.refresh_posts().await {
                tracing::error!(
                    "Failed to refresh posts '{}' (site '{}'): {}",
                    posts_name,
                    site_name,
                    e
                );
            }

            // Start background refresh if configured
            if let Some(interval_minutes) = posts_config.refresh_interval_minutes
                && interval_minutes > 0
            {
                info!(
                    "Starting background posts refresh for '{}' (site '{}') every {} minutes",
                    posts_name, site_name, interval_minutes
                );
                posts::PostsManager::start_background_refresh(
                    posts_manager.clone(),
                    interval_minutes,
                );
            }
        }

        site_manager.add_site(site, hostnames.clone()).await;
        info!("Site '{}' ready with hostnames: {:?}", site_name, hostnames);
    }

    let app = create_app_with_site_manager(config.clone(), site_manager.clone()).await;

    // Convert all_galleries_map to Arc for background analysis
    let galleries_arc = Arc::new(all_galleries_map);

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

    // Set up SIGHUP handler for config reload (Unix only)
    #[cfg(unix)]
    let reload_token = tokio_util::sync::CancellationToken::new();
    #[cfg(unix)]
    {
        let (storage, storage_url) = config_storage_for_reload;
        let config_reloader = Arc::new(ConfigReloader::new(
            storage,
            config.app.cookie_secret.clone(),
            storage_url,
        ));
        let manager_clone = site_manager.clone();
        let reload_token_clone = reload_token.clone();

        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};

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
