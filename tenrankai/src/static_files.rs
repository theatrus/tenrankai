use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info};

use crate::storage::{DynStorage, FilesystemStorage, StorageUrl};

/// Handler for serving static files from multiple storage backends.
///
/// Supports both filesystem and S3 storage. For S3 backends, can optionally
/// redirect to signed URLs to reduce server bandwidth.
#[derive(Clone)]
pub struct StaticFileHandler {
    /// Storage backends in priority order (first match wins)
    storages: Vec<DynStorage>,
    /// File versions for cache busting (filename -> mtime)
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
    /// Whether to use signed URL redirects for S3 backends
    use_redirects: bool,
    /// Expiry time for signed URLs
    redirect_expiry: Duration,
}

impl StaticFileHandler {
    /// Create a new static file handler from storage backends.
    ///
    /// # Arguments
    /// * `storages` - Storage backends in priority order
    pub fn new(storages: Vec<DynStorage>) -> Self {
        Self {
            storages,
            file_versions: Arc::new(RwLock::new(HashMap::new())),
            use_redirects: true,
            redirect_expiry: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Create a static file handler from filesystem paths.
    ///
    /// This is a convenience method for backward compatibility.
    pub fn from_paths(static_dirs: Vec<PathBuf>) -> Self {
        let storages: Vec<DynStorage> = static_dirs
            .into_iter()
            .map(|path| Arc::new(FilesystemStorage::new(path)) as DynStorage)
            .collect();

        Self::new(storages)
    }

    /// Create a static file handler from storage URL strings.
    ///
    /// # Arguments
    /// * `urls` - Storage URL strings (paths or s3:// URLs)
    pub async fn from_urls(urls: Vec<String>) -> Result<Self, crate::storage::StorageError> {
        let mut storages = Vec::with_capacity(urls.len());

        for url_str in urls {
            let url = StorageUrl::parse(&url_str)?;
            let storage = url.into_storage().await?;
            storages.push(storage);
        }

        Ok(Self::new(storages))
    }

    /// Set whether to use signed URL redirects for S3 backends.
    pub fn with_redirects(mut self, use_redirects: bool) -> Self {
        self.use_redirects = use_redirects;
        self
    }

    /// Set the expiry time for signed URLs.
    pub fn with_redirect_expiry(mut self, expiry: Duration) -> Self {
        self.redirect_expiry = expiry;
        self
    }

    /// Get the storage backends (for compatibility with existing code).
    pub fn storages(&self) -> &[DynStorage] {
        &self.storages
    }

    /// Check if a file exists in any storage backend.
    pub async fn exists(&self, path: &str) -> bool {
        let clean_path = path.trim_start_matches('/');
        for storage in &self.storages {
            if let Ok(true) = storage.exists(clean_path).await {
                return true;
            }
        }
        false
    }

    /// Refresh file version cache by scanning all storage backends.
    pub async fn refresh_file_versions(&self) {
        info!(
            "Refreshing static file versions from {} storage backends",
            self.storages.len()
        );
        let mut versions = self.file_versions.write().await;
        versions.clear();

        // Scan CSS and JS files from all backends
        // Files in earlier backends override files in later backends
        for (index, storage) in self.storages.iter().enumerate() {
            debug!(
                "Scanning static storage {}: {}",
                index,
                storage.storage_type()
            );

            match storage.list_recursive("").await {
                Ok(entries) => {
                    for entry in entries {
                        if entry.is_dir {
                            continue;
                        }

                        // Check if it's a CSS or JS file
                        let path = &entry.path;
                        if path.ends_with(".css") || path.ends_with(".js") {
                            // Extract filename
                            let filename = path.rsplit('/').next().unwrap_or(path);

                            // Get modification time and insert if not already present
                            if let Some(ref meta) = entry.metadata
                                && let Some(mtime) = meta.last_modified
                                && let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH)
                                && !versions.contains_key(filename)
                            {
                                versions.insert(filename.to_string(), duration.as_secs());
                                info!(
                                    "File version: {} -> {} (from storage {})",
                                    filename,
                                    duration.as_secs(),
                                    index
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to scan storage backend {}: {}", index, e);
                }
            }
        }
    }

    /// Get the cached version for a filename.
    pub async fn get_file_version(&self, filename: &str) -> Option<u64> {
        let versions = self.file_versions.read().await;
        versions.get(filename).copied()
    }

    /// Get all cached file versions.
    pub async fn get_all_versions(&self) -> HashMap<String, u64> {
        let versions = self.file_versions.read().await;
        versions.clone()
    }

    /// Get a versioned URL for a file path.
    pub async fn get_versioned_url(&self, path: &str) -> String {
        // Extract filename from path
        let filename = path.rsplit('/').next().unwrap_or(path);

        if let Some(version) = self.get_file_version(filename).await {
            format!("{}?v={}", path, version)
        } else {
            path.to_string()
        }
    }

    /// Serve a static file.
    ///
    /// Tries each storage backend in order until the file is found.
    /// For S3 backends with redirect support enabled, redirects to a signed URL.
    pub async fn serve(&self, path: &str, has_version: bool) -> Response {
        let clean_path = path.trim_start_matches('/');

        // Try each storage backend in order
        for (index, storage) in self.storages.iter().enumerate() {
            debug!(
                "Attempting to serve static file from storage {}: {}",
                index, clean_path
            );

            // Check if file exists
            match storage.exists(clean_path).await {
                Ok(true) => {
                    debug!("Found file in storage {}", index);
                }
                Ok(false) => {
                    debug!("File not found in storage {}", index);
                    continue;
                }
                Err(e) => {
                    debug!("Error checking file in storage {}: {}", index, e);
                    continue;
                }
            }

            // Get metadata for headers
            let metadata = match storage.metadata(clean_path).await {
                Ok(m) => m,
                Err(e) => {
                    debug!("Failed to get metadata from storage {}: {}", index, e);
                    continue;
                }
            };

            // Determine content type
            let content_type = metadata.content_type.clone().unwrap_or_else(|| {
                mime_guess::from_path(clean_path)
                    .first_or_octet_stream()
                    .to_string()
            });

            // Check if we should redirect to signed URL (S3 only)
            if self.use_redirects
                && storage.supports_redirect()
                && let Some(signed_url) = storage.signed_url(clean_path, self.redirect_expiry).await
            {
                debug!("Redirecting to signed URL for {}", clean_path);
                return Redirect::temporary(&signed_url).into_response();
            }

            // Stream the file content
            let stream = match storage.read_stream(clean_path).await {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to read file from storage {}: {}", index, e);
                    continue;
                }
            };

            let body =
                Body::from_stream(ReaderStream::new(tokio_util::io::StreamReader::new(stream)));

            // Determine cache headers
            let cache_control = if has_version {
                "public, max-age=31536000, immutable"
            } else if content_type.starts_with("image/") {
                "public, max-age=31536000"
            } else if content_type.starts_with("text/css")
                || content_type.starts_with("application/javascript")
            {
                "public, max-age=300, must-revalidate"
            } else {
                "public, max-age=3600"
            };

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, cache_control)
                .header(header::CONTENT_LENGTH, metadata.size);

            // Add Last-Modified header
            if let Some(modified) = metadata.last_modified {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let http_date = httpdate::fmt_http_date(modified);
                    response = response.header(header::LAST_MODIFIED, http_date);

                    // Add ETag
                    let etag = format!("\"{}-{}\"", duration.as_secs(), metadata.size);
                    response = response.header(header::ETAG, etag);
                }
            } else if let Some(ref etag) = metadata.etag {
                response = response.header(header::ETAG, etag.as_str());
            }

            return response.body(body).unwrap();
        }

        debug!("File not found in any storage backend: {}", clean_path);
        (StatusCode::NOT_FOUND, "File not found").into_response()
    }
}

// Backward compatibility: allow creating from Vec<PathBuf> directly
impl From<Vec<PathBuf>> for StaticFileHandler {
    fn from(paths: Vec<PathBuf>) -> Self {
        Self::from_paths(paths)
    }
}
