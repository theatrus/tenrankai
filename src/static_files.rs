use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::path::PathBuf;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::{debug, error};

#[derive(Clone)]
pub struct StaticFileHandler {
    static_dir: PathBuf,
}

impl StaticFileHandler {
    pub fn new(static_dir: PathBuf) -> Self {
        Self { static_dir }
    }

    pub async fn serve(&self, path: &str) -> Response {
        let file_path = self.static_dir.join(path.trim_start_matches('/'));

        debug!("Attempting to serve static file: {:?}", file_path);

        if !file_path.starts_with(&self.static_dir) {
            error!("Path traversal attempt: {:?}", file_path);
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }

        let file = match File::open(&file_path).await {
            Ok(file) => file,
            Err(e) => {
                debug!("Failed to open file {:?}: {}", file_path, e);
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
        };

        let content_type = mime_guess::from_path(&file_path)
            .first_or_octet_stream()
            .to_string();

        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        // Add cache headers for static files (especially images)
        let cache_control = if content_type.starts_with("image/") {
            "public, max-age=31536000, immutable"
        } else if content_type.starts_with("text/css")
            || content_type.starts_with("application/javascript")
        {
            "public, max-age=86400"
        } else {
            "public, max-age=3600"
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, cache_control)
            .body(body)
            .unwrap()
    }
}
