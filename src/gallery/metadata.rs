use super::metadata_sources::{
    merge_metadata_sources, read_image_markdown_metadata, read_xmp_metadata,
};
use super::{CameraInfo, Gallery, ImageMetadata, LocationInfo};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, error, info, trace};

impl Gallery {
    pub(crate) async fn extract_all_exif_data(
        &self,
        image_path: &Path,
    ) -> (Option<SystemTime>, Option<CameraInfo>, Option<LocationInfo>) {
        // Check file extension to determine extraction method
        let extension = image_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());

        match extension.as_deref() {
            #[cfg(feature = "avif")]
            Some("avif") => {
                // For AVIF files, extract EXIF data using libavif
                match super::image_processing::formats::avif::extract_exif_data(image_path) {
                    Some(exif_bytes) => {
                        // Parse the EXIF data from bytes
                        match rexif::parse_buffer(&exif_bytes) {
                            Ok(exif_data) => {
                                let capture_date = self.extract_capture_date(&exif_data);
                                let camera_info = self.extract_camera_info(&exif_data);
                                let location_info = self.extract_location_info(&exif_data);
                                debug!("Successfully extracted EXIF from AVIF: {:?}", image_path);
                                (capture_date, camera_info, location_info)
                            }
                            Err(e) => {
                                trace!(
                                    "Failed to parse EXIF data from AVIF {}: {}",
                                    image_path.display(),
                                    e
                                );
                                (None, None, None)
                            }
                        }
                    }
                    None => {
                        trace!("No EXIF data found in AVIF: {}", image_path.display());
                        (None, None, None)
                    }
                }
            }
            _ => {
                // For other formats (JPEG, etc), use rexif's file parser
                match rexif::parse_file(image_path) {
                    Ok(exif_data) => {
                        let capture_date = self.extract_capture_date(&exif_data);
                        let camera_info = self.extract_camera_info(&exif_data);
                        let location_info = self.extract_location_info(&exif_data);
                        (capture_date, camera_info, location_info)
                    }
                    Err(e) => {
                        trace!("No EXIF data for {}: {}", image_path.display(), e);
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
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.exists() {
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

        // Extract and cache metadata
        if let Ok(metadata) = self.extract_image_metadata(&full_path).await {
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
        use walkdir::WalkDir;

        let full_path = self.config.source_directory.join(directory_path);
        let mut count = 0;

        for entry in WalkDir::new(&full_path)
            .follow_links(true)
            .max_depth(1) // Only immediate children
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file()
                && self.is_image(&path.file_name().unwrap_or_default().to_string_lossy())
                && let Ok(relative_path) = path.strip_prefix(&self.config.source_directory)
            {
                let relative_str = relative_path.to_string_lossy().replace('\\', "/");

                if let Ok(metadata) = self.extract_image_metadata(path).await {
                    self.insert_metadata_with_tracking(relative_str, metadata)
                        .await;
                    count += 1;
                }
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
        use walkdir::WalkDir;

        info!("Starting metadata refresh (checking modification dates)");
        let start_time = std::time::Instant::now();
        let mut refreshed_count = 0;
        let mut skipped_count = 0;
        let mut all_image_paths = Vec::new();

        for entry in WalkDir::new(&self.config.source_directory)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file()
                && self.is_image(&path.file_name().unwrap_or_default().to_string_lossy())
                && let Ok(relative_path) = path.strip_prefix(&self.config.source_directory)
            {
                let relative_str = relative_path.to_string_lossy().replace('\\', "/");

                // Collect all image paths for indexing
                all_image_paths.push(relative_str.clone());

                // Check if we need to refresh this file's metadata
                let needs_refresh = self.needs_metadata_refresh(path, &relative_str).await;

                if needs_refresh {
                    // Extract metadata for this image
                    if let Ok(metadata) = self.extract_image_metadata(path).await {
                        self.insert_metadata_with_tracking(relative_str, metadata)
                            .await;
                        refreshed_count += 1;

                        if refreshed_count % 100 == 0 {
                            debug!("Refreshed {} images...", refreshed_count);
                        }
                    }
                } else {
                    skipped_count += 1;
                }
            }
        }

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
    pub(crate) async fn remove_stale_metadata_entries(&self, current_image_paths: &[String]) -> usize {
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

    /// Check if a file's metadata needs to be refreshed based on modification date
    /// Also checks sidecar files (XMP, markdown) for changes
    async fn needs_metadata_refresh(&self, path: &Path, relative_path: &str) -> bool {
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

        // Get current file modification time
        let current_mtime = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

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

        // Check XMP sidecar file
        let xmp_path = path.with_extension("xmp");
        if xmp_path.exists()
            && let Ok(meta) = tokio::fs::metadata(&xmp_path).await
            && let Ok(xmp_mtime) = meta.modified()
            && let Some(cached) = cached_mtime
            && xmp_mtime > cached
        {
            return true;
        }

        // Check markdown sidecar files (image.jpg.md or image.md)
        let md_path1 = path.with_file_name(format!(
            "{}.md",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        let md_path2 = path.with_extension("md");

        for md_path in [md_path1, md_path2] {
            if md_path.exists()
                && let Ok(meta) = tokio::fs::metadata(&md_path).await
                && let Ok(md_mtime) = meta.modified()
                && let Some(cached) = cached_mtime
                && md_mtime > cached
            {
                return true;
            }
        }

        false
    }

    pub(crate) async fn extract_image_metadata(
        &self,
        path: &Path,
    ) -> Result<ImageMetadata, super::GalleryError> {
        // Get image dimensions
        #[allow(unused_variables)] // ext is used conditionally based on features
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());
        let dimensions = match image::image_dimensions(path) {
            Ok((w, h)) => (w, h),
            Err(_) => {
                // For AVIF, try our custom dimension extraction
                #[cfg(feature = "avif")]
                if ext.as_deref() == Some("avif") {
                    super::image_processing::formats::avif::extract_dimensions(path)
                        .unwrap_or((0, 0))
                } else {
                    (0, 0)
                }
                #[cfg(not(feature = "avif"))]
                {
                    (0, 0)
                }
            }
        };

        // Extract EXIF data
        let (capture_date, exif_camera_info, exif_location_info) =
            self.extract_all_exif_data(path).await;

        // Check for XMP sidecar file
        let xmp_path = path.with_extension("xmp");
        let xmp_metadata = if xmp_path.exists() {
            read_xmp_metadata(&xmp_path).await
        } else {
            None
        };

        // Check for markdown metadata file (e.g., image.jpg.md or image.md)
        let markdown_metadata = read_image_markdown_metadata(path).await;

        // Merge metadata from all sources
        let (camera_info, location_info) = merge_metadata_sources(
            exif_camera_info,
            exif_location_info,
            xmp_metadata,
            markdown_metadata.as_ref().map(|m| m.config.clone()),
        );

        // Override capture date if specified in markdown
        let capture_date = if let Some(ref md) = markdown_metadata {
            if let Some(ref date_str) = md.config.capture_date {
                // Try to parse ISO 8601 date
                if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
                    Some(SystemTime::from(dt))
                } else {
                    capture_date
                }
            } else {
                capture_date
            }
        } else {
            capture_date
        };

        // Get file modification date
        let modification_date = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

        // Extract ICC profile name if present
        let color_profile = match path.extension().and_then(|s| s.to_str()) {
            Some("jpg") | Some("jpeg") => {
                if let Some(icc_data) = super::image_processing::extract_icc_profile_from_jpeg(path)
                {
                    super::image_processing::extract_icc_profile_name(&icc_data)
                } else {
                    None
                }
            }
            Some("png") => {
                if let Some(icc_data) = super::image_processing::extract_icc_profile_from_png(path)
                {
                    super::image_processing::extract_icc_profile_name(&icc_data)
                } else {
                    None
                }
            }
            #[cfg(feature = "avif")]
            Some("avif") => {
                // For AVIF files, generate a descriptive color space string
                super::image_processing::formats::avif::extract_color_description(path)
            }
            _ => None,
        };

        Ok(ImageMetadata {
            dimensions,
            capture_date,
            camera_info,
            location_info,
            modification_date,
            color_profile,
        })
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
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_location_extraction_a7c5795() {
        // Create a test gallery instance
        let gallery_config = crate::GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: PathBuf::from("photos"),
            cache_directory: PathBuf::from("test_cache"),
            cache_refresh_interval_minutes: Some(60),
            ..Default::default()
        };

        let gallery = Gallery::new(gallery_config);

        // Test the specific image
        let image_path = PathBuf::from("photos/landscapes/_A7C5795.jpg");

        // First check if file exists
        if !image_path.exists() {
            println!("Working directory: {:?}", std::env::current_dir().unwrap());
            println!("Looking for file at: {:?}", image_path);
            panic!("Test image file not found");
        }

        // Extract EXIF data
        println!("Extracting EXIF data from: {:?}", image_path);

        // Try parsing with rexif directly to debug
        match rexif::parse_file(&image_path) {
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
            gallery.extract_all_exif_data(&image_path).await;

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
