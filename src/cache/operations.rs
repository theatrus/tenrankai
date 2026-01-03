use crate::{CacheType, gallery::{CacheMetadata, ImageMetadata}};
use std::collections::HashMap;
use tracing::{debug, info};

/// Load image metadata cache from disk
pub(crate) fn load_image_metadata_cache(
    config: &crate::GallerySystemConfig,
) -> Result<HashMap<String, ImageMetadata>, crate::gallery::GalleryError> {
    let cache_type = CacheType::ImageMetadata;
    let cache_file = config.cache_directory.join(cache_type.filename(None));

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
    config: &crate::GallerySystemConfig,
) -> Result<CacheMetadata, crate::gallery::GalleryError> {
    let cache_type = CacheType::CacheMetadata;
    let metadata_file = config.cache_directory.join(cache_type.filename(None));

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