use crate::{
    ApiResponse,
    gallery::{Gallery, GalleryError},
    storage::StorageError,
};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures::TryStreamExt;
use tracing::{debug, error};

impl Gallery {
    /// Main entry point for serving images
    pub async fn serve_image(
        &self,
        relative_path: &str,
        size: Option<String>,
        accept_header: &str,
    ) -> Response {
        // Security check - prevent path traversal
        if relative_path.contains("..") || relative_path.starts_with('/') {
            return ApiResponse::Forbidden.into_response();
        }

        // Ensure the file exists using storage abstraction
        if !self
            .source_storage
            .exists(relative_path)
            .await
            .unwrap_or(false)
        {
            error!("Image file not found in storage: {}", relative_path);
            return ApiResponse::ImageNotFound.into_response();
        }

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

                    // Check if already cached using storage abstraction
                    if self
                        .is_cache_valid_by_key(&cache_filename, relative_path)
                        .await
                        .unwrap_or(false)
                    {
                        let mime_type = output_format.mime_type();
                        return self
                            .serve_from_cache_storage(&cache_filename, mime_type)
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
                                .serve_from_cache_storage(&cache_filename, mime_type)
                                .await;
                        }
                        Err(e) => {
                            error!("Failed to generate tile: {}", e);
                            return ApiResponse::InternalServerError.into_response();
                        }
                    }
                } else {
                    // Regular size request
                    // Determine if this size would have a watermark
                    let (_, is_medium) = match self.parse_size(size) {
                        Ok(result) => result,
                        Err(_) => {
                            return ApiResponse::InvalidSizeParameter.into_response();
                        }
                    };
                    let apply_watermark = is_medium && self.config.copyright_holder.is_some();

                    let cache_filename = self.generate_cache_filename(
                        relative_path,
                        size,
                        output_format.extension(),
                        apply_watermark,
                    );

                    // Check if already cached using storage abstraction
                    let was_cached = self
                        .cache_storage
                        .exists(&cache_filename)
                        .await
                        .unwrap_or(false);

                    match self
                        .get_resized_image(relative_path, size, output_format)
                        .await
                    {
                        Ok(_) => {
                            // Image was generated and written to storage, serve it
                            let mime_type = output_format.mime_type();
                            if was_cached {
                                debug!("Serving cached image: {}", cache_filename);
                            }
                            return self
                                .serve_from_cache_storage(&cache_filename, mime_type)
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
        self.serve_from_source_storage(relative_path).await
    }

    /// Serve file from source storage with streaming
    pub(crate) async fn serve_from_source_storage(&self, path: &str) -> Response {
        // Determine MIME type from path
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        // Get metadata for content-length
        let size = match self.source_storage.metadata(path).await {
            Ok(meta) => Some(meta.size),
            Err(_) => None,
        };

        // Get stream from storage
        match self.source_storage.read_stream(path).await {
            Ok(stream) => {
                // Convert StorageError stream to std::io::Error stream for axum Body
                let mapped_stream = stream.map_err(|e| std::io::Error::other(e.to_string()));
                let body = Body::from_stream(mapped_stream);

                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
                if let Some(len) = size {
                    headers.insert(header::CONTENT_LENGTH, len.to_string().parse().unwrap());
                }

                // Add cache headers - max 1 day for all images
                headers.insert(
                    header::CACHE_CONTROL,
                    "public, max-age=86400".parse().unwrap(),
                );

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
    pub async fn serve_cached_image(&self, cache_key: &str) -> Result<Response, GalleryError> {
        // Determine MIME type from extension using OutputFormat
        let mime_type = super::types::OutputFormat::from_file_extension(cache_key)
            .map(|format| format.mime_type())
            .unwrap_or("image/jpeg");

        // Serve from cache storage (handles NotFound internally)
        Ok(self.serve_from_cache_storage(cache_key, mime_type).await)
    }

    /// Serve file from cache storage with streaming
    pub(crate) async fn serve_from_cache_storage(
        &self,
        path: &str,
        content_type: &str,
    ) -> Response {
        // Get metadata for content-length
        let size = match self.cache_storage.metadata(path).await {
            Ok(meta) => Some(meta.size),
            Err(_) => None,
        };

        // Get stream from storage
        match self.cache_storage.read_stream(path).await {
            Ok(stream) => {
                // Convert StorageError stream to std::io::Error stream for axum Body
                let mapped_stream = stream.map_err(|e| std::io::Error::other(e.to_string()));
                let body = Body::from_stream(mapped_stream);

                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
                if let Some(len) = size {
                    headers.insert(header::CONTENT_LENGTH, len.to_string().parse().unwrap());
                }

                // Add cache headers - max 1 day for all images
                headers.insert(
                    header::CACHE_CONTROL,
                    "public, max-age=86400".parse().unwrap(),
                );

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

        // Create response
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            output_format.mime_type().parse().unwrap(),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            image_data.len().to_string().parse().unwrap(),
        );
        headers.insert(
            header::CACHE_CONTROL,
            "public, max-age=86400".parse().unwrap(),
        );

        Ok((StatusCode::OK, headers, Body::from(image_data)).into_response())
    }
}
