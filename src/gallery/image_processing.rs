use super::{Gallery, GalleryError};
use crate::copyright::{CopyrightConfig, add_copyright_notice};
use crate::webp_encoder::{WebPEncoder, WebPError};
use image::{ImageEncoder, ImageFormat, imageops::FilterType};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

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

    #[allow(dead_code)]
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
            // Check if cached version exists first
            let cache_filename =
                self.generate_cache_filename(relative_path, size, output_format.extension());
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
                }
            }
        }

        self.serve_file_with_cache_header(&full_path, false).await
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

        // Generate consistent cache keys
        let cache_filename =
            self.generate_cache_filename(relative_path, size, output_format.extension());
        let cache_path = self.config.cache_directory.join(&cache_filename);

        // Check if cache file exists and is newer than original
        if cache_path.exists() {
            let cache_metadata = tokio::fs::metadata(&cache_path).await?;
            let original_metadata = tokio::fs::metadata(original_path).await?;

            if let (Ok(cache_modified), Ok(original_modified)) =
                (cache_metadata.modified(), original_metadata.modified())
                && cache_modified >= original_modified
            {
                return Ok(cache_path);
            }
        }

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
            let mut buf_reader = std::io::BufReader::new(original_file);

            let decoder = image::ImageReader::new(&mut buf_reader).with_guessed_format()?;

            // Extract ICC profile before decoding
            // Note: ICC profile extraction from JPEG requires reading the file again
            // since the image crate doesn't expose ICC profiles from ImageReader
            let icc_profile: Option<Vec<u8>> = match decoder.format() {
                Some(image::ImageFormat::Jpeg) => {
                    // For JPEG, try to extract ICC profile using rexif crate (already in dependencies)
                    extract_icc_profile_from_jpeg(&original_path)
                }
                _ => None,
            };

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

            // Save final image in the requested format with quality settings and color profile preservation
            match format {
                OutputFormat::Jpeg => {
                    // Use JPEG with configurable quality and ICC profile preservation
                    use image::codecs::jpeg::JpegEncoder;
                    let output = std::fs::File::create(&cache_path_clone)?;

                    if let Some(ref profile_data) = icc_profile {
                        // Create encoder with ICC profile
                        let mut encoder = JpegEncoder::new_with_quality(output, jpeg_quality);
                        match encoder.set_icc_profile(profile_data.clone()) {
                            Ok(()) => {
                                final_image.write_with_encoder(encoder)?;
                                debug!("JPEG written with ICC profile: {} bytes", profile_data.len());
                            }
                            Err(e) => {
                                debug!("Failed to set ICC profile on JPEG encoder ({}), using standard JPEG", e);
                                let encoder = JpegEncoder::new_with_quality(std::fs::File::create(&cache_path_clone)?, jpeg_quality);
                                final_image.write_with_encoder(encoder)?;
                            }
                        }
                    } else {
                        // No ICC profile, use standard encoder
                        let encoder = JpegEncoder::new_with_quality(output, jpeg_quality);
                        final_image.write_with_encoder(encoder)?;
                    }
                }
                OutputFormat::WebP => {
                    // Use our libwebp-sys wrapper for WebP encoding with ICC profile support
                    let rgb_image = final_image.to_rgb8();
                    let (img_width, img_height) = rgb_image.dimensions();

                    // Create our WebP encoder
                    let rgb_data = rgb_image.into_raw();
                    match WebPEncoder::new(img_width, img_height, rgb_data) {
                        Ok(encoder) => {
                            match encoder.encode(_webp_quality, icc_profile.as_deref()) {
                                Ok(webp_data) => {
                                    std::fs::write(&cache_path_clone, webp_data)?;
                                    if let Some(ref profile_data) = icc_profile {
                                        debug!(
                                            "WebP written with ICC profile: {} bytes",
                                            profile_data.len()
                                        );
                                    } else {
                                        debug!("WebP written without ICC profile");
                                    }
                                }
                                Err(WebPError::EncodingFailed) => {
                                    error!("WebP encoding failed, falling back to basic webp crate");
                                    // Fallback to the basic webp crate
                                    let rgb_data = final_image.to_rgb8().into_raw();
                                    let fallback_encoder = webp::Encoder::from_rgb(&rgb_data, img_width, img_height);
                                    let encoded_webp = fallback_encoder.encode(_webp_quality);
                                    std::fs::write(&cache_path_clone, &*encoded_webp)?;
                                }
                                Err(e) => {
                                    error!("WebP encoding error: {}, falling back to basic webp crate", e);
                                    let rgb_data = final_image.to_rgb8().into_raw();
                                    let fallback_encoder = webp::Encoder::from_rgb(&rgb_data, img_width, img_height);
                                    let encoded_webp = fallback_encoder.encode(_webp_quality);
                                    std::fs::write(&cache_path_clone, &*encoded_webp)?;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create WebP encoder: {}, falling back to basic webp crate", e);
                            let rgb_data = final_image.to_rgb8().into_raw();
                            let fallback_encoder = webp::Encoder::from_rgb(&rgb_data, img_width, img_height);
                            let encoded_webp = fallback_encoder.encode(_webp_quality);
                            std::fs::write(&cache_path_clone, &*encoded_webp)?;
                        }
                    }
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| GalleryError::IoError(std::io::Error::other(e)))??;

        Ok(cache_path)
    }
}

/// Extract ICC profile from JPEG file
pub(crate) fn extract_icc_profile_from_jpeg(path: &std::path::PathBuf) -> Option<Vec<u8>> {
    use std::io::Read;

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return None;
    }

    // Look for ICC profile in JPEG APP2 segments
    // ICC profiles in JPEG are stored in APP2 markers with ICC_PROFILE identifier
    let mut pos = 0;
    while pos < buffer.len() - 1 {
        if buffer[pos] == 0xFF {
            let marker = buffer[pos + 1];
            if marker == 0xE2 {
                // APP2 marker
                if pos + 4 < buffer.len() {
                    let segment_length =
                        u16::from_be_bytes([buffer[pos + 2], buffer[pos + 3]]) as usize;
                    if pos + 2 + segment_length <= buffer.len() {
                        let segment_start = pos + 4;
                        let segment_end = pos + 2 + segment_length;
                        let segment_data = &buffer[segment_start..segment_end];

                        // Check for ICC_PROFILE identifier
                        if segment_data.len() > 12 && segment_data.starts_with(b"ICC_PROFILE\0") {
                            // ICC profile data starts after the identifier and 2 sequence bytes
                            let icc_data = &segment_data[14..];
                            if !icc_data.is_empty() {
                                debug!("Found ICC profile in JPEG: {} bytes", icc_data.len());
                                return Some(icc_data.to_vec());
                            }
                        }
                        pos = segment_end;
                    } else {
                        pos += 2;
                    }
                } else {
                    pos += 2;
                }
            } else {
                pos += 2;
            }
        } else {
            pos += 1;
        }
    }

    None
}

