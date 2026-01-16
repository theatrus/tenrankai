//! URL-based storage configuration parsing.
//!
//! Supports various URL formats:
//! - `photos` - relative filesystem path
//! - `/absolute/path` - absolute filesystem path
//! - `file:///absolute/path` - explicit file:// scheme
//! - `s3://bucket/prefix` - S3 bucket with prefix
//! - `s3://bucket/prefix?region=us-east-1` - S3 with region
//! - `s3://bucket/prefix?endpoint=http://minio:9000` - S3 with custom endpoint

use super::{FilesystemStorage, S3Storage, Storage, StorageError};
use std::path::PathBuf;
use std::sync::Arc;

/// Parsed storage URL configuration.
///
/// This enum represents the different storage backends that can be
/// configured via URL strings.
#[derive(Debug, Clone)]
pub enum StorageUrl {
    /// Local filesystem storage.
    Filesystem {
        /// Path to the storage root directory.
        path: PathBuf,
    },
    /// Amazon S3 (or S3-compatible) storage.
    S3 {
        /// S3 bucket name.
        bucket: String,
        /// Key prefix within the bucket.
        prefix: String,
        /// AWS region (optional, uses SDK default if not specified).
        region: Option<String>,
        /// Custom endpoint URL (for MinIO, LocalStack, etc.).
        endpoint: Option<String>,
    },
}

impl StorageUrl {
    /// Parse a storage URL string into a StorageUrl.
    ///
    /// # Examples
    ///
    /// ```
    /// use tenrankai_storage::StorageUrl;
    ///
    /// // Local filesystem
    /// let url = StorageUrl::parse("photos").unwrap();
    /// let url = StorageUrl::parse("/var/data/photos").unwrap();
    /// let url = StorageUrl::parse("file:///var/data/photos").unwrap();
    ///
    /// // S3
    /// let url = StorageUrl::parse("s3://my-bucket/prefix").unwrap();
    /// let url = StorageUrl::parse("s3://my-bucket/prefix?region=us-west-2").unwrap();
    /// ```
    pub fn parse(s: &str) -> Result<Self, StorageError> {
        // Check for s3:// scheme
        if s.starts_with("s3://") {
            Self::parse_s3_url(s)
        } else if let Some(path) = s.strip_prefix("file://") {
            // Explicit file:// URL
            Ok(StorageUrl::Filesystem {
                path: PathBuf::from(path),
            })
        } else {
            // Plain path = filesystem
            Ok(StorageUrl::Filesystem {
                path: PathBuf::from(s),
            })
        }
    }

    /// Parse an S3 URL.
    fn parse_s3_url(s: &str) -> Result<Self, StorageError> {
        let url = url::Url::parse(s).map_err(|e| StorageError::InvalidUrl(e.to_string()))?;

        let bucket = url
            .host_str()
            .ok_or_else(|| StorageError::InvalidUrl("S3 URL missing bucket name".into()))?
            .to_string();

        if bucket.is_empty() {
            return Err(StorageError::InvalidUrl(
                "S3 URL has empty bucket name".into(),
            ));
        }

        // Get prefix from path, stripping leading slash
        let prefix = url.path().trim_start_matches('/').to_string();

        // Parse query parameters for options
        let mut region = None;
        let mut endpoint = None;

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "region" => region = Some(value.to_string()),
                "endpoint" => endpoint = Some(value.to_string()),
                _ => {
                    // Ignore unknown parameters for forward compatibility
                    tracing::debug!("Ignoring unknown S3 URL parameter: {}={}", key, value);
                }
            }
        }

        Ok(StorageUrl::S3 {
            bucket,
            prefix,
            region,
            endpoint,
        })
    }

    /// Convert this URL into a storage backend.
    ///
    /// For S3 URLs, this will initialize the AWS SDK and create an S3 client.
    /// For filesystem URLs, this returns immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage backend cannot be initialized
    /// (e.g., invalid AWS credentials for S3).
    pub async fn into_storage(self) -> Result<Arc<dyn Storage>, StorageError> {
        match self {
            StorageUrl::Filesystem { path } => Ok(Arc::new(FilesystemStorage::new(path))),
            StorageUrl::S3 {
                bucket,
                prefix,
                region,
                endpoint,
            } => {
                let storage = S3Storage::new(bucket, prefix, region, endpoint).await?;
                Ok(Arc::new(storage))
            }
        }
    }

    /// Check if this URL represents filesystem storage.
    pub fn is_filesystem(&self) -> bool {
        matches!(self, StorageUrl::Filesystem { .. })
    }

    /// Check if this URL represents S3 storage.
    pub fn is_s3(&self) -> bool {
        matches!(self, StorageUrl::S3 { .. })
    }

    /// Get the clean path/prefix without query parameters.
    ///
    /// For filesystem: returns the path as a string.
    /// For S3: returns just the prefix (e.g., "theatr.us/cache/photos").
    ///
    /// This is useful for constructing file paths within the storage.
    pub fn prefix(&self) -> &str {
        match self {
            StorageUrl::Filesystem { path } => path.to_str().unwrap_or(""),
            StorageUrl::S3 { prefix, .. } => prefix,
        }
    }

    /// Get a clean display string without query parameters.
    ///
    /// Useful for logging and debugging where the full URL with
    /// region/endpoint params would be noisy.
    ///
    /// For filesystem: returns the path.
    /// For S3: returns "s3://bucket/prefix" without query string.
    pub fn display_clean(&self) -> String {
        match self {
            StorageUrl::Filesystem { path } => path.display().to_string(),
            StorageUrl::S3 { bucket, prefix, .. } => {
                if prefix.is_empty() {
                    format!("s3://{}", bucket)
                } else {
                    format!("s3://{}/{}", bucket, prefix)
                }
            }
        }
    }

    /// Get the filesystem path if this is a filesystem URL.
    ///
    /// Returns None for S3 URLs.
    pub fn filesystem_path(&self) -> Option<&PathBuf> {
        match self {
            StorageUrl::Filesystem { path } => Some(path),
            StorageUrl::S3 { .. } => None,
        }
    }
}

impl std::fmt::Display for StorageUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageUrl::Filesystem { path } => write!(f, "{}", path.display()),
            StorageUrl::S3 {
                bucket,
                prefix,
                region,
                endpoint,
            } => {
                write!(f, "s3://{}/{}", bucket, prefix)?;
                let mut has_query = false;
                if let Some(r) = region {
                    write!(f, "?region={}", r)?;
                    has_query = true;
                }
                if let Some(e) = endpoint {
                    write!(f, "{}endpoint={}", if has_query { "&" } else { "?" }, e)?;
                }
                Ok(())
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for StorageUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        StorageUrl::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for StorageUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
