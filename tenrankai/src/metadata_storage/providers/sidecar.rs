use crate::metadata_storage::{ImageUserMetadata, MetadataStorage, MetadataStorageError};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Sidecar file metadata storage (filesystem-based)
///
/// Stores metadata in TOML files alongside images with the naming convention:
/// - For image.jpg -> image.toml
/// - Same pattern as markdown files (image.jpg -> image.md)
///
/// This implementation uses a base path and relative paths.
pub struct SidecarMetadataStorage {
    /// Base path for resolving relative paths
    base_path: PathBuf,
    /// Whether to create parent directories if they don't exist
    create_dirs: bool,
}

impl SidecarMetadataStorage {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            create_dirs: true,
        }
    }

    /// Get the sidecar file path for an image
    fn get_sidecar_path(&self, relative_path: &str) -> PathBuf {
        // Replace extension with .toml
        // e.g., image.jpg -> image.toml
        let full_path = self.base_path.join(relative_path);
        let mut sidecar_path = full_path;
        sidecar_path.set_extension("toml");
        sidecar_path
    }
}

#[async_trait]
impl MetadataStorage for SidecarMetadataStorage {
    async fn load(
        &self,
        relative_path: &str,
    ) -> Result<Option<ImageUserMetadata>, MetadataStorageError> {
        let sidecar_path = self.get_sidecar_path(relative_path);

        match fs::read_to_string(&sidecar_path).await {
            Ok(content) => {
                // Use toml_edit's serde feature to deserialize
                let metadata: ImageUserMetadata = toml_edit::de::from_str(&content)
                    .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;
                Ok(Some(metadata))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn save(
        &self,
        relative_path: &str,
        metadata: &ImageUserMetadata,
    ) -> Result<(), MetadataStorageError> {
        let sidecar_path = self.get_sidecar_path(relative_path);

        // Create parent directory if needed
        if self.create_dirs
            && let Some(parent) = sidecar_path.parent()
        {
            fs::create_dir_all(parent).await?;
        }

        // Serialize to TOML with pretty formatting
        let content = toml_edit::ser::to_string_pretty(metadata)
            .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;

        // Write atomically by writing to temp file first
        let temp_path = sidecar_path.with_extension("toml.tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await?;
        drop(file);

        // Rename temp file to final location
        fs::rename(temp_path, sidecar_path).await?;

        Ok(())
    }

    async fn delete(&self, relative_path: &str) -> Result<(), MetadataStorageError> {
        let sidecar_path = self.get_sidecar_path(relative_path);

        match fs::remove_file(&sidecar_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_all(&self) -> Result<Vec<String>, MetadataStorageError> {
        // This would need to scan directories for sidecar files
        // For now, return empty list as this is primarily used by other storage backends
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_storage::PickStatus;

    #[test]
    fn test_sidecar_path_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = SidecarMetadataStorage::new(temp_dir.path().to_path_buf());

        let sidecar = storage.get_sidecar_path("photos/vacation/beach.jpg");
        assert_eq!(sidecar, temp_dir.path().join("photos/vacation/beach.toml"));

        let sidecar_png = storage.get_sidecar_path("photos/vacation/sunset.png");
        assert_eq!(
            sidecar_png,
            temp_dir.path().join("photos/vacation/sunset.toml")
        );

        let sidecar_no_ext = storage.get_sidecar_path("photos/vacation/beach");
        assert_eq!(
            sidecar_no_ext,
            temp_dir.path().join("photos/vacation/beach.toml")
        );
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = SidecarMetadataStorage::new(temp_dir.path().to_path_buf());

        let mut metadata = ImageUserMetadata::new();
        metadata.highlighted = true;
        metadata.pick_status = Some(PickStatus::Pick);
        metadata.add_comment("user1".to_string(), "Great shot!".to_string(), None);
        metadata.tags = vec!["landscape".to_string(), "sunset".to_string()];

        // Save metadata using relative path
        storage.save("test_image.jpg", &metadata).await.unwrap();

        // Verify sidecar file exists
        let sidecar_path = temp_dir.path().join("test_image.toml");
        assert!(tokio::fs::metadata(&sidecar_path).await.is_ok());

        // Load it back
        let loaded = storage.load("test_image.jpg").await.unwrap().unwrap();
        assert!(loaded.highlighted);
        assert_eq!(loaded.pick_status, Some(PickStatus::Pick));
        assert_eq!(loaded.comments.len(), 1);
        assert_eq!(loaded.comments[0].text, "Great shot!");
        assert_eq!(loaded.comments[0].author, "user1");
        assert_eq!(loaded.tags.len(), 2);
        assert_eq!(loaded.tags[0], "landscape");
        assert_eq!(loaded.tags[1], "sunset");

        // Test delete
        storage.delete("test_image.jpg").await.unwrap();
        assert!(storage.load("test_image.jpg").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_toml_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = SidecarMetadataStorage::new(temp_dir.path().to_path_buf());

        let mut metadata = ImageUserMetadata::new();
        metadata.highlighted = true;
        metadata.pick_status = Some(PickStatus::NoPick);
        metadata.add_comment("alice".to_string(), "Nice composition".to_string(), None);
        metadata.add_comment("bob".to_string(), "I agree!".to_string(), None);
        metadata.tags = vec!["nature".to_string(), "macro".to_string()];

        // Save metadata
        storage.save("photo.jpg", &metadata).await.unwrap();

        // Read the TOML file directly
        let sidecar_path = temp_dir.path().join("photo.toml");
        let toml_content = tokio::fs::read_to_string(&sidecar_path).await.unwrap();

        // Verify TOML structure
        assert!(toml_content.contains("highlighted = true"));
        assert!(toml_content.contains("pick_status = \"no_pick\""));
        assert!(toml_content.contains("[[comments]]"));
        assert!(toml_content.contains("author = \"alice\""));
        assert!(toml_content.contains("text = \"Nice composition\""));
        assert!(toml_content.contains("author = \"bob\""));
        assert!(toml_content.contains("text = \"I agree!\""));
        assert!(toml_content.contains("tags = ["));
        assert!(toml_content.contains("\"nature\""));
        assert!(toml_content.contains("\"macro\""));
    }

    #[tokio::test]
    async fn test_empty_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = SidecarMetadataStorage::new(temp_dir.path().to_path_buf());

        // Load non-existent metadata
        let loaded = storage.load("empty.jpg").await.unwrap();
        assert!(loaded.is_none());

        // Save empty metadata
        let metadata = ImageUserMetadata::new();
        storage.save("empty.jpg", &metadata).await.unwrap();

        // Load it back
        let loaded = storage.load("empty.jpg").await.unwrap().unwrap();
        assert!(!loaded.highlighted);
        assert!(loaded.pick_status.is_none());
        assert!(loaded.comments.is_empty());
        assert!(loaded.tags.is_empty());
    }
}
