use crate::AppState;
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// Handler for /robots.txt
/// Returns a permissive robots.txt that allows all crawlers
pub async fn robots_txt_handler(State(app_state): State<AppState>) -> Response {
    // Check if a custom robots.txt exists in the static directory
    let custom_robots_path = app_state.config.static_files.directory.join("robots.txt");

    if custom_robots_path.exists() {
        // Serve the custom robots.txt file
        match tokio::fs::read_to_string(&custom_robots_path).await {
            Ok(content) => {
                return (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    content,
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to read custom robots.txt: {}", e);
            }
        }
    }

    // Return default permissive robots.txt
    let default_robots = r#"# robots.txt for Tenrankai Gallery
# This file allows all web crawlers to access all content

User-agent: *
Allow: /
Crawl-delay: 1

# Sitemap location (if you have one)
# Sitemap: /sitemap.xml
"#;

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        default_robots,
    )
        .into_response()
}
