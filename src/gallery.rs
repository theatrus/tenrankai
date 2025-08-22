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
use std::fs;
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

#[derive(Debug)]
pub enum GalleryError {
    IoError(std::io::Error),
    ImageError(image::ImageError),
    InvalidPath,
}

impl std::fmt::Display for GalleryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GalleryError::IoError(e) => write!(f, "IO error: {}", e),
            GalleryError::ImageError(e) => write!(f, "Image error: {}", e),
            GalleryError::InvalidPath => write!(f, "Invalid path"),
        }
    }
}

impl std::error::Error for GalleryError {}

impl From<std::io::Error> for GalleryError {
    fn from(error: std::io::Error) -> Self {
        GalleryError::IoError(error)
    }
}

impl From<image::ImageError> for GalleryError {
    fn from(error: image::ImageError) -> Self {
        GalleryError::ImageError(error)
    }
}

use crate::GalleryConfig;

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
    pub dimensions: Option<(u32, u32)>,
    pub capture_date: Option<SystemTime>,
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
    pub capture_date: Option<String>,
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
    metadata_cache: Arc<RwLock<HashMap<String, ImageMetadata>>>,
}

struct CachedImage {
    path: PathBuf,
    modified: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageMetadata {
    dimensions: (u32, u32),
    file_size: u64,
    modified: SystemTime,
    capture_date: Option<SystemTime>,
}

impl Gallery {
    pub fn new(config: GalleryConfig) -> Self {
        let metadata_cache = Self::load_metadata_cache(&config).unwrap_or_default();
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(metadata_cache)),
        }
    }

    pub async fn scan_directory(
        &self,
        relative_path: &str,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err(GalleryError::InvalidPath);
        }

        let mut items = Vec::new();

        let entries = tokio::fs::read_dir(&full_path).await?;

        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name().to_string_lossy().to_string();

            if file_name.starts_with('.') || file_name.ends_with(".md") {
                continue;
            }

            let metadata = entry.metadata().await?;
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
                    dimensions: None,
                    capture_date: None,
                });
            } else if self.is_image(&file_name) {
                let thumbnail_url = format!(
                    "/{}/image/{}?size=gallery",
                    self.config.path_prefix,
                    urlencoding::encode(&item_path)
                );

                // Get capture date from metadata cache if available
                let capture_date = {
                    let cache = self.metadata_cache.read().await;
                    cache.get(&item_path).and_then(|m| m.capture_date)
                };

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
                    dimensions: None, // Could add dimensions here too if needed
                    capture_date,
                });
            }
        }

        items.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // For directories, sort by display_name if available, otherwise by name
                if a.is_directory && b.is_directory {
                    let a_sort_name = a.display_name.as_ref().unwrap_or(&a.name);
                    let b_sort_name = b.display_name.as_ref().unwrap_or(&b.name);
                    a_sort_name.cmp(b_sort_name)
                } else {
                    // For files, sort by capture date first, then by name
                    match (&a.capture_date, &b.capture_date) {
                        (Some(a_date), Some(b_date)) => a_date.cmp(b_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.name.cmp(&b.name),
                    }
                }
            }
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

        // Get up to configured number of images for preview
        let max_preview_images = self.config.preview.max_images;

        for entry in WalkDir::new(&full_path)
            .min_depth(1)
            .max_depth(self.config.preview.max_depth)
        {
            if preview_images.len() >= max_preview_images {
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
    ) -> Result<Vec<ImageInfo>, GalleryError> {
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
    ) -> Result<(ImageInfo, Option<NavigationImage>, Option<NavigationImage>), GalleryError> {
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

    pub async fn get_image_info(&self, relative_path: &str) -> Result<ImageInfo, GalleryError> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err(GalleryError::InvalidPath);
        }

        // Get cached metadata (includes dimensions and file size)
        let cached_metadata = self.get_image_metadata_cached(relative_path).await?;
        let file_size = cached_metadata.file_size;
        let dimensions = cached_metadata.dimensions;

        let description = self.read_sidecar_markdown(relative_path).await;
        let (camera_info, location_info) = self.extract_structured_exif_data(&full_path).await;

        let encoded_path = urlencoding::encode(relative_path);

        // Format capture date if available
        let capture_date = cached_metadata.capture_date.and_then(|date| {
            match date.duration_since(SystemTime::UNIX_EPOCH) {
                Ok(duration) => {
                    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(duration.as_secs() as i64, 0)?;
                    Some(datetime.format("%B %d, %Y at %H:%M:%S").to_string())
                }
                Err(_) => None
            }
        });

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
            capture_date,
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

    async fn extract_capture_date(&self, path: &PathBuf) -> Option<SystemTime> {
        let path = path.clone();
        tokio::task::spawn_blocking(move || -> Option<SystemTime> {
            let file_contents = match std::fs::read(&path) {
                Ok(contents) => contents,
                Err(_) => return None,
            };

            let exif_data = match rexif::parse_buffer(&file_contents) {
                Ok(data) => data,
                Err(_) => return None,
            };

            // Look for DateTimeOriginal first, then DateTimeDigitized, then DateTime
            // DateTimeOriginal is the actual capture time
            // DateTime is often the file modification time
            let tags_priority = [
                rexif::ExifTag::DateTimeOriginal,
                rexif::ExifTag::DateTimeDigitized,
                rexif::ExifTag::DateTime,
            ];

            for tag in &tags_priority {
                for entry in &exif_data.entries {
                    if entry.tag == *tag {
                        let date_str = entry.value_more_readable.to_string();
                        
                        // EXIF date format is "YYYY:MM:DD HH:MM:SS"
                        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(&date_str, "%Y:%m:%d %H:%M:%S") {
                            use std::time::{Duration, UNIX_EPOCH};
                            let timestamp = parsed.and_utc().timestamp();
                            if timestamp > 0 {
                                return Some(UNIX_EPOCH + Duration::from_secs(timestamp as u64));
                            }
                        }
                    }
                }
            }

            None
        })
        .await
        .ok()
        .flatten()
    }

    async fn extract_structured_exif_data(
        &self,
        path: &PathBuf,
    ) -> (Option<CameraInfo>, Option<LocationInfo>) {
        let path = path.clone();
        let result =
            tokio::task::spawn_blocking(move || -> (Option<CameraInfo>, Option<LocationInfo>) {
                let file_contents = match std::fs::read(&path) {
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
                            camera_info.shutter_speed =
                                Some(format!("{}s", entry.value_more_readable));
                        }
                        rexif::ExifTag::FocalLength => {
                            camera_info.focal_length =
                                Some(format!("{}mm", entry.value_more_readable));
                        }
                        rexif::ExifTag::GPSLatitude => {
                            if let Ok(lat) = Self::parse_gps_coordinate(&entry.value_more_readable)
                            {
                                latitude = Some(lat);
                            }
                        }
                        rexif::ExifTag::GPSLongitude => {
                            if let Ok(lon) = Self::parse_gps_coordinate(&entry.value_more_readable)
                            {
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
            })
            .await
            .unwrap_or((None, None));

        result
    }

    fn parse_gps_coordinate(coord_str: &str) -> Result<f64, String> {
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

    fn metadata_cache_path(&self) -> PathBuf {
        self.config.cache_directory.join("metadata_cache.json")
    }

    fn load_metadata_cache(
        config: &GalleryConfig,
    ) -> Result<HashMap<String, ImageMetadata>, GalleryError> {
        let cache_path = config.cache_directory.join("metadata_cache.json");
        if !cache_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&cache_path)?;
        serde_json::from_str(&content).map_err(|_| GalleryError::InvalidPath)
    }

    async fn save_metadata_cache(&self) -> Result<(), GalleryError> {
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        let cache = self.metadata_cache.read().await;
        let content =
            serde_json::to_string_pretty(&*cache).map_err(|_| GalleryError::InvalidPath)?;

        tokio::fs::write(self.metadata_cache_path(), content).await?;
        Ok(())
    }

    async fn get_image_metadata_cached(
        &self,
        relative_path: &str,
    ) -> Result<ImageMetadata, GalleryError> {
        let full_path = self.config.source_directory.join(relative_path);

        // Get current file stats
        let metadata = tokio::fs::metadata(&full_path).await?;
        let current_modified = metadata.modified()?;
        let current_size = metadata.len();

        // Check cache first
        {
            let cache = self.metadata_cache.read().await;
            if let Some(cached) = cache.get(relative_path) {
                // If cached data is still valid, return it
                if cached.modified >= current_modified && cached.file_size == current_size {
                    return Ok(cached.clone());
                }
            }
        }

        // Cache miss or invalid - read image dimensions
        let dimensions =
            self.get_image_dimensions_fast(&full_path)
                .await
                .ok_or(GalleryError::ImageError(image::ImageError::Unsupported(
                    image::error::UnsupportedError::from(image::error::ImageFormatHint::Unknown),
                )))?;

        // Extract capture date from EXIF
        let capture_date = self.extract_capture_date(&full_path).await;

        let new_metadata = ImageMetadata {
            dimensions,
            file_size: current_size,
            modified: current_modified,
            capture_date,
        };

        // Update cache
        {
            let mut cache = self.metadata_cache.write().await;
            cache.insert(relative_path.to_string(), new_metadata.clone());
        }

        // Save cache to disk periodically (every 10 new entries)
        if self.metadata_cache.read().await.len() % 10 == 0 {
            let _ = self.save_metadata_cache().await; // Don't fail if cache save fails
        }

        Ok(new_metadata)
    }

    #[allow(dead_code)]
    pub async fn cleanup_metadata_cache(&self) -> Result<(), GalleryError> {
        let mut cache = self.metadata_cache.write().await;
        let mut keys_to_remove = Vec::new();

        for (relative_path, cached_metadata) in cache.iter() {
            let full_path = self.config.source_directory.join(relative_path);

            // Check if file still exists and hasn't been modified
            match tokio::fs::metadata(&full_path).await {
                Ok(metadata) => {
                    let current_modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    let current_size = metadata.len();

                    // Remove if file was modified or size changed
                    if current_modified > cached_metadata.modified
                        || current_size != cached_metadata.file_size
                    {
                        keys_to_remove.push(relative_path.clone());
                    }
                }
                Err(_) => {
                    // File no longer exists, remove from cache
                    keys_to_remove.push(relative_path.clone());
                }
            }
        }

        for key in keys_to_remove {
            cache.remove(&key);
        }

        // Save the cleaned cache
        drop(cache);
        self.save_metadata_cache().await?;

        Ok(())
    }

    pub async fn save_cache_on_shutdown(&self) {
        let _ = self.save_metadata_cache().await;
    }

    pub async fn refresh_metadata_cache(&self) -> Result<(usize, usize), GalleryError> {
        use std::collections::HashSet;
        use tokio::fs;

        tracing::info!("Starting metadata cache refresh...");

        let source_dir = &self.config.source_directory;
        let mut discovered_paths = HashSet::new();
        let mut added_count = 0;
        let mut removed_count = 0;

        // Recursively scan the gallery directory in blocking thread
        let source_dir_clone = source_dir.clone();
        let discovered_files = tokio::task::spawn_blocking(move || {
            walkdir::WalkDir::new(&source_dir_clone)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.file_type().is_file()
                        && entry
                            .path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| {
                                matches!(
                                    ext.to_ascii_lowercase().as_str(),
                                    "jpg" | "jpeg" | "png" | "gif" | "webp"
                                )
                            })
                            .unwrap_or(false)
                })
                .filter_map(|entry| {
                    entry
                        .path()
                        .strip_prefix(&source_dir_clone)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .collect::<Vec<String>>()
        })
        .await
        .map_err(|e| GalleryError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        for relative_path_str in discovered_files {
            discovered_paths.insert(relative_path_str.clone());

            // Check if this image is already in cache with valid metadata
            let full_path = source_dir.join(&relative_path_str);
            let needs_update = {
                let cache = self.metadata_cache.read().await;
                if let Some(cached) = cache.get(&relative_path_str) {
                    // Check if file has been modified since cache entry
                    if let Ok(metadata) = fs::metadata(&full_path).await {
                        if let Ok(modified) = metadata.modified() {
                            modified > cached.modified
                        } else {
                            true // If we can't get modified time, update it
                        }
                    } else {
                        true // If we can't get metadata, update it
                    }
                } else {
                    true // Not in cache, need to add
                }
            };

            if needs_update {
                // Update metadata for this image
                match self.get_image_metadata_cached(&relative_path_str).await {
                    Ok(_) => {
                        added_count += 1;
                        tracing::debug!("Updated metadata for: {}", relative_path_str);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to update metadata for {}: {}",
                            relative_path_str,
                            e
                        );
                    }
                }
            }
        }

        // Remove cache entries for files that no longer exist
        let mut cache = self.metadata_cache.write().await;
        let cached_paths: Vec<String> = cache.keys().cloned().collect();

        for cached_path in cached_paths {
            if !discovered_paths.contains(&cached_path) {
                cache.remove(&cached_path);
                removed_count += 1;
                tracing::debug!("Removed from cache: {}", cached_path);
            }
        }

        drop(cache);

        // Save the updated cache
        if added_count > 0 || removed_count > 0 {
            if let Err(e) = self.save_metadata_cache().await {
                tracing::error!("Failed to save metadata cache: {}", e);
            } else {
                tracing::info!(
                    "Metadata cache refresh complete: {} added, {} removed",
                    added_count,
                    removed_count
                );
            }
        } else {
            tracing::info!("Metadata cache refresh complete: no changes needed");
        }

        Ok((added_count, removed_count))
    }

    pub fn start_background_cache_refresh(gallery: SharedGallery, interval_minutes: u64) {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            // Skip the first tick (immediate execution)
            interval.tick().await;

            loop {
                interval.tick().await;

                match gallery.refresh_metadata_cache().await {
                    Ok((added, removed)) => {
                        if added > 0 || removed > 0 {
                            tracing::info!(
                                "Background cache refresh: {} images added, {} images removed",
                                added,
                                removed
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Background cache refresh failed: {}", e);
                    }
                }
            }
        });
    }

    async fn get_image_dimensions_fast(&self, path: &PathBuf) -> Option<(u32, u32)> {
        let path = path.clone();
        tokio::task::spawn_blocking(move || -> Option<(u32, u32)> {
            use std::fs::File;
            use std::io::BufReader;

            let file = File::open(&path).ok()?;
            let reader = BufReader::new(file);

            // Use ImageReader to get dimensions without decoding the full image
            let reader = image::ImageReader::new(reader).with_guessed_format().ok()?;

            if let Ok(dimensions) = reader.into_dimensions() {
                Some(dimensions)
            } else {
                None
            }
        })
        .await
        .ok()
        .flatten()
    }

    async fn build_breadcrumbs(&self, current_path: &str) -> Vec<liquid::model::Object> {
        let mut breadcrumbs = Vec::new();

        // Add root breadcrumb
        let mut root_crumb = liquid::model::Object::new();
        root_crumb.insert("name".into(), liquid::model::Value::scalar("Gallery"));
        root_crumb.insert(
            "display_name".into(),
            liquid::model::Value::scalar("Gallery"),
        );
        root_crumb.insert("path".into(), liquid::model::Value::scalar(""));
        root_crumb.insert(
            "is_current".into(),
            liquid::model::Value::scalar(current_path.is_empty()),
        );
        breadcrumbs.push(root_crumb);

        if !current_path.is_empty() {
            let path_parts: Vec<&str> = current_path.split('/').collect();
            let mut accumulated_path = String::new();

            for (index, part) in path_parts.iter().enumerate() {
                if !accumulated_path.is_empty() {
                    accumulated_path.push('/');
                }
                accumulated_path.push_str(part);

                let is_current = index == path_parts.len() - 1;

                // Get display name for this folder
                let (display_name, _) = self.read_folder_metadata(&accumulated_path).await;
                let display_name = display_name.unwrap_or_else(|| part.to_string());

                let mut crumb = liquid::model::Object::new();
                crumb.insert(
                    "name".into(),
                    liquid::model::Value::scalar(part.to_string()),
                );
                crumb.insert(
                    "display_name".into(),
                    liquid::model::Value::scalar(display_name),
                );
                crumb.insert(
                    "path".into(),
                    liquid::model::Value::scalar(accumulated_path.clone()),
                );
                crumb.insert(
                    "is_current".into(),
                    liquid::model::Value::scalar(is_current),
                );
                breadcrumbs.push(crumb);
            }
        }

        breadcrumbs
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
    ) -> Result<PathBuf, GalleryError> {
        let (width, height) = match size {
            "thumbnail" => (self.config.thumbnail.width, self.config.thumbnail.height),
            "gallery" => (
                self.config.gallery_size.width,
                self.config.gallery_size.height,
            ),
            "medium" => (self.config.medium.width, self.config.medium.height),
            "large" => (self.config.large.width, self.config.large.height),
            _ => return Err(GalleryError::InvalidPath),
        };

        let hash = self.generate_cache_key(relative_path, size);
        let cache_filename = format!("{}.jpg", hash);
        let cache_path = self.config.cache_directory.join(cache_filename);

        let original_metadata = tokio::fs::metadata(original_path).await?;
        let original_modified = original_metadata.modified()?;

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(&hash) {
            if cached.modified >= original_modified && cached.path.exists() {
                return Ok(cached.path.clone());
            }
        }
        drop(cache);

        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Move CPU-intensive and blocking I/O operations to blocking thread pool
        let original_path = original_path.clone();
        let cache_path_clone = cache_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), GalleryError> {
            // Open image with decoder to access ICC profile
            let original_file = std::fs::File::open(&original_path)?;

            let decoder = image::ImageReader::new(std::io::BufReader::new(original_file))
                .with_guessed_format()?;

            let img = decoder.decode()?;

            let resized = img.resize(width, height, FilterType::Lanczos3);

            // Save resized image
            // Note: The standard image crate JPEG encoder doesn't support embedding ICC profiles
            // For production use, consider using a library like turbojpeg-sys or mozjpeg
            // that supports ICC profile embedding during encoding
            resized.save_with_format(&cache_path_clone, ImageFormat::Jpeg)?;

            Ok(())
        })
        .await
        .map_err(|e| GalleryError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

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

    pub async fn get_gallery_preview(
        &self,
        max_items: usize,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), GalleryError>> + Send + 'a>>
    {
        Box::pin(async move {
            let full_path = self.config.source_directory.join(relative_path);

            if !full_path.starts_with(&self.config.source_directory) {
                return Err(GalleryError::InvalidPath);
            }

            let entries = tokio::fs::read_dir(&full_path).await?;

            let mut entries = entries;
            let mut folder_images = Vec::new();

            while let Some(entry) = entries.next_entry().await? {
                let file_name = entry.file_name().to_string_lossy().to_string();

                if file_name.starts_with('.') || file_name.ends_with(".md") {
                    continue;
                }

                let metadata = entry.metadata().await?;
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
                            dimensions: None,
                            capture_date: None,
                        });

                        // Recursively collect from subdirectories (limited depth)
                        if relative_path.split('/').count() < self.config.preview.max_depth - 1 {
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

                    // Get image dimensions and capture date from cache
                    let (dimensions, capture_date) = match self.get_image_metadata_cached(&item_path).await {
                        Ok(metadata) => (Some(metadata.dimensions), metadata.capture_date),
                        Err(_) => (None, None),
                    };

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
                        dimensions,
                        capture_date,
                    });
                }
            }

            // Add a random subset of images from this folder
            let mut rng = rand::thread_rng();
            folder_images.shuffle(&mut rng);
            folder_images.truncate(max_per_folder.min(self.config.preview.max_per_folder));
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
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .header(
                header::ETAG,
                format!(
                    "\"{}\"",
                    self.generate_cache_key(path.to_str().unwrap_or(""), "etag")
                ),
            )
            .body(body)
            .unwrap()
    }
}

