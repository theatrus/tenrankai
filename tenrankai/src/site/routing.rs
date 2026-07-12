use axum::{
    body::Body,
    extract::FromRef,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::debug;

use crate::AppState;

/// Extract the hostname from the request's Host header
fn extract_hostname(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Middleware that resolves the site based on the Host header.
///
/// In multi-site mode, this middleware:
/// 1. Extracts the Host header from the request
/// 2. Looks up the site in the SiteManager
/// 3. Creates a new AppState with the resolved site
/// 4. Injects it as a request extension for handlers to use
///
/// In single-site mode (site_manager is None), this middleware does nothing.
pub async fn site_resolution_middleware(
    State(app_state): axum::extract::State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Skip resolution if not in multi-site mode
    let site_manager = match &app_state.site_manager {
        Some(manager) => manager,
        None => return next.run(request).await,
    };

    // Extract hostname from Host header
    let hostname = match extract_hostname(&request) {
        Some(h) => h,
        None => {
            // No Host header - use default site
            debug!("No Host header, using default site");
            match site_manager.get_default_site().await {
                Some(site) => {
                    let new_state = app_state.with_site(site);
                    request.extensions_mut().insert(new_state);
                    return next.run(request).await;
                }
                None => {
                    return (StatusCode::NOT_FOUND, "No default site configured").into_response();
                }
            }
        }
    };

    debug!("Resolving site for hostname: {}", hostname);

    // Look up site for this hostname
    match site_manager.get_site(&hostname).await {
        Some(site) => {
            debug!("Resolved site '{}' for hostname '{}'", site.name, hostname);
            let new_state = app_state.with_site(site);
            request.extensions_mut().insert(new_state);
            next.run(request).await
        }
        None => {
            debug!("No site found for hostname: {}", hostname);
            (StatusCode::NOT_FOUND, "Site not found").into_response()
        }
    }
}

/// Extractor for getting the resolved AppState from a request.
///
/// This extractor first checks for an AppState in the request extensions
/// (set by site_resolution_middleware), and falls back to the original
/// State<AppState> if not found.
///
/// Use this in handlers that need the per-request resolved site in multi-site mode.
pub struct ResolvedState(pub AppState);

impl<S> axum::extract::FromRequestParts<S> for ResolvedState
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Check for resolved state in extensions first
        if let Some(resolved) = parts.extensions.get::<AppState>() {
            return Ok(ResolvedState(resolved.clone()));
        }

        // Fall back to the original state
        let app_state = AppState::from_ref(state);
        Ok(ResolvedState(app_state))
    }
}

// Re-export State for use in handlers
use axum::extract::State;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::{Site, SiteManager, SiteResources};
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn create_test_site(name: &str) -> Arc<Site> {
        let resources = SiteResources {
            base_url: None,
            cookie_secret: "test-secret".to_string(),
            template_engine: Arc::new(crate::templating::TemplateEngine::new(vec![])),
            static_handler: crate::static_files::StaticFileHandler::new(vec![]),
            favicon_renderer: crate::favicon::FaviconRenderer::new(vec![]),
            galleries: Arc::new(HashMap::new()),
            posts_managers: Arc::new(HashMap::new()),
            login_state: Arc::new(tokio::sync::RwLock::new(crate::login::LoginState::new())),
            user_storage: None,
            email_config: None,
            config_storage: None,
            config_storage_url: None,
            site_admins: Vec::new(),
            theme: None,
            hosted_mode: false,
            shortcode_index: Arc::new(tokio::sync::RwLock::new(
                crate::short_url::ShortcodeIndex::new(),
            )),
            webauthn: None,
        };
        Arc::new(Site::new(name.to_string(), resources))
    }

    async fn test_handler(ResolvedState(state): ResolvedState) -> String {
        format!("Site: {}", state.site.name)
    }

    #[tokio::test]
    async fn test_site_resolution_with_host() {
        // Create sites
        let default_site = create_test_site("default");
        let photos_site = create_test_site("photos");

        // Create site manager
        let site_manager = Arc::new(SiteManager::new());
        site_manager
            .add_site(default_site.clone(), vec!["*".to_string()])
            .await;
        site_manager
            .add_site(photos_site.clone(), vec!["photos.example.com".to_string()])
            .await;

        // Create app state with site manager
        let app_state = AppState {
            site: default_site,
            site_manager: Some(site_manager),
            email_provider: None,
            openai_client: None,
            cache_queue: None,
            astro: None,
        };

        // Create router with middleware
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                site_resolution_middleware,
            ))
            .with_state(app_state);

        // Test with photos.example.com host
        let request = Request::builder()
            .uri("/test")
            .header("host", "photos.example.com")
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Site: photos");
    }

    #[tokio::test]
    async fn test_site_resolution_fallback_to_default() {
        let default_site = create_test_site("default");

        let site_manager = Arc::new(SiteManager::new());
        site_manager
            .add_site(default_site.clone(), vec!["*".to_string()])
            .await;

        let app_state = AppState {
            site: default_site,
            site_manager: Some(site_manager),
            email_provider: None,
            openai_client: None,
            cache_queue: None,
            astro: None,
        };

        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                site_resolution_middleware,
            ))
            .with_state(app_state);

        // Test with unknown hostname - should fall back to default
        let request = Request::builder()
            .uri("/test")
            .header("host", "unknown.example.com")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Site: default");
    }

    #[tokio::test]
    async fn test_site_resolution_no_match() {
        let default_site = create_test_site("specific");

        let site_manager = Arc::new(SiteManager::new());
        // No catch-all site
        site_manager
            .add_site(
                default_site.clone(),
                vec!["specific.example.com".to_string()],
            )
            .await;

        let app_state = AppState {
            site: default_site,
            site_manager: Some(site_manager),
            email_provider: None,
            openai_client: None,
            cache_queue: None,
            astro: None,
        };

        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                site_resolution_middleware,
            ))
            .with_state(app_state);

        // Test with unknown hostname - should return 404
        let request = Request::builder()
            .uri("/test")
            .header("host", "unknown.example.com")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_single_site_mode_bypass() {
        let default_site = create_test_site("single");

        // No site manager - single site mode
        let app_state = AppState {
            site: default_site,
            site_manager: None,
            email_provider: None,
            openai_client: None,
            cache_queue: None,
            astro: None,
        };

        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                site_resolution_middleware,
            ))
            .with_state(app_state);

        // Any hostname should work - middleware is bypassed
        let request = Request::builder()
            .uri("/test")
            .header("host", "any.hostname.com")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Site: single");
    }
}
