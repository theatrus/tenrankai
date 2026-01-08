pub mod error;
pub mod providers;
pub mod types;

pub use error::MetadataStorageError;
pub use providers::SidecarMetadataStorage;
pub use types::{Comment, ImageUserMetadata, PickStatus};

use async_trait::async_trait;
use std::path::Path;

/// Trait for pluggable metadata storage backends
#[async_trait]
pub trait MetadataStorage: Send + Sync {
    /// Load metadata for a specific image
    async fn load(
        &self,
        image_path: &Path,
    ) -> Result<Option<ImageUserMetadata>, MetadataStorageError>;

    /// Save metadata for a specific image
    async fn save(
        &self,
        image_path: &Path,
        metadata: &ImageUserMetadata,
    ) -> Result<(), MetadataStorageError>;

    /// Delete metadata for a specific image
    async fn delete(&self, image_path: &Path) -> Result<(), MetadataStorageError>;

    /// List all images that have metadata
    async fn list_all(&self) -> Result<Vec<String>, MetadataStorageError>;

    /// Batch load metadata for multiple images
    async fn load_batch(
        &self,
        image_paths: &[&Path],
    ) -> Result<Vec<(String, ImageUserMetadata)>, MetadataStorageError> {
        let mut results = Vec::new();
        for path in image_paths {
            if let Some(metadata) = self.load(path).await? {
                results.push((path.to_string_lossy().to_string(), metadata));
            }
        }
        Ok(results)
    }
}