pub type SharedGallery = Arc<Gallery>;

pub async fn gallery_handler(
    State(app_state): State<crate::AppState>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;
    let gallery = &app_state.gallery;
    let page = query.page.unwrap_or(0);

    let items = match gallery.scan_directory(&path).await {
        Ok(items) => items,
        Err(GalleryError::InvalidPath) => {
            return (StatusCode::NOT_FOUND, "Directory not found").into_response();
        }
        Err(e) => {
            error!("Failed to scan gallery directory: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let images = match gallery.get_images_for_page(&path, page).await {
        Ok(images) => images,
        Err(e) => {
            error!("Failed to get images: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };

    // Send images without pre-computed layout - client will handle this
    let images_with_layout = images;

    // Get folder metadata for current directory
    let (folder_title, folder_description) = gallery.read_folder_metadata(&path).await;

    // Build breadcrumb data with proper display names
    let breadcrumbs = gallery.build_breadcrumbs(&path).await;

    let total_images = items.iter().filter(|i| !i.is_directory).count();
    let total_pages =
        (total_images + gallery.config.images_per_page - 1) / gallery.config.images_per_page;

    // Serialize images to JSON string for client-side use
    let images_json =
        serde_json::to_string(&images_with_layout).unwrap_or_else(|_| "[]".to_string());

    let globals = liquid::object!({
        "gallery_path": path,
        "folder_title": folder_title,
        "folder_description": folder_description,
        "breadcrumbs": breadcrumbs,
        "items": items,
        "images": images_with_layout,
        "images_json": images_json,
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
    State(app_state): State<crate::AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let template_engine = &app_state.template_engine;
    let gallery = &app_state.gallery;
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
    State(app_state): State<crate::AppState>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    app_state.gallery.serve_image(&path, query.size).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GalleryConfig, ImageSizeConfig, PreviewConfig};
    use chrono::{Datelike, Timelike};
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    #[tokio::test]
    async fn test_extract_capture_date_with_valid_exif() {
        // Create a test gallery instance
        let config = GalleryConfig {
            path_prefix: "gallery".to_string(),
            source_directory: PathBuf::from("photos"),
            cache_directory: PathBuf::from("cache/test"),
            images_per_page: 20,
            thumbnail: ImageSizeConfig { width: 300, height: 300 },
            gallery_size: ImageSizeConfig { width: 800, height: 800 },
            medium: ImageSizeConfig { width: 1200, height: 1200 },
            large: ImageSizeConfig { width: 1600, height: 1600 },
            preview: PreviewConfig {
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
        };
        let gallery = Gallery::new(config);

        // Test with the provided test image
        let test_image_path = PathBuf::from("photos/landscapes/CRW_1978.jpg");
        let capture_date = gallery.extract_capture_date(&test_image_path).await;

        assert!(capture_date.is_some(), "Should extract capture date from CRW_1978.jpg");
        
        if let Some(date) = capture_date {
            // Convert to timestamp for easier comparison
            let timestamp = date.duration_since(UNIX_EPOCH).unwrap().as_secs();
            
            // The EXIF date from the image is 2005:07:30 07:22:46
            // We expect this to be parsed as-is without timezone adjustment
            let expected_datetime = chrono::NaiveDateTime::parse_from_str(
                "2005-07-30 07:22:46", 
                "%Y-%m-%d %H:%M:%S"
            ).unwrap();
            let expected_timestamp = expected_datetime.and_utc().timestamp() as u64;
            
            // Should match exactly
            assert_eq!(
                timestamp, expected_timestamp,
                "Capture date should be 2005-07-30 07:22:46"
            );
        }
    }

    #[tokio::test]
    async fn test_extract_capture_date_with_no_exif() {
        let config = GalleryConfig {
            path_prefix: "gallery".to_string(),
            source_directory: PathBuf::from("."),
            cache_directory: PathBuf::from("cache/test"),
            images_per_page: 20,
            thumbnail: ImageSizeConfig { width: 300, height: 300 },
            gallery_size: ImageSizeConfig { width: 800, height: 800 },
            medium: ImageSizeConfig { width: 1200, height: 1200 },
            large: ImageSizeConfig { width: 1600, height: 1600 },
            preview: PreviewConfig {
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
        };
        let gallery = Gallery::new(config);

        // Test with a non-existent file
        let test_image_path = PathBuf::from("non_existent_image.jpg");
        let capture_date = gallery.extract_capture_date(&test_image_path).await;

        assert!(capture_date.is_none(), "Should return None for non-existent file");
    }

    #[tokio::test]
    async fn test_extract_capture_date_formats() {
        // This test validates that the EXIF date format parsing works correctly
        let date_str = "2005:07:30 07:22:46";
        let parsed = chrono::NaiveDateTime::parse_from_str(date_str, "%Y:%m:%d %H:%M:%S");
        
        assert!(parsed.is_ok(), "Should parse EXIF date format");
        
        if let Ok(datetime) = parsed {
            assert_eq!(datetime.year(), 2005);
            assert_eq!(datetime.month(), 7);
            assert_eq!(datetime.day(), 30);
            assert_eq!(datetime.hour(), 7);
            assert_eq!(datetime.minute(), 22);
            assert_eq!(datetime.second(), 46);
        }
    }

    #[test]
    fn test_capture_date_formatting() {
        // Test the formatting used in get_image_info
        let timestamp = 1122719766u64; // Approximately 2005-07-30 10:36:06 UTC
        let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp as i64, 0).unwrap();
        let formatted = datetime.format("%B %d, %Y at %H:%M:%S").to_string();
        
        // The timestamp corresponds to this exact time
        assert_eq!(formatted, "July 30, 2005 at 10:36:06");
    }
}
