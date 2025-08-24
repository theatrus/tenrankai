use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::UNIX_EPOCH};
use tokio::{fs::File, sync::RwLock};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct StaticFileHandler {
    pub static_dir: PathBuf,
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
}

impl StaticFileHandler {
    pub fn new(static_dir: PathBuf) -> Self {
        let handler = Self {
            static_dir,
            file_versions: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize file versions on startup
        let handler_clone = handler.clone();
        tokio::spawn(async move {
            handler_clone.refresh_file_versions().await;
        });

        handler
    }

    pub async fn refresh_file_versions(&self) {
        info!("Refreshing static file versions");
        let mut versions = self.file_versions.write().await;
        versions.clear();

        // Scan CSS and JS files
        if let Ok(entries) = std::fs::read_dir(&self.static_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata()
                    && metadata.is_file()
                {
                    let path = entry.path();
                    if let Some(ext) = path.extension()
                        && (ext == "css" || ext == "js")
                        && let Ok(modified) = metadata.modified()
                        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
                        && let Some(file_name) = path.file_name()
                        && let Some(file_name_str) = file_name.to_str()
                    {
                        versions.insert(file_name_str.to_string(), duration.as_secs());
                        debug!("File version: {} -> {}", file_name_str, duration.as_secs());
                    }
                }
            }
        }
    }

    pub async fn get_file_version(&self, filename: &str) -> Option<u64> {
        let versions = self.file_versions.read().await;
        versions.get(filename).copied()
    }

    pub async fn get_versioned_url(&self, path: &str) -> String {
        // Extract filename from path
        let filename = path.rsplit('/').next().unwrap_or(path);

        if let Some(version) = self.get_file_version(filename).await {
            format!("{}?v={}", path, version)
        } else {
            path.to_string()
        }
    }

    pub async fn serve(&self, path: &str, has_version: bool) -> Response {
        let file_path = self.static_dir.join(path.trim_start_matches('/'));

        debug!("Attempting to serve static file: {:?}", file_path);

        if !file_path.starts_with(&self.static_dir) {
            error!("Path traversal attempt: {:?}", file_path);
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }

        let metadata = match tokio::fs::metadata(&file_path).await {
            Ok(m) => m,
            Err(e) => {
                debug!("Failed to get metadata for {:?}: {}", file_path, e);
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
        };

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

        // Determine cache headers based on content type and whether version is present
        let cache_control = if has_version {
            // If file has version parameter, cache forever
            "public, max-age=31536000, immutable"
        } else if content_type.starts_with("image/") {
            // Images without version get long cache
            "public, max-age=31536000"
        } else if content_type.starts_with("text/css")
            || content_type.starts_with("application/javascript")
        {
            // CSS/JS without version get short cache
            "public, max-age=300, must-revalidate"
        } else {
            // Other files get moderate cache
            "public, max-age=3600"
        };

        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, cache_control);

        // Add Last-Modified header
        if let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
        {
            let http_date = httpdate::fmt_http_date(modified);
            response = response.header(header::LAST_MODIFIED, http_date);

            // Add ETag based on modification time and file size
            let etag = format!("\"{}-{}\"", duration.as_secs(), metadata.len());
            response = response.header(header::ETAG, etag);
        }

        response.body(body).unwrap()
    }
}
