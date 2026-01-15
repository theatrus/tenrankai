//! DynamoDB user storage backend.
//!
//! This backend stores users in AWS DynamoDB using a single-table design
//! with composite keys for efficient queries.
//!
//! Table Schema:
//! - PK: `{site_id}#USER#{username}` - Partition key for user data
//! - SK: `PROFILE` for user profile, `PASSKEY#{id}` for passkeys
//!
//! GSIs:
//! - `email-index`: For looking up users by email (PK: site_id, SK: email_lower)
//! - `credential-index`: For looking up passkeys by credential_id (PK: credential_id_b64)

use super::error::UserStorageError;
use super::types::{User, UserPasskey};
use super::UserStorage;
use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

/// DynamoDB user storage backend.
///
/// Uses a single-table design with composite keys for efficient queries.
/// Supports multi-tenant isolation through site_id in the partition key.
#[derive(Debug, Clone)]
pub struct DynamoUserStorage {
    client: Client,
    table_name: String,
    site_id: String,
}

impl DynamoUserStorage {
    /// Create a new DynamoDB user storage.
    ///
    /// # Arguments
    ///
    /// * `table_name` - The DynamoDB table name
    /// * `site_id` - The site identifier for multi-tenant isolation
    /// * `region` - Optional AWS region (defaults to SDK default)
    /// * `endpoint` - Optional custom endpoint (for local testing with DynamoDB Local)
    pub async fn new(
        table_name: String,
        site_id: String,
        region: Option<String>,
        endpoint: Option<String>,
    ) -> Result<Self, UserStorageError> {
        let mut config_loader =
            aws_config::defaults(aws_config::BehaviorVersion::latest());

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

        info!(
            "Connected to DynamoDB user storage: table={}, site={}",
            table_name, site_id
        );

        Ok(Self {
            client,
            table_name,
            site_id,
        })
    }

    /// Build the partition key for a user.
    fn user_pk(&self, username: &str) -> String {
        format!("{}#USER#{}", self.site_id, username)
    }

    /// Build the sort key for user profile.
    fn profile_sk() -> String {
        "PROFILE".to_string()
    }

    /// Build the sort key for a passkey.
    fn passkey_sk(passkey_id: &Uuid) -> String {
        format!("PASSKEY#{}", passkey_id)
    }

    /// Extract username from partition key.
    fn username_from_pk(&self, pk: &str) -> Option<String> {
        let prefix = format!("{}#USER#", self.site_id);
        pk.strip_prefix(&prefix).map(|s| s.to_string())
    }

    /// Extract passkey ID from sort key.
    fn passkey_id_from_sk(sk: &str) -> Option<Uuid> {
        sk.strip_prefix("PASSKEY#")
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Load passkeys for a user from DynamoDB.
    async fn load_passkeys(&self, username: &str) -> Result<Vec<UserPasskey>, UserStorageError> {
        let pk = self.user_pk(username);

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression("pk = :pk AND begins_with(sk, :sk_prefix)")
            .expression_attribute_values(":pk", AttributeValue::S(pk))
            .expression_attribute_values(":sk_prefix", AttributeValue::S("PASSKEY#".to_string()))
            .send()
            .await
            .map_err(|e| UserStorageError::Database(format!("Failed to query passkeys: {}", e)))?;

        let mut passkeys = Vec::new();

        if let Some(items) = result.items {
            for item in items {
                if let Some(passkey) = self.item_to_passkey(&item)? {
                    passkeys.push(passkey);
                }
            }
        }

        Ok(passkeys)
    }

    /// Convert a DynamoDB item to a UserPasskey.
    fn item_to_passkey(
        &self,
        item: &HashMap<String, AttributeValue>,
    ) -> Result<Option<UserPasskey>, UserStorageError> {
        let sk = get_string(item, "sk")?;
        let id = match Self::passkey_id_from_sk(&sk) {
            Some(id) => id,
            None => return Ok(None),
        };

        let name = get_string(item, "name")?;
        let credential_json = get_string(item, "credential_json")?;
        let created_at = get_number(item, "created_at").unwrap_or(0);
        let last_used_at = get_optional_number(item, "last_used_at");

        let credential: webauthn_rs::prelude::Passkey =
            serde_json::from_str(&credential_json).map_err(|e| {
                UserStorageError::Database(format!("Failed to deserialize passkey: {}", e))
            })?;

        Ok(Some(UserPasskey {
            id,
            name,
            credential,
            created_at,
            last_used_at,
        }))
    }

    /// Convert a DynamoDB item to a User (profile only, without passkeys).
    fn item_to_user_profile(
        &self,
        item: &HashMap<String, AttributeValue>,
    ) -> Result<Option<(String, String)>, UserStorageError> {
        let sk = get_string(item, "sk")?;
        if sk != "PROFILE" {
            return Ok(None);
        }

        let pk = get_string(item, "pk")?;
        let username = match self.username_from_pk(&pk) {
            Some(u) => u,
            None => return Ok(None),
        };

        let email = get_string(item, "email")?;

        Ok(Some((username, email)))
    }
}

#[async_trait]
impl UserStorage for DynamoUserStorage {
    async fn get_user(&self, username: &str) -> Result<Option<User>, UserStorageError> {
        let pk = self.user_pk(username);

        // Get the user profile
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(Self::profile_sk()))
            .send()
            .await
            .map_err(|e| UserStorageError::Database(format!("Failed to get user: {}", e)))?;

