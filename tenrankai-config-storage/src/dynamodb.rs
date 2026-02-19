use crate::{
    AuditAction, AuditEntry, ConfigStorage, ConfigStorageError, GalleryPermissionConfig, Result,
    StoredGalleryConfig, StoredPostsConfig, StoredSiteConfig,
};
use async_trait::async_trait;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

pub struct DynamoConfigStorage {
    client: Client,
    table_name: String,
    shard_id: String,
}

impl DynamoConfigStorage {
    pub async fn new(
        table_name: String,
        region: Option<String>,
        endpoint: Option<String>,
        shard: Option<String>,
    ) -> Result<Self> {
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Some(region) = &region {
            config_loader = config_loader.region(aws_config::Region::new(region.clone()));
        }

        let config = config_loader.load().await;

        let client = if let Some(endpoint) = &endpoint {
            aws_sdk_dynamodb::config::Builder::from(&config)
                .endpoint_url(endpoint)
                .build()
        } else {
            aws_sdk_dynamodb::config::Builder::from(&config).build()
        };

        let client = Client::from_conf(client);
        let shard_id = shard.unwrap_or_else(|| "default".to_string());

        info!(
            "Connected to DynamoDB config storage: table={}, shard={}",
            table_name, shard_id
        );

        Ok(Self {
            client,
            table_name,
            shard_id,
        })
    }

    // ========================================================================
    // Key helpers
    // ========================================================================

    fn site_pk(site: &str) -> String {
        format!("SITE#{}", site)
    }

