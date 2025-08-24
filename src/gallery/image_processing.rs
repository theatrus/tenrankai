use super::{Gallery, GalleryError};
use crate::copyright::{CopyrightConfig, add_copyright_notice};
use crate::webp_encoder::{WebPEncoder, WebPError};
use image::{ImageFormat, ImageEncoder, imageops::FilterType};
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
                    Self::extract_icc_profile_from_jpeg(&original_path)
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

    /// Embed ICC profile in WebP file by converting to VP8X format
    #[allow(dead_code)]
    fn embed_icc_in_webp(webp_data: &[u8], icc_profile: &[u8], width: u32, height: u32) -> Result<Vec<u8>, GalleryError> {
        // WebP file format:
        // - RIFF header (12 bytes): "RIFF" + file_size + "WEBP"
        // - For extended WebP: VP8X chunk, then optional chunks (ICCP, etc.), then VP8/VP8L

        if webp_data.len() < 20 || &webp_data[0..4] != b"RIFF" || &webp_data[8..12] != b"WEBP" {
            return Err(GalleryError::ImageError(image::ImageError::Decoding(
                image::error::DecodingError::new(
                    image::ImageFormat::WebP.into(),
                    "Invalid WebP format",
                ),
            )));
        }

        // Check if this is already VP8X (extended) or VP8/VP8L (simple)
        let first_chunk = &webp_data[12..16];
        
        if first_chunk == b"VP8X" {
            // Already extended format - insert ICCP after VP8X chunk
            Self::insert_iccp_in_vp8x(webp_data, icc_profile)
        } else if first_chunk == b"VP8 " || first_chunk == b"VP8L" {
            // Simple format - convert to VP8X format with ICCP
            Self::convert_to_vp8x_with_iccp(webp_data, icc_profile, width, height)
        } else {
            Err(GalleryError::ImageError(image::ImageError::Decoding(
                image::error::DecodingError::new(
                    image::ImageFormat::WebP.into(),
                    "Unknown WebP chunk type",
                ),
            )))
        }
    }
    
    /// Convert simple WebP (VP8/VP8L) to VP8X format with ICCP chunk
    #[allow(dead_code)]
    fn convert_to_vp8x_with_iccp(webp_data: &[u8], icc_profile: &[u8], width: u32, height: u32) -> Result<Vec<u8>, GalleryError> {
        let mut result = Vec::new();
        
        // Copy RIFF header (12 bytes)
        result.extend_from_slice(&webp_data[0..12]);
        
        // Create VP8X chunk
        // VP8X chunk: "VP8X" + size(4) + flags(1) + reserved(3) + width(3) + height(3)
        result.extend_from_slice(b"VP8X");
        result.extend_from_slice(&10u32.to_le_bytes()); // VP8X chunk size (always 10)
        result.push(0x10); // flags: ICCP bit set (bit 5)
        result.extend_from_slice(&[0, 0, 0]); // reserved bytes
        
        // Use the provided dimensions
        let width_minus_1 = width - 1;
        let height_minus_1 = height - 1;
        
        // Width and height as 24-bit little-endian (minus 1)
        result.push((width_minus_1 & 0xFF) as u8);
        result.push(((width_minus_1 >> 8) & 0xFF) as u8);
        result.push(((width_minus_1 >> 16) & 0xFF) as u8);
        result.push((height_minus_1 & 0xFF) as u8);
        result.push(((height_minus_1 >> 8) & 0xFF) as u8);
        result.push(((height_minus_1 >> 16) & 0xFF) as u8);
        
        // Add ICCP chunk
        let iccp_chunk = Self::create_webp_iccp_chunk(icc_profile);
        result.extend_from_slice(&iccp_chunk);
        
        // Copy the original VP8/VP8L chunk
        result.extend_from_slice(&webp_data[12..]);
        
        // Update RIFF size
        let new_size = (result.len() - 8) as u32;
        result[4..8].copy_from_slice(&new_size.to_le_bytes());
        
        Ok(result)
    }
    
    /// Insert ICCP chunk in existing VP8X format
    #[allow(dead_code)]
    fn insert_iccp_in_vp8x(webp_data: &[u8], icc_profile: &[u8]) -> Result<Vec<u8>, GalleryError> {
        // For VP8X format, insert ICCP chunk after VP8X chunk but before VP8/VP8L
        let mut result = Vec::new();
        
        // Find the end of the VP8X chunk
        let vp8x_size = u32::from_le_bytes([webp_data[16], webp_data[17], webp_data[18], webp_data[19]]) as usize;
        let vp8x_end = 20 + vp8x_size + (vp8x_size % 2); // Include padding
        
        // Copy up to end of VP8X chunk
        result.extend_from_slice(&webp_data[0..vp8x_end]);
        
        // Update VP8X flags to include ICCP bit
        result[20] |= 0x10; // Set ICCP flag
        
        // Add ICCP chunk
        let iccp_chunk = Self::create_webp_iccp_chunk(icc_profile);
        result.extend_from_slice(&iccp_chunk);
        
        // Copy remaining chunks
        result.extend_from_slice(&webp_data[vp8x_end..]);
        
        // Update RIFF size
        let new_size = (result.len() - 8) as u32;
        result[4..8].copy_from_slice(&new_size.to_le_bytes());
        
        Ok(result)
    }
    

    /// Extract ICC profile from JPEG file
    fn extract_icc_profile_from_jpeg(path: &std::path::PathBuf) -> Option<Vec<u8>> {
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
                            if segment_data.len() > 12 && segment_data.starts_with(b"ICC_PROFILE\0")
                            {
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

    /// Create a WebP ICCP chunk containing the ICC profile
    #[allow(dead_code)]
    fn create_webp_iccp_chunk(icc_profile: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();

        // ICCP chunk header: "ICCP" + chunk_size
        chunk.extend_from_slice(b"ICCP");

        // Chunk size (excluding the 8-byte chunk header)
        let chunk_size = icc_profile.len() as u32;
        chunk.extend_from_slice(&chunk_size.to_le_bytes());

        // ICC profile data
        chunk.extend_from_slice(icc_profile);

        // Add padding if needed (WebP chunks must be even-sized)
        if chunk.len() % 2 == 1 {
            chunk.push(0);
        }

        chunk
    }

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

        // Use a semaphore to limit concurrent processing
        let semaphore = Arc::new(Semaphore::new(4)); // Process 4 images concurrently
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
}
