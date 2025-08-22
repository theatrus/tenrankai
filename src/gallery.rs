use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use image::{ImageFormat, imageops::FilterType};
use pulldown_cmark::{Parser, html};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tracing::error;
use walkdir::WalkDir;

use crate::{GalleryConfig, static_files::StaticFileHandler, templating::TemplateEngine};

#[derive(Debug, Clone, Serialize)]
pub struct GalleryItem {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub parent_path: Option<String>,
    pub is_directory: bool,
    pub thumbnail_url: Option<String>,
    pub preview_images: Option<Vec<String>>,
    pub item_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub name: String,
    pub path: String,
    pub url: String,
    pub thumbnail_url: String,
    pub gallery_url: String,
    pub medium_url: String,
    pub description: Option<String>,
    pub camera_info: Option<CameraInfo>,
    pub location_info: Option<LocationInfo>,
    pub file_size: u64,
    pub dimensions: (u32, u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct CameraInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<String>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocationInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub google_maps_url: String,
    pub apple_maps_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationImage {
    pub path: String,
    pub name: String,
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryQuery {
    pub page: Option<usize>,
    pub size: Option<String>,
}

pub struct Gallery {
    config: GalleryConfig,
    cache: Arc<RwLock<HashMap<String, CachedImage>>>,
}

struct CachedImage {
    path: PathBuf,
    modified: SystemTime,
}

impl Gallery {
    pub fn new(config: GalleryConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn scan_directory(&self, relative_path: &str) -> Result<Vec<GalleryItem>, String> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err("Invalid path".to_string());
        }

        let mut items = Vec::new();

        let entries = tokio::fs::read_dir(&full_path)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let file_name = entry.file_name().to_string_lossy().to_string();

            if file_name.starts_with('.') || file_name.ends_with(".md") {
                continue;
            }

            let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
            let is_directory = metadata.is_dir();

            let item_path = if relative_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", relative_path, file_name)
            };

            if is_directory {
                let item_count = self.count_images_in_directory(&item_path).await;
                let preview_images = self.get_directory_preview_images(&item_path).await;
                let (display_name, description) = self.read_folder_metadata(&item_path).await;
                items.push(GalleryItem {
                    name: file_name,
                    display_name,
                    description,
                    path: item_path,
                    parent_path: Some(relative_path.to_string()),
                    is_directory: true,
                    thumbnail_url: None,
                    preview_images: Some(preview_images),
                    item_count: Some(item_count),
                });
            } else if self.is_image(&file_name) {
                let thumbnail_url = format!(
                    "/{}/image/{}?size=gallery",
                    self.config.path_prefix,
                    urlencoding::encode(&item_path)
                );

                items.push(GalleryItem {
                    name: file_name,
                    display_name: None,
                    description: None,
                    path: item_path,
                    parent_path: Some(relative_path.to_string()),
                    is_directory: false,
                    thumbnail_url: Some(thumbnail_url),
                    preview_images: None,
                    item_count: None,
                });
            }
        }

        items.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(items)
    }

