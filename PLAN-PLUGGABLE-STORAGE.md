# Pluggable Storage Abstraction Plan

## Status Overview

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Define Storage Traits | ✅ Complete |
| Phase 2.1 | Filesystem Storage | ✅ Complete |
| Phase 2.2 | S3 Storage | ✅ Complete |
| Phase 3 | Refactor LoadedImage | ⏳ Not Started |
| Phase 4 | Refactor Gallery Module | ⏳ Not Started |
| Phase 5 | Refactor Image Serving | ⏳ Not Started |
| Phase 6 | Refactor Cache Module | ⏳ Not Started |
| Phase 7 | URL-Based Configuration | ✅ Complete |
| Phase 7B | Static Files Storage | ✅ Complete |
| Phase 7C | Template Storage | ✅ Complete |
| Phase 7D | Posts Storage | ✅ Complete |
| Phase 8 | Signed URL Redirects | ✅ Complete (static files) |
| Phase 9 | Migration Strategy | 🔄 In Progress |

## Overview

This plan outlines how to refactor Tenrankai's gallery, image processing, and cache code to support pluggable storage backends, enabling both local filesystem and S3 storage for galleries and caches.

## Current State Analysis

### File I/O Patterns Found

| Category | Sync Operations | Async Operations |
|----------|-----------------|------------------|
| Image Loading | 8 `std::fs::File::open()` | - |
| Image Saving | 5 `std::fs::File::create()`, 7 `std::fs::write()` | 5 `tokio::fs::write()` |
| Directory Scanning | 3 `WalkDir::new()` | 4 `tokio::fs::read_dir()` |
| Metadata | 6 `std::fs::read_to_string()` | 6 `tokio::fs::read_to_string()`, 8 `tokio::fs::metadata()` |
| Cache Validation | `path.exists()` scattered | `tokio::fs::remove_file()` |

### Library Streaming Capability Analysis

| Library | Current API | Supports Streaming | Needs Seeking | Adaptation |
|---------|-------------|-------------------|---------------|------------|
| **rexif** | `parse_file(path)` | ✅ YES via `parse_buffer(&[u8])` | ❌ NO | Easy - already using buffer parsing for AVIF |
| **image crate** | `image::open(path)` | ✅ YES via `ImageReader<R: Read+Seek>` | ✅ YES | Moderate - needs seek wrapper for S3 |
| **libavif-rs** | bytes-based | ✅ YES via `avifDecoderReadMemory` | ✅ YES (box structure) | Easy - already in-memory |
| **JPEG ICC** | `File::open(path)` | ✅ YES via `Read` trait | ❌ NO (linear scan) | Very easy |
| **PNG ICC** | `File::open(path)` | ✅ YES via `Read` trait | ❌ NO (sequential chunks) | Very easy |
| **WebP** | in-memory only | N/A (output format) | N/A | N/A |

**Key Finding**: Most operations can work with bytes/readers. The critical challenge is that **image decoding requires `Read + Seek`** due to format structure (JPEG markers, PNG chunks, AVIF boxes).

### Key Challenges

1. **Seeking Requirement**: Image formats require random access. S3 doesn't natively support seeking, but can use HTTP Range requests.

2. **Sync/Async Split**: Image loading (`LoadedImage::load()`) is synchronous, runs in `spawn_blocking()`. S3 operations are inherently async.

3. **WalkDir Dependency**: Recursive directory scanning uses sync `WalkDir` which is filesystem-specific.

4. **Modification Time Comparisons**: Cache invalidation compares filesystem `mtime`. S3 uses `LastModified` differently.

### Streaming Strategy

For S3 support, we have three approaches:

1. **Full Download** (Simple): Download entire file to memory, pass to existing byte-based APIs
   - Pros: Simplest implementation, works with all libraries
   - Cons: Memory usage for large files
   - Best for: Most image processing (files are typically 1-20MB)

2. **Range-Based Seeking** (Advanced): Implement `Read + Seek` using HTTP Range requests
   - Pros: Memory efficient, only downloads needed bytes
   - Cons: Complex implementation, many small requests
   - Best for: Large files, metadata-only extraction

3. **Hybrid** (Recommended): Full download for processing, streaming for serving cached results
   - Processing: Download source → process → upload to cache
   - Serving: Stream directly from cache storage
   - Best balance of simplicity and performance

---

## Phase 1: Define Storage Traits ✅

### 1.1 Core Storage Trait

```rust
// src/storage/mod.rs

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::time::SystemTime;

/// Metadata about a stored object
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub size: u64,
    pub last_modified: Option<SystemTime>,
    pub content_type: Option<String>,
    pub etag: Option<String>,
}

/// A directory entry from listing operations
#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub path: String,
    pub is_dir: bool,
    pub metadata: Option<ObjectMetadata>,
}

/// Error type for storage operations
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Object not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Other(String),
}

/// Async storage operations trait
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Read entire object into memory
    async fn read(&self, path: &str) -> Result<Bytes, StorageError>;

    /// Write data to storage
    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError>;

    /// Check if object exists
    async fn exists(&self, path: &str) -> Result<bool, StorageError>;

    /// Get object metadata without reading content
    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError>;

    /// Delete an object
    async fn delete(&self, path: &str) -> Result<(), StorageError>;

    /// List objects with a prefix (non-recursive by default)
    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError>;

    /// List objects recursively with a prefix
    async fn list_recursive(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError>;

    /// Create a streaming reader for large objects
    async fn read_stream(&self, path: &str)
        -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>, StorageError>;

    /// Create directory (no-op for S3, creates dir for filesystem)
    async fn create_dir(&self, path: &str) -> Result<(), StorageError>;

    /// Get the storage type name (for logging)
    fn storage_type(&self) -> &'static str;
}
```

### 1.2 Sync Storage Wrapper

For operations that must run in `spawn_blocking()` context:

```rust
// src/storage/blocking.rs

/// Blocking wrapper for storage operations
/// Used in spawn_blocking contexts like image loading
pub struct BlockingStorage {
    storage: Arc<dyn Storage>,
    runtime: tokio::runtime::Handle,
}

impl BlockingStorage {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    /// Read synchronously by blocking on async operation
    pub fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        self.runtime.block_on(self.storage.read(path))
    }

    /// Write synchronously
    pub fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        self.runtime.block_on(self.storage.write(path, data))
    }

    /// Check existence synchronously
    pub fn exists(&self, path: &str) -> bool {
        self.runtime.block_on(self.storage.exists(path)).unwrap_or(false)
    }

    /// Get metadata synchronously
    pub fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        self.runtime.block_on(self.storage.metadata(path))
    }
}
```

### 1.3 Implementation Notes

**Implemented in commits `cdb27de` and `9825199`:**

- Created `src/storage/mod.rs` with `Storage` trait (13 async methods)
- Created `src/storage/error.rs` with `StorageError` enum
- Created `src/storage/filesystem.rs` with `FilesystemStorage`
- Added path traversal prevention via `normalize_path()` function
- Added `BlockingStorage` wrapper for `spawn_blocking` contexts
- Added `read_to_string()` helper method to trait
- 15 unit tests for filesystem operations

---

## Phase 2: Implement Storage Backends ✅

### 2.1 Filesystem Storage

```rust
// src/storage/filesystem.rs

pub struct FilesystemStorage {
    base_path: PathBuf,
}

impl FilesystemStorage {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self { base_path: base_path.into() }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path.trim_start_matches('/'))
    }
}

#[async_trait]
impl Storage for FilesystemStorage {
    async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        let full_path = self.resolve_path(path);
        let data = tokio::fs::read(&full_path).await?;
        Ok(Bytes::from(data))
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full_path, data).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let full_path = self.resolve_path(path);
        Ok(tokio::fs::try_exists(&full_path).await.unwrap_or(false))
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        let full_path = self.resolve_path(path);
        let meta = tokio::fs::metadata(&full_path).await?;
        Ok(ObjectMetadata {
            size: meta.len(),
            last_modified: meta.modified().ok(),
            content_type: None,
            etag: None,
        })
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path);
        tokio::fs::remove_file(&full_path).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_path = self.resolve_path(prefix);
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&full_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await.ok();
            entries.push(StorageEntry {
                path: entry.file_name().to_string_lossy().to_string(),
                is_dir: entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false),
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
        let full_path = self.resolve_path(prefix);
        let entries = tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            for entry in walkdir::WalkDir::new(&full_path)
                .min_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let rel_path = entry.path().strip_prefix(&full_path)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();
                let metadata = entry.metadata().ok();
                results.push(StorageEntry {
                    path: rel_path,
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
        }).await.map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(entries)
    }

    async fn read_stream(&self, path: &str)
        -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>, StorageError>
    {
        let full_path = self.resolve_path(path);
        let file = tokio::fs::File::open(&full_path).await?;
        let stream = tokio_util::io::ReaderStream::new(file)
            .map(|r| r.map(Bytes::from).map_err(StorageError::from));
        Ok(Box::pin(stream))
    }

    async fn create_dir(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path);
        tokio::fs::create_dir_all(&full_path).await?;
        Ok(())
    }

    fn storage_type(&self) -> &'static str {
        "filesystem"
    }
}
```

### 2.2 S3 Storage

