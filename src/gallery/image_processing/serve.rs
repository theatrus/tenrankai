use crate::{
    ApiResponse,
    gallery::{Gallery, GalleryError},
};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::path::Path;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::{debug, error};

impl Gallery {
    /// Main entry point for serving images
    pub async fn serve_image(
        &self,
        relative_path: &str,
        size: Option<String>,
        accept_header: &str,
    ) -> Response {
        // Security check
        let full_path = self.config.source_directory.join(relative_path);
        if !full_path.starts_with(&self.config.source_directory) {
            return ApiResponse::Forbidden.into_response();
        }

        // Ensure the file exists
        if !full_path.exists() {
            error!("Image file not found: {:?}", full_path);
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
                    let cache_path = self.config.cache_directory.join(&cache_filename);

                    // Check if already cached
                    if self
                        .is_cache_valid(&cache_path, &full_path)
                        .await
                        .unwrap_or(false)
                    {
                        return self.serve_file_with_cache_header(&cache_path, true).await;
                    }

                    // Generate the tile
                    // Note: For retina, we use the same tile coordinates but cache with @2x suffix
                    debug!("Generating tile ({}, {}) retina={}", x, y, is_retina_tile);

                    match self
                        .get_image_tile(&full_path, relative_path, x, y, output_format)
                        .await
                    {
                        Ok(generated_path) => {
                            return self
                                .serve_file_with_cache_header(&generated_path, false)
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
                    let cache_path = self.config.cache_directory.join(&cache_filename);
                    let was_cached = cache_path.exists();

                    match self
                        .get_resized_image(&full_path, relative_path, size, output_format)
                        .await
                    {
                        Ok(cached_path) => {
                            return self
                                .serve_file_with_cache_header(&cached_path, was_cached)
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

        // Serve original file
        self.serve_file_with_cache_header(&full_path, false).await
    }

    /// Serve file with appropriate cache headers
    pub(crate) async fn serve_file_with_cache_header(
        &self,
        path: &Path,
        was_cached: bool,
    ) -> Response {
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        self.serve_file_with_content_type_and_cache_header(path, &mime_type, was_cached)
            .await
    }

    /// Serve file with content type and cache headers
    async fn serve_file_with_content_type_and_cache_header(
        &self,
        path: &Path,
        content_type: &str,
        _was_cached: bool,
    ) -> Response {
        match File::open(path).await {
            Ok(file) => {
                let metadata = match file.metadata().await {
                    Ok(m) => m,
                    Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR).into_response(),
                };

                let stream = ReaderStream::new(file);
                let body = Body::from_stream(stream);

                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
                headers.insert(
                    header::CONTENT_LENGTH,
                    metadata.len().to_string().parse().unwrap(),
                );

                // Add cache headers - max 1 day for all images
                headers.insert(
                    header::CACHE_CONTROL,
                    "public, max-age=86400".parse().unwrap(),
                );

                (StatusCode::OK, headers, body).into_response()
            }
            Err(e) => {
                error!("Failed to open file: {:?}, error: {}", path, e);
                (StatusCode::NOT_FOUND).into_response()
            }
        }
    }

    /// Serve cached image by key
    pub async fn serve_cached_image(
        &self,
        cache_key: &str,
        _size: &str,
        _accept_header: &str,
    ) -> Result<Response, GalleryError> {
        let cache_path = self.config.cache_directory.join(cache_key);

        if !cache_path.exists() {
            return Ok(ApiResponse::CacheEntryNotFound.into_response());
        }

        // Determine MIME type from extension using OutputFormat
        let mime_type = super::types::OutputFormat::from_file_extension(cache_key)
            .map(|format| format.mime_type())
            .unwrap_or("image/jpeg");

        Ok(self
            .serve_file_with_content_type_and_cache_header(&cache_path, mime_type, true)
            .await)
    }

    /// Store and serve composite image
    pub async fn store_and_serve_composite(
        &self,
        cache_key: &str,
        image: image::DynamicImage,
    ) -> Result<Response, GalleryError> {
        // Always use JPEG for composites
        let output_format = super::types::OutputFormat::Jpeg;
        // Note: cache_key is the enhanced composite key (e.g., "composite_default_MjAwOC1ldXJla2E")
        // with gallery context and safe base64 encoding
        let hash = self.generate_cache_key(cache_key, output_format.extension());

        // Use the enhanced CacheType system for consistent filename generation
        if let Some(cache_type) = crate::CacheType::from_composite_cache_key(cache_key) {
            let cache_filename = cache_type.filename(Some(&hash));
            let cache_path = self.config.cache_directory.join(&cache_filename);

            // Store the cache_filename for later use in response
            self.store_composite_with_path(image, cache_path, output_format, cache_filename)
                .await
        } else {
            // Fallback to old method if cache_key format is unexpected
            let cache_filename = format!("{}.{}", hash, output_format.extension());
            let cache_path = self.config.cache_directory.join(&cache_filename);
            self.store_composite_with_path(image, cache_path, output_format, cache_filename)
                .await
        }
    }

    /// Helper method to store composite with given path and return response
    async fn store_composite_with_path(
        &self,
        image: image::DynamicImage,
        cache_path: std::path::PathBuf,
        output_format: super::types::OutputFormat,
        cache_filename: String,
    ) -> Result<Response, GalleryError> {
        use image::ImageEncoder;
        use std::io::Cursor;

        // Ensure cache directory exists
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

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

        // Write to cache
        tokio::fs::write(&cache_path, &image_data).await?;
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
