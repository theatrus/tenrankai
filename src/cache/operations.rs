use crate::{
    CacheType,
    gallery::{CacheMetadata, ImageMetadata},
    storage::{DynStorage, StorageError},
};
use bytes::Bytes;
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, info};

/// Load image metadata cache from storage
pub(crate) async fn load_image_metadata_cache(
    storage: &DynStorage,
) -> Result<HashMap<String, ImageMetadata>, crate::gallery::GalleryError> {
    let cache_type = CacheType::ImageMetadata;
    let cache_file = cache_type.filename(None);

    match storage.exists(&cache_file).await {
        Ok(true) => {}
        Ok(false) => {
            debug!("Metadata cache file not found, starting with empty cache");
            return Ok(HashMap::new());
        }
        Err(e) => {
            debug!("Error checking metadata cache file: {}", e);
            return Ok(HashMap::new());
        }
    }

    let data = storage.read(&cache_file).await?;
    let json = String::from_utf8(data.to_vec())
        .map_err(|e| crate::gallery::GalleryError::IoError(std::io::Error::other(e)))?;
    let cache: HashMap<String, ImageMetadata> = serde_json::from_str(&json)?;

    info!("Loaded {} cached image metadata entries", cache.len());
    Ok(cache)
}

/// Load cache version metadata from storage
pub(crate) async fn load_cache_version_metadata(
    storage: &DynStorage,
) -> Result<CacheMetadata, crate::gallery::GalleryError> {
    let cache_type = CacheType::CacheMetadata;
    let metadata_file = cache_type.filename(None);

    match storage.exists(&metadata_file).await {
        Ok(true) => {}
        Ok(false) => {
            debug!("Cache metadata file not found");
            return Err(crate::gallery::GalleryError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cache metadata not found",
            )));
        }
        Err(e) => {
            return Err(crate::gallery::GalleryError::IoError(std::io::Error::other(
                e.to_string(),
            )));
        }
    }

    let data = storage.read(&metadata_file).await?;
    let json = String::from_utf8(data.to_vec())
        .map_err(|e| crate::gallery::GalleryError::IoError(std::io::Error::other(e)))?;
    let metadata: CacheMetadata = serde_json::from_str(&json)?;

    debug!("Loaded cache metadata: version={}", metadata.version);
    Ok(metadata)
}

/// Save any serializable data as JSON to storage
pub(crate) async fn save_cache_json<T: Serialize>(
    cache_type: CacheType,
    storage: &DynStorage,
    data: &T,
) -> Result<(), StorageError> {
    let cache_file = cache_type.filename(None);
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| StorageError::Other(e.to_string()))?;
    storage.write(&cache_file, Bytes::from(json)).await
}

/// Save image metadata cache to storage
pub(crate) async fn save_image_metadata_cache(
    storage: &DynStorage,
    metadata: &HashMap<String, ImageMetadata>,
) -> Result<(), StorageError> {
    save_cache_json(CacheType::ImageMetadata, storage, metadata).await
}

/// Save cache version metadata to storage
pub(crate) async fn save_cache_version_metadata(
    storage: &DynStorage,
    metadata: &CacheMetadata,
) -> Result<(), StorageError> {
    save_cache_json(CacheType::CacheMetadata, storage, metadata).await
}
