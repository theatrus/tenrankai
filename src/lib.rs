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
pub mod posts;
pub mod robots;
pub mod startup_checks;
pub mod static_files;
pub mod template_system;
pub mod templating;
pub mod webp_encoder;

// Re-export core types
pub use api_response::ApiResponse;
pub use cache::{CacheType, FormatCoverage};
pub use config::{
    AppConfig, Config, GallerySystemConfig, ImageIndexingMode, ImageSizeConfig, PostsSystemConfig,
    PreviewConfig, ServerConfig, StaticConfig, TemplateConfig,
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
    let mut template_engine = templating::TemplateEngine::new(config.templates.directories.clone());

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

    // Use provided galleries or create new ones
    let galleries_arc = if let Some(provided_galleries) = galleries {
        provided_galleries
    } else {
        // Create galleries if not provided
        let mut galleries = HashMap::new();
        if let Some(gallery_configs) = &config.galleries {
            for gallery_config in gallery_configs {
                let gallery = Arc::new(gallery::Gallery::new(gallery_config.clone()));
                galleries.insert(gallery_config.name.clone(), gallery);
            }
        }
        Arc::new(galleries)
    };

    // Initialize posts managers
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

            // Image serving
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
                        api::gallery_api_handler_for_named(
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
                        api::image_detail_api_handler_for_named(
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
                &format!("/api/gallery/{}/comment/{{comment_id}}/edit/{{*image_path}}", name),
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
                &format!("/api/gallery/{}/comment/{{comment_id}}/delete/{{*image_path}}", name),
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
