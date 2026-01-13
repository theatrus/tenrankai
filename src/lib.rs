pub mod api;
pub mod api_response;
pub mod cache;
pub mod commands;
pub mod composite;
pub mod config;
pub mod copyright;
pub mod email;
pub mod favicon;
pub mod gallery;
pub mod logging;
pub mod login;
pub mod metadata_storage;
pub mod openai;
pub mod permissions;
pub mod posts;
pub mod robots;
pub mod startup_checks;
pub mod static_files;
pub mod storage;
pub mod template_system;
pub mod templating;
pub mod webp_encoder;

// Re-export core types
pub use api_response::ApiResponse;
pub use cache::{CacheType, FormatCoverage};
pub use config::{
    AppConfig, Config, GallerySystemConfig, ImageIndexingMode, ImageSizeConfig, PostsSystemConfig,
    PregenerateConfig, PregenerateFormats, PregenerateSizes, PreviewConfig, ServerConfig,
    StaticConfig, TemplateConfig, TileConfig,
};
pub use logging::LogLevel;
pub use template_system::{TemplateCategory, TemplatePath, TemplateType};

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
    pub openai_client: Option<Arc<openai::OpenAIClient>>,
    pub config: Config,
}

// FromRef is automatically implemented for AppState since it implements Clone

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

