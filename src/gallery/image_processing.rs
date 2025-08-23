use super::{CachedImage, Gallery, GalleryError};
use crate::copyright::{CopyrightConfig, add_copyright_notice};
use image::{ImageFormat, imageops::FilterType};
use std::path::PathBuf;
use tracing::{debug, error};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum OutputFormat {
    Jpeg,
    WebP,
}

impl OutputFormat {
    fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::WebP => "webp",
        }
    }

    fn image_format(&self) -> ImageFormat {
        match self {
            OutputFormat::Jpeg => ImageFormat::Jpeg,
            OutputFormat::WebP => ImageFormat::WebP,
        }
    }
}

impl Gallery {
    fn determine_output_format(&self, accept_header: &str) -> OutputFormat {
        // Check if browser accepts WebP
        if accept_header.contains("image/webp") {
            OutputFormat::WebP
        } else {
            OutputFormat::Jpeg
        }
    }

    pub async fn serve_image(
        &self,
        relative_path: &str,
        size: Option<String>,
        accept_header: &str,
    ) -> axum::response::Response {
        use axum::{http::StatusCode, response::IntoResponse};

        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }

        let size = size.as_deref();
        let output_format = self.determine_output_format(accept_header);

        if let Some(size) = size {
            match self
                .get_resized_image(&full_path, relative_path, size, output_format)
                .await
            {
                Ok(cached_path) => {
                    return self.serve_file(&cached_path).await;
                }
                Err(e) => {
                    error!("Failed to resize image: {}", e);
                }
            }
        }