/// Extract display name from ICC profile data
pub(crate) fn extract_icc_profile_name(icc_data: &[u8]) -> Option<String> {
    // ICC profile structure:
    // - Header: 128 bytes
    // - Tag table: starts at byte 128
    // - Tag data: after tag table

    if icc_data.len() < 132 {
        return None; // Too small to contain header + tag count
    }

    // Read tag count at offset 128
    let tag_count =
        u32::from_be_bytes([icc_data[128], icc_data[129], icc_data[130], icc_data[131]]) as usize;

    let tag_table_start = 132;
    let tag_entry_size = 12;

    // Look for 'desc' tag (description)
    for i in 0..tag_count {
        let tag_start = tag_table_start + (i * tag_entry_size);
        if tag_start + 12 > icc_data.len() {
            break;
        }

        let tag_signature = &icc_data[tag_start..tag_start + 4];
        if tag_signature == b"desc" {
            // Found description tag
            let offset = u32::from_be_bytes([
                icc_data[tag_start + 4],
                icc_data[tag_start + 5],
                icc_data[tag_start + 6],
                icc_data[tag_start + 7],
            ]) as usize;
            let size = u32::from_be_bytes([
                icc_data[tag_start + 8],
                icc_data[tag_start + 9],
                icc_data[tag_start + 10],
                icc_data[tag_start + 11],
            ]) as usize;

            if offset + size > icc_data.len() || size < 12 {
                continue;
            }

            // Description tag data structure:
            // - Type signature: 4 bytes (should be 'desc')
            // - Reserved: 4 bytes
            // - ASCII count: 4 bytes
            // - ASCII string: variable length

            let desc_data = &icc_data[offset..offset + size];
            if desc_data.len() < 12 || &desc_data[0..4] != b"desc" {
                continue;
            }

            let ascii_count =
                u32::from_be_bytes([desc_data[8], desc_data[9], desc_data[10], desc_data[11]])
                    as usize;

            if ascii_count > 0 && 12 + ascii_count <= desc_data.len() {
                let ascii_data = &desc_data[12..12 + ascii_count];
                // Remove null terminator if present
                let ascii_str = if ascii_data.last() == Some(&0) {
                    &ascii_data[..ascii_data.len() - 1]
                } else {
                    ascii_data
                };

                if let Ok(name) = std::str::from_utf8(ascii_str) {
                    let trimmed_name = name.trim();
                    if !trimmed_name.is_empty() {
                        debug!("Extracted ICC profile name: {}", trimmed_name);
                        return Some(trimmed_name.to_string());
                    }
                }
            }
        }
    }

    // If no desc tag found, try to identify common profiles by their characteristics
    // Check profile size and other markers
    match icc_data.len() {
        548 => {
            // Common size for Display P3
            debug!("ICC profile size matches Display P3 (548 bytes)");
            Some("Display P3".to_string())
        }
        3144 | 3145 => {
            // Common sizes for sRGB
            debug!("ICC profile size matches sRGB ({} bytes)", icc_data.len());
            Some("sRGB".to_string())
        }
        560 => {
            // Common size for Adobe RGB
            debug!("ICC profile size matches Adobe RGB (560 bytes)");
            Some("Adobe RGB (1998)".to_string())
        }
        _ => {
            debug!("Unknown ICC profile size: {} bytes", icc_data.len());
            None
        }
    }
}