    fn config_sk() -> &'static str {
        "CONFIG"
    }

    fn gallery_sk(name: &str) -> String {
        format!("GALLERY#{}", name)
    }

    fn posts_sk(name: &str) -> String {
        format!("POSTS#{}", name)
    }

    fn permissions_sk() -> &'static str {
        "PERMISSIONS"
    }

    fn shard_pk(shard_id: &str) -> String {
        format!("SHARD#{}", shard_id)
    }

    fn version_sk() -> &'static str {
        "VERSION"
    }

    fn audit_pk() -> &'static str {
        "AUDIT"
    }

    fn audit_sk() -> String {
        let ts = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4();
        format!("{}#{}", ts, id)
    }

    fn name_from_gallery_sk(sk: &str) -> Option<&str> {
        sk.strip_prefix("GALLERY#")
    }

    fn name_from_posts_sk(sk: &str) -> Option<&str> {
        sk.strip_prefix("POSTS#")
    }

    // ========================================================================
    // Low-level DynamoDB helpers
    // ========================================================================

    async fn get_item_json<T: DeserializeOwned>(&self, pk: &str, sk: &str) -> Result<Option<T>> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk.to_string()))
            .key("sk", AttributeValue::S(sk.to_string()))
            .send()
            .await
            .map_err(|e| ConfigStorageError::DynamoDb(format!("GetItem failed: {}", e)))?;

        match result.item {
            Some(item) => {
                let data = get_string(&item, "data")?;
                let value: T = serde_json::from_str(&data)
                    .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    async fn put_item_json<T: Serialize>(&self, pk: &str, sk: &str, data: &T) -> Result<()> {
        let json = serde_json::to_string(data)
            .map_err(|e| ConfigStorageError::Serialization(e.to_string()))?;

        self.client
            .put_item()
            .table_name(&self.table_name)
            .item("pk", AttributeValue::S(pk.to_string()))
            .item("sk", AttributeValue::S(sk.to_string()))
            .item("data", AttributeValue::S(json))
            .send()
            .await
            .map_err(|e| ConfigStorageError::DynamoDb(format!("PutItem failed: {}", e)))?;

        Ok(())
    }

    async fn delete_item(&self, pk: &str, sk: &str) -> Result<bool> {
        let result = self
            .client
            .delete_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk.to_string()))
            .key("sk", AttributeValue::S(sk.to_string()))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllOld)
            .send()
            .await
            .map_err(|e| ConfigStorageError::DynamoDb(format!("DeleteItem failed: {}", e)))?;

        Ok(result.attributes.is_some())
    }

    async fn query_sk_prefix(
        &self,
        pk: &str,
        prefix: &str,
    ) -> Result<Vec<HashMap<String, AttributeValue>>> {
        let mut items = Vec::new();
        let mut exclusive_start_key = None;

        loop {
            let mut query = self
                .client
                .query()
                .table_name(&self.table_name)
                .key_condition_expression("pk = :pk AND begins_with(sk, :prefix)")
                .expression_attribute_values(":pk", AttributeValue::S(pk.to_string()))
                .expression_attribute_values(":prefix", AttributeValue::S(prefix.to_string()));

            if let Some(key) = exclusive_start_key {
                query = query.set_exclusive_start_key(Some(key));
            }

            let result = query
                .send()
                .await
                .map_err(|e| ConfigStorageError::DynamoDb(format!("Query failed: {}", e)))?;

            if let Some(page_items) = result.items {
                items.extend(page_items);
            }

            exclusive_start_key = result.last_evaluated_key;
            if exclusive_start_key.is_none() {
                break;
            }
        }

        Ok(items)
    }

    fn site_membership_sk(site: &str) -> String {
        format!("SITE#{}", site)
    }

    // ========================================================================
    // Mutation wrappers
    //
    // All config mutations MUST go through write_config() or delete_config()
    // to keep the shard version counter in sync. The version counter enables
    // poll-based change detection across multiple server instances.
    // ========================================================================

    /// Record that a site belongs to this shard.
    async fn upsert_shard_membership(&self, site: &str) {
        let result = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("pk", AttributeValue::S(Self::shard_pk(&self.shard_id)))
            .item("sk", AttributeValue::S(Self::site_membership_sk(site)))
            .send()
            .await;

        if let Err(e) = result {
            warn!(error = %e, site = site, shard = %self.shard_id, "Failed to upsert shard membership");
        }
    }

    /// Remove a site's shard membership record.
    async fn delete_shard_membership(&self, site: &str) {
        let result = self
            .client
            .delete_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(Self::shard_pk(&self.shard_id)))
            .key("sk", AttributeValue::S(Self::site_membership_sk(site)))
            .send()
            .await;

        if let Err(e) = result {
            warn!(error = %e, site = site, shard = %self.shard_id, "Failed to delete shard membership");
        }
    }

    /// Atomically increment the config version for this shard.
    async fn bump_version(&self) {
        let result = self
            .client
            .update_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(Self::shard_pk(&self.shard_id)))
            .key("sk", AttributeValue::S(Self::version_sk().to_string()))
            .update_expression("ADD version :one")
            .expression_attribute_values(":one", AttributeValue::N("1".to_string()))
            .send()
            .await;

        if let Err(e) = result {
            warn!(error = %e, shard = %self.shard_id, "Failed to bump config version");
        }
    }

    async fn append_audit(&self, entry: AuditEntry) {
        let json = match serde_json::to_string(&entry) {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, "Failed to serialize audit entry");
                return;
            }
        };

        let result = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("pk", AttributeValue::S(Self::audit_pk().to_string()))
            .item("sk", AttributeValue::S(Self::audit_sk()))
            .item("data", AttributeValue::S(json))
            .send()
            .await;

        if let Err(e) = result {
            warn!(error = %e, "Failed to write audit log");
        }
    }

    /// Write a config item, bump the shard version, and append an audit entry.
    async fn write_config<T: Serialize>(
        &self,
        site: &str,
        sk: &str,
        data: &T,
        audit_entry: AuditEntry,
    ) -> Result<()> {
        self.put_item_json(&Self::site_pk(site), sk, data).await?;
        self.bump_version().await;
        self.append_audit(audit_entry).await;
        Ok(())
    }

    /// Delete a config item, bump the shard version if deleted, and append an audit entry.
    async fn delete_config(&self, site: &str, sk: &str, audit_entry: AuditEntry) -> Result<bool> {
        let deleted = self.delete_item(&Self::site_pk(site), sk).await?;
        if deleted {
            self.bump_version().await;
            self.append_audit(audit_entry).await;
        }
        Ok(deleted)
    }
}

