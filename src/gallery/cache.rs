use super::{CacheMetadata, Gallery, ImageMetadata};
use crate::GalleryConfig;
use std::collections::HashMap;
use tracing::{debug, error, info};

impl Gallery {
    pub async fn initialize_and_check_version(&self) -> Result<(), super::GalleryError> {
        let current_version = env!("CARGO_PKG_VERSION");
        
        let mut metadata = self.cache_metadata.write().await;
        let needs_refresh = metadata.version != current_version;
        
        if needs_refresh {
            info!("Version change detected ({}), refreshing metadata cache", current_version);
            
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
            
            // Trigger a full metadata refresh
            self.refresh_all_metadata().await?;
        }
        
        Ok(())
    }

    pub fn start_background_cache_refresh(gallery: super::SharedGallery, interval_minutes: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick
            
            loop {
                interval.tick().await;
                info!("Starting scheduled metadata cache refresh");
                
                if let Err(e) = gallery.refresh_all_metadata().await {
                    error!("Failed to refresh metadata cache: {}", e);
                }
            }
        });
    }
    
    pub fn start_periodic_cache_save(gallery: super::SharedGallery, interval_minutes: u64) {
        use std::sync::atomic::Ordering;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
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
                        gallery.metadata_updates_since_save.store(0, Ordering::Relaxed);
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
}

pub(crate) fn load_metadata_cache(config: &GalleryConfig) -> Result<HashMap<String, ImageMetadata>, super::GalleryError> {
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

pub(crate) fn load_cache_metadata(config: &GalleryConfig) -> Result<CacheMetadata, super::GalleryError> {
    let metadata_file = config.cache_directory.join("cache_metadata.json");
    
    if !metadata_file.exists() {
        debug!("Cache metadata file not found");
        return Err(super::GalleryError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cache metadata not found"
        )));
    }
    
    let json = std::fs::read_to_string(&metadata_file)?;
    let metadata: CacheMetadata = serde_json::from_str(&json)?;
    
    debug!("Loaded cache metadata: version={}", metadata.version);
    Ok(metadata)
}