```rust
// src/storage/s3.rs

use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;

pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Storage {
    pub async fn new(bucket: String, prefix: String, region: Option<String>) -> Result<Self, StorageError> {
        let config = aws_config::from_env();
        let config = if let Some(region) = region {
            config.region(aws_sdk_s3::config::Region::new(region))
        } else {
            config
        };
        let config = config.load().await;
        let client = Client::new(&config);

        Ok(Self { client, bucket, prefix })
    }

    fn full_key(&self, path: &str) -> String {
        format!("{}/{}", self.prefix.trim_end_matches('/'), path.trim_start_matches('/'))
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        let key = self.full_key(path);
        let resp = self.client.get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        let data = resp.body.collect().await
            .map_err(|e| StorageError::Other(e.to_string()))?;
        Ok(data.into_bytes())
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        let key = self.full_key(path);
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let key = self.full_key(path);
        match self.client.head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("NotFound") || e.to_string().contains("404") {
                    Ok(false)
                } else {
                    Err(StorageError::Other(e.to_string()))
                }
            }
        }
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        let key = self.full_key(path);
        let resp = self.client.head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(ObjectMetadata {
            size: resp.content_length().unwrap_or(0) as u64,
            last_modified: resp.last_modified().map(|dt| {
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.secs() as u64)
            }),
            content_type: resp.content_type().map(|s| s.to_string()),
            etag: resp.e_tag().map(|s| s.to_string()),
        })
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let key = self.full_key(path);
        self.client.delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_prefix = self.full_key(prefix);
        let resp = self.client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&full_prefix)
            .delimiter("/")
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        let mut entries = Vec::new();

        // Common prefixes are "directories"
        if let Some(prefixes) = resp.common_prefixes() {
            for prefix in prefixes {
                if let Some(p) = prefix.prefix() {
                    let name = p.strip_prefix(&full_prefix)
                        .unwrap_or(p)
                        .trim_end_matches('/');
                    entries.push(StorageEntry {
                        path: name.to_string(),
                        is_dir: true,
                        metadata: None,
                    });
                }
            }
        }

        // Objects are "files"
        if let Some(objects) = resp.contents() {
            for obj in objects {
                if let Some(key) = obj.key() {
                    let name = key.strip_prefix(&full_prefix).unwrap_or(key);
                    if !name.is_empty() && !name.ends_with('/') {
                        entries.push(StorageEntry {
                            path: name.to_string(),
                            is_dir: false,
                            metadata: Some(ObjectMetadata {
                                size: obj.size().unwrap_or(0) as u64,
                                last_modified: obj.last_modified().map(|dt| {
                                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.secs() as u64)
                                }),
                                content_type: None,
                                etag: obj.e_tag().map(|s| s.to_string()),
                            }),
                        });
                    }
                }
            }
        }

        Ok(entries)
    }

    async fn list_recursive(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let full_prefix = self.full_key(prefix);
        let mut entries = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);

            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await
                .map_err(|e| StorageError::Other(e.to_string()))?;

            if let Some(objects) = resp.contents() {
                for obj in objects {
                    if let Some(key) = obj.key() {
                        let rel_path = key.strip_prefix(&full_prefix).unwrap_or(key);
                        if !rel_path.is_empty() {
                            entries.push(StorageEntry {
                                path: rel_path.to_string(),
                                is_dir: rel_path.ends_with('/'),
                                metadata: Some(ObjectMetadata {
                                    size: obj.size().unwrap_or(0) as u64,
                                    last_modified: obj.last_modified().map(|dt| {
                                        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.secs() as u64)
                                    }),
                                    content_type: None,
                                    etag: obj.e_tag().map(|s| s.to_string()),
                                }),
                            });
                        }
                    }
                }
            }

            if resp.is_truncated() == Some(true) {
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(entries)
    }

    async fn read_stream(&self, path: &str)
        -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>, StorageError>
    {
        let key = self.full_key(path);
        let resp = self.client.get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        let stream = resp.body
            .map(|r| r.map_err(|e| StorageError::Other(e.to_string())));
        Ok(Box::pin(stream))
    }

    async fn create_dir(&self, _path: &str) -> Result<(), StorageError> {
        // S3 doesn't have real directories - no-op
        Ok(())
    }

    fn storage_type(&self) -> &'static str {
        "s3"
    }
}
```

### 2.3 Implementation Notes

**Implemented in commits `9825199` and `d6165f2`:**

- Created `src/storage/s3.rs` with full S3Storage implementation (~500 lines)
- Supports custom endpoints for MinIO/LocalStack
- Presigned URL generation for redirect-based serving
- Handles AWS SDK v1 API patterns (BehaviorVersion, DateTime conversion)
- Created `tests/s3_storage_integration.rs` with 8 comprehensive tests
- Tests auto-skip if MinIO is not available (`minio_available()` check)

---

## Phase 3: Refactor LoadedImage

The critical change is making `LoadedImage` work with bytes and readers instead of paths.

### 3.1 New LoadedImage API

```rust
// src/gallery/image_processing/types.rs

use std::io::{Read, Seek, Cursor};

impl LoadedImage {
    /// Load an image from any seekable reader
    /// This is the most flexible method - works with files, S3 streams, or memory
    pub fn from_reader<R: Read + Seek>(
        mut reader: R,
        source_path: PathBuf,
        format_hint: Option<ImageFormat>,
    ) -> Result<Self, GalleryError> {
        // For formats that need the full bytes (ICC extraction, AVIF parsing),
        // we read into memory. The image crate's decoder handles streaming internally.
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Self::from_bytes(&data, source_path, format_hint)
    }

    /// Load an image from raw bytes (in-memory)
    /// Use this when you already have the bytes (e.g., from S3 download)
    pub fn from_bytes(
        data: &[u8],
        source_path: PathBuf,
        format_hint: Option<ImageFormat>,
    ) -> Result<Self, GalleryError> {
        // Detect format from bytes
        let cursor = Cursor::new(data);
        let reader = image::ImageReader::new(cursor).with_guessed_format()?;
        let detected_format = reader.format().or(format_hint);

        // Extract ICC profile from bytes based on format
        // These extractors work with slices - no seeking needed
        let icc_profile = match detected_format {
            Some(ImageFormat::Jpeg) => {
                formats::jpeg::extract_icc_profile_from_bytes(data)
            }
            Some(ImageFormat::Png) => {
                formats::png::extract_icc_profile_from_bytes(data)
            }
            #[cfg(feature = "avif")]
            Some(ImageFormat::Avif) => {
                formats::avif::extract_icc_profile_from_bytes(data)
            }
            _ => None,
        };

        // Load image from bytes - Cursor<&[u8]> implements Read + Seek
        #[cfg(feature = "avif")]
        let (image, avif_info) = if detected_format == Some(ImageFormat::Avif) {
            // AVIF uses libavif which needs full bytes anyway
            match formats::avif::read_avif_info_from_bytes(data) {
                Ok((img, info)) => (img, Some(info)),
                Err(e) => {
                    tracing::debug!("AVIF custom reader failed: {}, falling back", e);
                    let cursor = Cursor::new(data);
                    let reader = image::ImageReader::new(cursor).with_guessed_format()?;
                    (reader.decode()?, None)
                }
            }
        } else {
            let cursor = Cursor::new(data);
            let reader = image::ImageReader::new(cursor).with_guessed_format()?;
            (reader.decode()?, None)
        };

        #[cfg(not(feature = "avif"))]
        let image = {
            let cursor = Cursor::new(data);
            let reader = image::ImageReader::new(cursor).with_guessed_format()?;
            reader.decode()?
        };

        Ok(Self {
            image,
            icc_profile,
            #[cfg(feature = "avif")]
            avif_info,
            format: detected_format,
            source_path,
        })
    }

    /// Encode image to bytes in the specified format
    /// Returns encoded bytes - caller decides where to write them
    pub fn encode_as(
        &self,
        format: OutputFormat,
        jpeg_quality: u8,
        webp_quality: f32,
    ) -> Result<Vec<u8>, GalleryError> {
        match format {
            OutputFormat::Jpeg => {
                formats::jpeg::encode_to_bytes(&self.image, jpeg_quality, self.icc_profile.as_deref())
            }
            OutputFormat::WebP => {
                formats::webp::encode_to_bytes(&self.image, webp_quality, self.icc_profile.as_deref())
            }
            OutputFormat::Png => {
                formats::png::encode_to_bytes(&self.image)
            }
            #[cfg(feature = "avif")]
            OutputFormat::Avif => {
                formats::avif::encode_to_bytes(&self.image, 85, 6, self.avif_info.as_ref(), self.icc_profile.as_deref())
            }
        }
    }

    /// Write encoded image to any writer (convenience wrapper)
    pub fn write_as<W: std::io::Write>(
        &self,
        writer: &mut W,
        format: OutputFormat,
        jpeg_quality: u8,
        webp_quality: f32,
    ) -> Result<(), GalleryError> {
        let bytes = self.encode_as(format, jpeg_quality, webp_quality)?;
        writer.write_all(&bytes)?;
        Ok(())
    }
}
```

### 3.2 Existing Crates for Seekable Remote Readers

There are several well-maintained crates that solve the "seekable reader over remote storage" problem. We should use these instead of rolling our own:

#### Option A: `object_store` (Recommended - Has Everything We Need)

