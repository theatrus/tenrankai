use crate::{
    ApiResponse,
    gallery::{Gallery, GalleryError},
    storage::{ObjectMetadata, StorageError},
};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures::TryStreamExt;
use tracing::{debug, error};

/// Short cache duration for images (5 minutes)
/// This allows quick updates while still benefiting from caching
const IMAGE_CACHE_MAX_AGE: u32 = 300;

/// Check if the request has matching conditional headers that would result in 304
fn check_conditional_request(
    req_headers: &HeaderMap,
    etag: Option<&str>,
    last_modified: Option<std::time::SystemTime>,
) -> bool {
    // Check If-None-Match (ETag comparison)
    if let Some(if_none_match) = req_headers.get(header::IF_NONE_MATCH)
        && let (Ok(inm_str), Some(current_etag)) = (if_none_match.to_str(), etag)
    {
        // Handle both quoted and unquoted ETags
        let inm_clean = inm_str.trim().trim_matches('"');
        let etag_clean = current_etag.trim().trim_matches('"');
        if inm_clean == etag_clean || inm_str == "*" {
            return true;
        }
    }

    // Check If-Modified-Since (date comparison)
    if let Some(if_modified_since) = req_headers.get(header::IF_MODIFIED_SINCE)
        && let (Ok(ims_str), Some(current_mtime)) = (if_modified_since.to_str(), last_modified)
        && let Ok(ims_time) = httpdate::parse_http_date(ims_str)
    {
        // If the file hasn't been modified since the client's cached version
        if current_mtime <= ims_time {
            return true;
        }
    }

    false
}

/// Generate an ETag from file metadata
fn generate_etag(size: u64, last_modified: Option<std::time::SystemTime>) -> String {
    let mtime_secs = last_modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("\"{:x}-{:x}\"", size, mtime_secs)
}

/// Add caching headers to a response
fn add_cache_headers(headers: &mut HeaderMap, metadata: &ObjectMetadata) {
    // Cache-Control: short duration with revalidation
    headers.insert(
        header::CACHE_CONTROL,
        format!("public, max-age={}, must-revalidate", IMAGE_CACHE_MAX_AGE)
            .parse()
            .unwrap(),
    );

    // ETag for cache validation
    let etag = metadata
        .etag
        .clone()
        .unwrap_or_else(|| generate_etag(metadata.size, metadata.last_modified));
    headers.insert(header::ETAG, etag.parse().unwrap());

    // Last-Modified header
    if let Some(mtime) = metadata.last_modified
        && let Ok(formatted) = httpdate::fmt_http_date(mtime).parse()
    {
        headers.insert(header::LAST_MODIFIED, formatted);
    }
}

/// Create a 304 Not Modified response
fn not_modified_response() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        format!("public, max-age={}, must-revalidate", IMAGE_CACHE_MAX_AGE)
            .parse()
            .unwrap(),
    );
    (StatusCode::NOT_MODIFIED, headers, Body::empty()).into_response()
}

