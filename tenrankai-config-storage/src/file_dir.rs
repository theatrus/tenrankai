use crate::{
    AuditAction, AuditEntry, ConfigStorage, ConfigStorageError, GalleryPermissionConfig, Result,
    StoredGalleryConfig, StoredPostsConfig, StoredSiteConfig,
};
use async_trait::async_trait;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tokio::task;
use tracing::{debug, instrument, warn};

pub struct FileDirConfigStorage {
    base_path: PathBuf,
    audit_path: PathBuf,
}

impl FileDirConfigStorage {
    pub async fn new(base_path: PathBuf) -> Result<Self> {
        let audit_path = base_path.join(".audit");

        let storage = Self {
            base_path,
            audit_path,
        };

        storage.ensure_directories().await?;

        Ok(storage)
    }

    async fn ensure_directories(&self) -> Result<()> {
        let audit_path = self.audit_path.clone();

        task::spawn_blocking(move || {
            fs::create_dir_all(&audit_path)?;
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))??;

        Ok(())
    }

    fn audit_log_path(&self) -> PathBuf {
        self.audit_path.join("audit.jsonl")
    }

    // New path helpers for site-based structure
    fn sites_path(&self) -> PathBuf {
        self.base_path.join("sites")
    }

    fn site_dir_path(&self, site: &str) -> PathBuf {
        self.sites_path().join(site)
    }

    fn site_config_path(&self, site: &str) -> PathBuf {
        self.site_dir_path(site).join("site.toml")
    }

    fn site_galleries_path(&self, site: &str) -> PathBuf {
        self.site_dir_path(site).join("galleries")
    }

    fn site_gallery_path(&self, site: &str, gallery: &str) -> PathBuf {
        self.site_galleries_path(site)
            .join(format!("{}.toml", gallery))
    }

    fn site_posts_path(&self, site: &str) -> PathBuf {
        self.site_dir_path(site).join("posts")
    }

    fn site_post_path(&self, site: &str, posts: &str) -> PathBuf {
        self.site_posts_path(site).join(format!("{}.toml", posts))
    }

    fn site_permissions_path(&self, site: &str) -> PathBuf {
        self.site_dir_path(site).join("permissions.toml")
    }

    async fn list_toml_files_in_dir(&self, dir: PathBuf) -> Result<Vec<String>> {
        task::spawn_blocking(move || {
            let mut names = Vec::new();
            if !dir.exists() {
                return Ok(names);
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    names.push(stem.to_string());
                }
            }
            Ok(names)
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }

    async fn list_subdirs(&self, dir: PathBuf) -> Result<Vec<String>> {
        task::spawn_blocking(move || {
            let mut names = Vec::new();
            if !dir.exists() {
                return Ok(names);
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name().and_then(|s| s.to_str())
                    && !name.starts_with('.')
                {
                    names.push(name.to_string());
                }
            }
            Ok(names)
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }

    async fn delete_dir_recursive(&self, path: PathBuf) -> Result<bool> {
        task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(false);
            }
            fs::remove_dir_all(&path)?;
            Ok(true)
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }

    async fn append_audit(&self, entry: AuditEntry) -> Result<()> {
        let audit_path = self.audit_log_path();

        task::spawn_blocking(move || {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&audit_path)?;

            file.lock_exclusive()?;

            // Use a scope to ensure writer is dropped before unlock
            {
                let mut writer = BufWriter::new(&file);
                let json = serde_json::to_string(&entry)
                    .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;
                writeln!(writer, "{}", json)?;
                writer.flush()?;
                // writer drops here, ensuring all data is written
            }

            // Ensure data is synced to disk (important on Windows)
            file.sync_all()?;

            file.unlock()?;
            Ok::<_, ConfigStorageError>(())
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))??;

        Ok(())
    }

    async fn read_toml_file<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        path: PathBuf,
    ) -> Result<Option<T>> {
        task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(None);
            }

            let file = File::open(&path)?;
            file.lock_shared()?;

            let content = fs::read_to_string(&path)?;
            file.unlock()?;

            let value: T = toml_edit::de::from_str(&content)
                .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

            Ok(Some(value))
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }

    async fn write_toml_file<T: serde::Serialize + Send + Sync + 'static>(
        &self,
        path: PathBuf,
        value: &T,
    ) -> Result<()> {
        let content = toml_edit::ser::to_string_pretty(value)
            .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

        task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;

            file.lock_exclusive()?;

            // Use a scope to ensure writer is dropped before unlock
            {
                let mut writer = BufWriter::new(&file);
                writer.write_all(content.as_bytes())?;
                writer.flush()?;
            }

            // Ensure data is synced to disk (important on Windows)
            file.sync_all()?;

            file.unlock()?;
            Ok::<_, ConfigStorageError>(())
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))??;

        Ok(())
    }

    async fn delete_file(&self, path: PathBuf) -> Result<bool> {
        task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(false);
            }

            fs::remove_file(&path)?;
            Ok(true)
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }
}

