//! Pluggable storage abstraction for Tenrankai.
//!
//! This module provides a unified interface for storage backends, enabling
//! both local filesystem and remote (S3) storage for galleries, caches,
//! static files, templates, and posts.
//!
//! # Example
//!
//! ```ignore
//! use tenrankai::storage::{Storage, StorageUrl};
//!
//! // Parse a storage URL
//! let url = StorageUrl::parse("s3://my-bucket/prefix?region=us-east-1")?;
//! let storage = url.into_storage().await?;
//!
//! // Use the storage
//! let data = storage.read("path/to/file.txt").await?;
//! storage.write("output.txt", data).await?;
//! ```

mod error;
mod filesystem;
mod s3;
mod sync_adapter;
mod url;

pub use error::StorageError;
pub use filesystem::FilesystemStorage;
pub use s3::S3Storage;
pub use sync_adapter::{
    SyncStorageAdapter, SyncStorageReader, storage_exists_sync, storage_open_sync,
    storage_read_range_sync, storage_read_sync, storage_write_sync,
};
pub use url::StorageUrl;

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Metadata about a stored object.
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    /// Size of the object in bytes.
    pub size: u64,
    /// Last modification time, if available.
    pub last_modified: Option<SystemTime>,
    /// MIME content type, if known.
    pub content_type: Option<String>,
    /// Entity tag for cache validation.
    pub etag: Option<String>,
}

/// A directory entry from listing operations.
#[derive(Debug, Clone)]
pub struct StorageEntry {
    /// Relative path of the entry.
    pub path: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Metadata about the entry, if available.
    pub metadata: Option<ObjectMetadata>,
}

/// Type alias for a boxed stream of bytes.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>;

/// Async storage operations trait.
///
/// This trait defines the interface for all storage backends. Implementations
/// must be thread-safe (`Send + Sync`) and have a static lifetime.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Read entire object into memory.
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    ///
    /// # Returns
    /// The object contents as `Bytes`, or an error if the object doesn't exist
    /// or cannot be read.
    async fn read(&self, path: &str) -> Result<Bytes, StorageError>;

    /// Write data to storage.
    ///
    /// Creates parent directories/prefixes as needed.
    ///
    /// # Arguments
    /// * `path` - Relative path to write to
    /// * `data` - The data to write
    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError>;

    /// Check if object exists.
    ///
    /// # Arguments
    /// * `path` - Relative path to check
    ///
    /// # Returns
    /// `true` if the object exists, `false` otherwise.
    async fn exists(&self, path: &str) -> Result<bool, StorageError>;

    /// Get object metadata without reading content.
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    ///
    /// # Returns
    /// Metadata about the object, or `NotFound` error if it doesn't exist.
    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError>;

    /// Delete an object.
    ///
    /// # Arguments
    /// * `path` - Relative path to delete
    ///
    /// # Returns
    /// Ok if deleted successfully, or `NotFound` if the object doesn't exist.
    async fn delete(&self, path: &str) -> Result<(), StorageError>;

    /// List objects with a prefix (non-recursive).
    ///
    /// Lists only immediate children of the given prefix.
    ///
    /// # Arguments
    /// * `prefix` - Directory/prefix to list
    ///
    /// # Returns
    /// Vector of entries in the directory.
    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError>;

    /// List objects recursively with a prefix.
    ///
    /// Lists all objects under the given prefix, including nested directories.
    ///
    /// # Arguments
    /// * `prefix` - Directory/prefix to list recursively
    ///
    /// # Returns
    /// Vector of all entries under the prefix.
    async fn list_recursive(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError>;

    /// Create a streaming reader for large objects.
    ///
    /// Use this for serving files to clients without loading into memory.
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    ///
    /// # Returns
    /// A stream of byte chunks.
    async fn read_stream(&self, path: &str) -> Result<ByteStream, StorageError>;

    /// Read a range of bytes from an object.
    ///
    /// This is useful for:
    /// - Resumable downloads
    /// - Seeking within large files
    /// - Efficient partial reads (e.g., reading headers without loading entire file)
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    /// * `offset` - Byte offset to start reading from
    /// * `length` - Number of bytes to read
    ///
    /// # Returns
    /// The requested byte range. May return fewer bytes if the range extends
    /// past the end of the file.
    async fn read_range(&self, path: &str, offset: u64, length: u64)
    -> Result<Bytes, StorageError>;

    /// Create directory (no-op for S3, creates dir for filesystem).
    ///
    /// # Arguments
    /// * `path` - Directory path to create
    async fn create_dir(&self, path: &str) -> Result<(), StorageError>;

    /// Get the storage type name (for logging/debugging).
    fn storage_type(&self) -> &'static str;

    /// Generate a signed URL for direct client access (optional).
    ///
    /// Returns `None` if the storage doesn't support signed URLs (e.g., filesystem).
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    /// * `expiry` - How long the signed URL should be valid
    ///
    /// # Returns
    /// A signed URL string, or `None` if not supported.
    async fn signed_url(&self, _path: &str, _expiry: Duration) -> Option<String> {
        None
    }

    /// Check if this storage supports redirect-based serving.
    ///
    /// If `true`, the storage can generate signed URLs that clients can
    /// be redirected to, reducing server bandwidth.
    fn supports_redirect(&self) -> bool {
        false
    }

    /// Read an object as a UTF-8 string.
    ///
    /// Convenience method for reading text files like templates, markdown, etc.
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    ///
    /// # Returns
    /// The object contents as a String, or an error if the object doesn't exist
    /// or contains invalid UTF-8.
    async fn read_to_string(&self, path: &str) -> Result<String, StorageError> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| StorageError::Other(format!("Invalid UTF-8 in {}: {}", path, e)))
    }

    /// Conditional write: only write if the current ETag matches.
    ///
    /// This provides optimistic concurrency control for distributed systems.
    /// If `expected_etag` is `None`, the write succeeds only if the file doesn't exist.
    /// If `expected_etag` is `Some(etag)`, the write succeeds only if the current ETag matches.
    ///
    /// # Arguments
    /// * `path` - Relative path to write to
    /// * `data` - The data to write
    /// * `expected_etag` - Expected ETag for conditional write, or None for create-only
    ///
    /// # Returns
    /// * `Ok(new_etag)` - Write succeeded, returns the new ETag
    /// * `Err(PreconditionFailed)` - ETag mismatch, write rejected
    /// * `Err(other)` - Other error occurred
    async fn write_if_match(
        &self,
        path: &str,
        data: Bytes,
        expected_etag: Option<&str>,
    ) -> Result<String, StorageError>;

    /// Read an object with its ETag for subsequent conditional writes.
    ///
    /// # Arguments
    /// * `path` - Relative path to the object
    ///
    /// # Returns
    /// Tuple of (data, etag) where etag can be used for `write_if_match`.
    async fn read_with_etag(&self, path: &str) -> Result<(Bytes, String), StorageError> {
        let data = self.read(path).await?;
        let meta = self.metadata(path).await?;
        let etag = meta.etag.unwrap_or_default();
        Ok((data, etag))
    }
}