pub async fn create_app(
    config: Config,
    galleries: Option<Arc<HashMap<String, gallery::SharedGallery>>>,
) -> axum::Router {
    // Create template storage backends from URLs (supports both filesystem and S3)
    let template_storages =
        match storage::create_storages_from_urls(&config.templates.directories).await {
            Ok(storages) => storages,
            Err(e) => {
                tracing::error!("Failed to initialize template storage: {}", e);
                vec![]
            }
        };

    let mut template_engine = templating::TemplateEngine::new(template_storages);

    // Create static file handler from storage URLs (supports both filesystem and S3)
    let static_handler =
        match static_files::StaticFileHandler::from_urls(config.static_files.directories.clone())
            .await
        {
            Ok(handler) => handler.with_redirects(config.static_files.use_redirects),
            Err(e) => {
                tracing::error!("Failed to initialize static file storage: {}", e);
                // Fall back to empty handler
                static_files::StaticFileHandler::from_paths(vec![])
            }
        };

    // Ensure file versions are loaded before proceeding
    static_handler.refresh_file_versions().await;

    // Set the static handler on the template engine for cache busting
    template_engine.set_static_handler(static_handler.clone());

    // Set whether user auth is enabled
    template_engine.set_has_user_auth(config.app.user_database.is_some());

    // Update file versions for the template engine
    template_engine.update_file_versions().await;

    let template_engine = Arc::new(template_engine);

    // Create favicon renderer using the same storage backends
    let favicon_renderer = favicon::FaviconRenderer::new(static_handler.storages().to_vec());

    // Use provided galleries or create new ones
    let galleries_arc = if let Some(provided_galleries) = galleries {
        provided_galleries
    } else {
        // Create galleries if not provided
        let mut galleries = HashMap::new();
        if let Some(gallery_configs) = &config.galleries {
            for gallery_config in gallery_configs {
                // Create source storage backend from source_directory URL
                let source_storage = match storage::create_storage_from_url(
                    &gallery_config.source_directory,
                )
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "Failed to create source storage for gallery '{}': {}",
                            gallery_config.name, e
                        );
                        continue;
                    }
                };

                // Create cache storage backend from cache_directory URL
                let cache_storage =
                    match storage::create_storage_from_url(&gallery_config.cache_directory).await {
                        Ok(s) => s,
                        Err(e) => {
                            error!(
                                "Failed to create cache storage for gallery '{}': {}",
                                gallery_config.name, e
                            );
                            continue;
                        }
                    };

                info!(
                    "Initializing gallery '{}' with source: {}, cache: {}",
                    gallery_config.name,
                    source_storage.storage_type(),
                    cache_storage.storage_type()
                );

                let gallery = Arc::new(gallery::Gallery::new(
                    gallery_config.clone(),
                    source_storage,
                    cache_storage,
                ));
                galleries.insert(gallery_config.name.clone(), gallery);
            }
        }
        Arc::new(galleries)
    };

    // Initialize posts managers
    let mut posts_managers = HashMap::new();
    if let Some(posts_configs) = &config.posts {
        for posts_config in posts_configs {
            // Create storage backend from source_directory URL
            let posts_storage =
                match storage::create_storage_from_url(&posts_config.source_directory).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "Failed to create posts storage for '{}': {}",
                            posts_config.name, e
                        );
                        continue;
                    }
                };

            info!(
                "Initializing posts for '{}' from {} (storage: {})",
                posts_config.name,
                posts_config.source_directory,
                posts_storage.storage_type()
            );

            let mut posts_manager =
                posts::PostsManager::new(posts::PostsConfig::from(posts_config), posts_storage);

            // Set galleries reference
            posts_manager.set_galleries(galleries_arc.clone());

            let posts_manager = Arc::new(posts_manager);

            // Load posts on startup
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

    // Initialize OpenAI client if configured
    let openai_client = if let Some(openai_config) = config.openai.clone() {
        match openai::OpenAIClient::new(openai_config) {
            Ok(client) => {
                info!("OpenAI client initialized");
                Some(Arc::new(client))
            }
            Err(e) => {
                error!("Failed to initialize OpenAI client: {}", e);
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
        openai_client,
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
                    move |state, query, auth| {
                        gallery::gallery_root_handler_for_named(state, Path(name), query, auth)
                    }
                }),
            );

            // Gallery folder browsing
            router = router.route(
                &format!("{}/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, query, auth| {
                        let gallery_path = path.0;
                        gallery::gallery_handler_for_named(
                            state,
                            Path((name, gallery_path)),
                            query,
                            auth,
                        )
                    }
                }),
            );

            // Image serving (legacy query parameter format)
            router = router.route(
                &format!("{}/image/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, query, headers, auth| {
                        let image_path = path.0;
                        gallery::image_handler_for_named(
                            state,
                            Path((name, image_path)),
                            query,
                            headers,
                            auth,
                        )
                    }
                }),
            );

            // Image serving (new path-based format)
            router = router.route(
                &format!("{}/_image/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, headers, auth| {
                        let full_path = path.0;
                        gallery::image_handler_for_named_v2(
                            state,
                            Path((name, full_path)),
                            headers,
                            auth,
                        )
                    }
                }),
            );

            // Image detail view
            router = router.route(
                &format!("{}/detail/{{*path}}", prefix),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, auth| {
                        let detail_path = path.0;
                        gallery::image_detail_handler_for_named(
                            state,
                            Path((name, detail_path)),
                            auth,
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

            // API route for gallery data (JSON response)
            router = router.route(
                &format!("/api/gallery/{}/data/{{*path}}", name),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, query, auth| {
                        let gallery_path = path.0;
                        api::gallery_api_handler_for_named_http(
                            state,
                            Path((name, gallery_path)),
                            query,
                            auth,
                        )
                    }
                }),
            );

            // API route for image detail data (JSON response)
            router = router.route(
                &format!("/api/gallery/{}/image/{{*path}}", name),
                axum::routing::get({
                    let name = name.clone();
                    move |state, path: Path<String>, auth| {
                        let image_path = path.0;
                        api::image_detail_api_handler_for_named_http(
                            state,
                            Path((name, image_path)),
                            auth,
                        )
                    }
                }),
            );

            // API route for image metadata (get/update)
            router = router
                .route(
                    &format!("/api/gallery/{}/metadata/{{*path}}", name),
                    axum::routing::get({
                        let name = name.clone();
                        move |state, path: Path<String>, auth| {
                            let image_path = path.0;
                            api::get_metadata_handler(state, Path((name, image_path)), auth)
                        }
                    }),
                )
                .route(
                    &format!("/api/gallery/{}/metadata/{{*path}}", name),
                    axum::routing::put({
                        let name = name.clone();
                        move |state, path: Path<String>, auth, request| {
                            let image_path = path.0;
                            api::update_metadata_handler(
                                state,
                                Path((name, image_path)),
                                auth,
                                request,
                            )
                        }
                    }),
                );

            // API route for adding comments
            router = router.route(
                &format!("/api/gallery/{}/comments/{{*path}}", name),
                axum::routing::post({
                    let name = name.clone();
                    move |state, path: Path<String>, auth, request| {
                        let image_path = path.0;
                        api::add_comment_handler(state, Path((name, image_path)), auth, request)
                    }
                }),
            );

            // API route for editing comments - using separate path structure
            router = router.route(
                &format!(
                    "/api/gallery/{}/comment/{{comment_id}}/edit/{{*image_path}}",
                    name
                ),
                axum::routing::put({
                    let name = name.clone();
                    move |state, path: Path<(String, String)>, auth, request| {
                        let (comment_id, image_path) = path.0;
                        api::edit_comment_handler(
                            state,
                            Path((name, image_path, comment_id)),
                            auth,
                            request,
                        )
                    }
                }),
            );

            // API route for deleting comments - using separate path structure
            router = router.route(
                &format!(
                    "/api/gallery/{}/comment/{{comment_id}}/delete/{{*image_path}}",
                    name
                ),
                axum::routing::delete({
                    let name = name.clone();
                    move |state, path: Path<(String, String)>, auth| {
                        let (comment_id, image_path) = path.0;
                        api::delete_comment_handler(
                            state,
                            Path((name, image_path, comment_id)),
                            auth,
                        )
                    }
                }),
            );

            // API route for AI image analysis
            router = router.route(
                &format!("/api/gallery/{}/analyze/{{*path}}", name),
                axum::routing::post({
                    let name = name.clone();
                    move |state, path: Path<String>, auth| {
                        let image_path = path.0;
                        api::analyze_image_handler(state, Path((name, image_path)), auth)
                    }
                }),
            );

            // API route for AI folder analysis (batch)
            router = router.route(
                &format!("/api/gallery/{}/analyze-folder/{{*path}}", name),
                axum::routing::post({
                    let name = name.clone();
                    move |state, path: Path<String>, auth, request| {
                        let folder_path = path.0;
                        api::analyze_folder_handler(state, Path((name, folder_path)), auth, request)
                    }
                }),
            );

            // API route for AI analysis of root folder
            router = router.route(
                &format!("/api/gallery/{}/analyze-folder", name),
                axum::routing::post({
                    let name = name.clone();
                    move |state, auth, request| {
                        api::analyze_folder_handler(
                            state,
                            Path((name, String::new())),
                            auth,
                            request,
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
