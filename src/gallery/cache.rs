use super::{CacheMetadata, Gallery, ImageMetadata};
use std::collections::HashMap;
use tracing::{debug, error, info};

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

    fn generate_cache_key(&self, path: &str, size: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(path);
        hasher.update(size);
        format!("{:x}", hasher.finalize())
    }

    /// Generate a cache key for regular images with size and format
    pub(crate) fn generate_image_cache_key(&self, path: &str, size: &str, format: &str) -> String {
        let cache_key = format!("{}_{}", size, format);
        self.generate_cache_key(path, &cache_key)
    }

    /// Generate a cache filename for storing in filesystem
    pub(crate) fn generate_cache_filename(&self, path: &str, size: &str, format: &str) -> String {
        let hash = self.generate_image_cache_key(path, size, format);
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

    /// Generate a composite image cache key with format
    pub(crate) fn generate_composite_cache_key_with_format(
        &self,
        gallery_path: &str,
        format: &str,
    ) -> String {
        let composite_key = Self::generate_composite_cache_key(gallery_path);
        self.generate_cache_key(&composite_key, format)
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
        let gallery = Gallery::new(
            default_config.galleries.unwrap()[0].clone(),
            default_config.app,
        );

        // Test regular image cache keys
        let path = "vacation/beach.jpg";
        let size = "thumbnail";
        let format = "webp";

        // These should produce consistent keys
        let key1 = gallery.generate_image_cache_key(path, size, format);
        let key2 = gallery.generate_image_cache_key(path, size, format);
        assert_eq!(key1, key2, "Cache keys should be identical for same inputs");

        // Different inputs should produce different keys
        let key3 = gallery.generate_image_cache_key(path, "medium", format);
        assert_ne!(key1, key3, "Different sizes should produce different keys");

        // Test that the same inputs always produce the same hash
        let another_key = gallery.generate_image_cache_key(path, size, format);
        assert_eq!(key1, another_key, "Keys should be deterministic");

        // Test composite cache keys
        let comp_key1 = Gallery::generate_composite_cache_key("gallery/2024");
        let comp_key2 = Gallery::generate_composite_cache_key("gallery/2024");
        assert_eq!(comp_key1, comp_key2, "Composite keys should be consistent");
        assert_eq!(comp_key1, "composite_gallery_2024");

        // Test root composite
        let root_key = Gallery::generate_composite_cache_key("");
        assert_eq!(root_key, "composite_root");

        // Test composite cache key with format
        let comp_format_key =
            gallery.generate_composite_cache_key_with_format("gallery/2024", "jpg");
        // Should be a hash since it goes through generate_cache_key
        assert_ne!(
            comp_format_key, "composite_gallery_2024_jpg",
            "Should be hashed"
        );
    }

    #[test]
    fn test_cache_filename_generation() {
        let default_config = crate::Config::default();
        let gallery = Gallery::new(
            default_config.galleries.unwrap()[0].clone(),
            default_config.app,
        );

        let filename = gallery.generate_cache_filename("test.jpg", "thumbnail", "webp");
        assert!(
            filename.ends_with(".webp"),
            "Filename should end with correct extension"
        );

        // Verify the hash part is consistent
        let hash = gallery.generate_image_cache_key("test.jpg", "thumbnail", "webp");
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
        };

        let app_config = crate::AppConfig {
            name: "Test".to_string(),
            log_level: "info".to_string(),
            download_secret: "secret".to_string(),
            download_password: "pass".to_string(),
            copyright_holder: None,
            base_url: None,
        };

        let gallery = Gallery::new(config, app_config);

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
        };

        let app_config = crate::AppConfig {
            name: "Test".to_string(),
            log_level: "info".to_string(),
            download_secret: "secret".to_string(),
            download_password: "pass".to_string(),
            copyright_holder: None,
            base_url: None,
        };

        let gallery = Gallery::new(config, app_config);

        // Should be able to access hidden folder directly
        let items = gallery.scan_directory("hidden").await.unwrap();
        assert_eq!(items.len(), 1); // Should see the image
        assert_eq!(items[0].name, "test.jpg");
    }
}
