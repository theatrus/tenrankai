use crate::site::ResolvedState;
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// Handler for /robots.txt
/// Returns a permissive robots.txt that allows all crawlers
pub async fn robots_txt_handler(ResolvedState(app_state): ResolvedState) -> Response {
    // Check if a custom robots.txt exists in any static directory (in order)
    // Note: Only checks filesystem paths, not S3 URLs
    for (index, static_url) in app_state.config.static_files.directories.iter().enumerate() {
        // Skip S3 URLs - they don't support direct filesystem checks
        if static_url.starts_with("s3://") {
            continue;
        }

        let static_dir = std::path::Path::new(static_url);
        let custom_robots_path = static_dir.join("robots.txt");
        if custom_robots_path.exists() {
            // Serve the custom robots.txt file
            match tokio::fs::read_to_string(&custom_robots_path).await {
                Ok(content) => {
                    tracing::debug!(
                        "Found robots.txt in directory {}: {:?}",
                        index,
                        custom_robots_path
                    );
                    return (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                        content,
                    )
                        .into_response();
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to read custom robots.txt from {:?}: {}",
                        custom_robots_path,
                        e
                    );
                }
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
