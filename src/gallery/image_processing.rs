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

        let content_type = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .body(body)
            .unwrap()
    }
}
