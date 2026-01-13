use crate::metadata_storage::{ImageUserMetadata, MetadataStorage, MetadataStorageError};
use crate::storage::DynStorage;
use async_trait::async_trait;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default LRU cache size if not configured
const DEFAULT_CACHE_SIZE: usize = 1000;

/// Default cache TTL in seconds
const DEFAULT_CACHE_TTL_SECS: u64 = 60;

/// Cached entry with timestamp for TTL expiration
struct CacheEntry {
    data: ImageUserMetadata,
    cached_at: Instant,
}

/// Storage-backed metadata storage with LRU caching and TTL
///
/// Stores metadata in two sidecar files using the storage abstraction:
/// - `.md` files: title, description, location, technical overrides (human-editable)
/// - `.toml` files: picks, comments, tags, AI analysis (app-managed)
///
/// Includes a write-through LRU cache with configurable TTL for read performance.
pub struct StorageMetadataBackend {
    storage: DynStorage,
    cache: Mutex<LruCache<String, CacheEntry>>,
    ttl: Duration,
}

/// Fields stored in the .md sidecar file (TOML frontmatter + markdown body)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MdFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aperture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutter_speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telescope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_exposure_time: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ra: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

/// Fields stored in the .toml sidecar file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TomlMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<crate::metadata_storage::Comment>,
    #[serde(default)]
    pub highlighted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_status: Option<crate::metadata_storage::PickStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_alt_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl StorageMetadataBackend {
    pub fn new(storage: DynStorage) -> Self {
        Self::with_options(storage, DEFAULT_CACHE_SIZE, DEFAULT_CACHE_TTL_SECS)
    }

    pub fn with_cache_size(storage: DynStorage, cache_size: usize) -> Self {
        Self::with_options(storage, cache_size, DEFAULT_CACHE_TTL_SECS)
    }

    pub fn with_options(storage: DynStorage, cache_size: usize, ttl_secs: u64) -> Self {
        let size = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            storage,
            cache: Mutex::new(LruCache::new(size)),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Check if a cache entry is still valid (not expired)
    fn is_entry_valid(&self, entry: &CacheEntry) -> bool {
        entry.cached_at.elapsed() < self.ttl
    }

    /// Get the .toml sidecar file path for an image
    fn get_toml_path(image_path: &str) -> String {
        if let Some(dot_pos) = image_path.rfind('.') {
            format!("{}.toml", &image_path[..dot_pos])
        } else {
            format!("{}.toml", image_path)
        }
    }

    /// Get the .md sidecar file path for an image (tries IMAGE.jpg.md first, then IMAGE.md)
    fn get_md_paths(image_path: &str) -> (String, String) {
        let extension = image_path.rsplit('.').next().unwrap_or("");
        let stem = if let Some(dot_pos) = image_path.rfind('.') {
            &image_path[..dot_pos]
        } else {
            image_path
        };

        // Full extension path: IMAGE.jpg.md
        let full_ext_path = format!("{}.{}.md", stem, extension);
        // Simple path: IMAGE.md
        let simple_path = format!("{}.md", stem);

        (full_ext_path, simple_path)
    }

    /// Read and parse .md sidecar file
    async fn read_md_file(&self, image_path: &str) -> Option<(MdFrontmatter, String)> {
        let (full_ext_path, simple_path) = Self::get_md_paths(image_path);

        // Try full extension path first
        if let Ok(content) = self.storage.read_to_string(&full_ext_path).await {
            return Self::parse_md_content(&content);
        }

        // Then try simple path
        if let Ok(content) = self.storage.read_to_string(&simple_path).await {
            return Self::parse_md_content(&content);
        }

        None
    }

    /// Parse markdown content with TOML frontmatter
    fn parse_md_content(content: &str) -> Option<(MdFrontmatter, String)> {
        let trimmed = content.trim_start();

        if trimmed.starts_with("+++") {
            let parts: Vec<&str> = content.splitn(3, "+++").collect();
            if parts.len() >= 3 {
                let toml_content = parts[1];
                // Trim leading whitespace but preserve internal whitespace, trim trailing newlines
                let markdown_body = parts[2].trim_start().trim_end_matches('\n').to_string();

                match toml_edit::de::from_str::<MdFrontmatter>(toml_content) {
                    Ok(frontmatter) => Some((frontmatter, markdown_body)),
                    Err(_) => Some((MdFrontmatter::default(), content.trim_end_matches('\n').to_string())),
                }
            } else {
                Some((MdFrontmatter::default(), content.trim_end_matches('\n').to_string()))
            }
        } else {
            // No frontmatter, just markdown body
            Some((MdFrontmatter::default(), content.trim_end_matches('\n').to_string()))
        }
    }

    /// Read and parse .toml sidecar file
    async fn read_toml_file(&self, image_path: &str) -> Option<TomlMetadata> {
        let toml_path = Self::get_toml_path(image_path);

        match self.storage.read_to_string(&toml_path).await {
            Ok(content) => toml_edit::de::from_str(&content).ok(),
            Err(_) => None,
        }
    }

    /// Write .md sidecar file
    async fn write_md_file(
        &self,
        image_path: &str,
        frontmatter: &MdFrontmatter,
        description: Option<&str>,
    ) -> Result<(), MetadataStorageError> {
        // Use the simple .md path for writing
        let (_, simple_path) = Self::get_md_paths(image_path);

        // Only write if there's actual content
        let has_frontmatter = frontmatter.title.is_some()
            || frontmatter.location.is_some()
            || frontmatter.camera_make.is_some()
            || frontmatter.camera_model.is_some()
            || frontmatter.lens_model.is_some()
            || frontmatter.iso.is_some()
            || frontmatter.aperture.is_some()
            || frontmatter.shutter_speed.is_some()
            || frontmatter.focal_length.is_some()
            || frontmatter.capture_date.is_some()
            || frontmatter.telescope.is_some()
            || frontmatter.mount.is_some()
            || frontmatter.filters.is_some()
            || frontmatter.total_exposure_time.is_some()
            || frontmatter.ra.is_some()
            || frontmatter.dec.is_some()
            || frontmatter.additional_details.is_some()
            || frontmatter.latitude.is_some()
            || frontmatter.longitude.is_some();

        let has_description = description.is_some_and(|d| !d.is_empty());

        if !has_frontmatter && !has_description {
            // Nothing to write, delete file if it exists
            let _ = self.storage.delete(&simple_path).await;
            return Ok(());
        }

        let mut content = String::new();

        if has_frontmatter {
            let toml = toml_edit::ser::to_string_pretty(frontmatter)
                .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;
            content.push_str("+++\n");
            content.push_str(&toml);
            content.push_str("+++\n\n");
        }

        if let Some(desc) = description {
            content.push_str(desc);
            if !desc.ends_with('\n') {
                content.push('\n');
            }
        }

        self.storage
            .write(&simple_path, bytes::Bytes::from(content))
            .await
            .map_err(|e| MetadataStorageError::Io(std::io::Error::other(e.to_string())))?;

        Ok(())
    }

    /// Write .toml sidecar file
    async fn write_toml_file(
        &self,
        image_path: &str,
        toml_data: &TomlMetadata,
    ) -> Result<(), MetadataStorageError> {
        let toml_path = Self::get_toml_path(image_path);

        // Check if there's actual content to write
        let has_content = !toml_data.comments.is_empty()
            || toml_data.highlighted
            || toml_data.pick_status.is_some()
            || !toml_data.tags.is_empty()
            || !toml_data.ai_keywords.is_empty()
            || toml_data.ai_alt_text.is_some();

        if !has_content {
            // Nothing to write, delete file if it exists
            let _ = self.storage.delete(&toml_path).await;
            return Ok(());
        }

        let content = toml_edit::ser::to_string_pretty(toml_data)
            .map_err(|e| MetadataStorageError::Serialization(e.to_string()))?;

        self.storage
            .write(&toml_path, bytes::Bytes::from(content))
            .await
            .map_err(|e| MetadataStorageError::Io(std::io::Error::other(e.to_string())))?;

        Ok(())
    }

    /// Merge data from .md and .toml files into ImageUserMetadata
    fn merge_metadata(
        md_data: Option<(MdFrontmatter, String)>,
        toml_data: Option<TomlMetadata>,
    ) -> Option<ImageUserMetadata> {
        // If both are None, return None
        if md_data.is_none() && toml_data.is_none() {
            return None;
        }

        let mut metadata = ImageUserMetadata::default();

        // Apply .md data
        if let Some((frontmatter, description)) = md_data {
            metadata.title = frontmatter.title;
            metadata.description = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            metadata.location = frontmatter.location;
            metadata.camera_make = frontmatter.camera_make;
            metadata.camera_model = frontmatter.camera_model;
            metadata.lens_model = frontmatter.lens_model;
            metadata.iso = frontmatter.iso;
            metadata.aperture = frontmatter.aperture;
            metadata.shutter_speed = frontmatter.shutter_speed;
            metadata.focal_length = frontmatter.focal_length;
            metadata.capture_date = frontmatter.capture_date;
            metadata.telescope = frontmatter.telescope;
            metadata.mount = frontmatter.mount;
            metadata.filters = frontmatter.filters;
            metadata.total_exposure_time = frontmatter.total_exposure_time;
            metadata.ra = frontmatter.ra;
            metadata.dec = frontmatter.dec;
            metadata.additional_details = frontmatter.additional_details;
            metadata.latitude = frontmatter.latitude;
            metadata.longitude = frontmatter.longitude;
        }

        // Apply .toml data
        if let Some(toml) = toml_data {
            metadata.comments = toml.comments;
            metadata.highlighted = toml.highlighted;
            metadata.pick_status = toml.pick_status;
            metadata.tags = toml.tags;
            metadata.last_modified = toml.last_modified;
            metadata.modified_by = toml.modified_by;
            metadata.ai_keywords = toml.ai_keywords;
            metadata.ai_alt_text = toml.ai_alt_text;
            metadata.ai_analyzed_at = toml.ai_analyzed_at;
        }

        Some(metadata)
    }

    /// Split ImageUserMetadata into .md and .toml components
    fn split_metadata(metadata: &ImageUserMetadata) -> (MdFrontmatter, Option<String>, TomlMetadata) {
        let frontmatter = MdFrontmatter {
            title: metadata.title.clone(),
            location: metadata.location.clone(),
            camera_make: metadata.camera_make.clone(),
            camera_model: metadata.camera_model.clone(),
            lens_model: metadata.lens_model.clone(),
            iso: metadata.iso,
            aperture: metadata.aperture.clone(),
            shutter_speed: metadata.shutter_speed.clone(),
            focal_length: metadata.focal_length.clone(),
            capture_date: metadata.capture_date.clone(),
            telescope: metadata.telescope.clone(),
            mount: metadata.mount.clone(),
            filters: metadata.filters.clone(),
            total_exposure_time: metadata.total_exposure_time,
            ra: metadata.ra.clone(),
            dec: metadata.dec.clone(),
            additional_details: metadata.additional_details.clone(),
            latitude: metadata.latitude,
            longitude: metadata.longitude,
        };

        let toml_data = TomlMetadata {
            comments: metadata.comments.clone(),
            highlighted: metadata.highlighted,
            pick_status: metadata.pick_status,
            tags: metadata.tags.clone(),
            last_modified: metadata.last_modified,
            modified_by: metadata.modified_by.clone(),
            ai_keywords: metadata.ai_keywords.clone(),
            ai_alt_text: metadata.ai_alt_text.clone(),
            ai_analyzed_at: metadata.ai_analyzed_at,
        };

        (frontmatter, metadata.description.clone(), toml_data)
    }

    /// Invalidate cache entry for a path
    pub fn invalidate(&self, relative_path: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.pop(relative_path);
        }
    }

    /// Clear the entire cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Get current cache size
    pub fn cache_len(&self) -> usize {
        self.cache.lock().map(|c| c.len()).unwrap_or(0)
    }
}

