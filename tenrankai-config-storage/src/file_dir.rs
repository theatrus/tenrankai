use crate::{
    AuditAction, AuditEntry, ConfigStorage, ConfigStorageError, GalleryPermissionConfig, Result,
    Role, StoredGalleryConfig, StoredPostsConfig, StoredSiteConfig,
};
use async_trait::async_trait;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tokio::task;
use tracing::{debug, instrument, warn};

pub struct FileDirConfigStorage {
    #[allow(dead_code)]
    base_path: PathBuf,
    galleries_path: PathBuf,
    roles_path: PathBuf,
    audit_path: PathBuf,
}

impl FileDirConfigStorage {
    pub async fn new(base_path: PathBuf) -> Result<Self> {
        let galleries_path = base_path.join("galleries");
        let roles_path = base_path.join("roles");
        let audit_path = base_path.join(".audit");

        let storage = Self {
            base_path,
            galleries_path,
            roles_path,
            audit_path,
        };

        storage.ensure_directories().await?;

        Ok(storage)
    }

    async fn ensure_directories(&self) -> Result<()> {
        let galleries_path = self.galleries_path.clone();
        let roles_path = self.roles_path.clone();
        let audit_path = self.audit_path.clone();

        task::spawn_blocking(move || {
            fs::create_dir_all(&galleries_path)?;
            fs::create_dir_all(&roles_path)?;
            fs::create_dir_all(&audit_path)?;
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))??;

        Ok(())
    }

    fn gallery_file_path(&self, gallery: &str) -> PathBuf {
        self.galleries_path.join(format!("{}.toml", gallery))
    }

