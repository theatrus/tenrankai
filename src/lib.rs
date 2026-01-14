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
pub mod site;
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
    // Site-specific resources (will eventually support multiple sites)
    pub site: Arc<site::Site>,
    // Global resources (shared across all sites)
    pub email_provider: Option<email::DynEmailProvider>,
    pub webauthn: Option<Arc<webauthn_rs::Webauthn>>,
    pub openai_client: Option<Arc<openai::OpenAIClient>>,
    pub config: Config,
}

// Accessor methods for site-specific resources
// These delegate to the Site for backward compatibility
impl AppState {
    pub fn template_engine(&self) -> &Arc<templating::TemplateEngine> {
        self.site.template_engine()
    }

    pub fn static_handler(&self) -> &static_files::StaticFileHandler {
        self.site.static_handler()
    }

    pub fn galleries(&self) -> &Arc<HashMap<String, gallery::SharedGallery>> {
        self.site.galleries()
    }

    pub fn favicon_renderer(&self) -> &favicon::FaviconRenderer {
        self.site.favicon_renderer()
    }

    pub fn posts_managers(&self) -> &Arc<HashMap<String, Arc<posts::PostsManager>>> {
        self.site.posts_managers()
    }

    pub fn login_state(&self) -> &Arc<tokio::sync::RwLock<login::LoginState>> {
        self.site.login_state()
    }

    pub fn user_database_manager(&self) -> &Option<login::types::UserDatabaseManager> {
        self.site.user_database_manager()
    }
}

// FromRef is automatically implemented for AppState since it implements Clone

async fn static_file_handler(
    State(app_state): State<AppState>,
    Path(path): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // Check if request has version parameter
    let has_version = params.contains_key("v");
    app_state.static_handler().serve(&path, has_version).await
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
    // Build the site using SiteBuilder
    let site_config = site::SiteConfig::from_legacy_config(&config);
    let mut site_builder = site::SiteBuilder::new(site_config);

    // Inject provided galleries if any (for testing)
    if let Some(provided_galleries) = galleries {
        site_builder = site_builder.with_galleries(provided_galleries);
    }

    let built_site = match site_builder.build().await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to build site: {}", e);
            // Create a minimal site for error cases
            panic!("Cannot start without a valid site configuration: {}", e);
        }
    };

    // Start periodic cleanup for login if user database is configured
    if config.app.user_database.is_some() {
        login::start_periodic_cleanup(built_site.login_state().clone());
    }

    // Initialize global resources (shared across all sites)

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
        site: Arc::new(built_site),
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
