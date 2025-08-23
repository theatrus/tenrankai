use super::{CameraInfo, Gallery, ImageMetadata, LocationInfo};
use chrono::{DateTime, Datelike, Local, NaiveDateTime, Timelike, Utc};
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, info, trace};

impl Gallery {
    pub(crate) async fn extract_all_exif_data(
        &self,
        image_path: &Path,
    ) -> (Option<SystemTime>, Option<CameraInfo>, Option<LocationInfo>) {
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

    fn extract_capture_date(&self, exif: &rexif::ExifData) -> Option<SystemTime> {
        // Try different date fields in order of preference
        let date_fields = [
            rexif::ExifTag::DateTimeOriginal,
            rexif::ExifTag::DateTimeDigitized,
            rexif::ExifTag::DateTime,
        ];

        for field in &date_fields {
            if let Some(entry) = exif.entries.iter().find(|e| e.tag == *field) {
                if let Some(date) = self.parse_exif_datetime(&entry.value_more_readable) {
                    debug!("Found capture date in {:?}: {:?}", field, date);
                    return Some(date);
                }
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

        if has_data {
            Some(camera_info)
        } else {
            None
        }
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
        // GPS coordinates in EXIF are typically in the format: "51 deg 30' 45.60\""
        let parts: Vec<&str> = coord_str.split_whitespace().collect();

        if parts.len() >= 6 {
            let degrees = parts[0]
                .parse::<f64>()
                .map_err(|_| "Invalid degrees")?;
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
            Err("Invalid GPS coordinate format".to_string())
        }
    }

    pub async fn refresh_all_metadata(&self) -> Result<(), super::GalleryError> {
        use walkdir::WalkDir;
        
        info!("Starting full metadata refresh");
        let start_time = std::time::Instant::now();
        let mut count = 0;

        for entry in WalkDir::new(&self.config.source_directory)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && self.is_image(&path.file_name().unwrap_or_default().to_string_lossy()) {
                if let Ok(relative_path) = path.strip_prefix(&self.config.source_directory) {
                    let relative_str = relative_path.to_string_lossy().to_string();
                    
                    // Extract metadata for this image
                    if let Ok(metadata) = self.extract_image_metadata(path).await {
                        let mut cache = self.metadata_cache.write().await;
                        cache.insert(relative_str, metadata);
                        count += 1;
                        
                        if count % 100 == 0 {
                            debug!("Processed {} images...", count);
                        }
                    }
                }
            }
        }

        // Save the cache to disk
        self.save_metadata_cache().await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "Metadata refresh completed: {} images in {:.2}s",
            count,
            elapsed.as_secs_f64()
        );
        
        Ok(())
    }
    
    pub(crate) async fn extract_image_metadata(&self, path: &Path) -> Result<ImageMetadata, super::GalleryError> {
        // Get image dimensions
        let dimensions = match image::image_dimensions(path) {
            Ok((w, h)) => (w, h),
            Err(_) => (0, 0),
        };
        
        // Extract EXIF data
        let (capture_date, camera_info, location_info) = self.extract_all_exif_data(path).await;
        
        Ok(ImageMetadata {
            dimensions,
            capture_date,
            camera_info,
            location_info,
        })
    }
}

pub(crate) fn format_capture_date(system_time: SystemTime) -> String {
    let datetime = DateTime::<Local>::from(system_time);
    
    format!(
        "{} {}, {} at {:02}:{:02}:{:02}",
        datetime.format("%B"),
        datetime.day(),
        datetime.year(),
        datetime.hour(),
        datetime.minute(),
        datetime.second()
    )
}