//! Amazon S3 storage backend implementation.

use super::{ByteStream, ObjectMetadata, Storage, StorageEntry, StorageError};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    Client,
    config::Region,
    presigning::PresigningConfig,
    primitives::{ByteStream as AwsByteStream, DateTime as AwsDateTime},
};
use bytes::Bytes;
use futures::StreamExt;
use std::time::{Duration, SystemTime};
use tokio_util::io::ReaderStream;
use tracing::debug;

/// Convert AWS DateTime to std::time::SystemTime
fn aws_datetime_to_system_time(dt: &AwsDateTime) -> Option<SystemTime> {
    let secs = dt.secs();
    if secs >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
    } else {
        None
    }
}

/// Amazon S3 (or S3-compatible) storage backend.
///
/// This implementation supports:
/// - Standard AWS S3
/// - S3-compatible services (MinIO, LocalStack, etc.) via custom endpoint
/// - Presigned URLs for redirect-based serving
#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Storage {
    /// Create a new S3 storage backend.
    ///
    /// # Arguments
    /// * `bucket` - S3 bucket name
    /// * `prefix` - Key prefix for all objects (can be empty)
    /// * `region` - AWS region (uses SDK default if None)
    /// * `endpoint` - Custom endpoint URL for S3-compatible services (e.g., MinIO)
    pub async fn new(
        bucket: String,
        prefix: String,
        region: Option<String>,
        endpoint: Option<String>,
    ) -> Result<Self, StorageError> {
        let mut config_builder = aws_config::defaults(BehaviorVersion::latest());

        // Set region if provided
        if let Some(region_str) = &region {
            config_builder = config_builder.region(Region::new(region_str.clone()));
        }

        let aws_config = config_builder.load().await;

        // Build S3 client config
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);

        // Set custom endpoint if provided (for MinIO, LocalStack, etc.)
        if let Some(endpoint_url) = endpoint {
            s3_config_builder = s3_config_builder
                .endpoint_url(endpoint_url)
                .force_path_style(true); // Required for most S3-compatible services
        }

        let client = Client::from_conf(s3_config_builder.build());

        // Normalize prefix - remove leading/trailing slashes
        let prefix = prefix.trim_matches('/').to_string();

        Ok(Self {
            client,
            bucket,
            prefix,
        })
    }

    /// Validate a path for security.
    ///
    /// Rejects paths containing directory traversal sequences or null bytes.
    fn validate_path(path: &str) -> Result<(), StorageError> {
        if path.contains("..") {
            return Err(StorageError::PermissionDenied(format!(
                "Path contains directory traversal: {}",
                path
            )));
        }
        if path.contains('\0') {
            return Err(StorageError::PermissionDenied(format!(
                "Path contains null byte: {}",
                path
            )));
        }
        Ok(())
    }

    /// Build the full S3 key from a relative path.
    fn build_key(&self, path: &str) -> Result<String, StorageError> {
        Self::validate_path(path)?;
        let path = path.trim_matches('/');
        if self.prefix.is_empty() {
            Ok(path.to_string())
        } else if path.is_empty() {
            Ok(self.prefix.clone())
        } else {
            Ok(format!("{}/{}", self.prefix, path))
        }
    }

    /// Extract the relative path from an S3 key.
    fn strip_prefix(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            key.strip_prefix(&self.prefix)
                .unwrap_or(key)
                .trim_start_matches('/')
                .to_string()
        }
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        let key = self.build_key(path)?;
        debug!("S3 read: bucket={}, key={}", self.bucket, key);

        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(output) => {
                let data =
                    output.body.collect().await.map_err(|e| {
                        StorageError::Other(format!("Failed to read S3 body: {}", e))
                    })?;
                Ok(data.into_bytes())
            }
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_no_such_key() {
                    Err(StorageError::NotFound(path.to_string()))
                } else {
                    Err(StorageError::Other(format!(
                        "S3 GetObject error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        let key = self.build_key(path)?;
        debug!(
            "S3 write: bucket={}, key={}, size={}",
            self.bucket,
            key,
            data.len()
        );

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(AwsByteStream::from(data))
            .send()
            .await
            .map_err(|e| StorageError::Other(format!("S3 PutObject error: {}", e)))?;

        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let key = self.build_key(path)?;
        debug!("S3 exists: bucket={}, key={}", self.bucket, key);

        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(StorageError::Other(format!(
                        "S3 HeadObject error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        let key = self.build_key(path)?;
        debug!("S3 metadata: bucket={}, key={}", self.bucket, key);

        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(output) => Ok(ObjectMetadata {
                size: output.content_length().unwrap_or(0) as u64,
                last_modified: output.last_modified().and_then(aws_datetime_to_system_time),
                content_type: output.content_type().map(|s| s.to_string()),
                etag: output.e_tag().map(|s| s.to_string()),
            }),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Err(StorageError::NotFound(path.to_string()))
                } else {
                    Err(StorageError::Other(format!(
                        "S3 HeadObject error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let key = self.build_key(path)?;
        debug!("S3 delete: bucket={}, key={}", self.bucket, key);

        // Note: S3 DeleteObject doesn't return an error if the object doesn't exist
        // We could add a head_object check first if we want strict NotFound behavior
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Other(format!("S3 DeleteObject error: {}", e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_prefix = self.build_key(prefix)?;
        let full_prefix = if full_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", full_prefix.trim_end_matches('/'))
        };

        debug!("S3 list: bucket={}, prefix={}", self.bucket, full_prefix);

        let mut entries = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .delimiter("/");

            if !full_prefix.is_empty() {
                request = request.prefix(&full_prefix);
            }

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let result = request
                .send()
                .await
                .map_err(|e| StorageError::Other(format!("S3 ListObjectsV2 error: {}", e)))?;

            // Process objects (files)
            for obj in result.contents() {
                if let Some(key) = obj.key() {
                    let rel_path = self.strip_prefix(key);
                    // Remove the prefix directory from the path for immediate children
                    let name = rel_path
                        .strip_prefix(prefix.trim_matches('/'))
                        .unwrap_or(&rel_path)
                        .trim_start_matches('/')
                        .to_string();

                    if !name.is_empty() && !name.contains('/') {
                        entries.push(StorageEntry {
                            path: name,
                            is_dir: false,
                            metadata: Some(ObjectMetadata {
                                size: obj.size().unwrap_or(0) as u64,
                                last_modified: obj
                                    .last_modified()
                                    .and_then(|dt: &AwsDateTime| aws_datetime_to_system_time(dt)),
                                content_type: None,
                                etag: obj.e_tag().map(|s: &str| s.to_string()),
                            }),
                        });
                    }
                }
            }

            // Process common prefixes (directories)
            for prefix_obj in result.common_prefixes() {
                if let Some(prefix_str) = prefix_obj.prefix() {
                    let rel_path = self.strip_prefix(prefix_str);
                    let name = rel_path
                        .strip_prefix(prefix.trim_matches('/'))
                        .unwrap_or(&rel_path)
                        .trim_matches('/')
                        .to_string();

                    if !name.is_empty() && !name.contains('/') {
                        entries.push(StorageEntry {
                            path: name,
                            is_dir: true,
                            metadata: None,
                        });
                    }
                }
            }

            // Check for more results
            if result.is_truncated() == Some(true) {
                continuation_token = result.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(entries)
    }

    async fn list_recursive(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_prefix = self.build_key(prefix)?;
        let full_prefix = if full_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", full_prefix.trim_end_matches('/'))
        };

        debug!(
            "S3 list_recursive: bucket={}, prefix={}",
            self.bucket, full_prefix
        );

        let mut entries = Vec::new();
        let mut continuation_token: Option<String> = None;
        let mut seen_dirs = std::collections::HashSet::new();

        loop {
            let mut request = self.client.list_objects_v2().bucket(&self.bucket);

            if !full_prefix.is_empty() {
                request = request.prefix(&full_prefix);
            }

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let result = request
                .send()
                .await
                .map_err(|e| StorageError::Other(format!("S3 ListObjectsV2 error: {}", e)))?;

            for obj in result.contents() {
                if let Some(key) = obj.key() {
                    let rel_path = self.strip_prefix(key);
                    let name = rel_path
                        .strip_prefix(prefix.trim_matches('/'))
                        .unwrap_or(&rel_path)
                        .trim_start_matches('/')
                        .to_string();

                    if name.is_empty() {
                        continue;
                    }

                    // Add synthetic directory entries for parent paths
                    let parts: Vec<&str> = name.split('/').collect();
                    for i in 0..parts.len().saturating_sub(1) {
                        let dir_path = parts[..=i].join("/");
                        if seen_dirs.insert(dir_path.clone()) {
                            entries.push(StorageEntry {
                                path: dir_path,
                                is_dir: true,
                                metadata: None,
                            });
                        }
                    }

                    // Add the file entry
                    entries.push(StorageEntry {
                        path: name,
                        is_dir: false,
                        metadata: Some(ObjectMetadata {
                            size: obj.size().unwrap_or(0) as u64,
                            last_modified: obj
                                .last_modified()
                                .and_then(|dt: &AwsDateTime| aws_datetime_to_system_time(dt)),
                            content_type: None,
                            etag: obj.e_tag().map(|s: &str| s.to_string()),
                        }),
                    });
                }
            }

            if result.is_truncated() == Some(true) {
                continuation_token = result.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(entries)
    }

    async fn read_stream(&self, path: &str) -> Result<ByteStream, StorageError> {
        let key = self.build_key(path)?;
        debug!("S3 read_stream: bucket={}, key={}", self.bucket, key);

        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(output) => {
                // Convert AWS ByteStream to AsyncRead, then wrap in ReaderStream
                let async_read = output.body.into_async_read();
                let stream =
                    ReaderStream::new(async_read).map(|result| result.map_err(StorageError::Io));

                Ok(Box::pin(stream))
            }
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_no_such_key() {
                    Err(StorageError::NotFound(path.to_string()))
                } else {
                    Err(StorageError::Other(format!(
                        "S3 GetObject error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Bytes, StorageError> {
        let key = self.build_key(path)?;
        // S3 Range header format: bytes=start-end (inclusive)
        let end = offset.saturating_add(length).saturating_sub(1);
        let range = format!("bytes={}-{}", offset, end);
        debug!(
            "S3 read_range: bucket={}, key={}, range={}",
            self.bucket, key, range
        );

        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .range(&range)
            .send()
            .await;

        match result {
            Ok(output) => {
                let data =
                    output.body.collect().await.map_err(|e| {
                        StorageError::Other(format!("Failed to read S3 body: {}", e))
                    })?;
                Ok(data.into_bytes())
            }
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_no_such_key() {
                    Err(StorageError::NotFound(path.to_string()))
                } else {
                    Err(StorageError::Other(format!(
                        "S3 GetObject error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn create_dir(&self, _path: &str) -> Result<(), StorageError> {
        // S3 doesn't have directories - they're implicit from object keys
        // This is a no-op for S3
        Ok(())
    }

    fn storage_type(&self) -> &'static str {
        "s3"
    }

    fn root_path(&self) -> String {
        if self.prefix.is_empty() {
            format!("s3://{}", self.bucket)
        } else {
            format!("s3://{}/{}", self.bucket, self.prefix)
        }
    }

    async fn signed_url(&self, path: &str, expiry: Duration) -> Option<String> {
        let key = self.build_key(path).ok()?;
        debug!(
            "S3 signed_url: bucket={}, key={}, expiry={:?}",
            self.bucket, key, expiry
        );

        let presigning_config = PresigningConfig::builder()
            .expires_in(expiry)
            .build()
            .ok()?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .presigned(presigning_config)
            .await
            .ok()?;

        Some(presigned.uri().to_string())
    }

    fn supports_redirect(&self) -> bool {
        true
    }

    async fn write_if_match(
        &self,
        path: &str,
        data: Bytes,
        expected_etag: Option<&str>,
    ) -> Result<String, StorageError> {
        let key = self.build_key(path)?;
        debug!(
            "S3 write_if_match: bucket={}, key={}, expected_etag={:?}",
            self.bucket, key, expected_etag
        );

        // For S3, we need to check first then write (no atomic conditional write)
        // This is a race condition but acceptable for our use case
        let current_meta = self.metadata(path).await.ok();

        match (expected_etag, &current_meta) {
            // Expect file to not exist, but it does
            (None, Some(_)) => {
                return Err(StorageError::PreconditionFailed(format!(
                    "Object already exists: {}",
                    path
                )));
            }
            // Expect specific ETag, but file doesn't exist
            (Some(expected), None) => {
                return Err(StorageError::PreconditionFailed(format!(
                    "Object does not exist, expected etag {}: {}",
                    expected, path
                )));
            }
            // Expect specific ETag, check it matches
            (Some(expected), Some(meta)) => {
                let current_etag = meta.etag.as_deref().unwrap_or("");
                // S3 ETags may have quotes around them
                let expected_normalized = expected.trim_matches('"');
                let current_normalized = current_etag.trim_matches('"');
                if current_normalized != expected_normalized {
                    return Err(StorageError::PreconditionFailed(format!(
                        "ETag mismatch: expected {}, got {}: {}",
                        expected, current_etag, path
                    )));
                }
            }
            // No expectation and file doesn't exist - OK to create
            (None, None) => {}
        }

        // Perform the write
        let put_result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(AwsByteStream::from(data))
            .send()
            .await
            .map_err(|e| StorageError::Other(format!("S3 PutObject error: {}", e)))?;

        // Return the new ETag
        let new_etag = put_result.e_tag().unwrap_or("").to_string();
        Ok(new_etag)
    }
}

impl std::fmt::Debug for S3Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Storage")
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .finish()
    }
}