The [object_store](https://docs.rs/object_store/latest/object_store/) crate from Apache Arrow provides a unified interface for S3, GCS, Azure, and local files. It includes a **buffered module** with `BufReader` that implements `AsyncRead + AsyncBufRead + AsyncSeek`:

```rust
use object_store::{ObjectStore, path::Path};
use object_store::aws::AmazonS3Builder;
use object_store::buffered::BufReader;

// Create S3 client
let s3 = AmazonS3Builder::from_env()
    .with_bucket_name("my-bucket")
    .build()?;

// Basic operations - stateless range APIs
let data = s3.get(&Path::from("photos/image.jpg")).await?.bytes().await?;
let range_data = s3.get_range(&Path::from("photos/image.jpg"), 0..1024).await?;
let stream = s3.get(&Path::from("photos/image.jpg")).await?.into_stream();

// BufReader for AsyncRead + AsyncSeek (with internal caching!)
let meta = s3.head(&Path::from("photos/image.jpg")).await?;
let reader = BufReader::new(Arc::new(s3), &meta);
// or with custom buffer size:
let reader = BufReader::with_capacity(Arc::new(s3), &meta, 1024 * 1024); // 1MB buffer

// Now you have AsyncRead + AsyncSeek!
reader.seek(SeekFrom::Start(1000)).await?;
reader.read(&mut buffer).await?;
```

**How BufReader Works:**
- Maintains an internal buffer of configurable size
- Uses `ObjectStore::get_range()` to populate the buffer when depleted
- Buffer clears on seek operations
- Implements `AsyncRead`, `AsyncBufRead`, and `AsyncSeek` traits

**Performance Note:** The docs caution that BufReader "will typically be outperformed by native ObjectStore methods" due to high first-byte latencies (100-200ms) on cloud stores. For sequential reads, prefer `get()` or `get_range()` directly.

**Pros:** Well-maintained (Apache project), supports all major cloud providers, has both stateless APIs AND buffered Read+Seek
**Cons:** Buffer clears on seek (may cause re-fetches), async traits require adaptation for sync image loading

#### Option B: `s3reader` (For Read + Seek on S3)

The [s3reader](https://lib.rs/crates/s3reader) crate provides `Read + Seek + BufRead` traits for S3 objects:

```rust
use s3reader::{S3ObjectUri, S3Reader};
use std::io::{BufRead, BufReader, Seek, SeekFrom};

let uri = S3ObjectUri::new(Some("us-east-1"), "bucket", "photos/image.jpg")?;
let mut reader = S3Reader::open(uri)?;

// Seek to any position
reader.seek(SeekFrom::Start(1000))?;

// Read bytes
let mut buffer = [0u8; 100];
reader.read(&mut buffer)?;

// Works with BufReader for line-by-line reading
let buf_reader = BufReader::new(reader);
```

**Pros:** Exactly what we need for image loading, implements standard traits
**Cons:** S3-specific (no local filesystem), uses blocking tokio runtime internally

#### Option C: `async_http_range_reader` (For HTTP with Prefetching)

The [async_http_range_reader](https://docs.rs/async_http_range_reader) crate provides `AsyncRead + AsyncSeek` with intelligent prefetching:

```rust
use async_http_range_reader::AsyncHttpRangeReader;

let mut reader = AsyncHttpRangeReader::new(client, url).await?;

// Prefetch bytes to avoid many small requests
reader.prefetch(0..1_000_000).await;

// Now reads come from cache
reader.seek(SeekFrom::Start(500)).await?;
reader.read(&mut buffer).await?;
```

**Pros:** Prefetching reduces request count, sparse memory map for large files
**Cons:** HTTP-specific (not S3 native), async traits require different handling

#### Option D: `seekable_reader` (Generic Wrapper)

The [seekable_reader](https://github.com/UgnilJoZ/seekable_reader) crate wraps any `Read` to add `Seek` by caching previously-read bytes:

```rust
use seekable_reader::SeekableReader;

// Wrap any reader with a cache size (in bytes)
let mut reader = SeekableReader::new(stream, 1024 * 1024); // 1MB cache

// Can now seek backwards within cached range
reader.read(&mut buffer)?;
reader.seek(SeekFrom::Start(0))?;  // Works if within cache
```

**Pros:** Works with any `Read`, simple API
**Cons:** Can only seek within cached data, not truly random access

### Recommended Approach

**Use `object_store` as our single dependency** - it provides everything we need:

| Need | object_store Solution |
|------|----------------------|
| Unified S3/local/GCS/Azure | Core `ObjectStore` trait |
| Full file download | `store.get(path).bytes()` |
| Partial range reads | `store.get_range(path, range)` |
| Streaming responses | `store.get(path).into_stream()` |
| `AsyncRead + AsyncSeek` | `BufReader::new(store, meta)` |
| Directory listing | `store.list(prefix)` |
| File metadata | `store.head(path)` |

**Implementation Strategy:**

```rust
use object_store::{ObjectStore, ObjectMeta, path::Path};
use object_store::buffered::BufReader;
use std::sync::Arc;

pub struct ObjectStoreBackend {
    store: Arc<dyn ObjectStore>,
}

impl ObjectStoreBackend {
    // Full file read (most common for image processing)
    pub async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        self.store.get(&Path::from(path)).await?.bytes().await
            .map_err(StorageError::from)
    }

    // Streaming read for serving to clients
    pub async fn read_stream(&self, path: &str) -> Result<BoxStream<Bytes>, StorageError> {
        let stream = self.store.get(&Path::from(path)).await?.into_stream();
        Ok(Box::pin(stream.map_err(StorageError::from)))
    }

    // AsyncRead + AsyncSeek for libraries that need it
    pub async fn open_reader(&self, path: &str) -> Result<BufReader, StorageError> {
        let meta = self.store.head(&Path::from(path)).await?;
        Ok(BufReader::with_capacity(self.store.clone(), &meta, 2 * 1024 * 1024)) // 2MB buffer
    }

    // Partial read (e.g., first 64KB for EXIF extraction)
    pub async fn read_range(&self, path: &str, range: Range<usize>) -> Result<Bytes, StorageError> {
        self.store.get_range(&Path::from(path), range).await
            .map_err(StorageError::from)
    }
}
```

**Bridging Async to Sync for Image Loading:**

Since `LoadedImage` and the `image` crate need sync `Read + Seek`, we bridge:

```rust
use tokio::runtime::Handle;

/// Sync wrapper around object_store's async BufReader
pub struct SyncObjectReader {
    inner: BufReader,
    handle: Handle,
}

impl std::io::Read for SyncObjectReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use tokio::io::AsyncReadExt;
        self.handle.block_on(async {
            self.inner.read(buf).await
        }).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

impl std::io::Seek for SyncObjectReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        use tokio::io::AsyncSeekExt;
        self.handle.block_on(async {
            self.inner.seek(pos).await
        }).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

// Usage in spawn_blocking context:
let reader = storage.open_reader("photos/image.jpg").await?;
let sync_reader = SyncObjectReader::new(reader);
let loaded = LoadedImage::from_reader(sync_reader, path, None)?;
```

**Intelligent Sync Adapter with Speculative Caching:**

The `object_store` `BufReader` clears its buffer on seek, which is inefficient for image formats that seek back and forth. We should build a smarter adapter:

```rust
// src/storage/cached_reader.rs

use bytes::Bytes;
use object_store::{ObjectStore, path::Path};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Configuration for the cached reader
#[derive(Clone)]
pub struct CachedReaderConfig {
    /// Minimum fetch size (fetches are rounded up to this)
    pub chunk_size: usize,
    /// Speculative prefetch: fetch this many bytes ahead of current read
    pub prefetch_ahead: usize,
    /// Maximum total cache size before eviction
    pub max_cache_size: usize,
}

impl Default for CachedReaderConfig {
    fn default() -> Self {
        Self {
            chunk_size: 256 * 1024,      // 256KB minimum fetch
            prefetch_ahead: 512 * 1024,   // 512KB prefetch ahead
            max_cache_size: 8 * 1024 * 1024, // 8MB max cache
        }
    }
}

/// A sync Read+Seek adapter with intelligent range caching
///
/// Features:
/// - Sparse cache: keeps non-contiguous ranges in memory
/// - Coalescing: small reads are expanded to chunk_size to reduce requests
/// - Prefetching: speculatively fetches ahead for sequential reads
/// - Seek-friendly: cached data survives seeks (unlike object_store BufReader)
pub struct CachedObjectReader {
    store: Arc<dyn ObjectStore>,
    path: Path,
    size: u64,
    position: u64,

    // Sparse cache: maps start_offset -> cached bytes
    // Using BTreeMap for efficient range queries
    cache: BTreeMap<u64, Bytes>,
    cache_size: usize,

    config: CachedReaderConfig,
    handle: Handle,
}

impl CachedObjectReader {
    pub async fn open(
        store: Arc<dyn ObjectStore>,
        path: &str,
        config: CachedReaderConfig,
    ) -> Result<Self, object_store::Error> {
        let path = Path::from(path);
        let meta = store.head(&path).await?;

        Ok(Self {
            store,
            path,
            size: meta.size as u64,
            position: 0,
            cache: BTreeMap::new(),
            cache_size: 0,
            config,
            handle: Handle::current(),
        })
    }

    /// Open with speculative prefetch of first N bytes (for headers/EXIF)
    pub async fn open_with_prefetch(
        store: Arc<dyn ObjectStore>,
        path: &str,
        config: CachedReaderConfig,
        prefetch_bytes: usize,
    ) -> Result<Self, object_store::Error> {
        let mut reader = Self::open(store.clone(), path, config).await?;

        // Prefetch the beginning (where headers and EXIF live)
        if prefetch_bytes > 0 {
            let end = (prefetch_bytes as u64).min(reader.size);
            reader.ensure_cached(0..end).await?;
        }

        Ok(reader)
    }

    /// Check if a range is fully cached
    fn is_cached(&self, range: Range<u64>) -> bool {
        for (&start, bytes) in self.cache.range(..=range.start).rev().take(1) {
            let end = start + bytes.len() as u64;
            if start <= range.start && end >= range.end {
                return true;
            }
        }
        false
    }

    /// Read from cache if available
    fn read_from_cache(&self, range: Range<u64>) -> Option<Bytes> {
        for (&start, bytes) in self.cache.range(..=range.start).rev().take(1) {
            let end = start + bytes.len() as u64;
            if start <= range.start && end >= range.end {
                let offset = (range.start - start) as usize;
                let len = (range.end - range.start) as usize;
                return Some(bytes.slice(offset..offset + len));
            }
        }
        None
    }

    /// Ensure a range is cached, fetching if necessary
    async fn ensure_cached(&mut self, range: Range<u64>) -> Result<(), object_store::Error> {
        if self.is_cached(range.clone()) {
            return Ok(());
        }

        // Expand to chunk boundaries for efficiency
        let fetch_start = (range.start / self.config.chunk_size as u64)
            * self.config.chunk_size as u64;
        let fetch_end = ((range.end + self.config.chunk_size as u64 - 1)
            / self.config.chunk_size as u64)
            * self.config.chunk_size as u64;
        let fetch_end = fetch_end.min(self.size);

        // Add prefetch ahead for sequential access patterns
        let prefetch_end = (fetch_end + self.config.prefetch_ahead as u64).min(self.size);

        // Evict if needed
        let fetch_size = (prefetch_end - fetch_start) as usize;
        while self.cache_size + fetch_size > self.config.max_cache_size && !self.cache.is_empty() {
            // Evict oldest (first) entry
            if let Some((&key, bytes)) = self.cache.iter().next() {
                self.cache_size -= bytes.len();
                self.cache.remove(&key);
            }
        }

        // Fetch the range
        let data = self.store
            .get_range(&self.path, fetch_start as usize..prefetch_end as usize)
            .await?;

        self.cache_size += data.len();
        self.cache.insert(fetch_start, data);

        Ok(())
    }

    /// Sync version of ensure_cached for use in Read/Seek impls
    fn ensure_cached_sync(&mut self, range: Range<u64>) -> std::io::Result<()> {
        self.handle.block_on(self.ensure_cached(range))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

impl Read for CachedObjectReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.size {
            return Ok(0); // EOF
        }

        let read_end = (self.position + buf.len() as u64).min(self.size);
        let range = self.position..read_end;

        // Ensure data is cached (with prefetching)
        self.ensure_cached_sync(range.clone())?;

        // Read from cache
        if let Some(data) = self.read_from_cache(range) {
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            self.position += len as u64;
            Ok(len)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Cache miss after ensure_cached",
            ))
        }
    }
}

impl Seek for CachedObjectReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.position = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) => (self.size as i64 + n) as u64,
            SeekFrom::Current(n) => (self.position as i64 + n) as u64,
        };
        // Note: we do NOT clear cache here - that's the key difference from object_store BufReader
        Ok(self.position)
    }
}

// Also implement BufRead for efficient line-by-line reading
impl std::io::BufRead for CachedObjectReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        // This is tricky with our sparse cache design
        // For simplicity, ensure one chunk is cached and return a slice
        let chunk_end = (self.position + self.config.chunk_size as u64).min(self.size);
        self.ensure_cached_sync(self.position..chunk_end)?;

        // We can't easily return a reference to cache data due to borrowing rules
        // BufRead is optional - Read + Seek is sufficient for image loading
        unimplemented!("Use Read trait instead")
    }

    fn consume(&mut self, amt: usize) {
        self.position += amt as u64;
    }
}
```

**Usage Example:**

```rust
// For image processing with intelligent caching
let config = CachedReaderConfig {
    chunk_size: 256 * 1024,       // 256KB chunks
    prefetch_ahead: 1024 * 1024,  // 1MB prefetch for sequential image data
    max_cache_size: 16 * 1024 * 1024, // 16MB max cache
};

// Prefetch first 64KB on open (catches EXIF in most formats)
let reader = CachedObjectReader::open_with_prefetch(
    store.clone(),
    "photos/large_image.jpg",
    config,
    64 * 1024,  // 64KB prefetch
).await?;

// Now use in spawn_blocking for sync image loading
tokio::task::spawn_blocking(move || {
    let loaded = LoadedImage::from_reader(reader, path, None)?;
    // Reader survives seeks - no redundant fetches!
}).await?
```

**Key Advantages Over object_store BufReader:**

| Feature | object_store BufReader | CachedObjectReader |
|---------|----------------------|-------------------|
| Cache on seek | ❌ Clears buffer | ✅ Preserves cache |
| Prefetching | ❌ None | ✅ Configurable prefetch ahead |
| Initial prefetch | ❌ None | ✅ Can prefetch headers on open |
| Chunk coalescing | ✅ Yes | ✅ Yes (configurable chunk_size) |
| Sparse caching | ❌ Single buffer | ✅ Multiple non-contiguous ranges |
| Sync Read+Seek | ❌ Async only | ✅ Native sync traits |
| Eviction policy | N/A (single buffer) | ✅ LRU-like (oldest first) |

**When to use each:**

| Scenario | Recommendation |
|----------|---------------|
| Small files (<1MB) | Just use `store.get().bytes()` - download whole thing |
| Large files, sequential read | `object_store::BufReader` is fine |
| Image processing (seeks around) | `CachedObjectReader` - preserves cache across seeks |
| EXIF-only extraction | `store.get_range(0..65536)` - just fetch what you need |

### 3.3 Format Module Updates

Each format module needs `*_from_bytes` variants:

```rust
// jpeg.rs additions
pub fn extract_icc_profile_from_bytes(data: &[u8]) -> Option<Vec<u8>> { ... }
pub fn encode_to_writer<W: Write>(image: &DynamicImage, writer: W, ...) -> Result<()> { ... }

// png.rs additions
pub fn extract_icc_profile_from_bytes(data: &[u8]) -> Option<Vec<u8>> { ... }
pub fn encode_to_writer<W: Write>(image: &DynamicImage, writer: W) -> Result<()> { ... }

// webp.rs additions
pub fn encode_to_bytes(image: &DynamicImage, quality: f32, icc: Option<&[u8]>) -> Result<Vec<u8>> { ... }

// avif.rs additions
pub fn extract_icc_profile_from_bytes(data: &[u8]) -> Option<Vec<u8>> { ... }
pub fn read_avif_info_from_bytes(data: &[u8]) -> Result<(DynamicImage, AvifImageInfo)> { ... }
pub fn encode_to_bytes(...) -> Result<Vec<u8>> { ... }
```

---

## Phase 4: Refactor Gallery Module

### 4.1 Gallery Configuration Update (URL-Based)

```toml
# config.toml - Simple URL-based configuration

[[galleries]]
name = "main"
url_prefix = "/gallery"

# Storage URLs (filesystem paths or s3:// URLs)
source_directory = "photos"                    # Local filesystem
cache_directory = "cache/main"                 # Local filesystem

# Or with S3:
# source_directory = "s3://my-photos/galleries/main"
# cache_directory = "s3://my-cache/galleries/main?region=us-west-2"

# Mixed example: local source, S3 cache for CDN
# source_directory = "/mnt/photos"
# cache_directory = "s3://cdn-cache/main"
```

### 4.2 Gallery Struct Changes

```rust
// src/gallery/types.rs

pub struct Gallery {
    pub name: String,
    pub url_prefix: String,
    pub source_storage: Arc<dyn Storage>,
    pub cache_storage: Arc<dyn Storage>,
    pub config: GalleryConfig,
    pub metadata_cache: Arc<RwLock<HashMap<String, ImageMetadata>>>,
    // ... other fields
}

impl Gallery {
    pub async fn new(config: GalleryConfig) -> Result<Self, GalleryError> {
        // Parse URL strings into storage backends
        let source_url = StorageUrl::parse(&config.source_directory)?;
        let cache_url = StorageUrl::parse(&config.cache_directory)?;

        let source_storage = source_url.into_storage().await?;
        let cache_storage = cache_url.into_storage().await?;

        Ok(Self {
            name: config.name.clone(),
            url_prefix: config.url_prefix.clone(),
            source_storage,
            cache_storage,
            config,
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}
```

### 4.3 Directory Scanning Update

```rust
// src/gallery/core.rs

impl Gallery {
    pub async fn scan_directory(&self, path: &str) -> Result<Vec<GalleryItem>, GalleryError> {
        let entries = self.source_storage.list(path).await?;

        let mut items = Vec::new();
        for entry in entries {
            if entry.is_dir {
                // Check for hidden folder
                let folder_md = format!("{}/{}/_folder.md", path, entry.path);
                if let Ok(content) = self.source_storage.read(&folder_md).await {
                    let content_str = String::from_utf8_lossy(&content);
                    if is_hidden_folder(&content_str) {
                        continue;
                    }
                }
                items.push(GalleryItem::Directory { ... });
            } else if self.is_image(&entry.path) {
                items.push(GalleryItem::Image { ... });
            }
        }

        Ok(items)
    }

    pub async fn scan_recursive(&self, path: &str) -> Result<Vec<GalleryItem>, GalleryError> {
        let entries = self.source_storage.list_recursive(path).await?;
        // Process entries...
    }
}
```

---

## Phase 5: Refactor Image Serving

### 5.1 Serve Module Update

```rust
// src/gallery/image_processing/serve.rs

pub async fn serve_image(
    gallery: &Gallery,
    image_path: &str,
    size: ImageSize,
    format: OutputFormat,
) -> Result<Response, GalleryError> {
    let cache_key = generate_cache_key(image_path, &size, format);

    // Check cache
    if gallery.cache_storage.exists(&cache_key).await? {
        if is_cache_valid(gallery, image_path, &cache_key).await? {
            // Stream from cache
            let stream = gallery.cache_storage.read_stream(&cache_key).await?;
            return Ok(build_streaming_response(stream, format));
        } else {
            // Remove stale cache
            gallery.cache_storage.delete(&cache_key).await?;
        }
    }

    // Generate and cache
    let source_data = gallery.source_storage.read(image_path).await?;
    let processed = process_image(source_data, size, format).await?;
    gallery.cache_storage.write(&cache_key, processed.clone().into()).await?;

    Ok(build_bytes_response(processed, format))
}

async fn is_cache_valid(
    gallery: &Gallery,
    source_path: &str,
    cache_path: &str,
) -> Result<bool, GalleryError> {
    let source_meta = gallery.source_storage.metadata(source_path).await?;
    let cache_meta = gallery.cache_storage.metadata(cache_path).await?;

    match (source_meta.last_modified, cache_meta.last_modified) {
        (Some(source_time), Some(cache_time)) => Ok(cache_time >= source_time),
        // If no modification times (S3 without LastModified), use ETag comparison
        _ => {
            // For S3, we could store source ETag in cache metadata
            // For now, assume cache is valid if it exists
            Ok(true)
        }
    }
}
```

### 5.2 Batch Processing Update

```rust
// src/gallery/image_processing/resize.rs

pub async fn process_image_variants(
    gallery: &Gallery,
    image_path: &str,
    sizes: &[ImageSize],
    formats: &[OutputFormat],
) -> Result<Vec<PathBuf>, GalleryError> {
    // Read source image once
    let source_data = gallery.source_storage.read(image_path).await?;
    let blocking_cache = BlockingStorage::new(gallery.cache_storage.clone());

    let results = tokio::task::spawn_blocking(move || {
        let loaded = LoadedImage::from_bytes(&source_data, PathBuf::from(image_path), None)?;
        let mut cached_paths = Vec::new();

        for size in sizes {
            for format in formats {
                let mut variant = loaded.clone();
                variant.resize(size.width, size.height)?;

                let encoded = variant.encode_as(*format, jpeg_quality, webp_quality)?;
                let cache_key = generate_cache_key(image_path, size, *format);

                blocking_cache.write(&cache_key, Bytes::from(encoded))?;
                cached_paths.push(PathBuf::from(cache_key));
            }
        }

        Ok::<_, GalleryError>(cached_paths)
    }).await??;

    Ok(results)
}
```

---

## Phase 6: Refactor Cache Module

### 6.1 Metadata Cache Persistence

```rust
// src/gallery/cache.rs

impl Gallery {
    pub async fn load_metadata_cache(&self) -> Result<(), GalleryError> {
        let cache_path = "metadata_cache.json";

        match self.cache_storage.read(cache_path).await {
            Ok(data) => {
                let cache: HashMap<String, ImageMetadata> = serde_json::from_slice(&data)?;
                *self.metadata_cache.write().await = cache;
            }
            Err(StorageError::NotFound(_)) => {
                // No existing cache, start fresh
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    pub async fn save_metadata_cache(&self) -> Result<(), GalleryError> {
        let cache = self.metadata_cache.read().await;
        let data = serde_json::to_vec_pretty(&*cache)?;
        self.cache_storage.write("metadata_cache.json", data.into()).await?;
        Ok(())
    }
}
```

---

## Phase 7: Configuration Schema (URL-Based) ✅

### 7.1 Storage URL Format

Instead of complex nested configuration structures, storage locations are specified as URLs:

```
Filesystem:
  photos                          → Local path "photos"
  /absolute/path/to/photos        → Absolute filesystem path
  file:///absolute/path/to/photos → Explicit file:// scheme

S3:
  s3://bucket-name/prefix         → S3 bucket with prefix
  s3://bucket-name/prefix?region=us-east-1  → With region override
  s3://bucket-name/prefix?endpoint=http://localhost:9000  → Custom endpoint (MinIO)
```

### 7.2 Storage URL Parsing

```rust
// src/storage/url.rs

use url::Url;

#[derive(Debug, Clone)]
pub enum StorageUrl {
    Filesystem { path: PathBuf },
    S3 {
        bucket: String,
        prefix: String,
        region: Option<String>,
        endpoint: Option<String>,
    },
}

impl StorageUrl {
    pub fn parse(s: &str) -> Result<Self, StorageError> {
        // Check for s3:// scheme
        if s.starts_with("s3://") {
            let url = Url::parse(s).map_err(|e| StorageError::InvalidUrl(e.to_string()))?;
            let bucket = url.host_str()
                .ok_or_else(|| StorageError::InvalidUrl("missing bucket".into()))?
                .to_string();
            let prefix = url.path().trim_start_matches('/').to_string();

            // Parse query params for options
            let mut region = None;
            let mut endpoint = None;
            for (key, value) in url.query_pairs() {
                match key.as_ref() {
                    "region" => region = Some(value.to_string()),
                    "endpoint" => endpoint = Some(value.to_string()),
                    _ => {} // Ignore unknown params
                }
            }

            Ok(StorageUrl::S3 { bucket, prefix, region, endpoint })
        } else if s.starts_with("file://") {
            // Explicit file:// URL
            let path = s.strip_prefix("file://").unwrap();
            Ok(StorageUrl::Filesystem { path: PathBuf::from(path) })
        } else {
            // Plain path = filesystem
            Ok(StorageUrl::Filesystem { path: PathBuf::from(s) })
        }
    }

    pub async fn into_storage(self) -> Result<Arc<dyn Storage>, StorageError> {
        match self {
            StorageUrl::Filesystem { path } => {
                Ok(Arc::new(FilesystemStorage::new(path)))
            }
            StorageUrl::S3 { bucket, prefix, region, endpoint } => {
                Ok(Arc::new(S3Storage::new(bucket, prefix, region, endpoint).await?))
            }
        }
    }
}

// Custom deserializer for storage URLs
impl<'de> Deserialize<'de> for StorageUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        StorageUrl::parse(&s).map_err(serde::de::Error::custom)
    }
}
```

### 7.3 Configuration Examples

```toml
# config.toml - Simple URL-based configuration

# ============================================================
# Gallery Configuration
# ============================================================
[[galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "photos"                    # Local filesystem
cache_directory = "cache/photos"               # Local filesystem

# Or with S3:
# source_directory = "s3://my-photos/galleries/main"
# cache_directory = "s3://my-cache/galleries/main?region=us-west-2"

# ============================================================
# Static Files Configuration
# ============================================================
[static_files]
# Array of storage locations (first match wins)
directories = ["static-custom", "static"]

# Or with S3 for custom overrides:
# directories = ["s3://my-assets/static-custom", "static"]

# ============================================================
# Templates Configuration
# ============================================================
[templates]
# Array of template locations (first match wins)
directories = ["templates-custom", "templates"]

# Or with S3:
# directories = ["s3://my-assets/templates-custom", "templates"]

# ============================================================
# Posts Configuration
# ============================================================
[[posts]]
name = "blog"
source_directory = "posts/blog"                # Local filesystem
url_prefix = "/blog"

# Or with S3:
# source_directory = "s3://my-content/posts/blog"
```

### 7.4 Mixed Storage Example

Real-world deployment might use different backends for different purposes:

```toml
# Local source images, S3 cache (for CDN serving)
[[galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "/mnt/photos"               # Fast local NVMe
cache_directory = "s3://cdn-cache/gallery?region=us-east-1"  # CDN-backed

# S3 source images, local cache (for edge server)
[[galleries]]
name = "archive"
url_prefix = "/archive"
source_directory = "s3://photo-archive/originals"
cache_directory = "/var/cache/tenrankai/archive"  # Local SSD cache

# Static files from S3 with local fallback
[static_files]
directories = [
    "s3://my-assets/static",    # Custom branding from S3
    "static"                     # Built-in defaults
]
```

### 7.5 Implementation Notes

**Implemented in commit `9825199`:**

- Created `src/storage/url.rs` with `StorageUrl` enum and parser
- Supports filesystem paths, `file://` URLs, and `s3://` URLs
- Query parameter parsing for `region` and `endpoint`
- Serde serialization/deserialization for config files
- Added `url = "2"` dependency for URL parsing

---

## Phase 7B: Static Files Storage Support ✅

Static files (`static/` directory) currently serve CSS, JavaScript, fonts, and other assets. Adding storage abstraction enables:
- Serving custom assets from S3/CDN
- Multi-directory cascading with mixed backends
- Versioned deployments from object storage

### 7B.1 Static File Handler Changes

```rust
// src/static_files.rs

pub struct StaticFileHandler {
    /// Storage backends in priority order (first match wins)
    storages: Vec<Arc<dyn Storage>>,
    /// File version cache for cache-busting
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
}

impl StaticFileHandler {
    pub async fn new(directories: &[String]) -> Result<Self, StorageError> {
        let mut storages = Vec::new();
        for dir in directories {
            let url = StorageUrl::parse(dir)?;
            storages.push(url.into_storage().await?);
        }
        Ok(Self {
            storages,
            file_versions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn serve(&self, path: &str, has_version: bool) -> Response {
        // Try each storage in order
        for storage in &self.storages {
            match storage.read(path).await {
                Ok(data) => {
                    // Build response with appropriate headers
                    return self.build_response(path, data, has_version);
                }
                Err(StorageError::NotFound(_)) => continue,
                Err(e) => {
                    tracing::error!("Storage error for {}: {}", path, e);
                    continue;
                }
            }
        }
        // Not found in any storage
        StatusCode::NOT_FOUND.into_response()
    }

    /// Refresh file versions from all storages for cache-busting
    pub async fn refresh_file_versions(&self) {
        let mut versions = HashMap::new();
        for (idx, storage) in self.storages.iter().enumerate() {
            if let Ok(entries) = storage.list_recursive("").await {
                for entry in entries {
                    if !entry.is_dir && (entry.path.ends_with(".css") || entry.path.ends_with(".js")) {
                        // Only insert if not already present (first storage wins)
                        if !versions.contains_key(&entry.path) {
                            if let Some(meta) = entry.metadata {
                                if let Some(mtime) = meta.last_modified {
                                    let secs = mtime.duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0);
                                    versions.insert(entry.path.clone(), secs);
                                }
                            }
                        }
                    }
                }
            }
        }
        *self.file_versions.write().await = versions;
    }
}
```

### 7B.2 Static Files with S3 Redirect

For S3-backed static files, we can use signed URL redirects:

```rust
impl StaticFileHandler {
    pub async fn serve(&self, path: &str, has_version: bool) -> Response {
        for storage in &self.storages {
            // Check if file exists
            if !storage.exists(path).await.unwrap_or(false) {
                continue;
            }

            // Try redirect for S3 (reduces server bandwidth)
            if storage.supports_redirect() {
                if let Some(signed_url) = storage.signed_url(path, Duration::from_secs(86400)).await {
                    return Response::builder()
                        .status(StatusCode::TEMPORARY_REDIRECT)
                        .header("Location", signed_url)
                        .header("Cache-Control", "public, max-age=86400")
                        .body(Body::empty())
                        .unwrap();
                }
            }

            // Fallback to proxying
            if let Ok(data) = storage.read(path).await {
                return self.build_response(path, data, has_version);
            }
        }
        StatusCode::NOT_FOUND.into_response()
    }
}
```

### 7B.3 Implementation Notes

**Implemented in commit `27bc262`:**

- `StaticFileHandler` refactored to use `Vec<DynStorage>` instead of `Vec<PathBuf>`
- Added `from_urls()` constructor for URL-based configuration
- Added `use_redirects` config option (defaults to `false`):
  ```toml
  [static_files]
  directories = ["s3://bucket/prefix?region=us-west-2", "static"]
  use_redirects = false  # proxy content (default)
  # use_redirects = true  # redirect to signed S3 URLs
  ```
- Signed URL redirects return `307 Temporary Redirect` with 1-hour expiry
- Cascading storage: first backend with matching file wins
- File version tracking works across all storage backends

---

## Phase 7C: Template Storage Support ✅

**Status**: Complete (commit `757d7e2`)

**Implementation Notes**:
- Changed `TemplateConfig.directories` from `Vec<PathBuf>` to `Vec<String>` for storage URLs
- Refactored `TemplateEngine` to use `Vec<DynStorage>` instead of `Vec<PathBuf>`
- Added `template_exists()` method for async storage existence checks
- Updated `load_template()` to use storage `read()` and `metadata()` operations
- Added `create_storages_from_urls()` helper in storage module
- Updated startup checks to recognize S3 template URLs
- TTL-based caching (5 minute default) reduces S3 API calls

Templates can also be loaded from object storage, enabling:
- Remote template management without redeployment
- A/B testing with different template sets
- Multi-tenant customization

### 7C.1 Template Engine Changes

```rust
// src/templating.rs

pub struct TemplateEngine {
    /// Storage backends for templates (first match wins)
    storages: Vec<Arc<dyn Storage>>,
    /// Compiled template cache
    template_cache: Arc<RwLock<HashMap<String, CachedTemplate>>>,
    /// Partial templates
    partials: Arc<RwLock<HashMap<String, String>>>,
    /// Liquid parser
    parser: liquid::Parser,
}

impl TemplateEngine {
    pub async fn new(directories: &[String]) -> Result<Self, TemplateError> {
        let mut storages = Vec::new();
        for dir in directories {
            let url = StorageUrl::parse(dir)?;
            storages.push(url.into_storage().await?);
        }

        let engine = Self {
            storages,
            template_cache: Arc::new(RwLock::new(HashMap::new())),
            partials: Arc::new(RwLock::new(HashMap::new())),
            parser: Self::create_parser(),
        };

        // Load partials on startup
        engine.load_partials().await?;

        Ok(engine)
    }

    async fn load_template(&self, path: &str) -> Result<String, TemplateError> {
        // Try each storage in order
        for storage in &self.storages {
            match storage.read(path).await {
                Ok(data) => {
                    let content = String::from_utf8(data.to_vec())
                        .map_err(|e| TemplateError::InvalidTemplate(e.to_string()))?;
                    return Ok(content);
                }
                Err(StorageError::NotFound(_)) => continue,
                Err(e) => {
                    tracing::warn!("Storage error loading template {}: {}", path, e);
                    continue;
                }
            }
        }
        Err(TemplateError::NotFound(path.to_string()))
    }

    async fn load_partials(&self) -> Result<(), TemplateError> {
        let mut all_partials = HashMap::new();

        // Scan partials from all storages (first match wins)
        for storage in &self.storages {
            if let Ok(entries) = storage.list_recursive("partials").await {
                for entry in entries {
                    if !entry.is_dir && entry.path.ends_with(".liquid") {
                        let partial_name = entry.path
                            .strip_prefix("partials/")
                            .unwrap_or(&entry.path)
                            .strip_suffix(".liquid")
                            .unwrap_or(&entry.path)
                            .to_string();

                        // Only insert if not already present
                        if !all_partials.contains_key(&partial_name) {
                            if let Ok(data) = storage.read(&format!("partials/{}", entry.path)).await {
                                if let Ok(content) = String::from_utf8(data.to_vec()) {
                                    all_partials.insert(partial_name, content);
                                }
                            }
                        }
                    }
                }
            }
        }

        *self.partials.write().await = all_partials;
        Ok(())
    }
}
```

### 7C.2 Template Caching with Storage

For S3-backed templates, caching is important to avoid repeated fetches:

```rust
struct CachedTemplate {
    content: String,
    compiled: liquid::Template,
    etag: Option<String>,
    fetched_at: Instant,
}

impl TemplateEngine {
    async fn get_template(&self, path: &str) -> Result<liquid::Template, TemplateError> {
        let cache = self.template_cache.read().await;

        // Check cache freshness (5 minute TTL for S3 templates)
        if let Some(cached) = cache.get(path) {
            if cached.fetched_at.elapsed() < Duration::from_secs(300) {
                return Ok(cached.compiled.clone());
            }
        }
        drop(cache);

        // Fetch and compile
        let content = self.load_template(path).await?;
        let compiled = self.parser.parse(&content)
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // Update cache
        let mut cache = self.template_cache.write().await;
        cache.insert(path.to_string(), CachedTemplate {
            content,
            compiled: compiled.clone(),
            etag: None,
            fetched_at: Instant::now(),
        });

        Ok(compiled)
    }
}
```

### 7C.3 Template TTL Caching (Implemented)

**Implemented in `src/templating.rs`:**

Added TTL-based caching to reduce filesystem/network calls for frequently accessed templates. The implementation uses a fast path that returns cached content immediately if within the TTL window, avoiding metadata checks entirely.

```rust
/// Default TTL for template cache entries (5 minutes).
const DEFAULT_TEMPLATE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

struct CachedTemplate {
    content: String,
    modified: SystemTime,
    fetched_at: Instant,  // Track when cache entry was created
}

impl TemplateEngine {
    // TTL can be customized per-engine
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self;

    async fn load_template(&self, path: &str) -> Result<String, String> {
        // Fast path: return immediately if within TTL
        if self.cache_ttl > Duration::ZERO {
            if let Some(cached) = cache.get(path) {
                if cached.fetched_at.elapsed() < self.cache_ttl {
                    return Ok(cached.content.clone());
                }
            }
        }
        // ... normal loading with modification time check
    }
}
```

**Benefits:**
- Reduces S3 API calls significantly (especially for partials loaded on every request)
- No modification time check within TTL window
- Cache entries extend their TTL when modification time is checked and unchanged
- Use `Duration::ZERO` to disable TTL and always check modification time

### 7C.4 Future: Storage-Level Caching (Option A - Design Notes)

An alternative to application-level TTL caching is implementing a generic caching wrapper at the storage layer. This approach would benefit all storage consumers (templates, posts, static files) without requiring per-module caching logic.

**Design Overview:**

```rust
// src/storage/cached.rs

use std::collections::HashMap;
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::sync::RwLock;

/// Configuration for the caching wrapper
pub struct CachedStorageConfig {
    /// Maximum number of entries to cache
    pub max_entries: usize,
    /// TTL for cache entries
    pub ttl: Duration,
    /// Maximum size of a single cacheable entry (larger files bypass cache)
    pub max_entry_size: usize,
}

impl Default for CachedStorageConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_secs(5 * 60),       // 5 minutes
            max_entry_size: 1024 * 1024,            // 1MB
        }
    }
}

struct CacheEntry {
    data: Bytes,
    fetched_at: Instant,
    size: usize,
}

/// A caching wrapper around any Storage implementation
pub struct CachedStorage<S: Storage> {
    inner: S,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    config: CachedStorageConfig,
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(inner: S, config: CachedStorageConfig) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Wrap an existing storage with default caching
    pub fn wrap(inner: S) -> Self {
        Self::new(inner, CachedStorageConfig::default())
    }
}

#[async_trait]
impl<S: Storage> Storage for CachedStorage<S> {
    async fn read(&self, path: &str) -> Result<Bytes, StorageError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(path) {
                if entry.fetched_at.elapsed() < self.config.ttl {
                    return Ok(entry.data.clone());
                }
            }
        }

        // Fetch from underlying storage
        let data = self.inner.read(path).await?;

        // Cache if small enough
        if data.len() <= self.config.max_entry_size {
            let mut cache = self.cache.write().await;

            // Evict oldest entries if at capacity
            while cache.len() >= self.config.max_entries {
                if let Some(oldest_key) = cache.iter()
                    .min_by_key(|(_, v)| v.fetched_at)
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest_key);
                }
            }

            cache.insert(path.to_string(), CacheEntry {
                data: data.clone(),
                fetched_at: Instant::now(),
                size: data.len(),
            });
        }

        Ok(data)
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(path) {
                if entry.fetched_at.elapsed() < self.config.ttl {
                    return Ok(true);
                }
            }
        }
        self.inner.exists(path).await
    }

    // Write operations invalidate cache
    async fn write(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        self.inner.write(path, data).await?;
        self.cache.write().await.remove(path);
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        self.inner.delete(path).await?;
        self.cache.write().await.remove(path);
        Ok(())
    }

    // Pass through other methods
    async fn metadata(&self, path: &str) -> Result<ObjectMetadata, StorageError> {
        self.inner.metadata(path).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        self.inner.list(prefix).await
    }

    // ... other trait methods
}
```

**Usage Example:**

```rust
// Wrap S3 storage with caching for template loading
let s3_storage = S3Storage::new("bucket", "prefix", Some("us-west-2")).await?;
let cached_storage = CachedStorage::wrap(s3_storage);

// Use in template engine
let template_engine = TemplateEngine::with_storage(Arc::new(cached_storage));
```

**Trade-offs vs Application-Level Caching:**

| Aspect | Storage-Level (Option A) | Application-Level (Option B) |
|--------|-------------------------|------------------------------|
| Scope | All storage consumers | Per-module implementation |
| Complexity | Single implementation | Per-module caching logic |
| Cache key | Path-based | Can include parsed/compiled data |
| Memory efficiency | Raw bytes only | Can cache parsed structures |
| Invalidation | Simple (by path) | Can be smarter (content-aware) |
| Configuration | Global | Per-module tuning |

**Recommendation:**

For templates, the current application-level TTL caching (Option B) is preferred because:
1. We can cache the compiled `liquid::Template` in addition to raw content
2. Template-specific invalidation logic (modification time check on TTL expiry)
3. Simpler implementation for the current use case

Storage-level caching (Option A) would be valuable as a future enhancement when:
- Adding more storage consumers (posts, static files with frequently accessed small assets)
- Wanting consistent caching behavior across all storage access
- Needing to reduce API calls at the infrastructure level

---

## Phase 7D: Posts Storage Support

The posts/blog system can also use storage abstraction for markdown files.

### 7D.1 Posts Manager Changes

```rust
// src/posts/core.rs

pub struct PostsManager {
    pub name: String,
    pub url_prefix: String,
    /// Storage for post markdown files
    storage: Arc<dyn Storage>,
    /// Cached post index
    posts_cache: Arc<RwLock<Vec<PostSummary>>>,
}

impl PostsManager {
    pub async fn new(config: &PostsConfig) -> Result<Self, PostsError> {
        let url = StorageUrl::parse(&config.source_directory)?;
        let storage = url.into_storage().await?;

        let manager = Self {
            name: config.name.clone(),
            url_prefix: config.url_prefix.clone(),
            storage,
            posts_cache: Arc::new(RwLock::new(Vec::new())),
        };

        // Initial scan
        manager.refresh_posts().await?;

        Ok(manager)
    }

    pub async fn refresh_posts(&self) -> Result<(), PostsError> {
        let mut posts = Vec::new();

        // List all markdown files
        let entries = self.storage.list_recursive("").await?;
        for entry in entries {
            if !entry.is_dir && entry.path.ends_with(".md") {
                if let Ok(data) = self.storage.read(&entry.path).await {
                    if let Ok(content) = String::from_utf8(data.to_vec()) {
                        if let Some(post) = self.parse_post(&entry.path, &content) {
                            posts.push(post);
                        }
                    }
                }
            }
        }

        // Sort by date descending
        posts.sort_by(|a, b| b.date.cmp(&a.date));

        *self.posts_cache.write().await = posts;
        Ok(())
    }

    pub async fn get_post(&self, slug: &str) -> Result<Post, PostsError> {
        let path = format!("{}.md", slug);
        let data = self.storage.read(&path).await
            .map_err(|_| PostsError::NotFound(slug.to_string()))?;
        let content = String::from_utf8(data.to_vec())
            .map_err(|e| PostsError::InvalidContent(e.to_string()))?;
        self.parse_full_post(&path, &content)
            .ok_or_else(|| PostsError::InvalidContent("Failed to parse post".to_string()))
    }
}
```

### 7D.2 Posts Configuration

```toml
[[posts]]
name = "blog"
url_prefix = "/blog"
source_directory = "posts/blog"              # Local filesystem

# Or from S3:
# source_directory = "s3://my-content/posts/blog"

[[posts]]
name = "docs"
url_prefix = "/docs"
source_directory = "s3://my-docs/documentation"  # S3-hosted docs
```

### 7D.3 Implementation Notes (Completed)

The posts storage abstraction has been implemented with the following changes:

1. **PostsConfig changes** (`src/posts/types.rs`):
   - Changed `source_directory` from `PathBuf` to `String` for URL-based configuration
   - Changed `Post.path` from `PathBuf` to `String` for storage-relative paths

2. **PostsManager refactoring** (`src/posts/core.rs`):
   - Added `storage: DynStorage` field to `PostsManager`
   - Updated `new()` to accept a storage backend parameter
   - Replaced `scan_directory()` with `storage.list_recursive("")`
   - Replaced `tokio::fs::read_to_string()` with `storage.read_to_string()`
   - Replaced `tokio::fs::metadata()` with `storage.metadata()` for modification times
   - Updated `generate_slug()` to work with string paths

3. **PostsError updates** (`src/posts/error.rs`):
   - Added `StorageError` variant for storage operation failures

4. **Initialization updates** (`src/lib.rs`, `src/main.rs`):
   - Parse `source_directory` as a storage URL
   - Create storage backend using `StorageUrl::parse().into_storage()`
   - Pass storage to `PostsManager::new()`

5. **Test updates** (`src/posts/tests.rs`, `tests/posts_integration.rs`):
   - Updated all tests to create `FilesystemStorage` and pass to `PostsManager::new()`
   - Updated `PostsConfig` source_directory to use `String`

The implementation supports both filesystem paths and S3 URLs for posts storage.

---

## Phase 8: Signed URL Redirects for Serving

Instead of proxying cached images through the server, we can redirect clients directly to S3:

```
Proxying (current approach for files):
  Client → Server → Read from S3 → Server → Client
  (Server bandwidth + CPU used)

Signed URL Redirect:
  Client → Server → 302 Redirect(signed_url) → Client fetches from S3/CDN
  (Minimal server load, leverages S3/CloudFront edge caching)
```

### 8.1 When to Use Redirects vs Proxying

| Scenario | Approach | Reason |
|----------|----------|--------|
| Cached images on S3 | **Redirect** | Already processed, just serve |
| Source images needing processing | **Proxy** | Must process before serving |
| Private galleries (auth required) | **Redirect** with short expiry | Signed URL enforces access |
| Public galleries on S3 | **Redirect** or direct URL | No signing needed |
| Local filesystem cache | **Proxy** | No redirect possible |

### 8.2 Implementation

```rust
// src/storage/mod.rs

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // ... existing methods ...

    /// Generate a signed URL for direct client access (optional)
    /// Returns None if storage doesn't support signed URLs (e.g., filesystem)
    async fn signed_url(&self, path: &str, expiry: Duration) -> Option<String>;

    /// Check if this storage supports redirect-based serving
    fn supports_redirect(&self) -> bool {
        false  // Default: no redirect support
    }
}

// Filesystem: no redirect support
impl Storage for FilesystemStorage {
    async fn signed_url(&self, _path: &str, _expiry: Duration) -> Option<String> {
        None
    }

    fn supports_redirect(&self) -> bool {
        false
    }
}

// S3: full redirect support
impl Storage for S3Storage {
    async fn signed_url(&self, path: &str, expiry: Duration) -> Option<String> {
        use aws_sdk_s3::presigning::PresigningConfig;

        let config = PresigningConfig::builder()
            .expires_in(expiry)
            .build()
            .ok()?;

        let presigned = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&self.full_key(path))
            .presigned(config)
            .await
            .ok()?;

        Some(presigned.uri().to_string())
    }

    fn supports_redirect(&self) -> bool {
        true
    }
}
```

### 8.3 Serving Handler with Redirect Support

```rust
// src/gallery/image_processing/serve.rs

pub async fn serve_cached_image(
    gallery: &Gallery,
    cache_key: &str,
    format: OutputFormat,
) -> Result<Response, GalleryError> {
    // Check if cache exists
    if !gallery.cache_storage.exists(cache_key).await? {
        return Err(GalleryError::NotFound);
    }

    // Try redirect first (for S3 cache)
    if gallery.cache_storage.supports_redirect() {
        if let Some(signed_url) = gallery.cache_storage
            .signed_url(cache_key, Duration::from_secs(3600))  // 1 hour expiry
            .await
        {
            return Ok(Response::builder()
                .status(StatusCode::TEMPORARY_REDIRECT)  // 307 preserves method
                .header("Location", signed_url)
                .header("Cache-Control", "private, max-age=3600")
                .body(Body::empty())
                .unwrap());
        }
    }

    // Fallback: proxy the content
    let stream = gallery.cache_storage.read_stream(cache_key).await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", format.mime_type())
        .header("Cache-Control", "public, max-age=31536000")  // 1 year for immutable cache keys
        .body(Body::from_stream(stream))
        .unwrap())
}
```

### 8.4 CloudFront Integration (Optional)

For even better performance, use CloudFront in front of S3:

```toml
# config.toml
[galleries.cache_storage]
type = "s3"
bucket = "my-gallery-cache"
prefix = "cache/main"

# Optional: Use CloudFront for signed URLs instead of S3 directly
cloudfront_distribution = "E1234567890ABC"
cloudfront_key_pair_id = "K1234567890ABC"
cloudfront_private_key_path = "/path/to/private_key.pem"
```

```rust
impl S3Storage {
    async fn signed_url(&self, path: &str, expiry: Duration) -> Option<String> {
        if let Some(cf_config) = &self.cloudfront_config {
            // Generate CloudFront signed URL (better caching, lower latency)
            self.cloudfront_signed_url(path, expiry, cf_config)
        } else {
            // Fall back to S3 presigned URL
            self.s3_presigned_url(path, expiry).await
        }
    }
}
```

### 8.5 Redirect Considerations

**Pros:**
- ✅ Reduces server bandwidth significantly
- ✅ Reduces server CPU (no proxying)
- ✅ Leverages S3/CloudFront edge caching
- ✅ Clients can cache based on URL

**Cons:**
- ⚠️ Exposes S3 bucket structure (mitigated by signed URLs)
- ⚠️ CORS configuration required on S3 bucket
- ⚠️ Signed URLs expire (need appropriate expiry time)
- ⚠️ Extra redirect latency (typically <50ms)

**CORS Configuration for S3:**
```json
{
  "CORSRules": [{
    "AllowedOrigins": ["https://yourdomain.com"],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["*"],
    "ExposeHeaders": ["Content-Length", "Content-Type"],
    "MaxAgeSeconds": 3600
  }]
}
```

---

## Phase 9: Migration Strategy

### 9.1 Why Cache First?

Starting with cache storage is easier because:

| Aspect | Cache Storage | Source Gallery |
|--------|---------------|----------------|
| Operations | Write, read, exists, delete | + Directory scan, recursive walk |
| Data format | Known (our processed images) | Unknown (user's various formats) |
| Metadata | Minimal (just cache keys) | Complex (EXIF, ICC, dimensions) |
| Seeking | Not needed (write whole files) | Needed (image format parsing) |
| Error handling | Simple (regenerate on failure) | Complex (user's source files) |

**Cache operations are essentially:**
1. `exists(cache_key)` - check if cached
2. `read(cache_key)` or `read_stream(cache_key)` - serve cached image
3. `write(cache_key, bytes)` - save processed image
4. `delete(cache_key)` - invalidate stale cache
5. `signed_url(cache_key)` - redirect to S3 (optional)

No directory scanning, no metadata extraction, no complex format parsing.

### 9.2 Implementation Order

**Phase A: Storage Abstraction (Foundation)**

1. **Create storage module** (`src/storage/mod.rs`)
   - Define `Storage` trait with object_store as backend
   - Define `StorageError`, `ObjectMetadata`, `StorageEntry`
   - Implement `FilesystemStorage` wrapper around object_store's LocalFileSystem
   - Implement `S3Storage` wrapper around object_store's AmazonS3

2. **Add configuration** (`src/config.rs`)
   - Add `StorageConfig` enum (filesystem vs s3)
   - Update `GalleryConfig` with `cache_storage` field
   - Keep `source_directory` as-is for now (Phase B)

**Phase B: Cache Storage Migration**

3. **Update cache operations** (`src/gallery/cache.rs`)
   - Replace `tokio::fs::write()` with `cache_storage.write()`
   - Replace `tokio::fs::metadata()` with `cache_storage.metadata()`
   - Replace path existence checks with `cache_storage.exists()`
   - Replace `tokio::fs::remove_file()` with `cache_storage.delete()`

4. **Update image serving** (`src/gallery/image_processing/serve.rs`)
   - Replace `File::open().await` with `cache_storage.read_stream()`
   - Add redirect support for S3 cache (`signed_url()`)
   - Keep source image loading unchanged for now

5. **Update batch processing** (`src/gallery/image_processing/resize.rs`)
   - Replace cache writes with `cache_storage.write()`
   - Source loading stays filesystem-based for now

6. **Test cache on S3**
   - Configure cache_storage as S3
   - Verify write → read → serve cycle works
   - Test redirect serving
   - Test cache invalidation

**Phase C: Source Gallery Migration**

7. **Add CachedObjectReader** (`src/storage/cached_reader.rs`)
   - Implement intelligent sync reader for source images
   - Add prefetch support for EXIF extraction

8. **Update LoadedImage** (`src/gallery/image_processing/types.rs`)
   - Add `from_bytes()` and `from_reader()` methods
   - Update format modules with `*_from_bytes` variants

9. **Update gallery scanning** (`src/gallery/core.rs`)
   - Replace `tokio::fs::read_dir()` with `source_storage.list()`
   - Replace `WalkDir` with `source_storage.list_recursive()`
   - Update metadata extraction to use storage abstraction

10. **Update source image loading**
    - Replace `std::fs::File::open()` with `CachedObjectReader`
    - Update all format-specific loaders

11. **Full integration test**
    - Source on S3, cache on S3
    - Source on filesystem, cache on S3
    - Source on S3, cache on filesystem

### 9.3 Incremental Migration Pattern

Each phase can be deployed independently:

```
Current:  source=filesystem, cache=filesystem

Phase B:  source=filesystem, cache=filesystem OR s3  ← Cache abstracted
          (source unchanged, cache pluggable)

Phase C:  source=filesystem OR s3, cache=filesystem OR s3  ← Fully pluggable
```

### 9.4 Minimal Phase B Implementation

For the quickest win, Phase B only needs:

```rust
// Minimal Storage trait for cache operations
#[async_trait]
pub trait CacheStorage: Send + Sync + 'static {
    async fn read(&self, key: &str) -> Result<Bytes, StorageError>;
    async fn read_stream(&self, key: &str) -> Result<BoxStream<Bytes>, StorageError>;
    async fn write(&self, key: &str, data: Bytes) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn signed_url(&self, key: &str, expiry: Duration) -> Option<String>;
    fn supports_redirect(&self) -> bool;
}
```

No directory listing, no metadata, no seeking - just simple key-value storage with optional redirect support. This is much simpler than the full `Storage` trait needed for source galleries.

### 9.5 Testing Strategy

```rust
// tests/storage_tests.rs

#[tokio::test]
async fn test_filesystem_storage() {
    let storage = FilesystemStorage::new("/tmp/test_storage");

    // Write
    storage.write("test.txt", Bytes::from("hello")).await.unwrap();

    // Read
    let data = storage.read("test.txt").await.unwrap();
    assert_eq!(&data[..], b"hello");

    // Exists
    assert!(storage.exists("test.txt").await.unwrap());

    // Delete
    storage.delete("test.txt").await.unwrap();
    assert!(!storage.exists("test.txt").await.unwrap());
}

#[tokio::test]
#[ignore] // Requires AWS credentials
async fn test_s3_storage() {
    let storage = S3Storage::new(
        "test-bucket".to_string(),
        "test-prefix".to_string(),
        Some("us-east-1".to_string()),
    ).await.unwrap();

    // Same tests as filesystem...
}
```

---

## Phase 9: New Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
aws-sdk-s3 = "1.0"
aws-config = "1.0"
bytes = "1.0"
futures = "0.3"
tokio-util = { version = "0.7", features = ["io"] }

[features]
default = ["avif"]
avif = ["libavif", "libavif-sys"]
s3 = ["aws-sdk-s3", "aws-config"]
```

---

## Summary

### Files to Create
- `src/storage/mod.rs` - Storage trait and types
- `src/storage/filesystem.rs` - Filesystem implementation
- `src/storage/s3.rs` - S3 implementation
- `src/storage/blocking.rs` - Sync wrapper
- `src/storage/url.rs` - URL-based storage configuration parsing
- `src/storage/cached_reader.rs` - Intelligent sync reader with sparse caching

### Files to Modify
- `src/gallery/image_processing/types.rs` - LoadedImage refactoring
- `src/gallery/image_processing/formats/*.rs` - Add bytes-based APIs
- `src/gallery/core.rs` - Use Storage for directory scanning
- `src/gallery/cache.rs` - Use Storage for cache operations
- `src/gallery/image_processing/serve.rs` - Use Storage for serving
- `src/gallery/image_processing/resize.rs` - Use Storage for batch processing
- `src/config.rs` - Add StorageConfig types, update to use URL strings
- `src/static_files.rs` - Multi-backend static file handler with redirect support
- `src/templating.rs` - Multi-backend template loading with caching
- `src/posts/core.rs` - Storage-backed posts manager
- `Cargo.toml` - Add dependencies (url, object_store, aws-sdk-s3)

### Estimated Scope
- ~18 files modified
- ~6 new files created
- ~2500 lines of new code
- ~700 lines of modified code

### Configuration Migration

Old configuration style (deprecated):
```toml
[galleries.cache_storage]
type = "s3"
bucket = "my-bucket"
prefix = "cache"
```

New URL-based configuration:
```toml
[[galleries]]
source_directory = "photos"                    # Local filesystem
cache_directory = "s3://my-bucket/cache"       # S3 with defaults
# Or with options:
# cache_directory = "s3://my-bucket/cache?region=us-west-2&endpoint=http://minio:9000"
```
