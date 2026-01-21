use crate::{
    AuditAction, AuditEntry, ConfigStorage, ConfigStorageError, GalleryPermissionConfig, Result,
    Role, StoredGalleryConfig, StoredPostsConfig, StoredSiteConfig, UserRole,
};
use async_trait::async_trait;
use std::sync::Arc;
use tenrankai_storage::{Storage, StorageUrl};
use tracing::{debug, instrument, warn};

pub struct StorageConfigStorage {
    storage: Arc<dyn Storage>,
}

impl StorageConfigStorage {
    pub async fn new(url: StorageUrl) -> Result<Self> {
        let storage = url
            .into_storage()
            .await
            .map_err(ConfigStorageError::Storage)?;
        Ok(Self { storage })
    }

    fn gallery_key(&self, gallery: &str) -> String {
        format!("galleries/{}.json", gallery)
    }

    fn role_key(&self, name: &str) -> String {
        format!("roles/{}.json", name)
    }

    fn audit_key(&self) -> String {
        "audit/audit.jsonl".to_string()
    }

    // New key helpers for site-based structure
    fn site_config_key(&self, site: &str) -> String {
        format!("sites/{}/site.json", site)
    }

    fn site_gallery_key(&self, site: &str, gallery: &str) -> String {
        format!("sites/{}/galleries/{}.json", site, gallery)
    }

    fn site_posts_key(&self, site: &str, posts: &str) -> String {
        format!("sites/{}/posts/{}.json", site, posts)
    }

    fn site_permissions_key(&self, site: &str) -> String {
        format!("sites/{}/permissions.json", site)
    }

    async fn read_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.storage.read(key).await {
            Ok(data) => {
                let value: T = serde_json::from_slice(&data)
                    .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            Err(tenrankai_storage::StorageError::NotFound(_)) => Ok(None),
            Err(e) => Err(ConfigStorageError::Storage(e)),
        }
    }

    async fn write_json<T: serde::Serialize>(
        &self,
        key: &str,
        value: &T,
        _username: &str,
    ) -> Result<()> {
        let content = serde_json::to_vec_pretty(value)
            .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

        self.storage
            .write(key, content.into())
            .await
            .map_err(ConfigStorageError::Storage)?;

        Ok(())
    }

    async fn delete_key(&self, key: &str) -> Result<bool> {
        match self.storage.delete(key).await {
            Ok(()) => Ok(true),
            Err(tenrankai_storage::StorageError::NotFound(_)) => Ok(false),
            Err(e) => Err(ConfigStorageError::Storage(e)),
        }
    }

    async fn append_audit(&self, entry: AuditEntry) -> Result<()> {
        let key = self.audit_key();

        let mut content = match self.storage.read(&key).await {
            Ok(data) => String::from_utf8(data.to_vec())
                .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?,
            Err(tenrankai_storage::StorageError::NotFound(_)) => String::new(),
            Err(e) => return Err(ConfigStorageError::Storage(e)),
        };

        let json = serde_json::to_string(&entry)
            .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

        content.push_str(&json);
        content.push('\n');

        self.storage
            .write(&key, content.into_bytes().into())
            .await
            .map_err(ConfigStorageError::Storage)?;

        Ok(())
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let entries = self
            .storage
            .list(prefix)
            .await
            .map_err(ConfigStorageError::Storage)?;
        Ok(entries.into_iter().map(|e| e.path).collect())
    }
}

#[async_trait]
impl ConfigStorage for StorageConfigStorage {
    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_gallery_config(&self, gallery: &str) -> Result<Option<GalleryPermissionConfig>> {
        let key = self.gallery_key(gallery);
        debug!(gallery = gallery, key = key, "Getting gallery config");
        self.read_json(&key).await
    }

