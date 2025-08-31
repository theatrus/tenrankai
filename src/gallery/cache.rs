use super::image_processing::OutputFormat;
use super::{CacheMetadata, Gallery, ImageMetadata};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Tracks which formats are available for a specific image and size
#[derive(Debug, Clone, Default)]
pub struct FormatCoverage {
    pub has_jpeg: bool,
    pub has_webp: bool,
    pub has_png: bool,
    #[cfg(feature = "avif")]
    pub has_avif: bool,
}

impl FormatCoverage {
    /// Get all expected formats for a given source image path
    fn expected_formats(source_path: &str) -> Vec<OutputFormat> {
        let mut formats = vec![OutputFormat::Jpeg];

        // Skip WebP for PNG sources to preserve transparency
        if !source_path.to_lowercase().ends_with(".png") {
            formats.push(OutputFormat::WebP);
        }

        #[cfg(feature = "avif")]
        formats.push(OutputFormat::Avif);

        formats
    }

    /// Check if this coverage has all expected formats for the source
    pub fn is_complete(&self, source_path: &str) -> bool {
        let expected = Self::expected_formats(source_path);

        expected.iter().all(|format| match format {
            OutputFormat::Jpeg => self.has_jpeg,
            OutputFormat::WebP => self.has_webp,
            OutputFormat::Png => self.has_png,
            #[cfg(feature = "avif")]
            OutputFormat::Avif => self.has_avif,
        })
    }

    /// Get missing formats for this coverage
    pub fn missing_formats(&self, source_path: &str) -> Vec<OutputFormat> {
        let expected = Self::expected_formats(source_path);

        expected
            .into_iter()
            .filter(|format| match format {
                OutputFormat::Jpeg => !self.has_jpeg,
                OutputFormat::WebP => !self.has_webp,
                OutputFormat::Png => !self.has_png,
                #[cfg(feature = "avif")]
                OutputFormat::Avif => !self.has_avif,
            })
            .collect()
    }
}

impl Gallery {
    pub async fn initialize_and_check_version(&self) -> Result<(), super::GalleryError> {
        let current_version = env!("CARGO_PKG_VERSION");

        let mut metadata = self.cache_metadata.write().await;
        let needs_refresh = metadata.version != current_version;

        if needs_refresh {
            info!(
                "Version change detected ({}), refreshing metadata cache",
                current_version
            );

            // Clear the old metadata cache
            let mut cache = self.metadata_cache.write().await;
            cache.clear();
            drop(cache);

            // Update version and trigger refresh
            metadata.version = current_version.to_string();
            metadata.last_full_refresh = std::time::SystemTime::now();
            drop(metadata);

            // Save the updated cache metadata
            self.save_cache_metadata().await?;
        }

        Ok(())
    }

