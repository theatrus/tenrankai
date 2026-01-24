//! Filesystem storage backend implementation.

use super::{
    ByteStream, ChunkedUpload, ObjectMetadata, Storage, StorageEntry, StorageError, UploadInfo,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

/// Local filesystem storage backend.
///
/// This implementation wraps standard filesystem operations with the Storage trait,
/// enabling consistent usage across different storage backends.
///
/// # Security
///
/// Path traversal attacks (e.g., `../../../etc/passwd`) are prevented by
/// normalizing paths and ensuring they stay within the base directory.
#[derive(Debug, Clone)]
pub struct FilesystemStorage {
    base_path: PathBuf,
}

impl FilesystemStorage {
    /// Create a new filesystem storage rooted at the given path.
    ///
    /// # Arguments
    /// * `base_path` - The root directory for this storage backend
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Resolve a relative path to an absolute path within the base directory.
    ///
    /// This method normalizes the path to prevent directory traversal attacks.
    /// Paths like `../../../etc/passwd` will be normalized to stay within
    /// the base directory.
    fn resolve_path(&self, path: &str) -> Result<PathBuf, StorageError> {
        // Normalize the path to prevent traversal attacks
        let normalized = Self::normalize_path(path);

        if normalized.is_empty() {
            return Ok(self.base_path.clone());
        }

        let full_path = self.base_path.join(&normalized);

        // Double-check that the resolved path is still under base_path
        // This handles edge cases and platform-specific path handling
        // If canonicalize fails (e.g., path doesn't exist yet), we rely on normalize_path
        if let (Ok(canonical_base), Ok(canonical_full)) = (
            std::fs::canonicalize(&self.base_path),
            std::fs::canonicalize(&full_path),
        ) && !canonical_full.starts_with(&canonical_base)
        {
            return Err(StorageError::PermissionDenied(format!(
                "Path escapes storage root: {}",
                path
            )));
        }

        Ok(full_path)
    }

    /// Normalize a path by removing `.` and `..` components.
    ///
    /// This prevents directory traversal attacks by ensuring paths
    /// cannot escape the base directory.
    pub(crate) fn normalize_path(path: &str) -> String {
        let mut components = Vec::new();

        for component in Path::new(path).components() {
            match component {
                Component::Normal(c) => {
                    components.push(c.to_string_lossy().to_string());
                }
                Component::ParentDir => {
                    // Pop the last component if there is one, otherwise ignore
                    // This prevents escaping the base directory
                    components.pop();
                }
                Component::CurDir => {
                    // Skip current directory references
                }
                Component::RootDir | Component::Prefix(_) => {
                    // Skip absolute path components - we always use relative paths
                }
            }
        }

        // Join with forward slashes for consistency across platforms
        components.join("/")
    }

    /// Get the base path of this storage.
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// Generate an ETag from file metadata (mtime + size).
    fn generate_etag(meta: &std::fs::Metadata) -> String {
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let size = meta.len();
        format!("{:x}-{:x}", mtime, size)
    }
}

#[async_trait]
impl Storage for FilesystemStorage {
    async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        let full_path = self.resolve_path(path)?;
        match tokio::fs::read(&full_path).await {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&full_path, data).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let full_path = self.resolve_path(path)?;
        Ok(tokio::fs::try_exists(&full_path).await.unwrap_or(false))
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        let full_path = self.resolve_path(path)?;
        match tokio::fs::metadata(&full_path).await {
            Ok(meta) => {
                // Generate ETag from mtime + size for conditional writes
                let etag = Self::generate_etag(&meta);
                Ok(ObjectMetadata {
                    size: meta.len(),
                    last_modified: meta.modified().ok(),
                    content_type: None, // Could use mime_guess here if needed
                    etag: Some(etag),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;
        match tokio::fs::remove_file(&full_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn delete_directory(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;
        match tokio::fs::remove_dir(&full_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Directory doesn't exist, that's fine
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                // Directory not empty, can't delete
                Err(StorageError::Other(format!(
                    "Directory not empty: {}",
                    path
                )))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_path = self.resolve_path(prefix)?;
        let mut entries = Vec::new();

        let mut read_dir = match tokio::fs::read_dir(&full_path).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::NotFound(prefix.to_string()));
            }
            Err(e) => return Err(StorageError::Io(e)),
        };

        while let Some(entry) = read_dir.next_entry().await? {
            let file_type = entry.file_type().await.ok();
            let metadata = entry.metadata().await.ok();

            entries.push(StorageEntry {
                path: entry.file_name().to_string_lossy().to_string(),
                is_dir: file_type.map(|t| t.is_dir()).unwrap_or(false),
                metadata: metadata.map(|m| ObjectMetadata {
                    size: m.len(),
                    last_modified: m.modified().ok(),
                    content_type: None,
                    etag: None,
                }),
            });
        }

        Ok(entries)
    }

    async fn list_recursive(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_path = self.resolve_path(prefix)?;

        // Check if directory exists first
        if !full_path.exists() {
            return Err(StorageError::NotFound(prefix.to_string()));
        }

        let base_path = full_path.clone();

        // Use spawn_blocking for walkdir which is sync
        let entries = tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();

            for entry in walkdir::WalkDir::new(&full_path)
                .min_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let rel_path = entry
                    .path()
                    .strip_prefix(&base_path)
                    .unwrap_or(entry.path());

                // Convert to forward slashes for consistency across platforms
                let rel_path_str = rel_path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");

                // Skip empty paths
                if rel_path_str.is_empty() {
                    continue;
                }

                let metadata = entry.metadata().ok();

                results.push(StorageEntry {
                    path: rel_path_str,
                    is_dir: entry.file_type().is_dir(),
                    metadata: metadata.map(|m| ObjectMetadata {
                        size: m.len(),
                        last_modified: m.modified().ok(),
                        content_type: None,
                        etag: None,
                    }),
                });
            }

            results
        })
        .await
        .map_err(|e| StorageError::Other(format!("Task join error: {}", e)))?;

        Ok(entries)
    }

    async fn read_stream(&self, path: &str) -> Result<ByteStream, StorageError> {
        let full_path = self.resolve_path(path)?;
        let file = match tokio::fs::File::open(&full_path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::NotFound(path.to_string()));
            }
            Err(e) => return Err(StorageError::Io(e)),
        };

        let stream = ReaderStream::new(file).map(|result| result.map_err(StorageError::Io));

        Ok(Box::pin(stream))
    }

    async fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Bytes, StorageError> {
        let full_path = self.resolve_path(path)?;
        let mut file = match tokio::fs::File::open(&full_path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::NotFound(path.to_string()));
            }
            Err(e) => return Err(StorageError::Io(e)),
        };

        // Seek to offset
        file.seek(std::io::SeekFrom::Start(offset)).await?;

        // Read up to length bytes
        let mut buffer = vec![0u8; length as usize];
        let bytes_read = file.read(&mut buffer).await?;
        buffer.truncate(bytes_read);

        Ok(Bytes::from(buffer))
    }

    async fn create_dir(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;
        tokio::fs::create_dir_all(&full_path).await?;
        Ok(())
    }

    fn storage_type(&self) -> &'static str {
        "filesystem"
    }

    fn root_path(&self) -> String {
        self.base_path.display().to_string()
    }

    async fn write_if_match(
        &self,
        path: &str,
        data: Bytes,
        expected_etag: Option<&str>,
    ) -> Result<String, StorageError> {
        let full_path = self.resolve_path(path)?;

        // Check current state
        let current_meta = tokio::fs::metadata(&full_path).await.ok();

        match (expected_etag, &current_meta) {
            // Expect file to not exist, but it does
            (None, Some(_)) => {
                return Err(StorageError::PreconditionFailed(format!(
                    "File already exists: {}",
                    path
                )));
            }
            // Expect specific ETag, but file doesn't exist
            (Some(expected), None) => {
                return Err(StorageError::PreconditionFailed(format!(
                    "File does not exist, expected etag {}: {}",
                    expected, path
                )));
            }
            // Expect specific ETag, check it matches
            (Some(expected), Some(meta)) => {
                let current_etag = Self::generate_etag(meta);
                if current_etag != expected {
                    return Err(StorageError::PreconditionFailed(format!(
                        "ETag mismatch: expected {}, got {}: {}",
                        expected, current_etag, path
                    )));
                }
            }
            // No expectation and file doesn't exist - OK to create
            (None, None) => {}
        }

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write the file
        tokio::fs::write(&full_path, &data).await?;

        // Get the new ETag after write
        let new_meta = tokio::fs::metadata(&full_path).await?;
        Ok(Self::generate_etag(&new_meta))
    }
}

