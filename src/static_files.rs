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
    pub static_dirs: Vec<PathBuf>,
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
}

impl StaticFileHandler {
    pub fn new(static_dirs: Vec<PathBuf>) -> Self {
        let handler = Self {
            static_dirs,
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
        info!(
            "Refreshing static file versions from {} directories",
            self.static_dirs.len()
        );
        let mut versions = self.file_versions.write().await;
        versions.clear();

        // Scan CSS and JS files from all directories
        // Files in earlier directories override files in later directories
        for (index, static_dir) in self.static_dirs.iter().enumerate() {
            debug!("Scanning static directory {}: {:?}", index, static_dir);

            if let Ok(entries) = std::fs::read_dir(static_dir) {
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
                            // Only insert if not already present (earlier directories take precedence)
                            if !versions.contains_key(file_name_str) {
                                versions.insert(file_name_str.to_string(), duration.as_secs());
                                info!(
                                    "File version: {} -> {} (from dir {})",
                                    file_name_str,
                                    duration.as_secs(),
                                    index
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn get_file_version(&self, filename: &str) -> Option<u64> {
        let versions = self.file_versions.read().await;
        versions.get(filename).copied()
    }

    pub async fn get_all_versions(&self) -> HashMap<String, u64> {
        let versions = self.file_versions.read().await;
        versions.clone()
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
        let clean_path = path.trim_start_matches('/');

        // Try each directory in order until we find the file
        let mut found_file_path = None;
        let mut found_metadata = None;

        for (index, static_dir) in self.static_dirs.iter().enumerate() {
            let file_path = static_dir.join(clean_path);

            debug!(
                "Attempting to serve static file from directory {}: {:?}",
                index, file_path
            );

            // Security check: ensure the resolved path is within the static directory
            if !file_path.starts_with(static_dir) {
                error!(
                    "Path traversal attempt in directory {}: {:?}",
                    index, file_path
                );
                continue;
            }

            match tokio::fs::metadata(&file_path).await {
                Ok(m) if m.is_file() => {
                    found_file_path = Some(file_path);
                    found_metadata = Some(m);
                    debug!("Found file in directory {}", index);
                    break;
                }
                Ok(_) => {
                    debug!(
                        "Path exists but is not a file in directory {}: {:?}",
                        index, file_path
                    );
                }
                Err(e) => {
                    debug!(
                        "File not found in directory {}: {:?} - {}",
                        index, file_path, e
                    );
                }
            }
        }

        let (file_path, metadata) = match (found_file_path, found_metadata) {
            (Some(path), Some(meta)) => (path, meta),
            _ => {
                debug!("File not found in any static directory: {}", clean_path);
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