    pub fn start_background_cache_refresh(gallery: super::SharedGallery, interval_minutes: u64) {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick

            loop {
                interval.tick().await;
                info!("Starting scheduled metadata cache refresh");

                let pregenerate = gallery.config.pregenerate_cache;
                if let Err(e) = gallery
                    .clone()
                    .refresh_metadata_and_pregenerate_cache(pregenerate)
                    .await
                {
                    error!("Failed to refresh metadata cache: {}", e);
                }
            }
        });
    }

    pub fn start_periodic_cache_save(gallery: super::SharedGallery, interval_minutes: u64) {
        use std::sync::atomic::Ordering;

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick

            loop {
                interval.tick().await;

                // Check if cache is dirty
                if gallery.metadata_cache_dirty.load(Ordering::Relaxed) {
                    debug!("Cache is dirty, saving to disk");

                    if let Err(e) = gallery.save_metadata_cache().await {
                        error!("Failed to save metadata cache: {}", e);
                    } else {
                        // Reset dirty flag and update counter
                        gallery.metadata_cache_dirty.store(false, Ordering::Relaxed);
                        gallery
                            .metadata_updates_since_save
                            .store(0, Ordering::Relaxed);
                        info!("Periodic metadata cache save completed");
                    }
                }
            }
        });
    }

    pub(crate) async fn save_metadata_cache(&self) -> Result<(), super::GalleryError> {
        use std::sync::atomic::Ordering;

        let cache_file = self.config.cache_directory.join("metadata_cache.json");
        let cache = self.metadata_cache.read().await;

        let json = serde_json::to_string_pretty(&*cache)?;
        tokio::fs::write(cache_file, json).await?;

        // Reset dirty flag after successful save
        self.metadata_cache_dirty.store(false, Ordering::Relaxed);
        self.metadata_updates_since_save.store(0, Ordering::Relaxed);

        Ok(())
    }

    pub(crate) async fn save_cache_metadata(&self) -> Result<(), super::GalleryError> {
        let metadata_file = self.config.cache_directory.join("cache_metadata.json");
        let metadata = self.cache_metadata.read().await;

        let json = serde_json::to_string_pretty(&*metadata)?;
        tokio::fs::write(metadata_file, json).await?;

        Ok(())
    }

    pub async fn save_caches(&self) -> Result<(), super::GalleryError> {
        // Create cache directory if it doesn't exist
        tokio::fs::create_dir_all(&self.config.cache_directory).await?;

        // Save both caches
        self.save_metadata_cache().await?;
        self.save_cache_metadata().await?;

        info!("Saved gallery caches to disk");
        Ok(())
    }

    pub(crate) fn generate_cache_key(&self, path: &str, size: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(path);
        hasher.update(size);
        format!("{:x}", hasher.finalize())
    }

    /// Generate a cache key for regular images with size, format, and watermark status
    pub(crate) fn generate_image_cache_key(
        &self,
        path: &str,
        size: &str,
        format: &str,
        has_watermark: bool,
    ) -> String {
        let cache_key = if has_watermark {
            format!("{}_{}_{}", size, format, "watermarked")
        } else {
            format!("{}_{}", size, format)
        };
        self.generate_cache_key(path, &cache_key)
    }

    /// Generate a cache filename for storing in filesystem with watermark status
    pub(crate) fn generate_cache_filename(
        &self,
        path: &str,
        size: &str,
        format: &str,
        has_watermark: bool,
    ) -> String {
        let hash = self.generate_image_cache_key(path, size, format, has_watermark);
        format!("{}.{}", hash, format)
    }

    /// Generate a cache key for composite images
    pub(crate) fn generate_composite_cache_key(gallery_path: &str) -> String {
        let safe_path = if gallery_path.is_empty() {
            "root".to_string()
        } else {
            gallery_path.replace('/', "_")
        };
        format!("composite_{}", safe_path)
    }

    /// Pre-generate cache for a single image
    pub async fn pregenerate_image_cache(
        &self,
        relative_path: &str,
    ) -> Result<(), super::GalleryError> {
        self.pregenerate_image_cache_selective(relative_path, false)
            .await
    }

    /// Pre-generate cache for a single image with option to only generate missing formats
    pub async fn pregenerate_image_cache_selective(
        &self,
        relative_path: &str,
        only_missing: bool,
    ) -> Result<(), super::GalleryError> {
        if !self.is_image(relative_path) {
            return Ok(());
        }

        let full_path = self.config.source_directory.join(relative_path);
        if !full_path.exists() {
            return Ok(());
        }

        let sizes = vec![
            "thumbnail",
            "thumbnail@2x",
            "gallery",
            "gallery@2x",
            "medium",
            "medium@2x",
            "large",
            "large@2x",
        ];
        let mut total_generated = 0;

        for size in &sizes {
            let formats_to_generate = if only_missing {
                // Only generate missing formats
                match self.check_format_coverage(relative_path, size).await {
                    Ok(coverage) => coverage.missing_formats(relative_path),
                    Err(e) => {
                        debug!(
                            "Failed to check format coverage for {} {}: {}",
                            relative_path, size, e
                        );
                        continue;
                    }
                }
            } else {
                // Generate all expected formats
                let mut formats = vec![OutputFormat::Jpeg];

                // Skip WebP for PNG sources to preserve transparency
                if !relative_path.to_lowercase().ends_with(".png") {
                    formats.push(OutputFormat::WebP);
                }

                #[cfg(feature = "avif")]
                formats.push(OutputFormat::Avif);

                formats
            };

            for format in formats_to_generate {
                match self
                    .get_resized_image(&full_path, relative_path, size, format)
                    .await
                {
                    Ok(_) => {
                        let action = if only_missing {
                            "Generated missing"
                        } else {
                            "Pre-generated"
                        };
                        debug!(
                            "{} {} {} for {}",
                            action,
                            size,
                            format.extension(),
                            relative_path
                        );
                        total_generated += 1;
                    }
                    Err(e) => {
                        let action = if only_missing {
                            "generate missing"
                        } else {
                            "pre-generate"
                        };
                        error!(
                            "Failed to {} {} {} for {}: {}",
                            action,
                            size,
                            format.extension(),
                            relative_path,
                            e
                        );
                    }
                }
            }
        }

        if only_missing && total_generated > 0 {
            info!(
                "Generated {} missing format variants for {}",
                total_generated, relative_path
            );
        }

        Ok(())
    }

    /// Pre-generate cache for all images in the gallery
    pub async fn pregenerate_all_images_cache(self: Arc<Self>) -> Result<(), super::GalleryError> {
        info!(
            "Starting cache pre-generation for gallery '{}'",
            self.config.name
        );

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
        };

        let total_images = image_paths.len();
        info!("Found {} images to pre-generate cache for", total_images);

        for (index, image_path) in image_paths.into_iter().enumerate() {
            if let Err(e) = self.pregenerate_image_cache(&image_path).await {
                error!("Failed to pre-generate cache for {}: {}", image_path, e);
            }

            if (index + 1) % 10 == 0 || index + 1 == total_images {
                info!(
                    "Pre-generated cache for {}/{} images",
                    index + 1,
                    total_images
                );
            }
        }

        info!(
            "Completed cache pre-generation for gallery '{}'",
            self.config.name
        );
        Ok(())
    }

    /// Generate missing formats for all images in the gallery
    pub async fn generate_all_missing_formats(self: Arc<Self>) -> Result<(), super::GalleryError> {
        info!(
            "Starting missing format generation for gallery '{}'",
            self.config.name
        );

        // First, report current format coverage
        if let Err(e) = self.report_format_coverage().await {
            debug!("Failed to report format coverage: {}", e);
        }

        // Then analyze what formats are missing
        let missing_formats_map = self.analyze_missing_formats().await?;
        let total_missing_variants = missing_formats_map.len();

        if total_missing_variants == 0 {
            info!("No missing formats found in gallery '{}'", self.config.name);
            return Ok(());
        }

        info!(
            "Found {} missing format variants to generate",
            total_missing_variants
        );

        // Get unique image paths that have missing formats
        let mut unique_images: HashSet<String> = HashSet::new();
        for (image_path, _) in missing_formats_map.keys() {
            unique_images.insert(image_path.clone());
        }

        let unique_image_count = unique_images.len();
        let mut processed_count = 0;

        for image_path in unique_images {
            if let Err(e) = self
                .pregenerate_image_cache_selective(&image_path, true)
                .await
            {
                error!(
                    "Failed to generate missing formats for {}: {}",
                    image_path, e
                );
            }

            processed_count += 1;
            if processed_count % 10 == 0 || processed_count == unique_image_count {
                info!(
                    "Generated missing formats for {}/{} images",
                    processed_count, unique_image_count
                );
            }
        }

        info!(
            "Completed missing format generation for gallery '{}'",
            self.config.name
        );
        Ok(())
    }

    /// Check what formats are available for a specific image and size in cache
    pub async fn check_format_coverage(
        &self,
        relative_path: &str,
        size: &str,
    ) -> Result<FormatCoverage, super::GalleryError> {
        let mut coverage = FormatCoverage::default();

        let formats_to_check = vec![
            (OutputFormat::Jpeg, &mut coverage.has_jpeg),
            (OutputFormat::WebP, &mut coverage.has_webp),
            (OutputFormat::Png, &mut coverage.has_png),
            #[cfg(feature = "avif")]
            (OutputFormat::Avif, &mut coverage.has_avif),
        ];

        for (format, has_format) in formats_to_check {
            // Skip WebP for PNG sources
            if format == OutputFormat::WebP && relative_path.to_lowercase().ends_with(".png") {
                *has_format = true; // Mark as "has" since we don't expect it
                continue;
            }

            // Determine if watermark applies (only for medium + copyright holder)
            let is_medium = size == "medium" || size == "medium@2x";
            let apply_watermark = is_medium && self.config.copyright_holder.is_some();

            let cache_filename = self.generate_cache_filename(
                relative_path,
                size,
                format.extension(),
                apply_watermark,
            );
            let cache_path = self.config.cache_directory.join(&cache_filename);
            let source_path = self.config.source_directory.join(relative_path);

            // Check if cache file exists and is newer than source
            *has_format = if cache_path.exists() {
                match (
                    tokio::fs::metadata(&cache_path).await,
                    tokio::fs::metadata(&source_path).await,
                ) {
                    (Ok(cache_meta), Ok(source_meta)) => {
                        match (cache_meta.modified(), source_meta.modified()) {
                            (Ok(cache_time), Ok(source_time)) => cache_time >= source_time,
                            _ => false,
                        }
                    }
                    _ => false,
                }
            } else {
                false
            };
        }

        Ok(coverage)
    }

    /// Get missing formats for all images and sizes
    pub async fn analyze_missing_formats(
        &self,
    ) -> Result<HashMap<(String, String), Vec<OutputFormat>>, super::GalleryError> {
        let mut missing_formats = HashMap::new();
        let sizes = vec!["thumbnail", "gallery", "medium", "large"];

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
        };

        for image_path in image_paths {
            if !self.is_image(&image_path) {
                continue;
            }

            for size in &sizes {
                match self.check_format_coverage(&image_path, size).await {
                    Ok(coverage) => {
                        let missing = coverage.missing_formats(&image_path);
                        if !missing.is_empty() {
                            missing_formats.insert((image_path.clone(), size.to_string()), missing);
                        }
                    }
                    Err(e) => {
                        debug!(
                            "Failed to check format coverage for {} {}: {}",
                            image_path, size, e
                        );
                    }
                }
            }
        }

        Ok(missing_formats)
    }

    /// Generate only missing formats for a specific image
    pub async fn generate_missing_formats(
        &self,
        relative_path: &str,
    ) -> Result<(), super::GalleryError> {
        self.pregenerate_image_cache_selective(relative_path, true)
            .await
    }

    /// Analyze and report format coverage statistics for the gallery
    pub async fn report_format_coverage(&self) -> Result<(), super::GalleryError> {
        info!(
            "Analyzing format coverage for gallery '{}'",
            self.config.name
        );

        let missing_formats_map = self.analyze_missing_formats().await?;
        let sizes = vec!["thumbnail", "gallery", "medium", "large"];

        // Get all image paths from metadata cache
        let image_paths: Vec<String> = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache.keys().cloned().collect()
        };

        let total_images = image_paths.len();
        if total_images == 0 {
            info!("No images found in gallery '{}'", self.config.name);
            return Ok(());
        }

        // Count format coverage for each size
        let mut coverage_stats = HashMap::new();
        for size in &sizes {
            let mut format_counts = HashMap::new();

            // Initialize counters
            format_counts.insert("jpeg", 0);
            format_counts.insert("webp", 0);
            format_counts.insert("png", 0);
            #[cfg(feature = "avif")]
            format_counts.insert("avif", 0);

            for image_path in &image_paths {
                if !self.is_image(image_path) {
                    continue;
                }

                match self.check_format_coverage(image_path, size).await {
                    Ok(coverage) => {
                        if coverage.has_jpeg {
                            *format_counts.get_mut("jpeg").unwrap() += 1;
                        }
                        if coverage.has_webp || image_path.to_lowercase().ends_with(".png") {
                            *format_counts.get_mut("webp").unwrap() += 1;
                        }
                        if coverage.has_png {
                            *format_counts.get_mut("png").unwrap() += 1;
                        }
                        #[cfg(feature = "avif")]
                        if coverage.has_avif {
                            *format_counts.get_mut("avif").unwrap() += 1;
                        }
                    }
                    Err(e) => debug!(
                        "Failed to check format coverage for {} {}: {}",
                        image_path, size, e
                    ),
                }
            }

            coverage_stats.insert(size, format_counts);
        }

        // Report statistics
        info!(
            "=== Format Coverage Report for Gallery '{}' ===",
            self.config.name
        );
        info!("Total images: {}", total_images);

        for size in &sizes {
            if let Some(format_counts) = coverage_stats.get(size) {
                info!("Size '{}' coverage:", size);

                for (format, &count) in format_counts {
                    let percentage = if total_images > 0 {
                        (count as f64 / total_images as f64) * 100.0
                    } else {
                        0.0
                    };

                    info!(
                        "  {}: {}/{} ({:.1}%)",
                        format.to_uppercase(),
                        count,
                        total_images,
                        percentage
                    );
                }
            }
        }

        let total_missing = missing_formats_map.len();
        if total_missing > 0 {
            info!(
                "Found {} missing format variants across all sizes",
                total_missing
            );

            // Count missing by format type
            let mut missing_by_format = HashMap::new();
            for (_, formats) in missing_formats_map {
                for format in formats {
                    *missing_by_format.entry(format.extension()).or_insert(0) += 1;
                }
            }

            info!("Missing formats breakdown:");
            for (format, count) in missing_by_format {
                info!("  {}: {} missing", format.to_uppercase(), count);
            }
        } else {
            info!("✓ All expected formats are available for all images");
        }

        info!("=== End Format Coverage Report ===");
        Ok(())
    }
}