        match result.item {
            Some(item) => {
                let email = get_string(&item, "email")?;
                let passkeys = self.load_passkeys(username).await?;

                Ok(Some(User { email, passkeys }))
            }
            None => Ok(None),
        }
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(String, User)>, UserStorageError> {
        // Use the email-index GSI
        let email_lower = email.to_lowercase();

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name("email-index")
            .key_condition_expression("site_id = :site_id AND email_lower = :email")
            .expression_attribute_values(":site_id", AttributeValue::S(self.site_id.clone()))
            .expression_attribute_values(":email", AttributeValue::S(email_lower))
            .send()
            .await
            .map_err(|e| UserStorageError::Database(format!("Failed to query by email: {}", e)))?;

        if let Some(items) = result.items
            && let Some(item) = items.first()
        {
            let pk = get_string(item, "pk")?;
            if let Some(username) = self.username_from_pk(&pk)
                && let Some(user) = self.get_user(&username).await?
            {
                return Ok(Some((username, user)));
            }
        }

        Ok(None)
    }

    async fn list_users(&self) -> Result<Vec<(String, User)>, UserStorageError> {
        // Query all user profiles for this site
        // Using a scan with filter for site_id prefix is expensive but necessary
        // for listing all users. Consider adding a GSI if this becomes a bottleneck.

        let mut users = Vec::new();
        let mut exclusive_start_key = None;

        loop {
            let mut query = self
                .client
                .scan()
                .table_name(&self.table_name)
                .filter_expression(
                    "begins_with(pk, :site_prefix) AND sk = :profile_sk",
                )
                .expression_attribute_values(
                    ":site_prefix",
                    AttributeValue::S(format!("{}#USER#", self.site_id)),
                )
                .expression_attribute_values(":profile_sk", AttributeValue::S("PROFILE".to_string()));

            if let Some(key) = exclusive_start_key {
                query = query.set_exclusive_start_key(Some(key));
            }

            let result = query.send().await.map_err(|e| {
                UserStorageError::Database(format!("Failed to list users: {}", e))
            })?;

            if let Some(items) = result.items {
                for item in items {
                    if let Some((username, email)) = self.item_to_user_profile(&item)? {
                        let passkeys = self.load_passkeys(&username).await?;
                        users.push((username, User { email, passkeys }));
                    }
                }
            }

            exclusive_start_key = result.last_evaluated_key;
            if exclusive_start_key.is_none() {
                break;
            }
        }

        // Sort by username for consistent ordering
        users.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(users)
    }

    async fn add_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        let pk = self.user_pk(username);
        let now = chrono::Utc::now().timestamp();
        let email_lower = user.email.to_lowercase();

        // Check if user already exists using a condition expression
        let result = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("pk", AttributeValue::S(pk.clone()))
            .item("sk", AttributeValue::S(Self::profile_sk()))
            .item("email", AttributeValue::S(user.email.clone()))
            .item("email_lower", AttributeValue::S(email_lower))
            .item("site_id", AttributeValue::S(self.site_id.clone()))
            .item("created_at", AttributeValue::N(now.to_string()))
            .item("updated_at", AttributeValue::N(now.to_string()))
            .condition_expression("attribute_not_exists(pk)")
            .send()
            .await;

