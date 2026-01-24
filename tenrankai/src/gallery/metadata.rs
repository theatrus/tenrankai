use super::indexing::ImageIndexer;
use super::metadata_sources::{merge_metadata_sources, read_xmp_metadata_from_storage};
use super::path_utils::{FileExtension, SidecarPaths};
use super::{CameraInfo, Gallery, ImageMetadata, LocationInfo};
use crate::storage::header_sizes;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::stream::{self, StreamExt};
use rand::rng;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tenrankai_storage::StorageEntry;
use tracing::{debug, info, trace};

/// Result of classifying a folder's entries for caching
struct FolderEntryClassification {
    /// Visible subdirectory names (excludes __ prefixed)
    subdirectories: Vec<String>,
    /// Image paths in this folder (displayable images only)
    images: Vec<String>,
    /// Files for grouping algorithm: (path, modification_time, size)
    groupable_files: Vec<(String, Option<SystemTime>, u64)>,
    /// Total size of all files in this folder (bytes)
    total_size: u64,
}

impl Gallery {
    /// Classify a folder's entries into subdirectories, images, and groupable files.
    /// This shared logic is used by both single-folder and full refresh operations.
    ///
    /// The `folder_path` parameter is the path of the folder being listed. Entry paths
    /// returned by storage are just filenames, so we prepend the folder path to build
    /// full relative paths for indexer lookups.
    fn classify_folder_entries(
        &self,
        folder_path: &str,
        entries: &[StorageEntry],
    ) -> FolderEntryClassification {
        let mut subdirectories = Vec::new();
        let mut images = Vec::new();
        let mut groupable_files = Vec::new();
        let mut total_size: u64 = 0;

        for entry in entries {
            let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);

            // Skip hidden files and markdown
            if name.starts_with('.') || name.ends_with(".md") {
                continue;
            }

            // Build full relative path by prepending folder_path
            let full_path = if folder_path.is_empty() {
                entry.path.clone()
            } else {
                format!("{}/{}", folder_path, entry.path)
            };

            if entry.is_dir {
                // Skip __ prefixed directories from visible list
                if !name.starts_with("__") {
                    subdirectories.push(name.to_string());
                }
            } else {
                // Count file size for all non-hidden files
                total_size += entry.metadata.as_ref().map(|m| m.size).unwrap_or(0);

                let ext = super::grouping::get_extension(name).map(|e| e.to_lowercase());
                let is_displayable_image = self.is_image(name);
                let is_raw = ext
                    .as_ref()
                    .is_some_and(|e| super::grouping::is_raw_extension(e));

                if is_displayable_image {
                    images.push(full_path.clone());
                }

                if is_displayable_image || is_raw {
                    let mod_time = entry.metadata.as_ref().and_then(|m| m.last_modified);
                    let file_size = entry.metadata.as_ref().map(|m| m.size).unwrap_or(0);
                    groupable_files.push((full_path, mod_time, file_size));
                }
            }
        }

