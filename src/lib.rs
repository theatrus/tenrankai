use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod api;
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
    pub log_level: String,
    pub cookie_secret: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub user_database: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateConfig {
    pub directory: PathBuf,
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
                cookie_secret: "change-me-in-production-use-a-long-random-string".to_string(),
                base_url: None,
                user_database: None,
            },
            templates: TemplateConfig {
                directory: PathBuf::from("templates"),
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
            }]),
            posts: None,
            email: None,
        }
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
    let mut template_engine = templating::TemplateEngine::new(config.templates.directory.clone());

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