#[async_trait]
impl ConfigStorage for FileDirConfigStorage {
    fn backend_name(&self) -> &'static str {
        "file_dir"
    }

    // ========================================================================
    // Site Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn list_sites(&self) -> Result<Vec<String>> {
        let sites_path = self.sites_path();
        debug!(path = ?sites_path, "Listing sites");
        self.list_subdirs(sites_path).await
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_site_config(&self, site: &str) -> Result<Option<StoredSiteConfig>> {
        let path = self.site_config_path(site);
        debug!(site = site, path = ?path, "Getting site config");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, config), fields(backend = "file_dir"))]
    async fn set_site_config(
        &self,
        site: &str,
        config: &StoredSiteConfig,
        username: &str,
    ) -> Result<()> {
        let path = self.site_config_path(site);
        debug!(site = site, path = ?path, user = username, "Setting site config");

        self.write_toml_file(path, config).await?;

        let entry = AuditEntry::new(username, AuditAction::SetSiteConfig)
            .with_target(site)
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn delete_site(&self, site: &str, username: &str) -> Result<bool> {
        let path = self.site_dir_path(site);
        debug!(site = site, path = ?path, user = username, "Deleting site");

        let deleted = self.delete_dir_recursive(path).await?;

        if deleted {
            let entry = AuditEntry::new(username, AuditAction::DeleteSite).with_target(site);

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    // ========================================================================
    // Gallery Management (full config)
    // ========================================================================

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn list_galleries(&self, site: &str) -> Result<Vec<String>> {
        let path = self.site_galleries_path(site);
        debug!(site = site, path = ?path, "Listing galleries");
        self.list_toml_files_in_dir(path).await
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
    ) -> Result<Option<StoredGalleryConfig>> {
        let path = self.site_gallery_path(site, gallery);
        debug!(site = site, gallery = gallery, path = ?path, "Getting gallery full config");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, config), fields(backend = "file_dir"))]
    async fn set_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
        config: &StoredGalleryConfig,
        username: &str,
    ) -> Result<()> {
        let path = self.site_gallery_path(site, gallery);
        debug!(site = site, gallery = gallery, path = ?path, user = username, "Setting gallery full config");

        self.write_toml_file(path, config).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryFullConfig)
            .with_target(format!("{}:{}", site, gallery))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn delete_gallery(&self, site: &str, gallery: &str, username: &str) -> Result<bool> {
        let path = self.site_gallery_path(site, gallery);
        debug!(site = site, gallery = gallery, path = ?path, user = username, "Deleting gallery");

        let deleted = self.delete_file(path).await?;

        if deleted {
            let entry = AuditEntry::new(username, AuditAction::DeleteGallery)
                .with_target(format!("{}:{}", site, gallery));

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    // ========================================================================
    // Posts Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn list_posts(&self, site: &str) -> Result<Vec<String>> {
        let path = self.site_posts_path(site);
        debug!(site = site, path = ?path, "Listing posts");
        self.list_toml_files_in_dir(path).await
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_posts_config(&self, site: &str, posts: &str) -> Result<Option<StoredPostsConfig>> {
        let path = self.site_post_path(site, posts);
        debug!(site = site, posts = posts, path = ?path, "Getting posts config");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, config), fields(backend = "file_dir"))]
    async fn set_posts_config(
        &self,
        site: &str,
        posts: &str,
        config: &StoredPostsConfig,
        username: &str,
    ) -> Result<()> {
        let path = self.site_post_path(site, posts);
        debug!(site = site, posts = posts, path = ?path, user = username, "Setting posts config");

        self.write_toml_file(path, config).await?;

        let entry = AuditEntry::new(username, AuditAction::SetPostsConfig)
            .with_target(format!("{}:{}", site, posts))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn delete_posts(&self, site: &str, posts: &str, username: &str) -> Result<bool> {
        let path = self.site_post_path(site, posts);
        debug!(site = site, posts = posts, path = ?path, user = username, "Deleting posts");

        let deleted = self.delete_file(path).await?;

        if deleted {
            let entry = AuditEntry::new(username, AuditAction::DeletePosts)
                .with_target(format!("{}:{}", site, posts));

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    // ========================================================================
    // Site-level Permission Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_site_permissions(&self, site: &str) -> Result<Option<GalleryPermissionConfig>> {
        let path = self.site_permissions_path(site);
        debug!(site = site, path = ?path, "Getting site permissions");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, config), fields(backend = "file_dir"))]
    async fn set_site_permissions(
        &self,
        site: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()> {
        let path = self.site_permissions_path(site);
        debug!(site = site, path = ?path, user = username, "Setting site permissions");

        self.write_toml_file(path, config).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryConfig)
            .with_target(format!("{}:permissions", site))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StoredImageSizeConfig, StoredPreviewConfig};
    use tempfile::TempDir;

    async fn create_test_storage() -> (FileDirConfigStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileDirConfigStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (storage, temp_dir)
    }

    fn create_test_gallery_config(
        name: &str,
        url_prefix: &str,
        source: &str,
        cache: &str,
    ) -> StoredGalleryConfig {
        StoredGalleryConfig {
            name: name.to_string(),
            url_prefix: url_prefix.to_string(),
            source_directory: source.to_string(),
            cache_directory: cache.to_string(),
            gallery_template: "modules/gallery.html.liquid".to_string(),
            image_detail_template: "modules/image_detail.html.liquid".to_string(),
            thumbnail: StoredImageSizeConfig {
                width: 300,
                height: 300,
            },
            gallery_size: StoredImageSizeConfig {
                width: 800,
                height: 800,
            },
            medium: StoredImageSizeConfig {
                width: 1200,
                height: 1200,
            },
            large: StoredImageSizeConfig {
                width: 1600,
                height: 1600,
            },
            cache_refresh_interval_minutes: None,
            jpeg_quality: None,
            webp_quality: None,
            new_threshold_days: None,
            copyright_holder: None,
            image_indexing: "filename".to_string(),
            metadata_cache_size: 100,
            tiles: None,
            pregenerate: None,
            preview: Some(StoredPreviewConfig {
                max_images: 10,
                max_depth: 3,
                max_per_folder: 5,
            }),
        }
    }

    #[tokio::test]
    async fn test_site_config_roundtrip() {
        let (storage, _dir) = create_test_storage().await;

        let config = StoredSiteConfig {
            hostnames: vec!["example.com".to_string()],
            base_url: Some("https://example.com".to_string()),
            templates: vec!["templates".to_string()],
            static_files: vec!["static".to_string()],
            static_use_redirects: false,
            user_database: Some("users.toml".to_string()),
            cookie_secret: None,
            storage_prefix: Some("/data/sites/default".to_string()),
            cache_prefix: Some("/var/cache/sites/default".to_string()),
            email: None,
            theme: None,
        };

        storage
            .set_site_config("default", &config, "alice")
            .await
            .unwrap();

        let loaded = storage.get_site_config("default").await.unwrap();
        assert_eq!(loaded, Some(config));
    }

    #[tokio::test]
    async fn test_list_sites() {
        let (storage, _dir) = create_test_storage().await;

        let config = StoredSiteConfig {
            hostnames: vec!["example.com".to_string()],
            ..Default::default()
        };

        storage
            .set_site_config("site1", &config, "alice")
            .await
            .unwrap();
        storage
            .set_site_config("site2", &config, "alice")
            .await
            .unwrap();

        let sites = storage.list_sites().await.unwrap();
        assert_eq!(sites.len(), 2);
        assert!(sites.contains(&"site1".to_string()));
        assert!(sites.contains(&"site2".to_string()));
    }

    #[tokio::test]
    async fn test_gallery_full_config_roundtrip() {
        let (storage, _dir) = create_test_storage().await;

        let config = create_test_gallery_config("main", "/gallery", "photos", "cache/main");

        storage
            .set_gallery_full_config("default", "main", &config, "alice")
            .await
            .unwrap();

        let loaded = storage
            .get_gallery_full_config("default", "main")
            .await
            .unwrap();
        assert_eq!(loaded, Some(config));
    }

    #[tokio::test]
    async fn test_list_galleries() {
        let (storage, _dir) = create_test_storage().await;

        let config = create_test_gallery_config("test", "/test", "photos", "cache");

        storage
            .set_gallery_full_config("default", "main", &config, "alice")
            .await
            .unwrap();
        storage
            .set_gallery_full_config("default", "archive", &config, "alice")
            .await
            .unwrap();

        let galleries = storage.list_galleries("default").await.unwrap();
        assert_eq!(galleries.len(), 2);
    }

    #[tokio::test]
    async fn test_site_permissions_roundtrip() {
        let (storage, _dir) = create_test_storage().await;

        let config = GalleryPermissionConfig {
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("editor".to_string()),
            ..Default::default()
        };

        storage
            .set_site_permissions("default", &config, "alice")
            .await
            .unwrap();

        let loaded = storage.get_site_permissions("default").await.unwrap();
        assert_eq!(loaded, Some(config));
    }

    #[tokio::test]
    async fn test_delete_site() {
        let (storage, _dir) = create_test_storage().await;

        let site_config = StoredSiteConfig {
            hostnames: vec!["example.com".to_string()],
            ..Default::default()
        };

        storage
            .set_site_config("testsite", &site_config, "alice")
            .await
            .unwrap();
        assert!(storage.get_site_config("testsite").await.unwrap().is_some());

        let deleted = storage.delete_site("testsite", "alice").await.unwrap();
        assert!(deleted);

        assert!(storage.get_site_config("testsite").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_audit_log_created() {
        let (storage, dir) = create_test_storage().await;

        let config = GalleryPermissionConfig {
            public_role: Some("viewer".to_string()),
            ..Default::default()
        };

        storage
            .set_site_permissions("default", &config, "alice")
            .await
            .unwrap();

        let audit_path = dir.path().join(".audit").join("audit.jsonl");
        assert!(audit_path.exists());

        let content = fs::read_to_string(&audit_path).unwrap();
        assert!(content.contains("alice"));
        assert!(content.contains("set_gallery_config"));
    }
}