    fn role_file_path(&self, name: &str) -> PathBuf {
        self.roles_path.join(format!("{}.toml", name))
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
        self.site_galleries_path(site).join(format!("{}.toml", gallery))
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
                if path.extension().is_some_and(|ext| ext == "toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
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
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        // Skip hidden directories
                        if !name.starts_with('.') {
                            names.push(name.to_string());
                        }
                    }
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

            let mut writer = BufWriter::new(&file);
            let json = serde_json::to_string(&entry)
                .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;
            writeln!(writer, "{}", json)?;
            writer.flush()?;

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

            let mut writer = BufWriter::new(&file);
            writer.write_all(content.as_bytes())?;
            writer.flush()?;

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
    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_gallery_config(&self, gallery: &str) -> Result<Option<GalleryPermissionConfig>> {
        let path = self.gallery_file_path(gallery);
        debug!(gallery = gallery, path = ?path, "Getting gallery config");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, config), fields(backend = "file_dir"))]
    async fn set_gallery_config(
        &self,
        gallery: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()> {
        let path = self.gallery_file_path(gallery);
        debug!(gallery = gallery, path = ?path, user = username, "Setting gallery config");

        self.write_toml_file(path, config).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryConfig)
            .with_target(gallery)
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn delete_gallery_config(&self, gallery: &str, username: &str) -> Result<bool> {
        let path = self.gallery_file_path(gallery);
        debug!(gallery = gallery, path = ?path, user = username, "Deleting gallery config");

        let deleted = self.delete_file(path).await?;

        if deleted {
            let entry =
                AuditEntry::new(username, AuditAction::DeleteGalleryConfig).with_target(gallery);

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn list_roles(&self) -> Result<Vec<(String, Role)>> {
        let roles_path = self.roles_path.clone();

        task::spawn_blocking(move || {
            let mut roles = Vec::new();

            if !roles_path.exists() {
                return Ok(roles);
            }

            for entry in fs::read_dir(&roles_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().is_some_and(|ext| ext == "toml") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(String::from)
                        .ok_or_else(|| {
                            ConfigStorageError::InvalidData("Invalid role filename".to_string())
                        })?;

                    let file = File::open(&path)?;
                    file.lock_shared()?;

                    let content = fs::read_to_string(&path)?;
                    file.unlock()?;

                    let role: Role = toml_edit::de::from_str(&content)
                        .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

                    roles.push((name, role));
                }
            }

            Ok(roles)
        })
        .await
        .map_err(|e| ConfigStorageError::Io(std::io::Error::other(e.to_string())))?
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_role(&self, name: &str) -> Result<Option<Role>> {
        let path = self.role_file_path(name);
        debug!(role = name, path = ?path, "Getting role");
        self.read_toml_file(path).await
    }

    #[instrument(skip(self, role), fields(backend = "file_dir"))]
    async fn set_role(&self, name: &str, role: &Role, username: &str) -> Result<()> {
        let path = self.role_file_path(name);
        debug!(role = name, path = ?path, user = username, "Setting role");

        self.write_toml_file(path, role).await?;

        let entry = AuditEntry::new(username, AuditAction::SetRole)
            .with_target(name)
            .with_changes(serde_json::to_value(role).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn delete_role(&self, name: &str, username: &str) -> Result<bool> {
        let path = self.role_file_path(name);
        debug!(role = name, path = ?path, user = username, "Deleting role");

        let deleted = self.delete_file(path).await?;

        if deleted {
            let entry = AuditEntry::new(username, AuditAction::DeleteRole).with_target(name);

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    #[instrument(skip(self), fields(backend = "file_dir"))]
    async fn get_user_roles(&self, gallery: &str, username: &str) -> Result<Vec<String>> {
        let config = self.get_gallery_config(gallery).await?;

        Ok(config
            .and_then(|c| {
                c.user_roles
                    .iter()
                    .find(|ur| ur.username == username)
                    .map(|ur| ur.roles.clone())
            })
            .unwrap_or_default())
    }

    #[instrument(skip(self, roles), fields(backend = "file_dir"))]
    async fn set_user_roles(
        &self,
        gallery: &str,
        target_username: &str,
        roles: &[String],
        actor_username: &str,
    ) -> Result<()> {
        let path = self.gallery_file_path(gallery);
        debug!(
            gallery = gallery,
            target = target_username,
            actor = actor_username,
            roles = ?roles,
            "Setting user roles"
        );

        let mut config = self
            .get_gallery_config(gallery)
            .await?
            .unwrap_or_default();

        if let Some(existing) = config
            .user_roles
            .iter_mut()
            .find(|ur| ur.username == target_username)
        {
            existing.roles = roles.to_vec();
        } else {
            config.user_roles.push(crate::UserRole {
                username: target_username.to_string(),
                roles: roles.to_vec(),
            });
        }

        config.user_roles.retain(|ur| !ur.roles.is_empty());

        self.write_toml_file(path, &config).await?;

        let entry = AuditEntry::new(actor_username, AuditAction::SetUserRoles)
            .with_target(format!("{}:{}", gallery, target_username))
            .with_changes(serde_json::json!({
                "roles": roles
            }));

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

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
    use crate::RolePermissions;
    use tempfile::TempDir;

    async fn create_test_storage() -> (FileDirConfigStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileDirConfigStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_gallery_config_roundtrip() {
        let (storage, _dir) = create_test_storage().await;

        let config = GalleryPermissionConfig {
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("editor".to_string()),
            ..Default::default()
        };

        storage
            .set_gallery_config("test", &config, "alice")
            .await
            .unwrap();

        let loaded = storage.get_gallery_config("test").await.unwrap();
        assert_eq!(loaded, Some(config));
    }

    #[tokio::test]
    async fn test_role_roundtrip() {
        let (storage, _dir) = create_test_storage().await;

        let role = Role {
            permissions: RolePermissions {
                can_view: true,
                can_download_medium: true,
                ..Default::default()
            },
            inherits: Some("viewer".to_string()),
        };

        storage.set_role("editor", &role, "alice").await.unwrap();

        let loaded = storage.get_role("editor").await.unwrap();
        assert_eq!(loaded, Some(role));
    }

    #[tokio::test]
    async fn test_list_roles() {
        let (storage, _dir) = create_test_storage().await;

        let role1 = Role {
            permissions: RolePermissions {
                can_view: true,
                ..Default::default()
            },
            inherits: None,
        };

        let role2 = Role {
            permissions: RolePermissions {
                can_view: true,
                can_download_medium: true,
                ..Default::default()
            },
            inherits: Some("viewer".to_string()),
        };

        storage.set_role("viewer", &role1, "alice").await.unwrap();
        storage.set_role("editor", &role2, "alice").await.unwrap();

        let roles = storage.list_roles().await.unwrap();
        assert_eq!(roles.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_role() {
        let (storage, _dir) = create_test_storage().await;

        let role = Role {
            permissions: RolePermissions {
                can_view: true,
                ..Default::default()
            },
            inherits: None,
        };

        storage.set_role("viewer", &role, "alice").await.unwrap();
        assert!(storage.get_role("viewer").await.unwrap().is_some());

        let deleted = storage.delete_role("viewer", "alice").await.unwrap();
        assert!(deleted);

        assert!(storage.get_role("viewer").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_user_roles() {
        let (storage, _dir) = create_test_storage().await;

        storage
            .set_user_roles("main", "bob", &["viewer".to_string()], "alice")
            .await
            .unwrap();

        let roles = storage.get_user_roles("main", "bob").await.unwrap();
        assert_eq!(roles, vec!["viewer"]);

        storage
            .set_user_roles(
                "main",
                "bob",
                &["viewer".to_string(), "editor".to_string()],
                "alice",
            )
            .await
            .unwrap();

        let roles = storage.get_user_roles("main", "bob").await.unwrap();
        assert_eq!(roles, vec!["viewer", "editor"]);
    }

    #[tokio::test]
    async fn test_audit_log_created() {
        let (storage, dir) = create_test_storage().await;

        let config = GalleryPermissionConfig {
            public_role: Some("viewer".to_string()),
            ..Default::default()
        };

        storage
            .set_gallery_config("test", &config, "alice")
            .await
            .unwrap();

        let audit_path = dir.path().join(".audit").join("audit.jsonl");
        assert!(audit_path.exists());

        let content = fs::read_to_string(&audit_path).unwrap();
        assert!(content.contains("alice"));
        assert!(content.contains("set_gallery_config"));
    }
}