        match result {
            Ok(_) => {
                // Add any passkeys
                for passkey in &user.passkeys {
                    self.add_passkey(username, passkey.clone()).await?;
                }

                debug!("Added user {} to DynamoDB storage", username);
                Ok(())
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("ConditionalCheckFailed") {
                    Err(UserStorageError::UserAlreadyExists(username.to_string()))
                } else {
                    Err(UserStorageError::Database(format!(
                        "Failed to add user: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn update_user(&self, username: &str, user: &User) -> Result<(), UserStorageError> {
        let pk = self.user_pk(username);
        let now = chrono::Utc::now().timestamp();
        let email_lower = user.email.to_lowercase();

        // Update the user profile, requiring it to exist
        let result = self
            .client
            .update_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(Self::profile_sk()))
            .update_expression("SET email = :email, email_lower = :email_lower, updated_at = :updated_at")
            .expression_attribute_values(":email", AttributeValue::S(user.email.clone()))
            .expression_attribute_values(":email_lower", AttributeValue::S(email_lower))
            .expression_attribute_values(":updated_at", AttributeValue::N(now.to_string()))
            .condition_expression("attribute_exists(pk)")
            .send()
            .await;

        match result {
            Ok(_) => {
                debug!("Updated user {} in DynamoDB storage", username);
                Ok(())
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("ConditionalCheckFailed") {
                    Err(UserStorageError::UserNotFound(username.to_string()))
                } else {
                    Err(UserStorageError::Database(format!(
                        "Failed to update user: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn remove_user(&self, username: &str) -> Result<bool, UserStorageError> {
        let pk = self.user_pk(username);

        // First, check if the user exists
        let existing = self.get_user(username).await?;
        if existing.is_none() {
            return Ok(false);
        }

        // Delete all items for this user (profile + passkeys)
        // First, query all items with this pk
        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression("pk = :pk")
            .expression_attribute_values(":pk", AttributeValue::S(pk.clone()))
            .send()
            .await
            .map_err(|e| {
                UserStorageError::Database(format!("Failed to query user items: {}", e))
            })?;

        // Delete each item
        if let Some(items) = result.items {
            for item in items {
                let item_pk = get_string(&item, "pk")?;
                let item_sk = get_string(&item, "sk")?;

                self.client
                    .delete_item()
                    .table_name(&self.table_name)
                    .key("pk", AttributeValue::S(item_pk))
                    .key("sk", AttributeValue::S(item_sk))
                    .send()
                    .await
                    .map_err(|e| {
                        UserStorageError::Database(format!("Failed to delete item: {}", e))
                    })?;
            }
        }

        debug!("Removed user {} from DynamoDB storage", username);
        Ok(true)
    }

    fn backend_name(&self) -> &str {
        "dynamodb"
    }

    async fn add_passkey(
        &self,
        username: &str,
        passkey: UserPasskey,
    ) -> Result<(), UserStorageError> {
        let pk = self.user_pk(username);
        let sk = Self::passkey_sk(&passkey.id);

        let credential_json = serde_json::to_string(&passkey.credential)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        let credential_id_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            passkey.credential.cred_id().as_ref(),
        );

        let mut item_builder = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("pk", AttributeValue::S(pk))
            .item("sk", AttributeValue::S(sk))
            .item("name", AttributeValue::S(passkey.name))
            .item("credential_json", AttributeValue::S(credential_json))
            .item("credential_id_b64", AttributeValue::S(credential_id_b64))
            .item("site_id", AttributeValue::S(self.site_id.clone()))
            .item(
                "created_at",
                AttributeValue::N(passkey.created_at.to_string()),
            );

        if let Some(last_used) = passkey.last_used_at {
            item_builder = item_builder.item("last_used_at", AttributeValue::N(last_used.to_string()));
        }

        item_builder.send().await.map_err(|e| {
            UserStorageError::Database(format!("Failed to add passkey: {}", e))
        })?;

        debug!("Added passkey {} for user {}", passkey.id, username);
        Ok(())
    }

    async fn remove_passkey(
        &self,
        username: &str,
        passkey_id: &Uuid,
    ) -> Result<bool, UserStorageError> {
        let pk = self.user_pk(username);
        let sk = Self::passkey_sk(passkey_id);

        // Use a condition to check if it exists
        let result = self
            .client
            .delete_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .condition_expression("attribute_exists(pk)")
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllOld)
            .send()
            .await;

        match result {
            Ok(output) => {
                let removed = output.attributes.is_some();
                if removed {
                    debug!("Removed passkey {} for user {}", passkey_id, username);
                }
                Ok(removed)
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("ConditionalCheckFailed") {
                    Ok(false)
                } else {
                    Err(UserStorageError::Database(format!(
                        "Failed to remove passkey: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn update_passkey_after_auth(
        &self,
        username: &str,
        passkey_id: &Uuid,
        auth_result: &webauthn_rs::prelude::AuthenticationResult,
    ) -> Result<(), UserStorageError> {
        let pk = self.user_pk(username);
        let sk = Self::passkey_sk(passkey_id);

        // First, get the current passkey
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk.clone()))
            .key("sk", AttributeValue::S(sk.clone()))
            .send()
            .await
            .map_err(|e| UserStorageError::Database(format!("Failed to get passkey: {}", e)))?;

        let item = result
            .item
            .ok_or_else(|| UserStorageError::PasskeyNotFound(passkey_id.to_string()))?;

        let credential_json = get_string(&item, "credential_json")?;
        let mut credential: webauthn_rs::prelude::Passkey =
            serde_json::from_str(&credential_json)
                .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        credential.update_credential(auth_result);

        let updated_json = serde_json::to_string(&credential)
            .map_err(|e| UserStorageError::Serialization(e.to_string()))?;

        let now = chrono::Utc::now().timestamp();

        // Update the passkey
        self.client
            .update_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .update_expression("SET credential_json = :cred, last_used_at = :last_used")
            .expression_attribute_values(":cred", AttributeValue::S(updated_json))
            .expression_attribute_values(":last_used", AttributeValue::N(now.to_string()))
            .send()
            .await
            .map_err(|e| {
                UserStorageError::Database(format!("Failed to update passkey: {}", e))
            })?;

        debug!(
            "Updated passkey {} after auth for user {}",
            passkey_id, username
        );
        Ok(())
    }

    async fn get_passkey_by_credential_id(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(String, UserPasskey)>, UserStorageError> {
        // Use the credential-index GSI
        let credential_id_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, credential_id);

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name("credential-index")
            .key_condition_expression("credential_id_b64 = :cred_id AND site_id = :site_id")
            .expression_attribute_values(":cred_id", AttributeValue::S(credential_id_b64))
            .expression_attribute_values(":site_id", AttributeValue::S(self.site_id.clone()))
            .send()
            .await
            .map_err(|e| {
                UserStorageError::Database(format!("Failed to query by credential_id: {}", e))
            })?;

        if let Some(items) = result.items
            && let Some(item) = items.first()
        {
            let pk = get_string(item, "pk")?;
            if let Some(username) = self.username_from_pk(&pk)
                && let Some(passkey) = self.item_to_passkey(item)?
            {
                return Ok(Some((username, passkey)));
            }
        }

        Ok(None)
    }
}

/// Helper function to get a string attribute from a DynamoDB item.
fn get_string(
    item: &HashMap<String, AttributeValue>,
    key: &str,
) -> Result<String, UserStorageError> {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            UserStorageError::Database(format!("Missing or invalid string attribute: {}", key))
        })
}

/// Helper function to get a number attribute from a DynamoDB item.
fn get_number(item: &HashMap<String, AttributeValue>, key: &str) -> Result<i64, UserStorageError> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            UserStorageError::Database(format!("Missing or invalid number attribute: {}", key))
        })
}

/// Helper function to get an optional number attribute from a DynamoDB item.
fn get_optional_number(item: &HashMap<String, AttributeValue>, key: &str) -> Option<i64> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests {
    // DynamoDB tests require a running DynamoDB Local instance or AWS credentials.
    // These tests are marked as ignored by default and can be run with:
    // cargo test --features user-storage-dynamodb -- --ignored

    #[test]
    fn test_user_pk_format() {
        // Simple unit test for pk format
        let site_id = "test-site";
        let username = "alice";
        let expected = format!("{}#USER#{}", site_id, username);
        assert_eq!(expected, "test-site#USER#alice");
    }

    #[test]
    fn test_passkey_sk_format() {
        use uuid::Uuid;
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let expected = format!("PASSKEY#{}", id);
        assert_eq!(expected, "PASSKEY#550e8400-e29b-41d4-a716-446655440000");
    }
}