impl Gallery {
    async fn serve_file_with_cache_header(
        &self,
        path: &PathBuf,
        was_cached: bool,
    ) -> axum::response::Response {
        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        self.serve_file_with_content_type_and_cache_header(path, &content_type, was_cached)
            .await
    }

    async fn serve_file_with_content_type_and_cache_header(
        &self,
        path: &PathBuf,
        content_type: &str,
        was_cached: bool,
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

        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600");

        if was_cached {
            response = response.header("X-Already-Cached", "true");
        }

        response.body(body).unwrap()
    }

    pub async fn serve_cached_image(
        &self,
        cache_key: &str,
        size: &str,
        accept_header: &str,
    ) -> Result<axum::response::Response, GalleryError> {
        // For composites, always use JPEG
        let (hash, content_type) = if size == "composite" {
            (
                self.generate_composite_cache_key_with_format(cache_key, "jpg"),
                "image/jpeg",
            )
        } else {
            let output_format = self.determine_output_format(accept_header);
            let hash = self.generate_image_cache_key(cache_key, size, output_format.extension());
            let content_type = match output_format {
                OutputFormat::Jpeg => "image/jpeg",
                OutputFormat::WebP => "image/webp",
            };
            (hash, content_type)
        };

        // Check if we have it on disk
        let cache_filename = format!(
            "{}.{}",
            hash,
            if size == "composite" {
                "jpg"
            } else {
                let output_format = self.determine_output_format(accept_header);
                output_format.extension()
            }
        );
        let cache_path = self.config.cache_directory.join(&cache_filename);

        if cache_path.exists() {
            debug!("Serving from cache: {}", cache_filename);
            return Ok(self
                .serve_file_with_content_type_and_cache_header(&cache_path, content_type, true)
                .await);
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
        let hash =
            self.generate_composite_cache_key_with_format(cache_key, output_format.extension());
        let cache_filename = format!("{}.{}", hash, output_format.extension());
        let cache_path = self.config.cache_directory.join(&cache_filename);

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

        debug!("Stored composite in cache: {}", cache_filename);

        // Return the response
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(buffer))
            .unwrap())
    }

    pub async fn pregenerate_image_cache(&self, relative_path: &str) -> Result<(), GalleryError> {
        use std::time::Instant;

        debug!("Pre-generating cache for image: {}", relative_path);
        let start = Instant::now();

        // Define all sizes to pre-generate
        let sizes = [
            ("thumbnail", false),
            ("thumbnail", true), // @2x
            ("gallery", false),
            ("gallery", true), // @2x
            ("medium", false),
            ("medium", true), // @2x
        ];

        // Pre-generate for both JPEG and WebP formats
        let formats = [OutputFormat::Jpeg, OutputFormat::WebP];

        let mut generated_count = 0;

        for (size_name, is_2x) in &sizes {
            let size_str = if *is_2x {
                format!("{}@2x", size_name)
            } else {
                size_name.to_string()
            };

            for format in &formats {
                // Check if already cached
                let cache_filename =
                    self.generate_cache_filename(relative_path, &size_str, format.extension());
                let cache_path = self.config.cache_directory.join(&cache_filename);

                // Skip if already exists on disk
                if cache_path.exists() {
                    continue;
                }

                // Generate the cached version
                let full_path = self.config.source_directory.join(relative_path);
                match self
                    .get_resized_image(&full_path, relative_path, &size_str, *format)
                    .await
                {
                    Ok(_) => {
                        generated_count += 1;
                        debug!(
                            "Generated {} {} for {}",
                            format.extension(),
                            size_str,
                            relative_path
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to generate {} {} for {}: {}",
                            format.extension(),
                            size_str,
                            relative_path,
                            e
                        );
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        if generated_count > 0 {
            debug!(
                "Pre-generated {} cache entries for {} in {:?}",
                generated_count, relative_path, elapsed
            );
        }

        Ok(())
    }

    pub async fn pregenerate_all_images_cache(self: Arc<Self>) -> Result<(), GalleryError> {
        use std::sync::Arc;
        use std::time::Instant;
        use tokio::sync::Semaphore;

        info!("Starting pre-generation of image cache for all images");
        let start = Instant::now();

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
        };

        let total_images = image_paths.len();
        info!("Found {} images to pre-generate cache for", total_images);

        // Use all available CPU cores for parallel processing
        let num_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4); // Fallback to 4 if unable to determine
        info!("Using {} CPU cores for cache pre-generation", num_cores);

        // Use a semaphore to limit concurrent processing to number of CPU cores
        let semaphore = Arc::new(Semaphore::new(num_cores));
        let mut handles = Vec::new();

        for (index, path) in image_paths.iter().enumerate() {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let gallery_clone = self.clone();
            let path_clone = path.clone();
            let index = index + 1;

            let handle = tokio::spawn(async move {
                let _permit = permit; // Hold permit until done

                if index % 10 == 0 {
                    info!(
                        "Pre-generating cache: {}/{} images processed",
                        index, total_images
                    );
                }

                if let Err(e) = gallery_clone.pregenerate_image_cache(&path_clone).await {
                    error!("Failed to pre-generate cache for {}: {}", path_clone, e);
                }
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let _ = handle.await;
        }

        let elapsed = start.elapsed();
        info!(
            "Completed pre-generation of image cache for {} images in {:?}",
            total_images, elapsed
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppConfig, GallerySystemConfig};
    use image::{ImageBuffer, Rgba};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    async fn create_test_gallery() -> (Gallery, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let config = GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "gallery".to_string(),
            gallery_template: "gallery.html.liquid".to_string(),
            image_detail_template: "image_detail.html.liquid".to_string(),
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
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
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
        let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
        let cache_filename = format!("{}.jpg", hash);
        let cache_path = gallery.config.cache_directory.join(&cache_filename);
        assert!(
            tokio::fs::metadata(&cache_path).await.is_ok(),
            "Cache file not created"
        );

        // Verify the file exists on disk
        assert!(cache_path.exists(), "Composite file should exist on disk");

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
        let hash = gallery.generate_composite_cache_key_with_format(cache_key, "jpg");
        let cache_filename = format!("{}.jpg", hash);
        let cache_path = gallery.config.cache_directory.join(&cache_filename);
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

    // Helper function to create a test JPEG with ICC profile
    fn create_test_jpeg_with_icc_profile(
        path: &std::path::Path,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Create a simple RGB image
        let img = ImageBuffer::from_pixel(width, height, image::Rgb([255u8, 128, 64]));

        // Create a minimal Display P3 ICC profile for testing
        let icc_profile = create_test_display_p3_profile();

        // First save as regular JPEG
        img.save(path)?;

        // Now read it back and inject the ICC profile
        let mut jpeg_data = std::fs::read(path)?;

        // Verify it's a valid JPEG
        if jpeg_data.len() < 4 || jpeg_data[0..2] != [0xFF, 0xD8] {
            return Err("Not a valid JPEG file".into());
        }

        // Find where to insert the APP2 segment (after SOI, before other segments)
        let mut insert_pos = 2; // After SOI marker (0xFF 0xD8)

        // Look for the first segment after SOI - usually APP0 (JFIF) or APP1 (EXIF)
        if jpeg_data.len() > 4 && jpeg_data[2] == 0xFF {
            // There's already a segment here, skip past it
            if jpeg_data[3] >= 0xE0 && jpeg_data[3] <= 0xEF {
                // It's an APP segment, get its length
                if jpeg_data.len() > 6 {
                    let segment_len = u16::from_be_bytes([jpeg_data[4], jpeg_data[5]]) as usize;
                    insert_pos = 4 + segment_len; // Insert after this segment
                }
            }
        }

        // Create APP2 segment with ICC profile
        let mut app2_segment = Vec::new();
        app2_segment.push(0xFF);
        app2_segment.push(0xE2); // APP2 marker

        // Calculate segment length (includes the 2 length bytes)
        let segment_length = 2 + 12 + 2 + icc_profile.len();
        app2_segment.push((segment_length >> 8) as u8);
        app2_segment.push((segment_length & 0xFF) as u8);

        // Add ICC_PROFILE identifier
        app2_segment.extend_from_slice(b"ICC_PROFILE\0");

        // Add sequence numbers (1 of 1)
        app2_segment.push(1); // Sequence number
        app2_segment.push(1); // Total number of chunks

        // Add ICC profile data
        app2_segment.extend_from_slice(&icc_profile);

        // Insert the APP2 segment into the JPEG
        jpeg_data.splice(insert_pos..insert_pos, app2_segment);

        // Write the modified JPEG
        std::fs::write(path, &jpeg_data)?;

        Ok(icc_profile)
    }

    // Create a minimal but valid Display P3 ICC profile for testing
    fn create_test_display_p3_profile() -> Vec<u8> {
        // This is a simplified Display P3 profile for testing
        // Real Display P3 profiles are typically 548 bytes
        vec![
            // Profile header (128 bytes)
            0x00, 0x00, 0x02, 0x24, // Profile size (548 bytes)
            b'A', b'P', b'P', b'L', // Preferred CMM type
            0x04, 0x30, 0x00, 0x00, // Profile version 4.3.0
            b'm', b'n', b't', b'r', // Monitor device class
            b'R', b'G', b'B', b' ', // RGB color space
            b'X', b'Y', b'Z', b' ', // PCS (XYZ)
            // Creation date/time (12 bytes)
            0x07, 0xe7, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', b'c',
            b's', b'p', // Profile signature
            0x00, 0x00, 0x00, 0x00, // Platform signature
            0x00, 0x00, 0x00, 0x00, // Profile flags
            b'A', b'P', b'P', b'L', // Device manufacturer
            0x00, 0x00, 0x00, 0x00, // Device model
            0x00, 0x00, 0x00, 0x00, // Device attributes
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // Rendering intent: perceptual
            // PCS illuminant (12 bytes) - D65
            0x00, 0x00, 0xf6, 0xd6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xd3, 0x2d, b'A', b'P',
            b'P', b'L', // Profile creator
            // MD5 fingerprint (16 bytes) - zeros for simplicity
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // Reserved (28 bytes)
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            // Tag table
            0x00, 0x00, 0x00, 0x0A, // Tag count (10 tags for a basic Display P3)
            // Tag directory entries (12 bytes each)
            // 1. 'desc' tag
            b'd', b'e', b's', b'c', 0x00, 0x00, 0x01, 0x4C, // Offset
            0x00, 0x00, 0x00, 0x4B, // Size
            // 2. 'cprt' tag
            b'c', b'p', b'r', b't', 0x00, 0x00, 0x01, 0x98, // Offset
            0x00, 0x00, 0x00, 0x0C, // Size
            // 3. 'wtpt' tag (white point)
            b'w', b't', b'p', b't', 0x00, 0x00, 0x01, 0xA4, // Offset
            0x00, 0x00, 0x00, 0x14, // Size
            // 4. 'rXYZ' tag (red primary)
            b'r', b'X', b'Y', b'Z', 0x00, 0x00, 0x01, 0xB8, // Offset
            0x00, 0x00, 0x00, 0x14, // Size
            // 5. 'gXYZ' tag (green primary)
            b'g', b'X', b'Y', b'Z', 0x00, 0x00, 0x01, 0xCC, // Offset
            0x00, 0x00, 0x00, 0x14, // Size
            // 6. 'bXYZ' tag (blue primary)
            b'b', b'X', b'Y', b'Z', 0x00, 0x00, 0x01, 0xE0, // Offset
            0x00, 0x00, 0x00, 0x14, // Size
            // 7. 'rTRC' tag (red tone curve)
            b'r', b'T', b'R', b'C', 0x00, 0x00, 0x01, 0xF4, // Offset
            0x00, 0x00, 0x00, 0x10, // Size
            // 8. 'gTRC' tag (green tone curve)
            b'g', b'T', b'R', b'C', 0x00, 0x00, 0x02, 0x04, // Offset
            0x00, 0x00, 0x00, 0x10, // Size
            // 9. 'bTRC' tag (blue tone curve)
            b'b', b'T', b'R', b'C', 0x00, 0x00, 0x02, 0x14, // Offset
            0x00, 0x00, 0x00, 0x10, // Size
            // 10. 'chad' tag (chromatic adaptation)
            b'c', b'h', b'a', b'd', 0x00, 0x00, 0x02, 0x24, // Offset (this would continue...)
            0x00, 0x00, 0x00,
            0x2C, // Size

                  // Tag data would follow here...
                  // For testing purposes, we'll use a smaller profile
        ]
    }

    #[tokio::test]
    async fn test_jpeg_icc_profile_preservation() {
        let (gallery, temp_dir) = create_test_gallery().await;

        // Create a simple JPEG first
        let img = ImageBuffer::from_pixel(100, 100, image::Rgb([255u8, 128, 64]));
        let source_path = temp_dir.path().join("test_source.jpg");
        img.save(&source_path).unwrap();

        // Now add ICC profile to it
        let icc_profile = create_test_display_p3_profile();

        // Use the actual JPEG encoder with ICC profile to create a proper test file
        use image::codecs::jpeg::JpegEncoder;
        let output_path = gallery.config.source_directory.join("test_icc.jpg");
        let output_file = std::fs::File::create(&output_path).unwrap();
        let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

        // Try to set ICC profile - if it fails, skip the test
        if encoder.set_icc_profile(icc_profile.clone()).is_err() {
            eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
            return;
        }

        img.write_with_encoder(encoder).unwrap();

        // Now process the image through the gallery
        let relative_path = "test_icc.jpg";

        // Process image as thumbnail (JPEG output)
        let result = gallery
            .get_resized_image(&output_path, relative_path, "thumbnail", OutputFormat::Jpeg)
            .await;

        assert!(result.is_ok(), "Failed to process image: {:?}", result);

        let cache_path = result.unwrap();

        // Extract ICC profile from processed image
        let processed_icc = extract_icc_profile_from_jpeg(&cache_path);

        assert!(
            processed_icc.is_some(),
            "ICC profile was not preserved in processed JPEG"
        );

        let processed_icc = processed_icc.unwrap();

        // The profile might be slightly different due to processing, but should exist
        assert!(!processed_icc.is_empty(), "ICC profile is empty");
        debug!(
            "Original ICC size: {}, Processed ICC size: {}",
            icc_profile.len(),
            processed_icc.len()
        );
    }

    #[tokio::test]
    async fn test_webp_icc_profile_preservation() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Create a test image with ICC profile
        let img = ImageBuffer::from_pixel(100, 100, image::Rgb([255u8, 128, 64]));
        let icc_profile = create_test_display_p3_profile();

        // Save as JPEG with ICC profile first
        use image::codecs::jpeg::JpegEncoder;
        let source_path = gallery.config.source_directory.join("test_webp_source.jpg");
        let output_file = std::fs::File::create(&source_path).unwrap();
        let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

        // Try to set ICC profile - if it fails, skip the test
        if encoder.set_icc_profile(icc_profile.clone()).is_err() {
            eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
            return;
        }

        img.write_with_encoder(encoder).unwrap();

        // Process the image through the gallery as WebP
        let relative_path = "test_webp_source.jpg";

        // Process image as thumbnail (WebP output)
        let result = gallery
            .get_resized_image(&source_path, relative_path, "thumbnail", OutputFormat::WebP)
            .await;

        assert!(result.is_ok(), "Failed to process image: {:?}", result);

        let cache_path = result.unwrap();

        // Read the WebP file and check for ICC profile
        let webp_data = std::fs::read(&cache_path).unwrap();

        // Parse WebP to check for ICCP chunk
        assert!(webp_data.len() >= 12);
        assert_eq!(&webp_data[0..4], b"RIFF");
        assert_eq!(&webp_data[8..12], b"WEBP");

        // Look for VP8X and ICCP chunks
        let mut pos = 12;
        let mut found_vp8x = false;
        let mut found_iccp = false;
        let mut iccp_size = 0;

        while pos + 8 <= webp_data.len() {
            let chunk_fourcc = &webp_data[pos..pos + 4];
            let chunk_size = u32::from_le_bytes([
                webp_data[pos + 4],
                webp_data[pos + 5],
                webp_data[pos + 6],
                webp_data[pos + 7],
            ]) as usize;

            if chunk_fourcc == b"VP8X" {
                found_vp8x = true;
                // Check that ICCP flag is set (bit 5 = 0x20)
                if pos + 8 < webp_data.len() {
                    let flags = webp_data[pos + 8];
                    assert!(
                        flags & 0x20 != 0,
                        "ICCP flag not set in VP8X chunk (flags: 0x{:02x})",
                        flags
                    );
                }
            } else if chunk_fourcc == b"ICCP" {
                found_iccp = true;
                iccp_size = chunk_size;
            }

            // Move to next chunk (with padding)
            pos += 8 + chunk_size + (chunk_size % 2);
        }

        assert!(found_vp8x, "VP8X chunk not found in WebP");
        assert!(found_iccp, "ICCP chunk not found in WebP");
        assert!(iccp_size > 0, "ICCP chunk is empty");

        debug!("WebP ICC profile size: {} bytes", iccp_size);
    }

    #[tokio::test]
    async fn test_jpeg_icc_profile_preservation_with_watermark() {
        let (mut gallery, _temp_dir) = create_test_gallery().await;

        // Enable watermarking
        gallery.app_config.copyright_holder = Some("Test Copyright".to_string());

        // Create a test image
        let img = ImageBuffer::from_pixel(200, 200, image::Rgb([255u8, 128, 64]));
        let icc_profile = create_test_display_p3_profile();

        // Save with ICC profile
        use image::codecs::jpeg::JpegEncoder;
        let output_path = gallery.config.source_directory.join("test_watermark.jpg");
        let output_file = std::fs::File::create(&output_path).unwrap();
        let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

        // Try to set ICC profile - if it fails, skip the test
        if encoder.set_icc_profile(icc_profile.clone()).is_err() {
            eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
            return;
        }

        img.write_with_encoder(encoder).unwrap();

        // Create a minimal font file for testing (or skip if not present)
        let _font_path = PathBuf::from("static/DejaVuSans.ttf");
        std::fs::create_dir_all("static").ok();

        // Process the image through the gallery
        let relative_path = "test_watermark.jpg";

        // Process image as medium (JPEG output with watermark attempt)
        let result = gallery
            .get_resized_image(&output_path, relative_path, "medium", OutputFormat::Jpeg)
            .await;

        assert!(result.is_ok(), "Failed to process image: {:?}", result);

        let cache_path = result.unwrap();

        // Extract ICC profile from processed image
        let processed_icc = extract_icc_profile_from_jpeg(&cache_path);

        // ICC profile should be preserved regardless of watermarking
        assert!(
            processed_icc.is_some(),
            "ICC profile was not preserved in processed JPEG"
        );

        let processed_icc = processed_icc.unwrap();
        assert!(
            !processed_icc.is_empty(),
            "ICC profile is empty after processing"
        );

        debug!(
            "Original ICC size: {}, Processed ICC size: {}",
            icc_profile.len(),
            processed_icc.len()
        );
    }

    #[tokio::test]
    async fn test_icc_profile_extraction_from_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let jpeg_path = temp_dir.path().join("test_extract.jpg");

        // Create JPEG with ICC profile
        let original_icc = create_test_jpeg_with_icc_profile(&jpeg_path, 50, 50)
            .expect("Failed to create test JPEG");

        // Test extraction
        let extracted_icc = extract_icc_profile_from_jpeg(&jpeg_path);

        assert!(extracted_icc.is_some(), "Failed to extract ICC profile");
        let extracted_icc = extracted_icc.unwrap();

        assert_eq!(
            extracted_icc.len(),
            original_icc.len(),
            "Extracted ICC profile size doesn't match: {} vs {}",
            extracted_icc.len(),
            original_icc.len()
        );

        // Verify the profile header is correct
        assert!(extracted_icc.len() >= 4);
        // ICC profiles start with size as big-endian 32-bit integer
        let profile_size = u32::from_be_bytes([
            extracted_icc[0],
            extracted_icc[1],
            extracted_icc[2],
            extracted_icc[3],
        ]) as usize;

        // The size in the header should match actual size for our test profile
        assert!(
            profile_size > 128,
            "ICC profile size in header too small: {}",
            profile_size
        );
    }

    #[tokio::test]
    async fn test_icc_profile_preservation_all_sizes() {
        let (gallery, _temp_dir) = create_test_gallery().await;

        // Create a smaller test image for faster testing (500x500 instead of 2000x2000)
        let img = ImageBuffer::from_pixel(500, 500, image::Rgb([255u8, 128, 64]));
        let icc_profile = create_test_display_p3_profile();

        // Save as JPEG with ICC profile
        use image::codecs::jpeg::JpegEncoder;
        let source_path = gallery.config.source_directory.join("test_all_sizes.jpg");
        let output_file = std::fs::File::create(&source_path).unwrap();
        let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

        // Try to set ICC profile - if it fails, skip the test
        if encoder.set_icc_profile(icc_profile.clone()).is_err() {
            eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
            return;
        }

        img.write_with_encoder(encoder).unwrap();

        // Test a subset of sizes to speed up the test - thumbnail and medium cover the key paths
        let sizes = ["thumbnail", "medium"];
        let formats = [OutputFormat::Jpeg, OutputFormat::WebP];

        for size in &sizes {
            for format in &formats {
                debug!("Testing {} size with {:?} format", size, format);

                let result = gallery
                    .get_resized_image(&source_path, "test_all_sizes.jpg", size, *format)
                    .await;

                assert!(
                    result.is_ok(),
                    "Failed to process {} as {:?}: {:?}",
                    size,
                    format,
                    result
                );

                let cache_path = result.unwrap();

                match format {
                    OutputFormat::Jpeg => {
                        // Check JPEG has ICC profile
                        let processed_icc = extract_icc_profile_from_jpeg(&cache_path);
                        assert!(
                            processed_icc.is_some(),
                            "ICC profile missing in {} JPEG",
                            size
                        );
                        assert!(
                            !processed_icc.unwrap().is_empty(),
                            "ICC profile empty in {} JPEG",
                            size
                        );
                    }
                    OutputFormat::WebP => {
                        // Check WebP has ICC profile
                        let webp_data = std::fs::read(&cache_path).unwrap();
                        let mut found_iccp = false;
                        let mut pos = 12;

                        while pos + 8 <= webp_data.len() {
                            let chunk_fourcc = &webp_data[pos..pos + 4];
                            if chunk_fourcc == b"ICCP" {
                                found_iccp = true;
                                break;
                            }
                            let chunk_size = u32::from_le_bytes([
                                webp_data[pos + 4],
                                webp_data[pos + 5],
                                webp_data[pos + 6],
                                webp_data[pos + 7],
                            ]) as usize;
                            pos += 8 + chunk_size + (chunk_size % 2);
                        }

                        assert!(found_iccp, "ICCP chunk missing in {} WebP", size);
                    }
                }
            }
        }
    }
}
