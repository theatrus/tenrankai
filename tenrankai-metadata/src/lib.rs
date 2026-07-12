pub mod error;
pub mod providers;
pub mod types;

pub use error::MetadataStorageError;
pub use providers::{SidecarMetadataStorage, StorageMetadataBackend};
pub use types::{AstroObject, AstroSolution, Comment, ImageUserMetadata, PickStatus};

use async_trait::async_trait;

/// Trait for pluggable metadata storage backends
///
/// Uses relative paths (strings) instead of filesystem paths to support
/// storage abstraction (filesystem, S3, etc.)
#[async_trait]
pub trait MetadataStorage: Send + Sync {
    /// Load metadata for a specific image by relative path
    async fn load(
        &self,
        relative_path: &str,
    ) -> Result<Option<ImageUserMetadata>, MetadataStorageError>;

    /// Save metadata for a specific image by relative path
    async fn save(
        &self,
        relative_path: &str,
        metadata: &ImageUserMetadata,
    ) -> Result<(), MetadataStorageError>;

    /// Persist a plate solution into the app-managed sidecar without
    /// rewriting the human-authored .md file. `None` clears it.
    async fn save_astro(
        &self,
        relative_path: &str,
        astro: Option<&types::AstroSolution>,
    ) -> Result<(), MetadataStorageError> {
        // Default: read-modify-write through the full metadata round trip
        let mut metadata = self.load(relative_path).await?.unwrap_or_default();
        metadata.astro = astro.cloned();
        self.save(relative_path, &metadata).await
    }

    /// Delete metadata for a specific image by relative path
    async fn delete(&self, relative_path: &str) -> Result<(), MetadataStorageError>;

    /// List all images that have metadata
    async fn list_all(&self) -> Result<Vec<String>, MetadataStorageError>;

    /// Batch load metadata for multiple images
    async fn load_batch(
        &self,
        relative_paths: &[&str],
    ) -> Result<Vec<(String, ImageUserMetadata)>, MetadataStorageError> {
        let mut results = Vec::new();
        for path in relative_paths {
            if let Some(metadata) = self.load(path).await? {
                results.push((path.to_string(), metadata));
            }
        }
        Ok(results)
    }
}