    #[instrument(skip(self, config), fields(backend = "storage"))]
    async fn set_gallery_config(
        &self,
        gallery: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()> {
        let key = self.gallery_key(gallery);
        debug!(gallery = gallery, key = key, user = username, "Setting gallery config");

        self.write_json(&key, config, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryConfig)
            .with_target(gallery)
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn delete_gallery_config(&self, gallery: &str, username: &str) -> Result<bool> {
        let key = self.gallery_key(gallery);
        debug!(gallery = gallery, key = key, user = username, "Deleting gallery config");

        let deleted = self.delete_key(&key).await?;

        if deleted {
            let entry =
                AuditEntry::new(username, AuditAction::DeleteGalleryConfig).with_target(gallery);

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn list_roles(&self) -> Result<Vec<(String, Role)>> {
        let keys = self.list_prefix("roles/").await?;
        let mut roles = Vec::new();

        for key in keys {
            if key.ends_with(".json") {
                let name = key
                    .strip_prefix("roles/")
                    .and_then(|s| s.strip_suffix(".json"))
                    .map(String::from)
                    .ok_or_else(|| {
                        ConfigStorageError::InvalidData("Invalid role key".to_string())
                    })?;

                if let Some(role) = self.read_json::<Role>(&key).await? {
                    roles.push((name, role));
                }
            }
        }

        Ok(roles)
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_role(&self, name: &str) -> Result<Option<Role>> {
        let key = self.role_key(name);
        debug!(role = name, key = key, "Getting role");
        self.read_json(&key).await
    }

    #[instrument(skip(self, role), fields(backend = "storage"))]
    async fn set_role(&self, name: &str, role: &Role, username: &str) -> Result<()> {
        let key = self.role_key(name);
        debug!(role = name, key = key, user = username, "Setting role");

        self.write_json(&key, role, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetRole)
            .with_target(name)
            .with_changes(serde_json::to_value(role).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn delete_role(&self, name: &str, username: &str) -> Result<bool> {
        let key = self.role_key(name);
        debug!(role = name, key = key, user = username, "Deleting role");

        let deleted = self.delete_key(&key).await?;

        if deleted {
            let entry = AuditEntry::new(username, AuditAction::DeleteRole).with_target(name);

            if let Err(e) = self.append_audit(entry).await {
                warn!(error = %e, "Failed to write audit log");
            }
        }

        Ok(deleted)
    }

    #[instrument(skip(self), fields(backend = "storage"))]
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

    #[instrument(skip(self, roles), fields(backend = "storage"))]
    async fn set_user_roles(
        &self,
        gallery: &str,
        target_username: &str,
        roles: &[String],
        actor_username: &str,
    ) -> Result<()> {
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
            config.user_roles.push(UserRole {
                username: target_username.to_string(),
                roles: roles.to_vec(),
            });
        }

        config.user_roles.retain(|ur| !ur.roles.is_empty());

        self.write_json(&self.gallery_key(gallery), &config, actor_username)
            .await?;

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
        "storage"
    }

    // ========================================================================
    // Site Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn list_sites(&self) -> Result<Vec<String>> {
        debug!("Listing sites");
        let keys = self.list_prefix("sites/").await?;
        let mut sites = std::collections::HashSet::new();

        for key in keys {
            // Extract site name from keys like "sites/{site}/..."
            if let Some(rest) = key.strip_prefix("sites/") {
                if let Some(site) = rest.split('/').next() {
                    sites.insert(site.to_string());
                }
            }
        }

        Ok(sites.into_iter().collect())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_site_config(&self, site: &str) -> Result<Option<StoredSiteConfig>> {
        let key = self.site_config_key(site);
        debug!(site = site, key = key, "Getting site config");
        self.read_json(&key).await
    }

    #[instrument(skip(self, config), fields(backend = "storage"))]
    async fn set_site_config(
        &self,
        site: &str,
        config: &StoredSiteConfig,
        username: &str,
    ) -> Result<()> {
        let key = self.site_config_key(site);
        debug!(site = site, key = key, user = username, "Setting site config");

        self.write_json(&key, config, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetSiteConfig)
            .with_target(site)
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn delete_site(&self, site: &str, username: &str) -> Result<bool> {
        debug!(site = site, user = username, "Deleting site");

        // Delete all files under sites/{site}/
        let prefix = format!("sites/{}/", site);
        let keys = self.list_prefix(&prefix).await?;

        if keys.is_empty() {
            return Ok(false);
        }

        for key in keys {
            self.delete_key(&key).await?;
        }

        let entry = AuditEntry::new(username, AuditAction::DeleteSite).with_target(site);

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(true)
    }

    // ========================================================================
    // Gallery Management (full config)
    // ========================================================================

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn list_galleries(&self, site: &str) -> Result<Vec<String>> {
        debug!(site = site, "Listing galleries");
        let prefix = format!("sites/{}/galleries/", site);
        let keys = self.list_prefix(&prefix).await?;
        let mut galleries = Vec::new();

        for key in keys {
            if key.ends_with(".json") {
                if let Some(name) = key
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                {
                    galleries.push(name.to_string());
                }
            }
        }

        Ok(galleries)
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
    ) -> Result<Option<StoredGalleryConfig>> {
        let key = self.site_gallery_key(site, gallery);
        debug!(site = site, gallery = gallery, key = key, "Getting gallery full config");
        self.read_json(&key).await
    }

    #[instrument(skip(self, config), fields(backend = "storage"))]
    async fn set_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
        config: &StoredGalleryConfig,
        username: &str,
    ) -> Result<()> {
        let key = self.site_gallery_key(site, gallery);
        debug!(site = site, gallery = gallery, key = key, user = username, "Setting gallery full config");

        self.write_json(&key, config, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryFullConfig)
            .with_target(format!("{}:{}", site, gallery))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn delete_gallery(&self, site: &str, gallery: &str, username: &str) -> Result<bool> {
        let key = self.site_gallery_key(site, gallery);
        debug!(site = site, gallery = gallery, key = key, user = username, "Deleting gallery");

        let deleted = self.delete_key(&key).await?;

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

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn list_posts(&self, site: &str) -> Result<Vec<String>> {
        debug!(site = site, "Listing posts");
        let prefix = format!("sites/{}/posts/", site);
        let keys = self.list_prefix(&prefix).await?;
        let mut posts_list = Vec::new();

        for key in keys {
            if key.ends_with(".json") {
                if let Some(name) = key
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                {
                    posts_list.push(name.to_string());
                }
            }
        }

        Ok(posts_list)
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_posts_config(&self, site: &str, posts: &str) -> Result<Option<StoredPostsConfig>> {
        let key = self.site_posts_key(site, posts);
        debug!(site = site, posts = posts, key = key, "Getting posts config");
        self.read_json(&key).await
    }

    #[instrument(skip(self, config), fields(backend = "storage"))]
    async fn set_posts_config(
        &self,
        site: &str,
        posts: &str,
        config: &StoredPostsConfig,
        username: &str,
    ) -> Result<()> {
        let key = self.site_posts_key(site, posts);
        debug!(site = site, posts = posts, key = key, user = username, "Setting posts config");

        self.write_json(&key, config, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetPostsConfig)
            .with_target(format!("{}:{}", site, posts))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn delete_posts(&self, site: &str, posts: &str, username: &str) -> Result<bool> {
        let key = self.site_posts_key(site, posts);
        debug!(site = site, posts = posts, key = key, user = username, "Deleting posts");

        let deleted = self.delete_key(&key).await?;

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

    #[instrument(skip(self), fields(backend = "storage"))]
    async fn get_site_permissions(&self, site: &str) -> Result<Option<GalleryPermissionConfig>> {
        let key = self.site_permissions_key(site);
        debug!(site = site, key = key, "Getting site permissions");
        self.read_json(&key).await
    }

    #[instrument(skip(self, config), fields(backend = "storage"))]
    async fn set_site_permissions(
        &self,
        site: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()> {
        let key = self.site_permissions_key(site);
        debug!(site = site, key = key, user = username, "Setting site permissions");

        self.write_json(&key, config, username).await?;

        let entry = AuditEntry::new(username, AuditAction::SetGalleryConfig)
            .with_target(format!("{}:permissions", site))
            .with_changes(serde_json::to_value(config).unwrap_or_default());

        if let Err(e) = self.append_audit(entry).await {
            warn!(error = %e, "Failed to write audit log");
        }

        Ok(())
    }
}