#[async_trait]
impl MetadataStorage for StorageMetadataBackend {
    async fn load(
        &self,
        relative_path: &str,
    ) -> Result<Option<ImageUserMetadata>, MetadataStorageError> {
        // Check cache first (with TTL validation)
        if let Ok(mut cache) = self.cache.lock()
            && let Some(entry) = cache.get(relative_path)
        {
            if self.is_entry_valid(entry) {
                return Ok(Some(entry.data.clone()));
            }
            // Entry expired, remove it
            cache.pop(relative_path);
        }

        // Cache miss or expired - read from storage
        let md_data = self.read_md_file(relative_path).await;
        let toml_data = self.read_toml_file(relative_path).await;

        let metadata = Self::merge_metadata(md_data, toml_data);

        // Update cache if we got data
        if let Some(ref data) = metadata
            && let Ok(mut cache) = self.cache.lock()
        {
            cache.put(
                relative_path.to_string(),
                CacheEntry {
                    data: data.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        Ok(metadata)
    }

    async fn save(
        &self,
        relative_path: &str,
        metadata: &ImageUserMetadata,
    ) -> Result<(), MetadataStorageError> {
        let (frontmatter, description, toml_data) = Self::split_metadata(metadata);

        // Write both files
        self.write_md_file(relative_path, &frontmatter, description.as_deref())
            .await?;
        self.write_toml_file(relative_path, &toml_data).await?;

        // Update cache (write-through)
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(
                relative_path.to_string(),
                CacheEntry {
                    data: metadata.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        Ok(())
    }

    async fn delete(&self, relative_path: &str) -> Result<(), MetadataStorageError> {
        let toml_path = Self::get_toml_path(relative_path);
        let (full_ext_path, simple_path) = Self::get_md_paths(relative_path);

        // Delete all possible sidecar files
        let _ = self.storage.delete(&toml_path).await;
        let _ = self.storage.delete(&full_ext_path).await;
        let _ = self.storage.delete(&simple_path).await;

        // Invalidate cache
        self.invalidate(relative_path);

        Ok(())
    }

    async fn list_all(&self) -> Result<Vec<String>, MetadataStorageError> {
        // List all .toml and .md files and extract image paths
        match self.storage.list_recursive("").await {
            Ok(entries) => {
                let mut image_paths = std::collections::HashSet::new();

                for entry in entries {
                    if entry.is_dir {
                        continue;
                    }

                    // Check for .toml files
                    if entry.path.ends_with(".toml") {
                        if let Some(pos) = entry.path.rfind(".toml") {
                            image_paths.insert(entry.path[..pos].to_string());
                        }
                    }
                    // Check for .md files (but not _folder.md)
                    else if entry.path.ends_with(".md") && !entry.path.ends_with("_folder.md") {
                        // Handle IMAGE.jpg.md format
                        if let Some(pos) = entry.path.rfind(".md") {
                            let base = &entry.path[..pos];
                            // Check if it ends with an image extension
                            let extensions = [".jpg", ".jpeg", ".png", ".gif", ".webp", ".avif"];
                            let mut found = false;
                            for ext in &extensions {
                                if base.to_lowercase().ends_with(ext) {
                                    // It's IMAGE.jpg.md format
                                    image_paths.insert(base.to_string());
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                // It's IMAGE.md format - base is the stem
                                image_paths.insert(base.to_string());
                            }
                        }
                    }
                }

                Ok(image_paths.into_iter().collect())
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
            StorageMetadataBackend::get_toml_path("photos/vacation/beach.jpg"),
            "photos/vacation/beach.toml"
        );
        assert_eq!(
            StorageMetadataBackend::get_toml_path("photos/vacation/sunset.png"),
            "photos/vacation/sunset.toml"
        );

        let (full, simple) = StorageMetadataBackend::get_md_paths("photos/beach.jpg");
        assert_eq!(full, "photos/beach.jpg.md");
        assert_eq!(simple, "photos/beach.md");
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let mut metadata = ImageUserMetadata::new();
        metadata.title = Some("Test Image".to_string());
        metadata.description = Some("A beautiful sunset".to_string());
        metadata.highlighted = true;
        metadata.pick_status = Some(PickStatus::Pick);
        metadata.add_comment("user1".to_string(), "Great shot!".to_string(), None);
        metadata.tags = vec!["landscape".to_string(), "sunset".to_string()];

        // Save metadata
        backend.save("test_image.jpg", &metadata).await.unwrap();

        // Verify .md file exists
        let md_path = temp_dir.path().join("test_image.md");
        assert!(md_path.exists());
        let md_content = std::fs::read_to_string(&md_path).unwrap();
        assert!(md_content.contains("title = \"Test Image\""));
        assert!(md_content.contains("A beautiful sunset"));

        // Verify .toml file exists
        let toml_path = temp_dir.path().join("test_image.toml");
        assert!(toml_path.exists());
        let toml_content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(toml_content.contains("highlighted = true"));
        assert!(toml_content.contains("pick_status = \"pick\""));

        // Clear cache and load back
        backend.clear_cache();
        let loaded = backend.load("test_image.jpg").await.unwrap().unwrap();
        assert_eq!(loaded.title, Some("Test Image".to_string()));
        assert_eq!(loaded.description, Some("A beautiful sunset".to_string()));
        assert!(loaded.highlighted);
        assert_eq!(loaded.pick_status, Some(PickStatus::Pick));
        assert_eq!(loaded.comments.len(), 1);
        assert_eq!(loaded.comments[0].text, "Great shot!");
        assert_eq!(loaded.tags.len(), 2);
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let mut metadata = ImageUserMetadata::new();
        metadata.title = Some("Cached".to_string());

        backend.save("cached.jpg", &metadata).await.unwrap();

        // First load populates cache
        let _ = backend.load("cached.jpg").await.unwrap();
        assert_eq!(backend.cache_len(), 1);

        // Second load should hit cache
        let loaded = backend.load("cached.jpg").await.unwrap().unwrap();
        assert_eq!(loaded.title, Some("Cached".to_string()));
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let loaded = backend.load("nonexistent.jpg").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_read_existing_md_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        // Create an existing .md file
        let md_content = r#"+++
title = "Existing Title"
telescope = "RedCat 51"
+++

This is a pre-existing description.
"#;
        std::fs::write(temp_dir.path().join("existing.md"), md_content).unwrap();

        let loaded = backend.load("existing.jpg").await.unwrap().unwrap();
        assert_eq!(loaded.title, Some("Existing Title".to_string()));
        assert_eq!(loaded.telescope, Some("RedCat 51".to_string()));
        assert!(loaded
            .description
            .as_ref()
            .unwrap()
            .contains("pre-existing"));
    }

    #[tokio::test]
    async fn test_delete_clears_both_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = create_test_storage(temp_dir.path());
        let backend = StorageMetadataBackend::new(storage);

        let mut metadata = ImageUserMetadata::new();
        metadata.title = Some("To Delete".to_string());
        metadata.highlighted = true;

        backend.save("delete_me.jpg", &metadata).await.unwrap();

        // Verify files exist
        assert!(temp_dir.path().join("delete_me.md").exists());
        assert!(temp_dir.path().join("delete_me.toml").exists());

        // Delete
        backend.delete("delete_me.jpg").await.unwrap();

        // Verify files are gone
        assert!(!temp_dir.path().join("delete_me.md").exists());
        assert!(!temp_dir.path().join("delete_me.toml").exists());

        // Cache should be invalidated
        let loaded = backend.load("delete_me.jpg").await.unwrap();
        assert!(loaded.is_none());
    }
}
