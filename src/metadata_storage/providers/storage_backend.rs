use crate::metadata_storage::{ImageUserMetadata, MetadataStorage, MetadataStorageError};
use crate::storage::DynStorage;
use async_trait::async_trait;

/// Storage-backed metadata storage
///
/// Stores metadata in TOML sidecar files using the storage abstraction.
/// Works with filesystem, S3, or any other storage backend.
///
/// Naming convention:
/// - For image.jpg -> image.toml
pub struct StorageMetadataBackend {
    storage: DynStorage,
}

impl StorageMetadataBackend {
    pub fn new(storage: DynStorage) -> Self {
        Self { storage }
    }

    /// Get the sidecar file path for an image
    fn get_sidecar_path(image_path: &str) -> String {
        // Replace extension with .toml
        // e.g., image.jpg -> image.toml
        if let Some(dot_pos) = image_path.rfind('.') {
            format!("{}.toml", &image_path[..dot_pos])
        } else {
            format!("{}.toml", image_path)
        }
    }
}

#[async_trait]
impl MetadataStorage for StorageMetadataBackend {
    async fn load(
        &self,
        relative_path: &str,
    ) -> Result<Option<ImageUserMetadata>, MetadataStorageError> {
        let sidecar_path = Self::get_sidecar_path(relative_path);

        match self.storage.read_to_string(&sidecar_path).await {
            Ok(content) => {
                let metadata: ImageUserMetadata = toml_edit::de::from_str(&content)
                    .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;
                Ok(Some(metadata))
            }
            Err(crate::storage::StorageError::NotFound(_)) => Ok(None),
            Err(e) => Err(MetadataStorageError::Io(std::io::Error::other(
                e.to_string(),
            ))),
        }
    }

    async fn save(
        &self,
        relative_path: &str,
        metadata: &ImageUserMetadata,
    ) -> Result<(), MetadataStorageError> {
        let sidecar_path = Self::get_sidecar_path(relative_path);

        // Serialize to TOML with pretty formatting
        let content = toml_edit::ser::to_string_pretty(metadata)
            .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;

        // Write via storage abstraction
        self.storage
            .write(&sidecar_path, bytes::Bytes::from(content))
            .await
            .map_err(|e| MetadataStorageError::Io(std::io::Error::other(e.to_string())))?;

        Ok(())
    }

    async fn delete(&self, relative_path: &str) -> Result<(), MetadataStorageError> {
        let sidecar_path = Self::get_sidecar_path(relative_path);

        match self.storage.delete(&sidecar_path).await {
            Ok(()) => Ok(()),
            Err(crate::storage::StorageError::NotFound(_)) => Ok(()),
            Err(e) => Err(MetadataStorageError::Io(std::io::Error::other(
                e.to_string(),
            ))),
        }
    }

    async fn list_all(&self) -> Result<Vec<String>, MetadataStorageError> {
        // List all .toml files in the storage and extract image paths
        match self.storage.list_recursive("").await {
            Ok(entries) => {
                let metadata_files: Vec<String> = entries
                    .into_iter()
                    .filter(|e| !e.is_dir && e.path.ends_with(".toml"))
                    .filter_map(|e| {
                        // Convert sidecar path back to image path (without .toml)
                        if let Some(pos) = e.path.rfind(".toml") {
                            Some(e.path[..pos].to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(metadata_files)
            }
            Err(e) => Err(MetadataStorageError::Io(std::io::Error::other(
                e.to_string(),
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_storage::PickStatus;
    use crate::storage::FilesystemStorage;
    use std::sync::Arc;

    fn create_test_storage(dir: &std::path::Path) -> DynStorage {
        std::fs::create_dir_all(dir).ok();
        Arc::new(FilesystemStorage::new(dir))
    }

    #[test]
    fn test_sidecar_path_generation() {
        assert_eq!(
            StorageMetadataBackend::get_sidecar_path("photos/vacation/beach.jpg"),
            "photos/vacation/beach.toml"
        );
        assert_eq!(
            StorageMetadataBackend::get_sidecar_path("photos/vacation/sunset.png"),
            "photos/vacation/sunset.toml"
        );
        assert_eq!(
            StorageMetadataBackend::get_sidecar_path("beach"),
            "beach.toml"
        );
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let mut metadata = ImageUserMetadata::new();
        metadata.highlighted = true;
        metadata.pick_status = Some(PickStatus::Pick);
        metadata.add_comment("user1".to_string(), "Great shot!".to_string(), None);
        metadata.tags = vec!["landscape".to_string(), "sunset".to_string()];

        // Save metadata
        backend.save("test_image.jpg", &metadata).await.unwrap();

        // Verify sidecar file exists
        let sidecar_path = temp_dir.path().join("test_image.toml");
        assert!(sidecar_path.exists());

        // Load it back
        let loaded = backend.load("test_image.jpg").await.unwrap().unwrap();
        assert!(loaded.highlighted);
        assert_eq!(loaded.pick_status, Some(PickStatus::Pick));
        assert_eq!(loaded.comments.len(), 1);
        assert_eq!(loaded.comments[0].text, "Great shot!");
        assert_eq!(loaded.tags.len(), 2);

        // Test delete
        backend.delete("test_image.jpg").await.unwrap();
        assert!(backend.load("test_image.jpg").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let loaded = backend.load("nonexistent.jpg").await.unwrap();
        assert!(loaded.is_none());
    }
}