#[async_trait]
impl ConfigStorage for DynamoConfigStorage {
    fn backend_name(&self) -> &'static str {
        "dynamodb"
    }

    // ========================================================================
    // Site Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn list_sites(&self) -> Result<Vec<String>> {
        debug!("Listing sites");

        let mut sites = Vec::new();
        let mut exclusive_start_key = None;

        loop {
            let mut scan = self
                .client
                .scan()
                .table_name(&self.table_name)
                .filter_expression("begins_with(pk, :site_prefix) AND sk = :config_sk")
                .expression_attribute_values(":site_prefix", AttributeValue::S("SITE#".to_string()))
                .expression_attribute_values(
                    ":config_sk",
                    AttributeValue::S(Self::config_sk().to_string()),
                );

            if let Some(key) = exclusive_start_key {
                scan = scan.set_exclusive_start_key(Some(key));
            }

            let result = scan
                .send()
                .await
                .map_err(|e| ConfigStorageError::DynamoDb(format!("Scan failed: {}", e)))?;

            if let Some(items) = result.items {
                for item in items {
                    if let Ok(pk) = get_string(&item, "pk")
                        && let Some(site) = pk.strip_prefix("SITE#")
                    {
                        sites.push(site.to_string());
                    }
                }
            }

            exclusive_start_key = result.last_evaluated_key;
            if exclusive_start_key.is_none() {
                break;
            }
        }

        sites.sort();
        Ok(sites)
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn get_site_config(&self, site: &str) -> Result<Option<StoredSiteConfig>> {
        debug!(site = site, "Getting site config");
        self.get_item_json(&Self::site_pk(site), Self::config_sk())
            .await
    }

    #[instrument(skip(self, config), fields(backend = "dynamodb"))]
    async fn set_site_config(
        &self,
        site: &str,
        config: &StoredSiteConfig,
        username: &str,
    ) -> Result<()> {
        debug!(site = site, user = username, "Setting site config");
        self.write_config(
            site,
            Self::config_sk(),
            config,
            AuditEntry::new(username, AuditAction::SetSiteConfig)
                .with_target(site)
                .with_changes(serde_json::to_value(config).unwrap_or_default()),
        )
        .await?;
        self.upsert_shard_membership(site).await;
        Ok(())
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn delete_site(&self, site: &str, username: &str) -> Result<bool> {
        debug!(site = site, user = username, "Deleting site");

        let pk = Self::site_pk(site);

        let mut exclusive_start_key = None;
        let mut found_any = false;

        loop {
            let mut query = self
                .client
                .query()
                .table_name(&self.table_name)
                .key_condition_expression("pk = :pk")
                .expression_attribute_values(":pk", AttributeValue::S(pk.clone()));

            if let Some(key) = exclusive_start_key {
                query = query.set_exclusive_start_key(Some(key));
            }

            let result = query
                .send()
                .await
                .map_err(|e| ConfigStorageError::DynamoDb(format!("Query failed: {}", e)))?;

            if let Some(items) = &result.items {
                if !items.is_empty() {
                    found_any = true;
                }

                for chunk in items.chunks(25) {
                    let mut batch = self.client.batch_write_item();

                    let mut requests = Vec::new();
                    for item in chunk {
                        if let (Ok(item_pk), Ok(item_sk)) =
                            (get_string(item, "pk"), get_string(item, "sk"))
                        {
                            let del = aws_sdk_dynamodb::types::DeleteRequest::builder()
                                .key("pk", AttributeValue::S(item_pk))
                                .key("sk", AttributeValue::S(item_sk))
                                .build()
                                .map_err(|e| {
                                    ConfigStorageError::DynamoDb(format!(
                                        "Failed to build delete request: {}",
                                        e
                                    ))
                                })?;
                            requests.push(
                                aws_sdk_dynamodb::types::WriteRequest::builder()
                                    .delete_request(del)
                                    .build(),
                            );
                        }
                    }

                    if !requests.is_empty() {
                        batch = batch.request_items(&self.table_name, requests);
                        batch.send().await.map_err(|e| {
                            ConfigStorageError::DynamoDb(format!("BatchWriteItem failed: {}", e))
                        })?;
                    }
                }
            }

            exclusive_start_key = result.last_evaluated_key;
            if exclusive_start_key.is_none() {
                break;
            }
        }

        if found_any {
            self.bump_version().await;
            self.delete_shard_membership(site).await;
            self.append_audit(AuditEntry::new(username, AuditAction::DeleteSite).with_target(site))
                .await;
        }

        Ok(found_any)
    }

    // ========================================================================
    // Gallery Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn list_galleries(&self, site: &str) -> Result<Vec<String>> {
        debug!(site = site, "Listing galleries");
        let items = self
            .query_sk_prefix(&Self::site_pk(site), "GALLERY#")
            .await?;

        let mut galleries = Vec::new();
        for item in items {
            if let Ok(sk) = get_string(&item, "sk")
                && let Some(name) = Self::name_from_gallery_sk(&sk)
            {
                galleries.push(name.to_string());
            }
        }
        Ok(galleries)
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn get_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
    ) -> Result<Option<StoredGalleryConfig>> {
        debug!(
            site = site,
            gallery = gallery,
            "Getting gallery full config"
        );
        self.get_item_json(&Self::site_pk(site), &Self::gallery_sk(gallery))
            .await
    }

    #[instrument(skip(self, config), fields(backend = "dynamodb"))]
    async fn set_gallery_full_config(
        &self,
        site: &str,
        gallery: &str,
        config: &StoredGalleryConfig,
        username: &str,
    ) -> Result<()> {
        debug!(
            site = site,
            gallery = gallery,
            user = username,
            "Setting gallery full config"
        );
        self.write_config(
            site,
            &Self::gallery_sk(gallery),
            config,
            AuditEntry::new(username, AuditAction::SetGalleryFullConfig)
                .with_target(format!("{}:{}", site, gallery))
                .with_changes(serde_json::to_value(config).unwrap_or_default()),
        )
        .await
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn delete_gallery(&self, site: &str, gallery: &str, username: &str) -> Result<bool> {
        debug!(
            site = site,
            gallery = gallery,
            user = username,
            "Deleting gallery"
        );
        self.delete_config(
            site,
            &Self::gallery_sk(gallery),
            AuditEntry::new(username, AuditAction::DeleteGallery)
                .with_target(format!("{}:{}", site, gallery)),
        )
        .await
    }

    // ========================================================================
    // Posts Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn list_posts(&self, site: &str) -> Result<Vec<String>> {
        debug!(site = site, "Listing posts");
        let items = self.query_sk_prefix(&Self::site_pk(site), "POSTS#").await?;

        let mut posts_list = Vec::new();
        for item in items {
            if let Ok(sk) = get_string(&item, "sk")
                && let Some(name) = Self::name_from_posts_sk(&sk)
            {
                posts_list.push(name.to_string());
            }
        }
        Ok(posts_list)
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn get_posts_config(&self, site: &str, posts: &str) -> Result<Option<StoredPostsConfig>> {
        debug!(site = site, posts = posts, "Getting posts config");
        self.get_item_json(&Self::site_pk(site), &Self::posts_sk(posts))
            .await
    }

    #[instrument(skip(self, config), fields(backend = "dynamodb"))]
    async fn set_posts_config(
        &self,
        site: &str,
        posts: &str,
        config: &StoredPostsConfig,
        username: &str,
    ) -> Result<()> {
        debug!(
            site = site,
            posts = posts,
            user = username,
            "Setting posts config"
        );
        self.write_config(
            site,
            &Self::posts_sk(posts),
            config,
            AuditEntry::new(username, AuditAction::SetPostsConfig)
                .with_target(format!("{}:{}", site, posts))
                .with_changes(serde_json::to_value(config).unwrap_or_default()),
        )
        .await
    }

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn delete_posts(&self, site: &str, posts: &str, username: &str) -> Result<bool> {
        debug!(
            site = site,
            posts = posts,
            user = username,
            "Deleting posts"
        );
        self.delete_config(
            site,
            &Self::posts_sk(posts),
            AuditEntry::new(username, AuditAction::DeletePosts)
                .with_target(format!("{}:{}", site, posts)),
        )
        .await
    }

    // ========================================================================
    // Permission Management
    // ========================================================================

    #[instrument(skip(self), fields(backend = "dynamodb"))]
    async fn get_site_permissions(&self, site: &str) -> Result<Option<GalleryPermissionConfig>> {
        debug!(site = site, "Getting site permissions");
        self.get_item_json(&Self::site_pk(site), Self::permissions_sk())
            .await
    }

    #[instrument(skip(self, config), fields(backend = "dynamodb"))]
    async fn set_site_permissions(
        &self,
        site: &str,
        config: &GalleryPermissionConfig,
        username: &str,
    ) -> Result<()> {
        debug!(site = site, user = username, "Setting site permissions");
        self.write_config(
            site,
            Self::permissions_sk(),
            config,
            AuditEntry::new(username, AuditAction::SetGalleryConfig)
                .with_target(format!("{}:permissions", site))
                .with_changes(serde_json::to_value(config).unwrap_or_default()),
        )
        .await
    }

    // ========================================================================
    // Change Detection
    // ========================================================================

    #[instrument(skip(self), fields(backend = "dynamodb", shard = %self.shard_id))]
    async fn get_config_versions(&self) -> Result<HashMap<String, u64>> {
        debug!("Getting shard config version");

        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(Self::shard_pk(&self.shard_id)))
            .key("sk", AttributeValue::S(Self::version_sk().to_string()))
            .send()
            .await
            .map_err(|e| ConfigStorageError::DynamoDb(format!("GetItem failed: {}", e)))?;

        let mut versions = HashMap::new();
        if let Some(item) = result.item {
            let version = get_number(&item, "version").unwrap_or(0);
            versions.insert(self.shard_id.clone(), version);
        }

        Ok(versions)
    }
}