impl Gallery {
    /// Main entry point for serving images
    pub async fn serve_image(
        &self,
        relative_path: &str,
        size: Option<String>,
        accept_header: &str,
        request_headers: &HeaderMap,
    ) -> Response {
        // Security check - prevent path traversal
        if relative_path.contains("..") || relative_path.starts_with('/') {
            return ApiResponse::Forbidden.into_response();
        }

        // Note: We defer the source existence check until we actually need to access
        // the source file. If serving from cache, we skip the check for better performance.

        let output_format = self.determine_output_format(accept_header, relative_path);
        debug!(
            "Serving image: {}, output format: {:?}",
            relative_path, output_format
        );

        // Handle resized images
        if let Some(size) = size.as_deref() {
            // Check if this is a tile request (including @2x)
            let is_retina_tile = size.ends_with("@2x") && size.starts_with("tile_");
            let size_to_parse = if is_retina_tile {
                &size[..size.len() - 3] // Remove @2x suffix
            } else {
                size
            };

            if let Some(parsed_size) = crate::gallery::types::ImageSize::parse(size_to_parse) {
                if let crate::gallery::types::ImageSize::Tile(x, y) = parsed_size {
                    // Handle tile request with retina support
                    let tile_size = self
                        .config
                        .tiles
                        .as_ref()
                        .map(|tc| tc.tile_size)
                        .unwrap_or(1024);

                    let cache_filename = crate::gallery::cache::generate_tile_cache_filename(
                        relative_path,
                        x,
                        y,
                        tile_size,
                        is_retina_tile,
                        output_format.extension(),
                    );

                    // Check if tile cache exists
                    if self
                        .cache_storage
                        .exists(&cache_filename)
                        .await
                        .unwrap_or(false)
                    {
                        let mime_type = output_format.mime_type();
                        return self
                            .serve_from_cache_storage(&cache_filename, mime_type, request_headers)
                            .await;
                    }

                    // Generate the tile
                    // Note: For retina, we use the same tile coordinates but cache with @2x suffix
                    debug!("Generating tile ({}, {}) retina={}", x, y, is_retina_tile);

                    match self.get_image_tile(relative_path, x, y).await {
                        Ok(_) => {
                            // Tile was generated and written to storage, serve it
                            let mime_type = output_format.mime_type();
                            return self
                                .serve_from_cache_storage(
                                    &cache_filename,
                                    mime_type,
                                    request_headers,
                                )
                                .await;
                        }
                        Err(e) => {
                            error!("Failed to generate tile: {}", e);
                            return ApiResponse::InternalServerError.into_response();
                        }
                    }
                } else {
                    // Regular size request
                    // Validate size parameter
                    if self.parse_size(size).is_err() {
                        return ApiResponse::InvalidSizeParameter.into_response();
                    }
                    let apply_watermark = self.should_apply_watermark(relative_path, size);

                    let cache_filename = self.generate_cache_filename(
                        relative_path,
                        size,
                        output_format.extension(),
                        apply_watermark,
                    );

                    // Check if cache exists
                    if self
                        .cache_storage
                        .exists(&cache_filename)
                        .await
                        .unwrap_or(false)
                    {
                        debug!("Serving cached image: {}", cache_filename);
                        let mime_type = output_format.mime_type();
                        return self
                            .serve_from_cache_storage(&cache_filename, mime_type, request_headers)
                            .await;
                    }

                    // Cache miss - generate the image
                    match self
                        .get_resized_image(relative_path, size, output_format)
                        .await
                    {
                        Ok(_) => {
                            // Image was generated and written to storage, serve it
                            let mime_type = output_format.mime_type();
                            return self
                                .serve_from_cache_storage(
                                    &cache_filename,
                                    mime_type,
                                    request_headers,
                                )
                                .await;
                        }
                        Err(e) => {
                            error!("Failed to resize image: {}", e);
                            // Fall through to serve original
                        }
                    }
                }
            } else {
                return ApiResponse::InvalidSizeParameter.into_response();
            }
        }

        // Serve original file from source storage
        self.serve_from_source_storage(relative_path, request_headers)
            .await
    }