// ============================================================================
// Chunked Upload Implementation
// ============================================================================

const UPLOADS_DIR: &str = "__uploads";

impl FilesystemStorage {
    /// Get the uploads directory path.
    fn uploads_dir(&self) -> PathBuf {
        self.base_path.join(UPLOADS_DIR)
    }

    /// Get the metadata file path for an upload.
    fn upload_meta_path(&self, upload_id: &str) -> PathBuf {
        self.uploads_dir().join(format!("{}.json", upload_id))
    }

    /// Get the data file path for an upload.
    fn upload_data_path(&self, upload_id: &str) -> PathBuf {
        self.uploads_dir().join(format!("{}.data", upload_id))
    }

    /// Generate a unique upload ID.
    fn generate_upload_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let random: u64 = rand_id();
        format!("{:x}{:016x}", timestamp, random)
    }

    /// Load upload info from disk.
    async fn load_upload_info(&self, upload_id: &str) -> Result<UploadInfo, StorageError> {
        let meta_path = self.upload_meta_path(upload_id);
        let content = tokio::fs::read_to_string(&meta_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::UploadNotFound(upload_id.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        serde_json::from_str(&content)
            .map_err(|e| StorageError::Other(format!("Failed to parse upload info: {}", e)))
    }

    /// Save upload info to disk.
    async fn save_upload_info(&self, info: &UploadInfo) -> Result<(), StorageError> {
        let meta_path = self.upload_meta_path(&info.upload_id);
        let content = serde_json::to_string_pretty(info)
            .map_err(|e| StorageError::Other(format!("Failed to serialize upload info: {}", e)))?;
        tokio::fs::write(&meta_path, content).await?;
        Ok(())
    }
}

/// Simple random ID generator (no external dependency).
fn rand_id() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

#[async_trait]
impl ChunkedUpload for FilesystemStorage {
    async fn create_upload(
        &self,
        path: &str,
        total_size: u64,
        metadata: Option<&str>,
    ) -> Result<String, StorageError> {
        // Ensure uploads directory exists
        let uploads_dir = self.uploads_dir();
        tokio::fs::create_dir_all(&uploads_dir).await?;

        // Generate unique upload ID
        let upload_id = Self::generate_upload_id();

        // Create upload info
        let info = UploadInfo {
            upload_id: upload_id.clone(),
            path: path.to_string(),
            total_size,
            current_offset: 0,
            metadata: metadata.map(String::from),
            created_at: SystemTime::now(),
        };

        // Save metadata
        self.save_upload_info(&info).await?;

        // Create empty data file
        let data_path = self.upload_data_path(&upload_id);
        tokio::fs::File::create(&data_path).await?;

        Ok(upload_id)
    }

    async fn append_chunk(
        &self,
        upload_id: &str,
        offset: u64,
        data: Bytes,
    ) -> Result<u64, StorageError> {
        // Load current state
        let mut info = self.load_upload_info(upload_id).await?;

        // Verify offset matches
        if offset != info.current_offset {
            return Err(StorageError::OffsetMismatch {
                expected: info.current_offset,
                actual: offset,
            });
        }

        // Append data to file
        let data_path = self.upload_data_path(upload_id);
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&data_path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    StorageError::UploadNotFound(upload_id.to_string())
                } else {
                    StorageError::Io(e)
                }
            })?;

        file.write_all(&data).await?;
        file.flush().await?;

        // Update offset
        info.current_offset += data.len() as u64;
        self.save_upload_info(&info).await?;

        Ok(info.current_offset)
    }

    async fn get_upload_info(&self, upload_id: &str) -> Result<UploadInfo, StorageError> {
        self.load_upload_info(upload_id).await
    }

    async fn complete_upload(&self, upload_id: &str) -> Result<(), StorageError> {
        let info = self.load_upload_info(upload_id).await?;

        // Verify upload is complete
        if info.current_offset != info.total_size {
            return Err(StorageError::Other(format!(
                "Upload incomplete: {} of {} bytes",
                info.current_offset, info.total_size
            )));
        }

        // Move data file to final destination
        let data_path = self.upload_data_path(upload_id);
        let final_path = self.resolve_path(&info.path)?;

        // Create parent directories if needed
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Move the file
        tokio::fs::rename(&data_path, &final_path).await?;

        // Remove metadata file
        let meta_path = self.upload_meta_path(upload_id);
        let _ = tokio::fs::remove_file(&meta_path).await;

        Ok(())
    }

    async fn terminate_upload(&self, upload_id: &str) -> Result<(), StorageError> {
        // Remove data file
        let data_path = self.upload_data_path(upload_id);
        let _ = tokio::fs::remove_file(&data_path).await;

        // Remove metadata file
        let meta_path = self.upload_meta_path(upload_id);
        let _ = tokio::fs::remove_file(&meta_path).await;

        Ok(())
    }

    async fn list_uploads(&self) -> Result<Vec<UploadInfo>, StorageError> {
        let uploads_dir = self.uploads_dir();

        // If uploads dir doesn't exist, return empty list
        if !uploads_dir.exists() {
            return Ok(Vec::new());
        }

        let mut uploads = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&uploads_dir).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(stem) = path.file_stem()
            {
                let upload_id = stem.to_string_lossy().to_string();
                if let Ok(info) = self.load_upload_info(&upload_id).await {
                    uploads.push(info);
                }
            }
        }

        Ok(uploads)
    }

    async fn cleanup_expired_uploads(&self, max_age: Duration) -> Result<usize, StorageError> {
        let uploads = self.list_uploads().await?;
        let now = SystemTime::now();
        let mut cleaned = 0;

        for upload in uploads {
            let age = now
                .duration_since(upload.created_at)
                .unwrap_or(Duration::ZERO);

            if age > max_age && self.terminate_upload(&upload.upload_id).await.is_ok() {
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }
}