    async fn count_images_in_directory(&self, relative_path: &str) -> usize {
        let full_path = self.config.source_directory.join(relative_path);
        let mut count = 0;

        for entry in WalkDir::new(full_path).min_depth(1) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if self.is_image(name) && !name.starts_with('.') {
                            count += 1;
                        }
                    }
                }
            }
        }

        count
    }

    async fn get_directory_preview_images(&self, relative_path: &str) -> Vec<String> {
        let full_path = self.config.source_directory.join(relative_path);
        let mut preview_images = Vec::new();

        // Get up to 4 images for preview
        const MAX_PREVIEW_IMAGES: usize = 4;

        for entry in WalkDir::new(&full_path).min_depth(1).max_depth(3) {
            if preview_images.len() >= MAX_PREVIEW_IMAGES {
                break;
            }

            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if self.is_image(name) && !name.starts_with('.') {
                            // Calculate relative path from gallery source directory
                            if let Ok(relative_to_source) =
                                entry.path().strip_prefix(&self.config.source_directory)
                            {
                                let path_string = relative_to_source.to_string_lossy().to_string();
                                let encoded_path = urlencoding::encode(&path_string);
                                let thumbnail_url = format!(
                                    "/{}/image/{}?size=thumbnail",
                                    self.config.path_prefix, encoded_path
                                );
                                preview_images.push(thumbnail_url);
                            }
                        }
                    }
                }
            }
        }

        preview_images
    }

    fn is_image(&self, filename: &str) -> bool {
        let lower = filename.to_lowercase();
        lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".bmp")
    }

    pub async fn get_images_for_page(
        &self,
        relative_path: &str,
        page: usize,
    ) -> Result<Vec<ImageInfo>, String> {
        let items = self.scan_directory(relative_path).await?;
        let images: Vec<_> = items
            .into_iter()
            .filter(|item| !item.is_directory)
            .collect();

        let start = page * self.config.images_per_page;
        let end = ((page + 1) * self.config.images_per_page).min(images.len());

        let mut image_infos = Vec::new();

        for item in &images[start..end] {
            let image_info = self.get_image_info(&item.path).await?;
            image_infos.push(image_info);
        }

        Ok(image_infos)
    }

    pub async fn get_image_info_with_navigation(
        &self,
        relative_path: &str,
    ) -> Result<(ImageInfo, Option<NavigationImage>, Option<NavigationImage>), String> {
        let image_info = self.get_image_info(relative_path).await?;

        // Get the directory containing this image
        let path = StdPath::new(relative_path);
        let parent_dir = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Get all images in the same directory
        let items = self.scan_directory(&parent_dir).await?;
        let images: Vec<_> = items
            .into_iter()
            .filter(|item| !item.is_directory)
            .collect();

        // Find current image index
        let current_index = images.iter().position(|item| item.path == relative_path);

        let (prev_image, next_image) = if let Some(index) = current_index {
            let prev = if index > 0 {
                let prev_item = &images[index - 1];
                Some(NavigationImage {
                    path: prev_item.path.clone(),
                    name: prev_item.name.clone(),
                    thumbnail_url: prev_item.thumbnail_url.clone().unwrap_or_default(),
                })
            } else {
                None
            };

            let next = if index + 1 < images.len() {
                let next_item = &images[index + 1];
                Some(NavigationImage {
                    path: next_item.path.clone(),
                    name: next_item.name.clone(),
                    thumbnail_url: next_item.thumbnail_url.clone().unwrap_or_default(),
                })
            } else {
                None
            };

            (prev, next)
        } else {
            (None, None)
        };

        Ok((image_info, prev_image, next_image))
    }

    pub async fn get_image_info(&self, relative_path: &str) -> Result<ImageInfo, String> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err("Invalid path".to_string());
        }

        let metadata = tokio::fs::metadata(&full_path)
            .await
            .map_err(|e| format!("Failed to read file metadata: {}", e))?;

        let file_size = metadata.len();

        let img = image::open(&full_path).map_err(|e| format!("Failed to open image: {}", e))?;

        let dimensions = (img.width(), img.height());

        let description = self.read_sidecar_markdown(relative_path).await;
        let (camera_info, location_info) = self.extract_structured_exif_data(&full_path).await;

        let encoded_path = urlencoding::encode(relative_path);

        Ok(ImageInfo {
            name: StdPath::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: relative_path.to_string(),
            url: format!("/{}/image/{}", self.config.path_prefix, encoded_path),
            thumbnail_url: format!(
                "/{}/image/{}?size=thumbnail",
                self.config.path_prefix, encoded_path
            ),
            gallery_url: format!(
                "/{}/image/{}?size=gallery",
                self.config.path_prefix, encoded_path
            ),
            medium_url: format!(
                "/{}/image/{}?size=medium",
                self.config.path_prefix, encoded_path
            ),
            description,
            camera_info,
            location_info,
            file_size,
            dimensions,
        })
    }

    async fn read_folder_metadata(&self, folder_path: &str) -> (Option<String>, Option<String>) {
        let folder_md_path = self
            .config
            .source_directory
            .join(folder_path)
            .join("_folder.md");

        match tokio::fs::read_to_string(&folder_md_path).await {
            Ok(content) => {
                let mut title: Option<String> = None;
                let mut description_content = String::new();
                let mut found_title = false;

                for line in content.lines() {
                    let trimmed = line.trim();

                    // Look for the first title (# heading)
                    if !found_title && trimmed.starts_with("# ") {
                        title = Some(trimmed.trim_start_matches("# ").to_string());
                        found_title = true;
                        continue;
                    }

                    // Skip empty lines immediately after title
                    if found_title && trimmed.is_empty() && description_content.is_empty() {
                        continue;
                    }

                    // Collect description content (everything after the title)
                    if found_title {
                        if !description_content.is_empty() {
                            description_content.push('\n');
                        }
                        description_content.push_str(line);
                    }
                }

                // Convert description markdown to HTML if there's content
                let description = if description_content.trim().is_empty() {
                    None
                } else {
                    let parser = Parser::new(&description_content);
                    let mut html_output = String::new();
                    html::push_html(&mut html_output, parser);
                    Some(html_output)
                };

                (title, description)
            }
            Err(_) => (None, None),
        }
    }

    async fn read_sidecar_markdown(&self, image_path: &str) -> Option<String> {
        let path = StdPath::new(image_path);
        let stem = path.file_stem()?;
        let parent = path.parent()?;

        let md_filename = format!("{}.md", stem.to_str()?);
        let md_path = self.config.source_directory.join(parent).join(md_filename);

        match tokio::fs::read_to_string(&md_path).await {
            Ok(content) => {
                let parser = Parser::new(&content);
                let mut html_output = String::new();
                html::push_html(&mut html_output, parser);
                Some(html_output)
            }
            Err(_) => None,
        }
    }

    async fn extract_structured_exif_data(
        &self,
        path: &PathBuf,
    ) -> (Option<CameraInfo>, Option<LocationInfo>) {
        let file_contents = match std::fs::read(path) {
            Ok(contents) => contents,
            Err(_) => return (None, None),
        };

        let exif_data = match rexif::parse_buffer(&file_contents) {
            Ok(data) => data,
            Err(_) => return (None, None),
        };

        let mut camera_info = CameraInfo {
            camera_make: None,
            camera_model: None,
            lens_model: None,
            iso: None,
            aperture: None,
            shutter_speed: None,
            focal_length: None,
        };

        let mut latitude: Option<f64> = None;
        let mut longitude: Option<f64> = None;
        let mut lat_ref: Option<String> = None;
        let mut lon_ref: Option<String> = None;

        for entry in &exif_data.entries {
            match entry.tag {
                rexif::ExifTag::Make => {
                    camera_info.camera_make = Some(entry.value_more_readable.to_string());
                }
                rexif::ExifTag::Model => {
                    camera_info.camera_model = Some(entry.value_more_readable.to_string());
                }
                rexif::ExifTag::LensModel => {
                    camera_info.lens_model = Some(entry.value_more_readable.to_string());
                }
                rexif::ExifTag::ISOSpeedRatings => {
                    camera_info.iso = Some(format!("ISO {}", entry.value_more_readable));
                }
                rexif::ExifTag::FNumber => {
                    camera_info.aperture = Some(format!("f/{}", entry.value_more_readable));
                }
                rexif::ExifTag::ExposureTime => {
                    camera_info.shutter_speed = Some(format!("{}s", entry.value_more_readable));
                }
                rexif::ExifTag::FocalLength => {
                    camera_info.focal_length = Some(format!("{}mm", entry.value_more_readable));
                }
                rexif::ExifTag::GPSLatitude => {
                    if let Ok(lat) = self.parse_gps_coordinate(&entry.value_more_readable) {
                        latitude = Some(lat);
                    }
                }
                rexif::ExifTag::GPSLongitude => {
                    if let Ok(lon) = self.parse_gps_coordinate(&entry.value_more_readable) {
                        longitude = Some(lon);
                    }
                }
                rexif::ExifTag::GPSLatitudeRef => {
                    lat_ref = Some(entry.value_more_readable.to_string());
                }
                rexif::ExifTag::GPSLongitudeRef => {
                    lon_ref = Some(entry.value_more_readable.to_string());
                }
                _ => {}
            }
        }

        // Clean up camera info
        if camera_info.camera_make.is_none()
            && camera_info.camera_model.is_none()
            && camera_info.lens_model.is_none()
            && camera_info.iso.is_none()
            && camera_info.aperture.is_none()
            && camera_info.shutter_speed.is_none()
            && camera_info.focal_length.is_none()
        {
            camera_info = CameraInfo {
                camera_make: None,
                camera_model: None,
                lens_model: None,
                iso: None,
                aperture: None,
                shutter_speed: None,
                focal_length: None,
            };
        }

        // Process GPS coordinates
        let location_info = if let (Some(mut lat), Some(mut lon)) = (latitude, longitude) {
            // Apply hemisphere references
            if let Some(ref lat_hemisphere) = lat_ref {
                if lat_hemisphere.to_uppercase().starts_with('S') {
                    lat = -lat;
                }
            }

            if let Some(ref lon_hemisphere) = lon_ref {
                if lon_hemisphere.to_uppercase().starts_with('W') {
                    lon = -lon;
                }
            }

            Some(LocationInfo {
                latitude: lat,
                longitude: lon,
                google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
                apple_maps_url: format!("http://maps.apple.com/?ll={},{}", lat, lon),
            })
        } else {
            None
        };

        let final_camera_info = if camera_info.camera_make.is_some()
            || camera_info.camera_model.is_some()
            || camera_info.lens_model.is_some()
            || camera_info.iso.is_some()
            || camera_info.aperture.is_some()
            || camera_info.shutter_speed.is_some()
            || camera_info.focal_length.is_some()
        {
            Some(camera_info)
        } else {
            None
        };

        (final_camera_info, location_info)
    }

    fn parse_gps_coordinate(&self, coord_str: &str) -> Result<f64, String> {
        // Parse GPS coordinate in format like "40° 45' 30.00''"
        let parts: Vec<&str> = coord_str.split_whitespace().collect();
        if parts.len() >= 3 {
            let degrees: f64 = parts[0]
                .trim_end_matches('°')
                .parse()
                .map_err(|_| "Invalid degrees")?;
            let minutes: f64 = parts[1]
                .trim_end_matches('\'')
                .parse()
                .map_err(|_| "Invalid minutes")?;
            let seconds: f64 = parts[2]
                .trim_end_matches("''")
                .trim_end_matches('\'')
                .parse()
                .map_err(|_| "Invalid seconds")?;

            Ok(degrees + minutes / 60.0 + seconds / 3600.0)
        } else {
            // Try parsing as decimal
            coord_str
                .parse::<f64>()
                .map_err(|_| "Invalid coordinate format".to_string())
        }
    }

    pub async fn serve_image(&self, relative_path: &str, size: Option<String>) -> Response {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }

        let size = size.as_deref();

        if let Some(size) = size {
            match self
                .get_resized_image(&full_path, relative_path, size)
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

    async fn get_resized_image(
        &self,
        original_path: &PathBuf,
        relative_path: &str,
        size: &str,
    ) -> Result<PathBuf, String> {
        let (width, height) = match size {
            "thumbnail" => (300, 300),
            "gallery" => (800, 800),
            "medium" => (1200, 1200),
            "large" => (1600, 1600),
            _ => return Err("Invalid size".to_string()),
        };

        let hash = self.generate_cache_key(relative_path, size);
        let cache_filename = format!("{}.jpg", hash);
        let cache_path = self.config.cache_directory.join(cache_filename);

        let original_metadata = tokio::fs::metadata(original_path)
            .await
            .map_err(|e| e.to_string())?;
        let original_modified = original_metadata.modified().map_err(|e| e.to_string())?;

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(&hash) {
            if cached.modified >= original_modified && cached.path.exists() {
                return Ok(cached.path.clone());
            }
        }
        drop(cache);

        tokio::fs::create_dir_all(&self.config.cache_directory)
            .await
            .map_err(|e| e.to_string())?;

        // Open image with decoder to access ICC profile
        let original_file = std::fs::File::open(original_path)
            .map_err(|e| format!("Failed to open original image: {}", e))?;

        let decoder = image::ImageReader::new(std::io::BufReader::new(original_file))
            .with_guessed_format()
            .map_err(|e| format!("Failed to create decoder: {}", e))?;

        let img = decoder
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let resized = img.resize(width, height, FilterType::Lanczos3);

        // Save resized image
        // Note: The standard image crate JPEG encoder doesn't support embedding ICC profiles
        // For production use, consider using a library like turbojpeg-sys or mozjpeg
        // that supports ICC profile embedding during encoding
        resized
            .save_with_format(&cache_path, ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to save resized image: {}", e))?;

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

    fn generate_cache_key(&self, path: &str, size: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path);
        hasher.update(size);
        format!("{:x}", hasher.finalize())
    }

    pub async fn get_gallery_preview(&self, max_items: usize) -> Result<Vec<GalleryItem>, String> {
        let mut all_items = Vec::new();

        // Recursively collect all folders and a sample of images
        self.collect_items_recursive("", &mut all_items, max_items)
            .await?;

        // Filter to only include images (not directories)
        let images_only: Vec<GalleryItem> = all_items
            .into_iter()
            .filter(|item| !item.is_directory)
            .collect();

        // Shuffle and take a subset
        let mut rng = rand::thread_rng();
        let mut selected_images = images_only;
        selected_images.shuffle(&mut rng);
        selected_images.truncate(max_items);

        Ok(selected_images)
    }

    fn collect_items_recursive<'a>(
        &'a self,
        relative_path: &'a str,
        items: &'a mut Vec<GalleryItem>,
        max_per_folder: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let full_path = self.config.source_directory.join(relative_path);

            if !full_path.starts_with(&self.config.source_directory) {
                return Err("Invalid path".to_string());
            }

            let entries = tokio::fs::read_dir(&full_path)
                .await
                .map_err(|e| format!("Failed to read directory: {}", e))?;

            let mut entries = entries;
            let mut folder_images = Vec::new();

            while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
                let file_name = entry.file_name().to_string_lossy().to_string();

                if file_name.starts_with('.') || file_name.ends_with(".md") {
                    continue;
                }

                let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
                let is_directory = metadata.is_dir();

                let item_path = if relative_path.is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", relative_path, file_name)
                };

                if is_directory {
                    let item_count = self.count_images_in_directory(&item_path).await;
                    if item_count > 0 {
                        let preview_images = self.get_directory_preview_images(&item_path).await;
                        let (display_name, description) =
                            self.read_folder_metadata(&item_path).await;
                        items.push(GalleryItem {
                            name: file_name,
                            display_name,
                            description,
                            path: item_path.clone(),
                            parent_path: Some(relative_path.to_string()),
                            is_directory: true,
                            thumbnail_url: preview_images.first().cloned(),
                            preview_images: Some(preview_images),
                            item_count: Some(item_count),
                        });

                        // Recursively collect from subdirectories (limited depth)
                        if relative_path.split('/').count() < 2 {
                            let _ = self
                                .collect_items_recursive(&item_path, items, max_per_folder / 2)
                                .await;
                        }
                    }
                } else if self.is_image(&file_name) {
                    let thumbnail_url = format!(
                        "/{}/image/{}?size=gallery",
                        self.config.path_prefix,
                        urlencoding::encode(&item_path)
                    );

                    folder_images.push(GalleryItem {
                        name: file_name,
                        display_name: None,
                        description: None,
                        path: item_path,
                        parent_path: Some(relative_path.to_string()),
                        is_directory: false,
                        thumbnail_url: Some(thumbnail_url),
                        preview_images: None,
                        item_count: None,
                    });
                }
            }

            // Add a random subset of images from this folder
            let mut rng = rand::thread_rng();
            folder_images.shuffle(&mut rng);
            folder_images.truncate(max_per_folder.min(3)); // Max 3 images per folder
            items.extend(folder_images);

            Ok(())
        })
    }

    async fn serve_file(&self, path: &PathBuf) -> Response {
        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
        };

        let content_type = mime_guess::from_path(path)
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