    /// Serve file from source storage with streaming
    pub(crate) async fn serve_from_source_storage(
        &self,
        path: &str,
        request_headers: &HeaderMap,
    ) -> Response {
        // Determine MIME type from path
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        // Get metadata for content-length and cache validation
        let metadata = match self.source_storage.metadata(path).await {
            Ok(meta) => meta,
            Err(StorageError::NotFound(_)) => return ApiResponse::ImageNotFound.into_response(),
            Err(e) => {
                error!("Failed to get metadata from source storage: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
            }
        };

        // Check for conditional request (304 Not Modified)
        let etag = metadata
            .etag
            .clone()
            .unwrap_or_else(|| generate_etag(metadata.size, metadata.last_modified));
        if check_conditional_request(request_headers, Some(&etag), metadata.last_modified) {
            return not_modified_response();
        }

        // Get stream from storage
        match self.source_storage.read_stream(path).await {
            Ok(stream) => {
                // Convert StorageError stream to std::io::Error stream for axum Body
                let mapped_stream = stream.map_err(|e| std::io::Error::other(e.to_string()));
                let body = Body::from_stream(mapped_stream);

                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
                headers.insert(
                    header::CONTENT_LENGTH,
                    metadata.size.to_string().parse().unwrap(),
                );

                // Add cache headers with ETag and Last-Modified
                add_cache_headers(&mut headers, &metadata);

                (StatusCode::OK, headers, body).into_response()
            }
            Err(StorageError::NotFound(_)) => ApiResponse::ImageNotFound.into_response(),
            Err(e) => {
                error!("Failed to read from source storage: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    }

    /// Serve cached image by key using storage abstraction
    pub async fn serve_cached_image(
        &self,
        cache_key: &str,
        request_headers: &HeaderMap,
    ) -> Result<Response, GalleryError> {
        // Determine MIME type from extension using OutputFormat
        let mime_type = super::types::OutputFormat::from_file_extension(cache_key)
            .map(|format| format.mime_type())
            .unwrap_or("image/jpeg");

        // Serve from cache storage (handles NotFound internally)
        Ok(self
            .serve_from_cache_storage(cache_key, mime_type, request_headers)
            .await)
    }

    /// Serve file from cache storage with streaming
    pub(crate) async fn serve_from_cache_storage(
        &self,
        path: &str,
        content_type: &str,
        request_headers: &HeaderMap,
    ) -> Response {
        // Get metadata for content-length and cache validation
        let metadata = match self.cache_storage.metadata(path).await {
            Ok(meta) => meta,
            Err(StorageError::NotFound(_)) => {
                return ApiResponse::CacheEntryNotFound.into_response();
            }
            Err(e) => {
                error!("Failed to get metadata from cache storage: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
            }
        };

        // Check for conditional request (304 Not Modified)
        let etag = metadata
            .etag
            .clone()
            .unwrap_or_else(|| generate_etag(metadata.size, metadata.last_modified));
        if check_conditional_request(request_headers, Some(&etag), metadata.last_modified) {
            return not_modified_response();
        }

        // Get stream from storage
        match self.cache_storage.read_stream(path).await {
            Ok(stream) => {
                // Convert StorageError stream to std::io::Error stream for axum Body
                let mapped_stream = stream.map_err(|e| std::io::Error::other(e.to_string()));
                let body = Body::from_stream(mapped_stream);

                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
                headers.insert(
                    header::CONTENT_LENGTH,
                    metadata.size.to_string().parse().unwrap(),
                );

                // Add cache headers with ETag and Last-Modified
                add_cache_headers(&mut headers, &metadata);

                (StatusCode::OK, headers, body).into_response()
            }
            Err(StorageError::NotFound(_)) => ApiResponse::CacheEntryNotFound.into_response(),
            Err(e) => {
                error!("Failed to read from cache storage: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    }

    /// Store and serve composite image
    pub async fn store_and_serve_composite(
        &self,
        cache_key: &str,
        image: image::DynamicImage,
    ) -> Result<Response, GalleryError> {
        use bytes::Bytes;
        use image::ImageEncoder;
        use std::io::Cursor;

        // Always use JPEG for composites
        let output_format = super::types::OutputFormat::Jpeg;
        // Note: cache_key is the enhanced composite key (e.g., "composite_default_MjAwOC1ldXJla2E")
        // with gallery context and safe base64 encoding
        let hash = self.generate_cache_key(cache_key, output_format.extension());

        // Use the enhanced CacheType system for consistent filename generation
        let cache_filename =
            if let Some(cache_type) = crate::CacheType::from_composite_cache_key(cache_key) {
                cache_type.filename(Some(&hash))
            } else {
                // Fallback to old method if cache_key format is unexpected
                format!("{}.{}", hash, output_format.extension())
            };

        // Ensure cache directory exists
        self.cache_storage.create_dir("").await?;

        // Convert to RGB (JPEG doesn't support alpha)
        let rgb_image = image.to_rgb8();

        // Encode to JPEG in memory
        let mut buffer = Cursor::new(Vec::new());
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut buffer,
            output_format.default_quality() as u8,
        );
        encoder.write_image(
            rgb_image.as_raw(),
            rgb_image.width(),
            rgb_image.height(),
            image::ExtendedColorType::Rgb8,
        )?;

        let image_data = buffer.into_inner();

        // Write to cache storage
        self.cache_storage
            .write(&cache_filename, Bytes::from(image_data.clone()))
            .await?;
        tracing::debug!("Stored composite image: {}", cache_filename);

        // Create response with cache headers
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            output_format.mime_type().parse().unwrap(),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            image_data.len().to_string().parse().unwrap(),
        );

        // Create metadata for cache headers (newly created file)
        let now = std::time::SystemTime::now();
        let metadata = ObjectMetadata {
            size: image_data.len() as u64,
            last_modified: Some(now),
            content_type: Some(output_format.mime_type().to_string()),
            etag: None,
        };
        add_cache_headers(&mut headers, &metadata);

        Ok((StatusCode::OK, headers, Body::from(image_data)).into_response())
    }
}