        self.serve_file(&full_path).await
    }

    pub(crate) async fn get_resized_image(
        &self,
        original_path: &PathBuf,
        relative_path: &str,
        size: &str,
        output_format: OutputFormat,
    ) -> Result<PathBuf, GalleryError> {
        // Check if it's a @2x variant
        let (base_size, multiplier) = if size.ends_with("@2x") {
            (size.trim_end_matches("@2x"), 2)
        } else {
            (size, 1)
        };

        let (base_width, base_height) = match base_size {
            "thumbnail" => (self.config.thumbnail.width, self.config.thumbnail.height),
            "gallery" => (
                self.config.gallery_size.width,
                self.config.gallery_size.height,
            ),
            "medium" => (self.config.medium.width, self.config.medium.height),
            "large" => (self.config.large.width, self.config.large.height),
            _ => return Err(GalleryError::InvalidPath),
        };

        // Apply multiplier for @2x variants
        let width = base_width * multiplier as u32;
        let height = base_height * multiplier as u32;

        // Include format in cache key
        let cache_key = format!("{}_{}", size, output_format.extension());
        let hash = self.generate_cache_key(relative_path, &cache_key);
        let cache_filename = format!("{}.{}", hash, output_format.extension());
        let cache_path = self.config.cache_directory.join(cache_filename);

        let original_metadata = tokio::fs::metadata(original_path).await?;
        let original_modified = original_metadata.modified()?;

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(&hash)
            && cached.modified >= original_modified
            && cached.path.exists()
        {
            return Ok(cached.path.clone());
        }
        drop(cache);

        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Move CPU-intensive and blocking I/O operations to blocking thread pool
        let original_path = original_path.clone();
        let cache_path_clone = cache_path.clone();
        let is_medium = base_size == "medium";
        let copyright_holder = self.app_config.copyright_holder.clone();
        let static_dir = PathBuf::from("static"); // Assume static dir for font
        let format = output_format;
        let jpeg_quality = self.config.jpeg_quality.unwrap_or(85);
        let _webp_quality = self.config.webp_quality.unwrap_or(85.0);

        tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
            // Open image with decoder to access ICC profile
            let original_file = std::fs::File::open(&original_path)?;

            let decoder = image::ImageReader::new(std::io::BufReader::new(original_file))
                .with_guessed_format()?;

            let img = decoder.decode()?;

            // Get original dimensions
            let (orig_width, orig_height) = (img.width(), img.height());

            // Don't upscale - if requested dimensions are larger than original, use original
            let final_width = width.min(orig_width);
            let final_height = height.min(orig_height);

            // Only resize if dimensions are different
            let resized = if final_width != orig_width || final_height != orig_height {
                img.resize(final_width, final_height, FilterType::Lanczos3)
            } else {
                img
            };

            // Apply copyright watermark if this is a medium image and copyright holder is configured
            let final_image = if is_medium && copyright_holder.is_some() {
                let font_path = static_dir.join("DejaVuSans.ttf");
                if font_path.exists() {
                    let copyright_config = CopyrightConfig {
                        copyright_holder: copyright_holder.unwrap_or_default(),
                        font_size: 20.0,
                        padding: 10,
                    };

                    match add_copyright_notice(&resized, &copyright_config, &font_path) {
                        Ok(watermarked) => watermarked,
                        Err(e) => {
                            error!("Failed to add copyright watermark: {}", e);
                            resized
                        }
                    }
                } else {
                    debug!("Font file not found at {:?}, skipping watermark", font_path);
                    resized
                }
            } else {
                resized
            };

            // Save final image in the requested format with quality settings
            match format {
                OutputFormat::Jpeg => {
                    // Use JPEG with configurable quality
                    use image::codecs::jpeg::JpegEncoder;
                    let mut output = std::fs::File::create(&cache_path_clone)?;
                    let encoder = JpegEncoder::new_with_quality(&mut output, jpeg_quality);
                    final_image.write_with_encoder(encoder)?;
                }
                OutputFormat::WebP => {
                    // WebP encoding with quality
                    // Note: The image crate's WebP support might be limited
                    // For better WebP encoding, consider using webp crate directly
                    final_image.save_with_format(&cache_path_clone, format.image_format())?;
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| GalleryError::IoError(std::io::Error::other(e)))??;

        let mut cache = self.cache.write().await;
        cache.insert(
            hash,
            CachedImage {
                path: cache_path.clone(),
                modified: original_modified,
            },
        );

        Ok(cache_path)
    }

    async fn serve_file(&self, path: &PathBuf) -> axum::response::Response {
        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        self.serve_file_with_content_type(path, &content_type).await
    }

    async fn serve_file_with_content_type(
        &self,
        path: &PathBuf,
        content_type: &str,
    ) -> axum::response::Response {
        use axum::{
            body::Body,
            http::{StatusCode, header},
            response::{IntoResponse, Response},
        };
        use tokio_util::io::ReaderStream;

        let file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open file: {:?}: {}", path, e);
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
        };

        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(body)
            .unwrap()
    }

    pub async fn serve_cached_image(
        &self,
        cache_key: &str,
        size: &str,
        accept_header: &str,
    ) -> Result<axum::response::Response, GalleryError> {
        // For composites, always use JPEG
        let (hash, content_type) = if size == "composite" {
            (format!("{}_composite_jpg", cache_key), "image/jpeg")
        } else {
            let output_format = self.determine_output_format(accept_header);
            let hash = format!("{}_{}_{}", cache_key, size, output_format.extension());
            let content_type = match output_format {
                OutputFormat::Jpeg => "image/jpeg",
                OutputFormat::WebP => "image/webp",
            };
            (hash, content_type)
        };

        // Check if we have it in cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&hash) {
                debug!("Serving composite from cache: {}", hash);
                return Ok(self
                    .serve_file_with_content_type(&cached.path, content_type)
                    .await);
            }
        }

        Err(GalleryError::NotFound)
    }

    pub async fn store_and_serve_composite(
        &self,
        cache_key: &str,
        image: image::DynamicImage,
    ) -> Result<axum::response::Response, GalleryError> {
        use axum::{
            body::Body,
            http::{StatusCode, header},
            response::Response,
        };
        use std::io::Cursor;

        // Always use JPEG for composites
        let output_format = OutputFormat::Jpeg;
        let hash = format!("{}_composite_{}", cache_key, output_format.extension());
        let cache_path = self.config.cache_directory.join(&hash);

        // Convert to RGB (JPEG doesn't support alpha)
        let rgb_image = image.to_rgb8();

        // Save to cache
        let mut buffer = Vec::new();
        {
            let mut cursor = Cursor::new(&mut buffer);
            image::DynamicImage::ImageRgb8(rgb_image.clone())
                .write_to(&mut cursor, image::ImageFormat::Jpeg)
                .map_err(GalleryError::ImageError)?;
        }

        // Ensure cache directory exists
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write to cache file
        tokio::fs::write(&cache_path, &buffer).await?;

        // Update in-memory cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                hash.clone(),
                CachedImage {
                    path: cache_path,
                    modified: std::time::SystemTime::now(),
                },
            );
        }

        debug!("Stored composite in cache: {}", hash);

        // Return the response
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(buffer))
            .unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppConfig, GalleryConfig};
    use image::{ImageBuffer, Rgba};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    async fn create_test_gallery() -> (Gallery, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let config = GalleryConfig {
            path_prefix: "gallery".to_string(),
            source_directory: temp_dir.path().to_path_buf(),
            cache_directory: cache_dir,
            images_per_page: 50,
            thumbnail: crate::ImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: crate::ImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: crate::ImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: crate::ImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            preview: crate::PreviewConfig {
                max_images: 6,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
        };

        let app_config = AppConfig {
            name: "Test".to_string(),
            log_level: "info".to_string(),
            download_secret: "secret".to_string(),
            download_password: "password".to_string(),
            copyright_holder: None,
            base_url: None,
        };

        let gallery = Gallery {
            config,
            app_config,
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cache_metadata: Arc::new(RwLock::new(super::super::CacheMetadata {
                version: String::new(),
                last_full_refresh: std::time::SystemTime::UNIX_EPOCH,
            })),
            metadata_cache_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            metadata_updates_since_save: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };

        (gallery, temp_dir)
    }

    #[tokio::test]
    async fn test_store_and_serve_composite() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Create a simple test image
        let img = ImageBuffer::from_pixel(100, 100, Rgba([255, 0, 0, 255]));
        let dynamic_img = image::DynamicImage::ImageRgba8(img);

        let cache_key = "test_composite";

        // Store the composite
        let result = gallery
            .store_and_serve_composite(cache_key, dynamic_img.clone())
            .await;
        assert!(result.is_ok(), "Failed to store composite: {:?}", result);

        // Check that the file was created
        let hash = format!("{}_composite_jpg", cache_key);
        let cache_path = gallery.config.cache_directory.join(&hash);
        assert!(
            tokio::fs::metadata(&cache_path).await.is_ok(),
            "Cache file not created"
        );

        // Check that it's in the in-memory cache
        {
            let cache = gallery.cache.read().await;
            assert!(cache.contains_key(&hash), "Composite not in memory cache");
        }

        // Test serving from cache
        let cached_result = gallery.serve_cached_image(cache_key, "composite", "").await;
        assert!(
            cached_result.is_ok(),
            "Failed to serve from cache: {:?}",
            cached_result
        );
    }

    #[tokio::test]
    async fn test_serve_cached_image_not_found() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Try to serve non-existent composite
        let result = gallery
            .serve_cached_image("non_existent", "composite", "")
            .await;
        assert!(matches!(result, Err(GalleryError::NotFound)));
    }

    #[tokio::test]
    async fn test_store_composite_with_complex_key() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Create a test image
        let img = ImageBuffer::from_pixel(50, 50, Rgba([0, 255, 0, 255]));
        let dynamic_img = image::DynamicImage::ImageRgba8(img);

        // Use a cache key that would have created subdirectories with the old format
        let cache_key = "composite_folder_subfolder_item";

        // Store the composite
        let result = gallery
            .store_and_serve_composite(cache_key, dynamic_img)
            .await;
        assert!(
            result.is_ok(),
            "Failed to store composite with complex key: {:?}",
            result
        );

        // Verify it was stored
        let hash = format!("{}_composite_jpg", cache_key);
        let cache_path = gallery.config.cache_directory.join(&hash);
        assert!(
            tokio::fs::metadata(&cache_path).await.is_ok(),
            "Cache file not created for complex key"
        );
    }

    #[tokio::test]
    async fn test_cached_composite_mime_type() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Create and store a test image
        let img = ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 255, 255]));
        let dynamic_img = image::DynamicImage::ImageRgba8(img);
        let cache_key = "test_mime_type";

        // Store the composite
        let store_result = gallery
            .store_and_serve_composite(cache_key, dynamic_img)
            .await;
        assert!(store_result.is_ok());

        // Serve from cache
        let cached_response = gallery.serve_cached_image(cache_key, "composite", "").await;
        assert!(cached_response.is_ok());

        // Check that the response has the correct MIME type
        let response = cached_response.unwrap();
        let content_type = response.headers().get("content-type");
        assert!(content_type.is_some(), "Content-Type header missing");
        assert_eq!(
            content_type.unwrap().to_str().unwrap(),
            "image/jpeg",
            "Wrong MIME type for cached composite"
        );
    }
}
