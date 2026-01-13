use crate::{
    CacheType,
    gallery::{CacheMetadata, ImageMetadata},
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Load image metadata cache from disk
pub(crate) fn load_image_metadata_cache(
    _config: &crate::GallerySystemConfig,
    cache_path: &Path,
) -> Result<HashMap<String, ImageMetadata>, crate::gallery::GalleryError> {
    let cache_type = CacheType::ImageMetadata;
    let cache_file = cache_path.join(cache_type.filename(None));

    if !cache_file.exists() {
        debug!("Metadata cache file not found, starting with empty cache");
        return Ok(HashMap::new());
    }

    let json = std::fs::read_to_string(&cache_file)?;
    let cache: HashMap<String, ImageMetadata> = serde_json::from_str(&json)?;

    info!("Loaded {} cached image metadata entries", cache.len());
    Ok(cache)
}

/// Load cache version metadata from disk
pub(crate) fn load_cache_version_metadata(
    _config: &crate::GallerySystemConfig,
    cache_path: &Path,
) -> Result<CacheMetadata, crate::gallery::GalleryError> {
    let cache_type = CacheType::CacheMetadata;
    let metadata_file = cache_path.join(cache_type.filename(None));

    if !metadata_file.exists() {
        debug!("Cache metadata file not found");
        return Err(crate::gallery::GalleryError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cache metadata not found",
        )));
    }

    let json = std::fs::read_to_string(&metadata_file)?;
    let metadata: CacheMetadata = serde_json::from_str(&json)?;

    debug!("Loaded cache metadata: version={}", metadata.version);
    Ok(metadata)
}

/// Save any serializable data as JSON to a cache file
pub(crate) async fn save_cache_json<T: Serialize>(
    cache_type: CacheType,
    cache_directory: &Path,
    data: &T,
) -> Result<(), std::io::Error> {
    let cache_file = cache_directory.join(cache_type.filename(None));
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    tokio::fs::write(cache_file, json).await
}

/// Save image metadata cache to disk
pub(crate) async fn save_image_metadata_cache(
    cache_directory: &Path,
    metadata: &HashMap<String, ImageMetadata>,
) -> Result<(), std::io::Error> {
    save_cache_json(CacheType::ImageMetadata, cache_directory, metadata).await
}

/// Save cache version metadata to disk
pub(crate) async fn save_cache_version_metadata(
    cache_directory: &Path,
    metadata: &CacheMetadata,
) -> Result<(), std::io::Error> {
    save_cache_json(CacheType::CacheMetadata, cache_directory, metadata).await
}