/// Type alias for a thread-safe storage backend.
pub type DynStorage = Arc<dyn Storage>;

/// Create a storage backend from a URL string.
///
/// Parses the URL and creates the appropriate storage backend (filesystem or S3).
///
/// # Arguments
/// * `url_str` - Storage URL string (filesystem path or s3://...)
///
/// # Returns
/// A storage backend, or an error if the URL fails to parse or initialize.
///
/// # Example
/// ```ignore
/// let storage = create_storage_from_url("posts/blog").await?;
/// let storage = create_storage_from_url("s3://bucket/prefix?region=us-west-2").await?;
/// ```
pub async fn create_storage_from_url(url_str: &str) -> Result<DynStorage, StorageError> {
    let url = StorageUrl::parse(url_str)?;
    url.into_storage().await
}

/// Create storage backends from a list of URL strings.
///
/// Parses each URL and creates the appropriate storage backend (filesystem or S3).
///
/// # Arguments
/// * `urls` - List of storage URL strings
///
/// # Returns
/// Vector of storage backends, or an error if any URL fails to parse or initialize.
pub async fn create_storages_from_urls(urls: &[String]) -> Result<Vec<DynStorage>, StorageError> {
    let mut storages = Vec::with_capacity(urls.len());
    for url_str in urls {
        let storage = create_storage_from_url(url_str).await?;
        storages.push(storage);
    }
    Ok(storages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_filesystem_storage_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Write
        storage
            .write("test.txt", Bytes::from("hello world"))
            .await
            .unwrap();

        // Exists
        assert!(storage.exists("test.txt").await.unwrap());
        assert!(!storage.exists("nonexistent.txt").await.unwrap());

        // Read
        let data = storage.read("test.txt").await.unwrap();
        assert_eq!(&data[..], b"hello world");

        // Metadata
        let meta = storage.metadata("test.txt").await.unwrap();
        assert_eq!(meta.size, 11);
        assert!(meta.last_modified.is_some());

        // Delete
        storage.delete("test.txt").await.unwrap();
        assert!(!storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_filesystem_storage_nested_paths() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Write to nested path (should create directories)
        storage
            .write("a/b/c/file.txt", Bytes::from("nested"))
            .await
            .unwrap();

        // Read back
        let data = storage.read("a/b/c/file.txt").await.unwrap();
        assert_eq!(&data[..], b"nested");

        // Check exists
        assert!(storage.exists("a/b/c/file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_filesystem_storage_list() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Create some files and directories
        storage.write("file1.txt", Bytes::from("1")).await.unwrap();
        storage.write("file2.txt", Bytes::from("2")).await.unwrap();
        storage
            .write("subdir/file3.txt", Bytes::from("3"))
            .await
            .unwrap();

        // List root
        let entries = storage.list("").await.unwrap();
        assert_eq!(entries.len(), 3); // file1.txt, file2.txt, subdir

        let file_names: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(file_names.contains(&"file1.txt"));
        assert!(file_names.contains(&"file2.txt"));
        assert!(file_names.contains(&"subdir"));

        // Check is_dir flags
        let subdir_entry = entries.iter().find(|e| e.path == "subdir").unwrap();
        assert!(subdir_entry.is_dir);
    }

    #[tokio::test]
    async fn test_filesystem_storage_list_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Create nested structure
        storage
            .write("a/file1.txt", Bytes::from("1"))
            .await
            .unwrap();
        storage
            .write("a/b/file2.txt", Bytes::from("2"))
            .await
            .unwrap();
        storage
            .write("a/b/c/file3.txt", Bytes::from("3"))
            .await
            .unwrap();

        // List recursively
        let entries = storage.list_recursive("a").await.unwrap();

        // Should find all files and directories
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"file1.txt"));
        assert!(paths.contains(&"b"));
        assert!(paths.contains(&"b/file2.txt"));
        assert!(paths.contains(&"b/c"));
        assert!(paths.contains(&"b/c/file3.txt"));
    }

    #[tokio::test]
    async fn test_filesystem_storage_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        let result = storage.read("nonexistent.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));

        let result = storage.metadata("nonexistent.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_storage_url_parsing() {
        // Filesystem paths
        let url = StorageUrl::parse("photos").unwrap();
        assert!(matches!(url, StorageUrl::Filesystem { .. }));

        let url = StorageUrl::parse("/absolute/path").unwrap();
        assert!(matches!(url, StorageUrl::Filesystem { .. }));

        let url = StorageUrl::parse("file:///absolute/path").unwrap();
        assert!(matches!(url, StorageUrl::Filesystem { .. }));

        // S3 URLs
        let url = StorageUrl::parse("s3://bucket/prefix").unwrap();
        match url {
            StorageUrl::S3 {
                bucket,
                prefix,
                region,
                endpoint,
            } => {
                assert_eq!(bucket, "bucket");
                assert_eq!(prefix, "prefix");
                assert!(region.is_none());
                assert!(endpoint.is_none());
            }
            _ => panic!("Expected S3 URL"),
        }

        // S3 with options
        let url = StorageUrl::parse(
            "s3://my-bucket/path/prefix?region=us-west-2&endpoint=http://minio:9000",
        )
        .unwrap();
        match url {
            StorageUrl::S3 {
                bucket,
                prefix,
                region,
                endpoint,
            } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(prefix, "path/prefix");
                assert_eq!(region, Some("us-west-2".to_string()));
                assert_eq!(endpoint, Some("http://minio:9000".to_string()));
            }
            _ => panic!("Expected S3 URL"),
        }
    }

    #[tokio::test]
    async fn test_path_traversal_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Create a file inside the storage
        storage
            .write("safe.txt", Bytes::from("safe content"))
            .await
            .unwrap();

        // These paths should be normalized to stay within the base directory
        // "../" at the start should be stripped
        let normalized = FilesystemStorage::normalize_path("../outside.txt");
        assert_eq!(normalized, "outside.txt");

        // Multiple "../" should be handled
        let normalized = FilesystemStorage::normalize_path("../../../etc/passwd");
        assert_eq!(normalized, "etc/passwd");

        // "." should be stripped
        let normalized = FilesystemStorage::normalize_path("./subdir/./file.txt");
        assert_eq!(normalized, "subdir/file.txt");

        // Mixed traversal
        let normalized = FilesystemStorage::normalize_path("subdir/../other/file.txt");
        assert_eq!(normalized, "other/file.txt");

        // Absolute paths should be made relative
        let normalized = FilesystemStorage::normalize_path("/etc/passwd");
        assert_eq!(normalized, "etc/passwd");
    }

    #[tokio::test]
    async fn test_read_to_string() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Write text file
        storage
            .write("hello.txt", Bytes::from("Hello, World!"))
            .await
            .unwrap();

        // Read as string
        let content = storage.read_to_string("hello.txt").await.unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_read_to_string_invalid_utf8() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // Write invalid UTF-8
        storage
            .write("binary.bin", Bytes::from(vec![0xFF, 0xFE, 0x00, 0x01]))
            .await
            .unwrap();

        // Should fail with error
        let result = storage.read_to_string("binary.bin").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid UTF-8"));
    }

    #[tokio::test]
    async fn test_list_recursive_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FilesystemStorage::new(temp_dir.path());

        // List a non-existent directory should return NotFound
        let result = storage.list_recursive("nonexistent").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