        FolderEntryClassification {
            subdirectories,
            images,
            groupable_files,
            total_size,
        }
    }

    /// Build a CachedPreviewItem for an image path.
    /// Used by both single-folder and full refresh operations.
    fn build_preview_item(
        &self,
        img_path: &str,
        indexer: &ImageIndexer,
        image_cache: &HashMap<String, ImageMetadata>,
    ) -> super::CachedPreviewItem {
        let url_id = indexer
            .get_index(img_path)
            .map(|s| s.to_string())
            .unwrap_or_else(|| urlencoding::encode(img_path).to_string());
        let thumbnail_url = self.build_thumbnail_url(&url_id);
        let gallery_url = self.build_gallery_url(&url_id);
        let dimensions = image_cache.get(img_path).map(|m| m.dimensions);

        super::CachedPreviewItem {
            path: img_path.to_string(),
            url_id,
            thumbnail_url,
            gallery_url,
            dimensions,
        }
    }
    /// Extract EXIF data from image bytes
    pub(crate) fn extract_all_exif_data_from_bytes(
        &self,
        image_data: &[u8],
        relative_path: &str,
    ) -> (Option<SystemTime>, Option<CameraInfo>, Option<LocationInfo>) {
        // Check file extension to determine extraction method
        let extension = relative_path.rsplit('.').next().map(|s| s.to_lowercase());

        match extension.as_deref() {
            #[cfg(feature = "avif")]
            Some("avif") => {
                // For AVIF files, extract EXIF data using libavif
                match super::image_processing::formats::avif::extract_exif_data_from_bytes(
                    image_data,
                ) {
                    Some(exif_bytes) => {
                        // Parse the EXIF data from bytes (use quiet version to avoid stderr spam)
                        let (result, _warnings) = rexif::parse_buffer_quiet(&exif_bytes);
                        match result {
                            Ok(exif_data) => {
                                let capture_date = self.extract_capture_date(&exif_data);
                                let camera_info = self.extract_camera_info(&exif_data);
                                let location_info = self.extract_location_info(&exif_data);
                                debug!("Successfully extracted EXIF from AVIF: {}", relative_path);
                                (capture_date, camera_info, location_info)
                            }
                            Err(e) => {
                                trace!(
                                    "Failed to parse EXIF data from AVIF {}: {}",
                                    relative_path, e
                                );
                                (None, None, None)
                            }
                        }
                    }
                    None => {
                        trace!("No EXIF data found in AVIF: {}", relative_path);
                        (None, None, None)
                    }
                }
            }
            #[cfg(feature = "heif")]
            Some("heic") | Some("heif") => {
                // For HEIF/HEIC files, extract EXIF data using libheif
                match tenrankai_image::formats::heif::extract_exif_data_from_bytes(image_data) {
                    Some(exif_bytes) => {
                        let (result, _warnings) = rexif::parse_buffer_quiet(&exif_bytes);
                        match result {
                            Ok(exif_data) => {
                                let capture_date = self.extract_capture_date(&exif_data);
                                let camera_info = self.extract_camera_info(&exif_data);
                                let location_info = self.extract_location_info(&exif_data);
                                debug!("Successfully extracted EXIF from HEIC: {}", relative_path);
                                (capture_date, camera_info, location_info)
                            }
                            Err(e) => {
                                trace!(
                                    "Failed to parse EXIF data from HEIC {}: {}",
                                    relative_path, e
                                );
                                (None, None, None)
                            }
                        }
                    }
                    None => {
                        trace!("No EXIF data found in HEIC: {}", relative_path);
                        (None, None, None)
                    }
                }
            }
            _ => {
                // For other formats (JPEG, etc), use rexif's buffer parser
                // Use quiet version to avoid stderr spam from malformed EXIF tags
                let (result, _warnings) = rexif::parse_buffer_quiet(image_data);
                match result {
                    Ok(exif_data) => {
                        let capture_date = self.extract_capture_date(&exif_data);
                        let camera_info = self.extract_camera_info(&exif_data);
                        let location_info = self.extract_location_info(&exif_data);
                        (capture_date, camera_info, location_info)
                    }
                    Err(e) => {
                        trace!("No EXIF data for {}: {}", relative_path, e);
                        (None, None, None)
                    }
                }
            }
        }
    }

    fn extract_capture_date(&self, exif: &rexif::ExifData) -> Option<SystemTime> {
        // Try different date fields in order of preference
        let date_fields = [
            rexif::ExifTag::DateTimeOriginal,
            rexif::ExifTag::DateTimeDigitized,
            rexif::ExifTag::DateTime,
        ];

        for field in &date_fields {
            if let Some(entry) = exif.entries.iter().find(|e| e.tag == *field)
                && let Some(date) = self.parse_exif_datetime(&entry.value_more_readable)
            {
                debug!("Found capture date in {:?}: {:?}", field, date);
                return Some(date);
            }
        }

        None
    }

    fn parse_exif_datetime(&self, datetime_str: &str) -> Option<SystemTime> {
        // EXIF datetime format: "2005:07:30 07:22:46"
        // First try the standard format
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(datetime_str, "%Y:%m:%d %H:%M:%S") {
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
            return Some(SystemTime::from(datetime_utc));
        }

        // Try with just date
        if let Ok(naive_date) = NaiveDateTime::parse_from_str(
            &format!("{} 00:00:00", datetime_str),
            "%Y:%m:%d %H:%M:%S",
        ) {
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);
            return Some(SystemTime::from(datetime_utc));
        }

        // Try alternative formats
        let formats = [
            "%Y-%m-%d %H:%M:%S",
            "%Y/%m/%d %H:%M:%S",
            "%Y:%m:%d",
            "%Y-%m-%d",
            "%Y/%m/%d",
        ];

        for format in &formats {
            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(datetime_str, format) {
                let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
                return Some(SystemTime::from(datetime_utc));
            }

            // Try parsing just as date and adding time
            if !format.contains("%H") {
                let with_time = format!("{} 00:00:00", datetime_str);
                let format_with_time = format!("{} %H:%M:%S", format);
                if let Ok(naive_dt) = NaiveDateTime::parse_from_str(&with_time, &format_with_time) {
                    let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
                    return Some(SystemTime::from(datetime_utc));
                }
            }
        }

        None
    }

    fn extract_camera_info(&self, exif: &rexif::ExifData) -> Option<CameraInfo> {
        let mut camera_info = CameraInfo {
            camera_make: None,
            camera_model: None,
            lens_model: None,
            iso: None,
            aperture: None,
            shutter_speed: None,
            focal_length: None,
            telescope: None,
            mount: None,
            filters: None,
            total_exposure_time: None,
            ra: None,
            dec: None,
            additional_details: None,
        };

        let mut has_data = false;

        for entry in &exif.entries {
            match entry.tag {
                rexif::ExifTag::Make => {
                    camera_info.camera_make = Some(entry.value_more_readable.trim().to_string());
                    has_data = true;
                }
                rexif::ExifTag::Model => {
                    camera_info.camera_model = Some(entry.value_more_readable.trim().to_string());
                    has_data = true;
                }
                rexif::ExifTag::LensModel => {
                    camera_info.lens_model = Some(entry.value_more_readable.trim().to_string());
                    has_data = true;
                }
                rexif::ExifTag::ISOSpeedRatings => {
                    if let Ok(iso) = entry.value_more_readable.parse::<u32>() {
                        camera_info.iso = Some(iso);
                        has_data = true;
                    }
                }
                rexif::ExifTag::FNumber => {
                    let aperture_str = entry.value_more_readable.to_string();
                    camera_info.aperture = if aperture_str.starts_with("f/") {
                        Some(aperture_str)
                    } else {
                        Some(format!("f/{}", aperture_str))
                    };
                    has_data = true;
                }
                rexif::ExifTag::ExposureTime => {
                    camera_info.shutter_speed = Some(entry.value_more_readable.to_string());
                    has_data = true;
                }
                rexif::ExifTag::FocalLength => {
                    let focal_str = entry.value_more_readable.to_string();
                    camera_info.focal_length = if focal_str.ends_with("mm") {
                        Some(focal_str)
                    } else {
                        Some(format!("{}mm", focal_str))
                    };
                    has_data = true;
                }
                _ => {}
            }
        }

        if has_data { Some(camera_info) } else { None }
    }

    fn extract_location_info(&self, exif: &rexif::ExifData) -> Option<LocationInfo> {
        let mut latitude: Option<f64> = None;
        let mut longitude: Option<f64> = None;
        let mut lat_ref: Option<String> = None;
        let mut lon_ref: Option<String> = None;

        for entry in &exif.entries {
            match entry.tag {
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

        if let (Some(mut lat), Some(mut lon), Some(lat_r), Some(lon_r)) =
            (latitude, longitude, lat_ref, lon_ref)
        {
            // Apply reference direction
            if lat_r == "S" {
                lat = -lat;
            }
            if lon_r == "W" {
                lon = -lon;
            }

            Some(LocationInfo {
                latitude: lat,
                longitude: lon,
                google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
                apple_maps_url: format!("https://maps.apple.com/?ll={},{}", lat, lon),
            })
        } else {
            None
        }
    }

    fn parse_gps_coordinate(&self, coord_str: &str) -> Result<f64, String> {
        // GPS coordinates can be in various formats:
        // Format 1: "51 deg 30' 45.60\""
        // Format 2: "34°39.0643' N"

        // Remove direction indicators (N, S, E, W) for parsing
        let cleaned = coord_str.trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace());

        // Try format with degree symbol (°)
        if cleaned.contains('°') {
            let parts: Vec<&str> = cleaned.split('°').collect();
            if parts.len() == 2 {
                let degrees = parts[0]
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Invalid degrees")?;
                let minutes_str = parts[1].trim_end_matches('\'').trim();
                let minutes = minutes_str.parse::<f64>().map_err(|_| "Invalid minutes")?;
                return Ok(degrees + minutes / 60.0);
            }
        }

        // Try original format with "deg"
        let parts: Vec<&str> = coord_str.split_whitespace().collect();
        if parts.len() >= 6 {
            let degrees = parts[0].parse::<f64>().map_err(|_| "Invalid degrees")?;
            let minutes = parts[2]
                .trim_end_matches('\'')
                .parse::<f64>()
                .map_err(|_| "Invalid minutes")?;
            let seconds = parts[4]
                .trim_end_matches('"')
                .parse::<f64>()
                .map_err(|_| "Invalid seconds")?;

            Ok(degrees + minutes / 60.0 + seconds / 3600.0)
        } else {
            Err(format!("Invalid GPS coordinate format: {}", coord_str))
        }
    }

    pub async fn refresh_single_image_metadata(
        &self,
        relative_path: &str,
    ) -> Result<(), super::GalleryError> {
        // Check if image exists using storage
        if !self
            .source_storage
            .exists(relative_path)
            .await
            .unwrap_or(false)
        {
            // If image doesn't exist, remove from cache (marks dirty automatically)
            if self.image_cache.remove(relative_path).await.is_some() {
                debug!("Removed deleted image from cache: {}", relative_path);
            }

            // Also update the indexer - rebuild it without this image
            let all_paths: Vec<String> = {
                let cache = self.image_cache.read_all().await;
                cache
                    .keys()
                    .filter(|p| *p != relative_path)
                    .cloned()
                    .collect()
            };
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_paths);

            return Ok(());
        }

        // Get modification date for this file
        let modification_date = self
            .source_storage
            .metadata(relative_path)
            .await
            .ok()
            .and_then(|m| m.last_modified);

        // Extract and cache metadata using storage
        if let Ok(metadata) = self
            .extract_image_metadata(relative_path, modification_date)
            .await
        {
            self.insert_metadata_with_tracking(relative_path.to_string(), metadata)
                .await;
            debug!("Updated metadata for: {}", relative_path);

            // Update the indexer with all current images
            let all_paths: Vec<String> = {
                let cache = self.image_cache.read_all().await;
                cache.keys().cloned().collect()
            };
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_paths);
        }

        Ok(())
    }

    /// Refresh the folder cache for a single folder after an upload.
    /// This updates the folder's image list, groups, preview items, and
    /// bubbles up recursive_image_count changes to parent folders.
    pub async fn refresh_single_folder_cache(
        &self,
        folder_path: &str,
    ) -> Result<(), super::GalleryError> {
        debug!("Refreshing folder cache for: {}", folder_path);

        // List and classify this folder's contents
        let entries = self.source_storage.list(folder_path).await?;
        let mut classified = self.classify_folder_entries(folder_path, &entries);

        // Also collect groupable files from __versions subfolder if it exists
        let versions_path = if folder_path.is_empty() {
            "__versions".to_string()
        } else {
            format!("{}/__versions", folder_path)
        };
        if let Ok(version_entries) = self.source_storage.list(&versions_path).await {
            let version_classified = self.classify_folder_entries(&versions_path, &version_entries);
            classified
                .groupable_files
                .extend(version_classified.groupable_files);
        }

        // Build image groups
        let indexer = self.image_indexer.read().await;
        let image_groups = super::grouping::group_files(
            classified
                .groupable_files
                .iter()
                .map(|(p, m, s)| (p.as_str(), *m, *s)),
            |path| {
                indexer
                    .get_index(path)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| urlencoding::encode(path).to_string())
            },
            |url_id| self.build_thumbnail_url(url_id),
        );

        // Build preview items for this folder's images
        let image_cache_guard = self.image_cache.read_all().await;
        let image_cache: HashMap<String, ImageMetadata> = image_cache_guard.clone();
        drop(image_cache_guard);

        let preview_items: Vec<super::CachedPreviewItem> = classified
            .images
            .iter()
            .take(self.config.preview.max_images)
            .map(|img_path| self.build_preview_item(img_path, &indexer, &image_cache))
            .collect();
        drop(indexer);

        // Get existing folder metadata (from _folder.md) or use defaults
        let (metadata, metadata_last_modified) =
            if let Some(existing) = self.folder_cache.get(folder_path).await {
                (existing.metadata, existing.metadata_last_modified)
            } else {
                // Try to read _folder.md for new folders
                let folder_meta = self.read_folder_metadata_from_storage(folder_path).await;
                (folder_meta, None)
            };

        // Calculate direct counts and sizes for this folder
        let direct_count = classified.images.len();
        let direct_size = classified.total_size;

        // Calculate recursive values: direct + all subdirectories
        let (recursive_image_count, recursive_size) = {
            let mut count = direct_count;
            let mut size = direct_size;
            for subdir_name in &classified.subdirectories {
                let subdir_path = if folder_path.is_empty() {
                    subdir_name.clone()
                } else {
                    format!("{}/{}", folder_path, subdir_name)
                };
                if let Some(subdir_cache) = self.folder_cache.get(&subdir_path).await {
                    count += subdir_cache.recursive_image_count;
                    size += subdir_cache.recursive_size;
                }
            }
            (count, size)
        };

        // Insert updated folder cache entry
        self.folder_cache
            .insert(
                folder_path.to_string(),
                super::CachedFolderMetadata {
                    metadata,
                    metadata_last_modified,
                    subdirectories: classified.subdirectories,
                    images: classified.images,
                    recursive_image_count,
                    direct_size,
                    recursive_size,
                    preview_items,
                    image_groups,
                },
            )
            .await;

        // Bubble up count and size changes to parent folders
        let mut current = folder_path.to_string();
        while let Some(last_slash) = current.rfind('/') {
            let parent = &current[..last_slash];
            if let Some(mut parent_cache) = self.folder_cache.get(parent).await {
                // Recalculate parent's recursive values from its subdirectories
                let mut new_count = parent_cache.images.len();
                let mut new_size = parent_cache.direct_size;
                for subdir_name in &parent_cache.subdirectories {
                    let subdir_path = if parent.is_empty() {
                        subdir_name.clone()
                    } else {
                        format!("{}/{}", parent, subdir_name)
                    };
                    if let Some(subdir_cache) = self.folder_cache.get(&subdir_path).await {
                        new_count += subdir_cache.recursive_image_count;
                        new_size += subdir_cache.recursive_size;
                    }
                }
                parent_cache.recursive_image_count = new_count;
                parent_cache.recursive_size = new_size;
                self.folder_cache
                    .insert(parent.to_string(), parent_cache)
                    .await;
            }
            current = parent.to_string();
        }

        // Also update root folder if we're not already at root
        if !folder_path.is_empty()
            && let Some(mut root_cache) = self.folder_cache.get("").await
        {
            let mut new_count = root_cache.images.len();
            let mut new_size = root_cache.direct_size;
            for subdir_name in &root_cache.subdirectories {
                if let Some(subdir_cache) = self.folder_cache.get(subdir_name).await {
                    new_count += subdir_cache.recursive_image_count;
                    new_size += subdir_cache.recursive_size;
                }
            }
            root_cache.recursive_image_count = new_count;
            root_cache.recursive_size = new_size;
            self.folder_cache.insert(String::new(), root_cache).await;
        }

        debug!(
            "Folder cache updated for {}: {} images, {} subdirs",
            if folder_path.is_empty() {
                "(root)"
            } else {
                folder_path
            },
            direct_count,
            self.folder_cache
                .get(folder_path)
                .await
                .map(|c| c.subdirectories.len())
                .unwrap_or(0)
        );

        Ok(())
    }

    pub async fn refresh_directory_metadata(
        &self,
        directory_path: &str,
    ) -> Result<(), super::GalleryError> {
        let mut count = 0;

        // Use storage abstraction to list immediate children
        let entries = self.source_storage.list(directory_path).await?;

        for entry in entries {
            // Skip directories
            if entry.is_dir {
                continue;
            }

            // Build full relative path
            let relative_str = if directory_path.is_empty() {
                entry.path.clone()
            } else {
                format!("{}/{}", directory_path, entry.path)
            };

            // Extract filename and check if it's an image
            let file_name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            if !self.is_image(file_name) {
                continue;
            }

            // Get modification date from entry metadata if available
            let modification_date = entry.metadata.and_then(|m| m.last_modified);

            // Extract metadata using storage
            if let Ok(metadata) = self
                .extract_image_metadata(&relative_str, modification_date)
                .await
            {
                self.insert_metadata_with_tracking(relative_str, metadata)
                    .await;
                count += 1;
            }
        }

        if count > 0 {
            info!(
                "Refreshed metadata for {} images in {}",
                count, directory_path
            );

            // Rebuild the index with all current images
            let all_paths: Vec<String> = {
                let cache = self.image_cache.read_all().await;
                cache.keys().cloned().collect()
            };
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_paths);
            debug!("Rebuilt image index after directory refresh");
        }

        Ok(())
    }

    pub async fn refresh_all_metadata(&self) -> Result<(), super::GalleryError> {
        // Check if storage has a newer cache file before expensive refresh
        match self.image_cache.load_if_newer(&self.cache_storage).await {
            Ok(true) => {
                info!(
                    "Image metadata cache reloaded from storage ({} entries)",
                    self.image_cache.len().await
                );
                // Still need to rebuild index from reloaded cache
                let all_paths: Vec<String> = {
                    let cache = self.image_cache.read_all().await;
                    cache.keys().cloned().collect()
                };
                let mut indexer = self.image_indexer.write().await;
                indexer.build_index(&all_paths);
                return Ok(());
            }
            Ok(false) => {} // Storage not newer, proceed with refresh
            Err(e) => {
                tracing::warn!("Failed to check image cache staleness: {}", e);
                // Continue with refresh on error
            }
        }

        info!("Starting metadata refresh (checking modification dates)");
        let start_time = std::time::Instant::now();
        let mut refreshed_count = 0;
        let mut skipped_count = 0;
        let mut all_image_paths = Vec::new();

        // Use storage abstraction for recursive listing
        debug!("Fetching file listing from storage...");
        let list_start = std::time::Instant::now();
        let entries = self.source_storage.list_recursive("").await?;
        info!(
            "Storage listing completed in {:.2}s: {} entries",
            list_start.elapsed().as_secs_f64(),
            entries.len()
        );

        // Build a map of path -> last_modified for efficient lookups
        // This avoids individual metadata() calls for each file
        let mut file_mtimes: std::collections::HashMap<String, std::time::SystemTime> =
            std::collections::HashMap::new();
        for entry in &entries {
            if !entry.is_dir
                && let Some(ref meta) = entry.metadata
                && let Some(mtime) = meta.last_modified
            {
                file_mtimes.insert(entry.path.clone(), mtime);
            }
        }

        let loop_start = std::time::Instant::now();

        // First pass: collect all image paths and determine which need refresh
        let mut paths_needing_refresh: Vec<String> = Vec::new();
        for entry in entries {
            // Skip directories
            if entry.is_dir {
                continue;
            }

            // Extract filename and check if it's an image
            let file_name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            if !self.is_image(file_name) {
                continue;
            }

            let relative_str = entry.path.clone();

            // Collect all image paths for indexing
            all_image_paths.push(relative_str.clone());

            // Check if we need to refresh this file's metadata using pre-fetched mtimes
            let needs_refresh = self
                .needs_metadata_refresh_with_mtimes(&relative_str, &file_mtimes)
                .await;

            if needs_refresh {
                paths_needing_refresh.push(relative_str);
            } else {
                skipped_count += 1;
            }
        }

        info!(
            "Found {} images needing metadata refresh, {} unchanged ({:.2}s)",
            paths_needing_refresh.len(),
            skipped_count,
            loop_start.elapsed().as_secs_f64()
        );

        // Second pass: parallel metadata extraction
        // Use Arc to share self reference across async tasks
        const PARALLEL_EXTRACTIONS: usize = 16;
        let total_to_refresh = paths_needing_refresh.len();

        if !paths_needing_refresh.is_empty() {
            let extraction_start = std::time::Instant::now();
            let refreshed_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

            // Build list of (path, mtime) tuples using pre-fetched modification times
            let paths_with_mtimes: Vec<(String, Option<SystemTime>)> = paths_needing_refresh
                .into_iter()
                .map(|path| {
                    let mtime = file_mtimes.get(&path).copied();
                    (path, mtime)
                })
                .collect();

            // Process in parallel with buffer_unordered
            let results: Vec<(String, Option<ImageMetadata>)> = stream::iter(paths_with_mtimes)
                .map(|(path, mtime)| {
                    let counter = Arc::clone(&refreshed_counter);
                    async move {
                        let extract_start = std::time::Instant::now();
                        let result = self.extract_image_metadata(&path, mtime).await.ok();

                        let extract_time = extract_start.elapsed();
                        if extract_time.as_millis() > 500 {
                            info!(
                                "Slow metadata extraction for {}: {:.2}s",
                                path,
                                extract_time.as_secs_f64()
                            );
                        }

                        // Update progress
                        let count = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        if count.is_multiple_of(50) {
                            info!("Progress: extracted {}/{}", count, total_to_refresh,);
                        }

                        (path, result)
                    }
                })
                .buffer_unordered(PARALLEL_EXTRACTIONS)
                .collect()
                .await;

            // Insert all results into cache
            for (path, metadata_opt) in results {
                if let Some(metadata) = metadata_opt {
                    self.insert_metadata_with_tracking(path, metadata).await;
                    refreshed_count += 1;
                }
            }

            info!(
                "Parallel extraction completed: {} images in {:.2}s ({:.1} images/sec)",
                refreshed_count,
                extraction_start.elapsed().as_secs_f64(),
                refreshed_count as f64 / extraction_start.elapsed().as_secs_f64()
            );
        }

        info!(
            "Loop completed: {} total images in {:.2}s",
            all_image_paths.len(),
            loop_start.elapsed().as_secs_f64()
        );

        // Build the image index with all collected paths
        {
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_image_paths);
            info!("Built image index with {} images", all_image_paths.len());
        }

        // Remove stale metadata cache entries (images that no longer exist)
        let removed_count = self.remove_stale_metadata_entries(&all_image_paths).await;

        // Refresh folder metadata cache
        self.refresh_folder_metadata().await?;

        // Save caches (only saves if dirty)
        self.save_metadata_cache().await?;

        let elapsed = start_time.elapsed();
        info!(
            "Metadata refresh completed in {:.2}s: {} refreshed, {} unchanged, {} removed (stale)",
            elapsed.as_secs_f64(),
            refreshed_count,
            skipped_count,
            removed_count
        );

        Ok(())
    }

    /// Refresh folder cache with metadata, contents, counts, and preview images
    /// This does a single recursive listing and pre-computes everything.
    /// This must be called before any gallery operations can succeed.
    ///
    /// Before doing an expensive refresh, this checks if the cache storage has
    /// a newer version of the cache file (e.g., from another process). If so,
    /// it reloads from storage instead of regenerating.
    pub async fn refresh_folder_cache(&self) -> Result<(), super::GalleryError> {
        use std::collections::{HashMap, HashSet};

        // Check if storage has a newer cache file before expensive refresh
        match self.folder_cache.load_if_newer(&self.cache_storage).await {
            Ok(true) => {
                info!(
                    "Folder cache reloaded from storage ({} entries)",
                    self.folder_cache.len().await
                );
                return Ok(());
            }
            Ok(false) => {} // Storage not newer, proceed with refresh
            Err(e) => {
                tracing::warn!("Failed to check folder cache staleness: {}", e);
                // Continue with refresh on error
            }
        }

        info!("Refreshing folder cache (metadata, contents, counts, previews)");
        let start_time = std::time::Instant::now();

        // 1. Get all entries in a single recursive call
        let all_entries = self.source_storage.list_recursive("").await?;

        // 2. Collect all folder paths (including root)
        let mut folder_paths: HashSet<String> = HashSet::new();
        folder_paths.insert(String::new()); // root folder

        for entry in &all_entries {
            if entry.is_dir {
                folder_paths.insert(entry.path.clone());
            }
        }

        // 3. Build parent -> direct children mapping
        // For each folder: (subdirectory names, image paths)
        let mut folder_children: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
        // Also collect all groupable files (images + RAW) with metadata for grouping
        // Key is folder path, value is Vec<(path, modification_date, file_size)>
        let mut folder_groupable_files: HashMap<
            String,
            Vec<(String, Option<std::time::SystemTime>, u64)>,
        > = HashMap::new();
        // Track direct size per folder (sum of all non-hidden files)
        let mut folder_direct_sizes: HashMap<String, u64> = HashMap::new();

        for entry in &all_entries {
            // Get parent folder path
            let parent = if let Some(last_slash) = entry.path.rfind('/') {
                entry.path[..last_slash].to_string()
            } else {
                String::new() // root
            };

            // Get entry name
            let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);

            // Skip hidden files and markdown files
            if name.starts_with('.') || name.ends_with(".md") {
                continue;
            }

            // Get extension for checking file type
            let ext = super::grouping::get_extension(name).map(|e| e.to_lowercase());
            let is_displayable_image = self.is_image(name);
            let is_raw = ext
                .as_ref()
                .map(|e| super::grouping::is_raw_extension(e))
                .unwrap_or(false);

            let children = folder_children.entry(parent.clone()).or_default();
            if entry.is_dir {
                children.0.push(name.to_string()); // subdirectory name
            } else {
                // Track file size for all non-hidden files
                let file_size = entry.metadata.as_ref().map(|m| m.size).unwrap_or(0);
                *folder_direct_sizes.entry(parent.clone()).or_default() += file_size;

                if is_displayable_image {
                    children.1.push(entry.path.clone()); // full image path
                }
            }

            // Collect groupable files (images + RAW) for the grouping algorithm
            if is_displayable_image || is_raw {
                let mod_time = entry.metadata.as_ref().and_then(|m| m.last_modified);
                let file_size = entry.metadata.as_ref().map(|m| m.size).unwrap_or(0);
                folder_groupable_files.entry(parent).or_default().push((
                    entry.path.clone(),
                    mod_time,
                    file_size,
                ));
            }
        }

        // 4. Read _folder.md for each folder and identify hidden folders
        let mut folder_metadata: HashMap<
            String,
            (Option<super::FolderMetadata>, Option<std::time::SystemTime>),
        > = HashMap::new();
        let mut hidden_folders: HashSet<String> = HashSet::new();

        for folder_path in &folder_paths {
            let metadata = self.read_folder_metadata_from_storage(folder_path).await;

            // Get the last modified time of the _folder.md file
            let folder_md_path = if folder_path.is_empty() {
                "_folder.md".to_string()
            } else {
                format!("{}/_folder.md", folder_path)
            };
            let last_modified = self
                .source_storage
                .metadata(&folder_md_path)
                .await
                .ok()
                .and_then(|m| m.last_modified);

            // Check if this folder is hidden
            if metadata.as_ref().map(|m| m.config.hidden).unwrap_or(false) {
                hidden_folders.insert(folder_path.clone());
            }

            folder_metadata.insert(folder_path.clone(), (metadata, last_modified));
        }

        // 5. Check if a folder is effectively hidden (it or any ancestor is hidden, or has __ prefix)
        let is_effectively_hidden = |path: &str| -> bool {
            // Check for __ prefix in any path component
            if super::grouping::path_contains_hidden_folder(path) {
                return true;
            }
            if hidden_folders.contains(path) {
                return true;
            }
            // Check ancestors
            let mut current = path;
            while let Some(last_slash) = current.rfind('/') {
                current = &current[..last_slash];
                if hidden_folders.contains(current) {
                    return true;
                }
            }
            // Check root
            hidden_folders.contains("")
        };

        // 6. Compute recursive image counts and sizes (bottom-up)
        // First, sort folders by depth (deepest first) for bottom-up processing
        let mut folders_by_depth: Vec<&String> = folder_paths.iter().collect();
        folders_by_depth.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));

        let mut recursive_counts: HashMap<String, usize> = HashMap::new();
        let mut recursive_sizes: HashMap<String, u64> = HashMap::new();

        for folder_path in &folders_by_depth {
            if is_effectively_hidden(folder_path) {
                recursive_counts.insert((*folder_path).clone(), 0);
                recursive_sizes.insert((*folder_path).clone(), 0);
                continue;
            }

            let (subdirs, images) = folder_children
                .get(*folder_path)
                .cloned()
                .unwrap_or_default();

            // Direct values for this folder
            let direct_count = images.len();
            let direct_size = folder_direct_sizes.get(*folder_path).copied().unwrap_or(0);

            // Add values from visible subdirectories
            let (subdir_count, subdir_size): (usize, u64) = subdirs
                .iter()
                .filter_map(|subdir_name| {
                    let subdir_path = if folder_path.is_empty() {
                        subdir_name.clone()
                    } else {
                        format!("{}/{}", folder_path, subdir_name)
                    };
                    if !is_effectively_hidden(&subdir_path) {
                        Some((
                            recursive_counts.get(&subdir_path).copied().unwrap_or(0),
                            recursive_sizes.get(&subdir_path).copied().unwrap_or(0),
                        ))
                    } else {
                        None
                    }
                })
                .fold((0, 0), |(acc_count, acc_size), (count, size)| {
                    (acc_count + count, acc_size + size)
                });

            recursive_counts.insert((*folder_path).clone(), direct_count + subdir_count);
            recursive_sizes.insert((*folder_path).clone(), direct_size + subdir_size);
        }

        // 7. Select preview images for each folder (BFS with depth limit)
        let max_preview = self.config.preview.max_images;
        let max_depth = self.config.preview.max_depth;
        let max_per_folder = self.config.preview.max_per_folder;

        // Pre-build image path -> preview item data (avoids lookups in closure)
        let image_preview_data: HashMap<String, super::CachedPreviewItem> = {
            let indexer = self.image_indexer.read().await;
            let image_cache_guard = self.image_cache.read_all().await;
            let image_cache: HashMap<String, ImageMetadata> = image_cache_guard.clone();
            drop(image_cache_guard);

            folder_children
                .values()
                .flat_map(|(_, images)| images.iter())
                .map(|img_path| {
                    (
                        img_path.clone(),
                        self.build_preview_item(img_path, &indexer, &image_cache),
                    )
                })
                .collect()
        };

        let select_preview_items = |folder_path: &str| -> Vec<super::CachedPreviewItem> {
            if is_effectively_hidden(folder_path) {
                return Vec::new();
            }

            let mut rng = rng();
            let mut previews = Vec::new();
            let mut queue: std::collections::VecDeque<(String, usize)> =
                std::collections::VecDeque::new();
            queue.push_back((folder_path.to_string(), 0));

            while let Some((current_path, depth)) = queue.pop_front() {
                if previews.len() >= max_preview || depth > max_depth {
                    break;
                }

                if is_effectively_hidden(&current_path) {
                    continue;
                }

                if let Some((subdirs, images)) = folder_children.get(&current_path) {
                    // Shuffle images for variety in preview selection
                    let mut images_shuffled: Vec<_> = images.iter().collect();
                    images_shuffled.shuffle(&mut rng);

                    // Add images from this folder using pre-built preview data
                    // Limited by both max_per_folder and max_preview
                    let mut folder_count = 0;
                    for img_path in images_shuffled {
                        if previews.len() >= max_preview || folder_count >= max_per_folder {
                            break;
                        }
                        if let Some(preview_item) = image_preview_data.get(img_path) {
                            previews.push(preview_item.clone());
                            folder_count += 1;
                        }
                    }

                    // Queue subdirectories in random order for variety
                    if depth < max_depth {
                        let mut subdirs_shuffled: Vec<_> = subdirs.iter().collect();
                        subdirs_shuffled.shuffle(&mut rng);
                        for subdir_name in subdirs_shuffled {
                            let subdir_path = if current_path.is_empty() {
                                subdir_name.clone()
                            } else {
                                format!("{}/{}", current_path, subdir_name)
                            };
                            queue.push_back((subdir_path, depth + 1));
                        }
                    }
                }
            }

            previews
        };

        // 8. Build final cache entries
        let mut new_cache: HashMap<String, super::CachedFolderMetadata> = HashMap::new();

        // Get the indexer for URL building
        let indexer = self.image_indexer.read().await;

        for folder_path in &folder_paths {
            // Skip __ prefixed folders (they contain version files, not browsable content)
            // Note: folders marked hidden via _folder.md should still have cache entries
            // so they can be accessed directly - they just don't appear in listings
            if super::grouping::path_contains_hidden_folder(folder_path) {
                continue;
            }

            let (metadata, metadata_last_modified) = folder_metadata
                .get(folder_path)
                .cloned()
                .unwrap_or((None, None));

            let (subdirs, images) = folder_children
                .get(folder_path)
                .cloned()
                .unwrap_or_default();

            // Filter out hidden subdirectories from the list
            let visible_subdirs: Vec<String> = subdirs
                .into_iter()
                .filter(|subdir_name| {
                    let subdir_path = if folder_path.is_empty() {
                        subdir_name.clone()
                    } else {
                        format!("{}/{}", folder_path, subdir_name)
                    };
                    !is_effectively_hidden(&subdir_path)
                })
                .collect();

            let recursive_count = recursive_counts.get(folder_path).copied().unwrap_or(0);
            let direct_size = folder_direct_sizes.get(folder_path).copied().unwrap_or(0);
            let recursive_size = recursive_sizes.get(folder_path).copied().unwrap_or(0);
            let preview_items = select_preview_items(folder_path);

            // Build image groups for this folder
            // Collect files from this folder and any __versions subfolder
            let mut groupable_files: Vec<(String, Option<std::time::SystemTime>, u64)> = Vec::new();

            // Files directly in this folder
            if let Some(files) = folder_groupable_files.get(folder_path) {
                groupable_files.extend(files.iter().cloned());
            }

            // Files in __versions subfolder (if it exists)
            let versions_path = if folder_path.is_empty() {
                "__versions".to_string()
            } else {
                format!("{}/__versions", folder_path)
            };
            if let Some(version_files) = folder_groupable_files.get(&versions_path) {
                groupable_files.extend(version_files.iter().cloned());
            }

            // Build image groups using the grouping algorithm
            let image_groups = super::grouping::group_files(
                groupable_files.iter().map(|(p, m, s)| (p.as_str(), *m, *s)),
                |path| {
                    indexer
                        .get_index(path)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| urlencoding::encode(path).to_string())
                },
                |url_id| self.build_thumbnail_url(url_id),
            );

            new_cache.insert(
                folder_path.clone(),
                super::CachedFolderMetadata {
                    metadata,
                    metadata_last_modified,
                    subdirectories: visible_subdirs,
                    images,
                    recursive_image_count: recursive_count,
                    direct_size,
                    recursive_size,
                    preview_items,
                    image_groups,
                },
            );
        }

        // Drop the read lock before replacing cache (we're done with indexer now)
        drop(indexer);

        // Replace entire folder cache
        self.folder_cache.replace_all(new_cache.clone()).await;

        // Rebuild image index from the collected image paths
        let all_image_paths: Vec<String> = folder_children
            .values()
            .flat_map(|(_, images)| images.iter().cloned())
            .collect();

        {
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_image_paths);
            debug!(
                "Rebuilt image index with {} images during folder cache refresh",
                all_image_paths.len()
            );
        }

        info!(
            "Folder cache refresh completed in {:.2}s: {} folders cached, {} images indexed",
            start_time.elapsed().as_secs_f64(),
            new_cache.len(),
            all_image_paths.len()
        );

        Ok(())
    }

    /// Legacy alias for refresh_folder_cache
    pub(crate) async fn refresh_folder_metadata(&self) -> Result<(), super::GalleryError> {
        self.refresh_folder_cache().await
    }

    /// Remove metadata cache entries for images that no longer exist on disk
    pub(crate) async fn remove_stale_metadata_entries(
        &self,
        current_image_paths: &[String],
    ) -> usize {
        use std::collections::HashSet;

        let current_paths: HashSet<&String> = current_image_paths.iter().collect();

        // Find stale entries (in cache but not on disk)
        let stale_paths: Vec<String> = {
            let cache = self.image_cache.read_all().await;
            cache
                .keys()
                .filter(|path| !current_paths.contains(path))
                .cloned()
                .collect()
        };

        if stale_paths.is_empty() {
            return 0;
        }

        // Remove stale entries (PersistentCache marks dirty automatically)
        let mut removed_count = 0;
        for path in &stale_paths {
            if self.image_cache.remove(path).await.is_some() {
                debug!("Removed stale metadata entry: {}", path);
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            info!(
                "Removed {} stale metadata entries for deleted/moved images",
                removed_count
            );
        }

        removed_count
    }

    /// Check if a file's metadata needs to be refreshed based on modification date.
    /// Uses pre-fetched modification times from list_recursive() to avoid individual
    /// storage.metadata() calls which are slow for S3.
    async fn needs_metadata_refresh_with_mtimes(
        &self,
        relative_path: &str,
        file_mtimes: &std::collections::HashMap<String, std::time::SystemTime>,
    ) -> bool {
        // Get cached metadata
        let cache = self.image_cache.read_all().await;
        let cached = cache.get(relative_path);

        let cached_mtime = match cached {
            None => {
                // No cached metadata, need to extract
                return true;
            }
            Some(cached_metadata) => cached_metadata.modification_date,
        };

        // Get current file modification time from pre-fetched map
        let current_mtime = file_mtimes.get(relative_path).copied();

        // Check main image file
        match (current_mtime, cached_mtime) {
            (Some(current), Some(cached)) if current > cached => {
                return true;
            }
            (Some(_), None) => {
                // Cached entry has no modification date, refresh to add it
                return true;
            }
            _ => {}
        }

        // Get sidecar paths
        let sidecars = SidecarPaths::for_image(relative_path);

        // Check XMP sidecar file using pre-fetched map
        if let Some(&xmp_mtime) = file_mtimes.get(&sidecars.xmp)
            && let Some(cached) = cached_mtime
            && xmp_mtime > cached
        {
            return true;
        }

        // Check markdown sidecar files
        for md_path in sidecars.markdown_paths() {
            if let Some(&md_mtime) = file_mtimes.get(md_path)
                && let Some(cached) = cached_mtime
                && md_mtime > cached
            {
                return true;
            }
        }

        false
    }

    /// Extract image metadata using storage abstraction.
    ///
    /// # Arguments
    /// * `relative_path` - Path to the image relative to the gallery source
    /// * `modification_date` - Pre-fetched modification date (avoids extra API call)
    pub(crate) async fn extract_image_metadata(
        &self,
        relative_path: &str,
        modification_date: Option<SystemTime>,
    ) -> Result<ImageMetadata, super::GalleryError> {
        // Get file extension
        let ext = FileExtension::from_path(relative_path);

        // Read image data with format-aware optimization
        let image_data = self
            .read_image_for_metadata(relative_path, ext.as_ref())
            .await?;

        // Extract EXIF data from the image bytes
        let (capture_date, exif_camera_info, exif_location_info) =
            self.extract_all_exif_data_from_bytes(&image_data, relative_path);

        // Get image dimensions from the data we have
        let dimensions =
            self.extract_dimensions_from_bytes(&image_data, ext.as_ref().map(|e| e.as_str()));

        // Get sidecar paths
        let sidecars = SidecarPaths::for_image(relative_path);

        // Check for XMP sidecar file using storage
        let xmp_metadata =
            read_xmp_metadata_from_storage(&self.source_storage, &sidecars.xmp).await;

        // Load user metadata from storage (handles both .md and .toml sidecars with caching)
        let user_metadata = self
            .user_metadata_storage
            .load(relative_path)
            .await
            .ok()
            .flatten();

        // Merge metadata from all sources
        let (camera_info, location_info) = merge_metadata_sources(
            exif_camera_info,
            exif_location_info,
            xmp_metadata,
            user_metadata.as_ref(),
        );

        // Override capture date if specified in user metadata
        let capture_date = if let Some(ref um) = user_metadata
            && let Some(ref date_str) = um.capture_date
            && let Ok(dt) = DateTime::parse_from_rfc3339(date_str)
        {
            Some(SystemTime::from(dt))
        } else {
            capture_date
        };

        // Extract ICC profile name / color description from bytes
        let color_profile =
            self.extract_color_profile_from_bytes(&image_data, ext.as_ref().map(|e| e.as_str()));

        Ok(ImageMetadata {
            dimensions,
            capture_date,
            camera_info,
            location_info,
            modification_date,
            color_profile,
        })
    }

    /// Read image data optimized for metadata extraction.
    ///
    /// Uses header-only reads for formats where metadata is at the start of the file,
    /// falling back to full reads when necessary.
    async fn read_image_for_metadata(
        &self,
        relative_path: &str,
        ext: Option<&FileExtension>,
    ) -> Result<bytes::Bytes, super::GalleryError> {
        let is_jpeg = ext.is_some_and(|e| e.is_jpeg());

        #[cfg(feature = "avif")]
        let is_avif = ext.is_some_and(|e| e.is_avif());
        #[cfg(not(feature = "avif"))]
        let is_avif = false;

        if is_jpeg {
            // JPEG EXIF is in the first 256KB
            Ok(self
                .source_storage
                .read_header(relative_path, header_sizes::JPEG_EXIF)
                .await?)
        } else if is_avif {
            // AVIF: always do full read because libavif needs the complete file
            // to decode and extract EXIF metadata. Header-only reads break EXIF extraction.
            Ok(self.source_storage.read(relative_path).await?)
        } else {
            // Other formats: full read
            Ok(self.source_storage.read(relative_path).await?)
        }
    }

    /// Extract image dimensions from bytes
    fn extract_dimensions_from_bytes(&self, image_data: &[u8], ext: Option<&str>) -> (u32, u32) {
        // Try using image crate's reader with a cursor
        use std::io::Cursor;
        let cursor = Cursor::new(image_data);

        if let Ok(reader) = image::ImageReader::new(cursor).with_guessed_format()
            && let Ok((w, h)) = reader.into_dimensions()
        {
            return (w, h);
        }

        // For AVIF, try our custom dimension extraction
        #[cfg(feature = "avif")]
        if ext == Some("avif")
            && let Some((w, h)) =
                super::image_processing::formats::avif::extract_dimensions_from_bytes(image_data)
        {
            return (w, h);
        }

        // For HEIC/HEIF, use libheif to extract dimensions
        #[cfg(feature = "heif")]
        if matches!(ext, Some("heic") | Some("heif"))
            && let Some((w, h)) =
                tenrankai_image::formats::heif::extract_dimensions_from_bytes(image_data)
        {
            return (w, h);
        }

        #[cfg(all(not(feature = "avif"), not(feature = "heif")))]
        let _ = ext; // Suppress unused warning

        (0, 0)
    }

    /// Extract color profile name from image bytes
    fn extract_color_profile_from_bytes(
        &self,
        image_data: &[u8],
        ext: Option<&str>,
    ) -> Option<String> {
        match ext {
            Some("jpg") | Some("jpeg") => {
                if let Some(icc_data) =
                    super::image_processing::extract_icc_profile_from_jpeg_bytes(image_data)
                {
                    super::image_processing::extract_icc_profile_name(&icc_data)
                } else {
                    None
                }
            }
            Some("png") => {
                if let Some(icc_data) =
                    super::image_processing::extract_icc_profile_from_png_bytes(image_data)
                {
                    super::image_processing::extract_icc_profile_name(&icc_data)
                } else {
                    None
                }
            }
            #[cfg(feature = "avif")]
            Some("avif") => {
                // For AVIF files, generate a descriptive color space string
                super::image_processing::extract_avif_color_description_from_bytes(image_data)
            }
            #[cfg(feature = "heif")]
            Some("heic") | Some("heif") => {
                // For HEIC/HEIF files, extract ICC profile or generate color description
                tenrankai_image::formats::heif::extract_color_description_from_bytes(image_data)
            }
            _ => None,
        }
    }

    pub(crate) async fn insert_metadata_with_tracking(
        &self,
        path: String,
        metadata: ImageMetadata,
    ) {
        // PersistentCache handles dirty tracking automatically
        self.image_cache.insert(path, metadata).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FilesystemStorage;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn create_test_storage(dir: &str) -> crate::storage::DynStorage {
        let path = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    #[tokio::test]
    #[ignore = "requires specific test image that isn't in repository"]
    async fn test_location_extraction_a7c5795() {
        // Create a test gallery instance
        let gallery_config = crate::GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: "photos".to_string(),
            cache_directory: "test_cache".to_string(),
            cache_refresh_interval_minutes: Some(60),
            ..Default::default()
        };

        let source_storage = create_test_storage(&gallery_config.source_directory);
        let cache_storage = create_test_storage(&gallery_config.cache_directory);
        let gallery = Gallery::new(gallery_config, source_storage, cache_storage);

        // Test the specific image
        let image_path = PathBuf::from("photos/landscapes/_A7C5795.jpg");
        let relative_path = "landscapes/_A7C5795.jpg";

        // First check if file exists
        if !image_path.exists() {
            println!("Working directory: {:?}", std::env::current_dir().unwrap());
            println!("Looking for file at: {:?}", image_path);
            panic!("Test image file not found");
        }

        // Extract EXIF data
        println!("Extracting EXIF data from: {:?}", image_path);

        // Read image data
        let image_data = std::fs::read(&image_path).expect("Failed to read image file");

        // Try parsing with rexif directly to debug
        let (result, warnings) = rexif::parse_buffer_quiet(&image_data);
        if !warnings.is_empty() {
            println!("EXIF warnings: {:?}", warnings);
        }
        match result {
            Ok(exif_data) => {
                println!("Successfully parsed EXIF data");
                println!("Number of EXIF entries: {}", exif_data.entries.len());

                // Look for GPS tags
                for entry in &exif_data.entries {
                    if entry.tag.to_string().contains("GPS") {
                        println!("GPS Tag: {:?} = {:?}", entry.tag, entry.value_more_readable);
                    }
                }
            }
            Err(e) => {
                println!("Failed to parse EXIF data: {}", e);
            }
        }

        let (_capture_date, camera_info, location_info) =
            gallery.extract_all_exif_data_from_bytes(&image_data, relative_path);

        // Verify GPS coordinates were extracted
        assert!(
            location_info.is_some(),
            "Location info should be extracted from _A7C5795.jpg"
        );

        if let Some(location) = location_info {
            // Print the extracted coordinates for verification
            println!("Extracted GPS coordinates:");
            println!("  Latitude: {}", location.latitude);
            println!("  Longitude: {}", location.longitude);
            println!("  Google Maps URL: {}", location.google_maps_url);
            println!("  Apple Maps URL: {}", location.apple_maps_url);

            // Basic sanity checks for coordinates
            assert!(
                location.latitude >= -90.0 && location.latitude <= 90.0,
                "Latitude should be between -90 and 90"
            );
            assert!(
                location.longitude >= -180.0 && location.longitude <= 180.0,
                "Longitude should be between -180 and 180"
            );

            // Check that map URLs are properly formatted
            assert!(
                location
                    .google_maps_url
                    .contains(&location.latitude.to_string())
            );
            assert!(
                location
                    .google_maps_url
                    .contains(&location.longitude.to_string())
            );
            assert!(
                location
                    .apple_maps_url
                    .contains(&location.latitude.to_string())
            );
            assert!(
                location
                    .apple_maps_url
                    .contains(&location.longitude.to_string())
            );
        }

        // Also check camera info was extracted
        if let Some(camera) = camera_info {
            println!("\nExtracted camera info:");
            println!("  Make: {:?}", camera.camera_make);
            println!("  Model: {:?}", camera.camera_model);
            println!("  ISO: {:?}", camera.iso);
            println!("  Aperture: {:?}", camera.aperture);
            println!("  Shutter Speed: {:?}", camera.shutter_speed);
            println!("  Focal Length: {:?}", camera.focal_length);
        }
    }
}
