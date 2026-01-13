use super::metadata_sources::{
    merge_metadata_sources, read_image_markdown_metadata_from_storage,
    read_xmp_metadata_from_storage,
};
use super::path_utils::{FileExtension, SidecarPaths};
use super::{CameraInfo, Gallery, ImageMetadata, LocationInfo};
use crate::storage::header_sizes;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, error, info, trace};

impl Gallery {
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
                        // Parse the EXIF data from bytes
                        match rexif::parse_buffer(&exif_bytes) {
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
            _ => {
                // For other formats (JPEG, etc), use rexif's buffer parser
                match rexif::parse_buffer(image_data) {
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
            // If image doesn't exist, remove from cache
            let mut cache = self.metadata_cache.write().await;
            if cache.remove(relative_path).is_some() {
                self.metadata_cache_dirty
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                debug!("Removed deleted image from cache: {}", relative_path);
            }

            // Also update the indexer - rebuild it without this image
            drop(cache); // Release the cache lock first
            let all_paths: Vec<String> = {
                let cache = self.metadata_cache.read().await;
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
                let cache = self.metadata_cache.read().await;
                cache.keys().cloned().collect()
            };
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_paths);
        }

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
                let cache = self.metadata_cache.read().await;
                cache.keys().cloned().collect()
            };
            let mut indexer = self.image_indexer.write().await;
            indexer.build_index(&all_paths);
            debug!("Rebuilt image index after directory refresh");
        }

        Ok(())
    }

    pub async fn refresh_all_metadata(&self) -> Result<(), super::GalleryError> {
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

        // Save the cache to disk if any changes were made
        if refreshed_count > 0 || removed_count > 0 {
            self.save_metadata_cache().await?;
        }

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

    /// Remove metadata cache entries for images that no longer exist on disk
    pub(crate) async fn remove_stale_metadata_entries(
        &self,
        current_image_paths: &[String],
    ) -> usize {
        use std::collections::HashSet;

        let current_paths: HashSet<&String> = current_image_paths.iter().collect();

        // Find stale entries (in cache but not on disk)
        let stale_paths: Vec<String> = {
            let cache = self.metadata_cache.read().await;
            cache
                .keys()
                .filter(|path| !current_paths.contains(path))
                .cloned()
                .collect()
        };

        if stale_paths.is_empty() {
            return 0;
        }

        // Remove stale entries
        let mut cache = self.metadata_cache.write().await;
        let mut removed_count = 0;

        for path in &stale_paths {
            if cache.remove(path).is_some() {
                debug!("Removed stale metadata entry: {}", path);
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            self.metadata_cache_dirty
                .store(true, std::sync::atomic::Ordering::Relaxed);
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
        let cache = self.metadata_cache.read().await;
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

        // Check for markdown metadata file using storage (e.g., image.jpg.md or image.md)
        let markdown_metadata =
            read_image_markdown_metadata_from_storage(&self.source_storage, relative_path).await;

        // Merge metadata from all sources
        let (camera_info, location_info) = merge_metadata_sources(
            exif_camera_info,
            exif_location_info,
            xmp_metadata,
            markdown_metadata.as_ref().map(|m| m.config.clone()),
        );

        // Override capture date if specified in markdown
        let capture_date = if let Some(ref md) = markdown_metadata
            && let Some(ref date_str) = md.config.capture_date
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
            // AVIF: try header read, fall back if dimensions not found
            let header_data = self
                .source_storage
                .read_header(relative_path, header_sizes::AVIF_METADATA)
                .await?;

            // Check if we got dimensions from the header
            let dimensions = self.extract_dimensions_from_bytes(&header_data, Some("avif"));
            if dimensions == (0, 0) {
                debug!(
                    "AVIF header read didn't contain dimensions, falling back to full read: {}",
                    relative_path
                );
                Ok(self.source_storage.read(relative_path).await?)
            } else {
                Ok(header_data)
            }
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

        #[cfg(not(feature = "avif"))]
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
            _ => None,
        }
    }

    pub(crate) async fn insert_metadata_with_tracking(
        &self,
        path: String,
        metadata: ImageMetadata,
    ) {
        use std::sync::atomic::Ordering;

        let mut cache = self.metadata_cache.write().await;
        cache.insert(path, metadata);

        // Mark cache as dirty
        self.metadata_cache_dirty.store(true, Ordering::Relaxed);

        // Increment update counter
        let updates = self
            .metadata_updates_since_save
            .fetch_add(1, Ordering::Relaxed)
            + 1;

        // If we've made enough updates, trigger a save
        const UPDATES_BEFORE_SAVE: usize = 100;
        if updates >= UPDATES_BEFORE_SAVE {
            drop(cache); // Release the lock before saving

            if let Err(e) = self.save_metadata_cache().await {
                error!(
                    "Failed to save metadata cache after {} updates: {}",
                    updates, e
                );
            } else {
                self.metadata_cache_dirty.store(false, Ordering::Relaxed);
                self.metadata_updates_since_save.store(0, Ordering::Relaxed);
                debug!("Saved metadata cache after {} updates", updates);
            }
        }
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

    fn create_test_storage_from_path(path: &std::path::Path) -> crate::storage::DynStorage {
        std::fs::create_dir_all(path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    #[tokio::test]
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
        match rexif::parse_buffer(&image_data) {
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