pub type SharedGallery = Arc<Gallery>;

pub async fn gallery_handler(
    State((template_engine, _, gallery)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(0);

    let items = match gallery.scan_directory(&path).await {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to scan gallery directory: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };

    let images = match gallery.get_images_for_page(&path, page).await {
        Ok(images) => images,
        Err(e) => {
            error!("Failed to get images: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };

    // Get folder metadata for current directory
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    let total_images = items.iter().filter(|i| !i.is_directory).count();
    let total_pages =
        (total_images + gallery.config.images_per_page - 1) / gallery.config.images_per_page;

    let globals = liquid::object!({
        "gallery_path": path,
        "folder_title": folder_title,
        "folder_description": folder_description,
        "items": items,
        "images": images,
        "current_page": page,
        "total_pages": total_pages,
        "has_prev": page > 0,
        "has_next": page + 1 < total_pages,
        "prev_page": if page > 0 { page - 1 } else { 0 },
        "next_page": page + 1,
    });

    match template_engine
        .render_template("gallery.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub async fn image_detail_handler(
    State((template_engine, _, gallery)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let (image_info, prev_image, next_image) =
        match gallery.get_image_info_with_navigation(&path).await {
            Ok((info, prev, next)) => (info, prev, next),
            Err(e) => {
                error!("Failed to get image info: {}", e);
                return (StatusCode::NOT_FOUND).into_response();
            }
        };

    let globals = liquid::object!({
        "image": image_info,
        "prev_image": prev_image,
        "next_image": next_image,
    });

    match template_engine
        .render_template("image_detail.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub async fn image_handler(
    State((_, _, gallery)): State<(Arc<TemplateEngine>, StaticFileHandler, SharedGallery)>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    gallery.serve_image(&path, query.size).await
}