fn get_string(item: &HashMap<String, AttributeValue>, key: &str) -> Result<String> {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ConfigStorageError::DynamoDb(format!("Missing or invalid string attribute: {}", key))
        })
}

fn get_number(item: &HashMap<String, AttributeValue>, key: &str) -> Option<u64> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_site_pk_format() {
        assert_eq!(DynamoConfigStorage::site_pk("default"), "SITE#default");
        assert_eq!(DynamoConfigStorage::site_pk("my-site"), "SITE#my-site");
    }

    #[test]
    fn test_config_sk() {
        assert_eq!(DynamoConfigStorage::config_sk(), "CONFIG");
    }

    #[test]
    fn test_gallery_sk_format() {
        assert_eq!(DynamoConfigStorage::gallery_sk("photos"), "GALLERY#photos");
    }

    #[test]
    fn test_posts_sk_format() {
        assert_eq!(DynamoConfigStorage::posts_sk("blog"), "POSTS#blog");
    }

    #[test]
    fn test_permissions_sk() {
        assert_eq!(DynamoConfigStorage::permissions_sk(), "PERMISSIONS");
    }

    #[test]
    fn test_shard_pk_format() {
        assert_eq!(DynamoConfigStorage::shard_pk("default"), "SHARD#default");
        assert_eq!(DynamoConfigStorage::shard_pk("prod"), "SHARD#prod");
    }

    #[test]
    fn test_site_membership_sk_format() {
        assert_eq!(
            DynamoConfigStorage::site_membership_sk("my-site"),
            "SITE#my-site"
        );
    }

    #[test]
    fn test_version_sk() {
        assert_eq!(DynamoConfigStorage::version_sk(), "VERSION");
    }

    #[test]
    fn test_audit_pk() {
        assert_eq!(DynamoConfigStorage::audit_pk(), "AUDIT");
    }

    #[test]
    fn test_audit_sk_uniqueness() {
        let sk1 = DynamoConfigStorage::audit_sk();
        let sk2 = DynamoConfigStorage::audit_sk();
        assert_ne!(sk1, sk2);
    }

    #[test]
    fn test_name_from_gallery_sk() {
        assert_eq!(
            DynamoConfigStorage::name_from_gallery_sk("GALLERY#photos"),
            Some("photos")
        );
        assert_eq!(
            DynamoConfigStorage::name_from_gallery_sk("POSTS#blog"),
            None
        );
        assert_eq!(DynamoConfigStorage::name_from_gallery_sk("CONFIG"), None);
    }

    #[test]
    fn test_name_from_posts_sk() {
        assert_eq!(
            DynamoConfigStorage::name_from_posts_sk("POSTS#blog"),
            Some("blog")
        );
        assert_eq!(
            DynamoConfigStorage::name_from_posts_sk("GALLERY#photos"),
            None
        );
    }
}