pub(crate) fn load_metadata_cache(
    config: &crate::GallerySystemConfig,
) -> Result<HashMap<String, ImageMetadata>, super::GalleryError> {
    let cache_file = config.cache_directory.join("metadata_cache.json");

    if !cache_file.exists() {
        debug!("Metadata cache file not found, starting with empty cache");
        return Ok(HashMap::new());
    }

    let json = std::fs::read_to_string(&cache_file)?;
    let cache: HashMap<String, ImageMetadata> = serde_json::from_str(&json)?;

    info!("Loaded {} cached image metadata entries", cache.len());
    Ok(cache)
}

pub(crate) fn load_cache_metadata(
    config: &crate::GallerySystemConfig,
) -> Result<CacheMetadata, super::GalleryError> {
    let metadata_file = config.cache_directory.join("cache_metadata.json");

    if !metadata_file.exists() {
        debug!("Cache metadata file not found");
        return Err(super::GalleryError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cache metadata not found",
        )));
    }

    let json = std::fs::read_to_string(&metadata_file)?;
    let metadata: CacheMetadata = serde_json::from_str(&json)?;

    debug!("Loaded cache metadata: version={}", metadata.version);
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::super::Gallery;

    #[test]
    fn test_cache_key_consistency() {
        let default_config = crate::Config::default();
        let gallery = Gallery::new(default_config.galleries.unwrap()[0].clone());

        // Test regular image cache keys
        let path = "vacation/beach.jpg";
        let size = "thumbnail";
        let format = "webp";

        // These should produce consistent keys
        let key1 = gallery.generate_image_cache_key(path, size, format, false);
        let key2 = gallery.generate_image_cache_key(path, size, format, false);
        assert_eq!(key1, key2, "Cache keys should be identical for same inputs");

        // Different inputs should produce different keys
        let key3 = gallery.generate_image_cache_key(path, "medium", format, false);
        assert_ne!(key1, key3, "Different sizes should produce different keys");

        // Test that the same inputs always produce the same hash
        let another_key = gallery.generate_image_cache_key(path, size, format, false);
        assert_eq!(key1, another_key, "Keys should be deterministic");

        // Test watermark differentiation
        let key_with_watermark = gallery.generate_image_cache_key(path, size, format, true);
        assert_ne!(
            key1, key_with_watermark,
            "Watermarked and non-watermarked keys should differ"
        );

        // Test composite cache keys
        let comp_key1 = Gallery::generate_composite_cache_key("gallery/2024");
        let comp_key2 = Gallery::generate_composite_cache_key("gallery/2024");
        assert_eq!(comp_key1, comp_key2, "Composite keys should be consistent");
        assert_eq!(comp_key1, "composite_gallery_2024");

        // Test root composite
        let root_key = Gallery::generate_composite_cache_key("");
        assert_eq!(root_key, "composite_root");
    }

    #[test]
    fn test_cache_filename_generation() {
        let default_config = crate::Config::default();
        let gallery = Gallery::new(default_config.galleries.unwrap()[0].clone());

        let filename = gallery.generate_cache_filename("test.jpg", "thumbnail", "webp", false);
        assert!(
            filename.ends_with(".webp"),
            "Filename should end with correct extension"
        );

        // Verify the hash part is consistent
        let hash = gallery.generate_image_cache_key("test.jpg", "thumbnail", "webp", false);
        assert_eq!(filename, format!("{}.webp", hash));
    }

    #[tokio::test]
    async fn test_hidden_folder_not_in_listing() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();

        // Create visible folder
        let visible_dir = source_dir.join("visible");
        fs::create_dir_all(&visible_dir).unwrap();
        fs::write(
            visible_dir.join("_folder.md"),
            "# Visible Folder\nThis is visible",
        )
        .unwrap();

        // Create hidden folder with TOML front matter
        let hidden_dir = source_dir.join("hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(
            hidden_dir.join("_folder.md"),
            r#"+++
hidden = true
title = "Hidden Folder"
+++

# Hidden Content

This folder should not appear in listings.
"#,
        )
        .unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: source_dir,
            cache_directory: cache_dir,
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
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
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
            copyright_holder: None,
        };

        let gallery = Gallery::new(config);

        let items = gallery.scan_directory("").await.unwrap();

        // Should have 1 visible folder, hidden folder should not appear
        assert_eq!(items.len(), 1);
        assert!(items.iter().any(|i| i.name == "visible"));
        assert!(!items.iter().any(|i| i.name == "hidden"));
    }

    #[tokio::test]
    async fn test_hidden_folder_directly_accessible() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("photos");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();

        // Create hidden folder
        let hidden_dir = source_dir.join("hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(
            hidden_dir.join("_folder.md"),
            r#"+++
hidden = true
+++
Hidden folder
"#,
        )
        .unwrap();
        // Create a test image file
        fs::write(hidden_dir.join("test.jpg"), vec![0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            url_prefix: "/gallery".to_string(),
            source_directory: source_dir,
            cache_directory: cache_dir,
            gallery_template: "gallery.html".to_string(),
            image_detail_template: "image.html".to_string(),
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
                max_images: 4,
                max_depth: 3,
                max_per_folder: 3,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: Some(85),
            webp_quality: Some(85.0),
            pregenerate_cache: false,
            new_threshold_days: None,
            approximate_dates_for_public: false,
            copyright_holder: None,
        };

        let gallery = Gallery::new(config);

        // Should be able to access hidden folder directly
        let items = gallery.scan_directory("hidden").await.unwrap();
        assert_eq!(items.len(), 1); // Should see the image
        assert_eq!(items[0].name, "test.jpg");
    }
}
