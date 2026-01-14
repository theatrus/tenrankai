use crate::site::ResolvedState;
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// Handler for /robots.txt
/// Returns a permissive robots.txt that allows all crawlers
pub async fn robots_txt_handler(ResolvedState(app_state): ResolvedState) -> Response {
    // Check if a custom robots.txt exists in any static directory (in order)
    // Uses the site's static handler storages for multi-site support
    for (index, storage) in app_state.static_handler().storages().iter().enumerate() {
        match storage.read_to_string("robots.txt").await {
            Ok(content) => {
                tracing::debug!("Found robots.txt in static directory {}", index);
                return (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    content,
                )
                    .into_response();
            }
            Err(crate::storage::StorageError::NotFound(_)) => {
                // Not found in this storage, try next
                continue;
            }
            Err(e) => {
                tracing::error!("Failed to read robots.txt from storage {}: {}", index, e);
                // Continue to next storage on error
                continue;
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
