# Pluggable Storage Abstraction Plan

## Status Overview

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Storage Traits | ✅ Complete |
| Phase 2 | Storage Backends (Filesystem + S3) | ✅ Complete |
| Phase 3 | Refactor LoadedImage | ⏳ Not Started |
| Phase 4 | Refactor Gallery Module | ⏳ Not Started |
| Phase 5 | Refactor Image Serving | ⏳ Not Started |
| Phase 6 | Cache Module Storage | ✅ Complete |
| Phase 7 | URL-Based Configuration | ✅ Complete |
| Phase 7B | Static Files Storage | ✅ Complete |
| Phase 7C | Template Storage | ✅ Complete |
| Phase 7D | Posts Storage | ✅ Complete |
| Phase 8 | Signed URL Redirects | ✅ Complete |
| Phase 9 | Migration Strategy | 🔄 In Progress |
| Phase 10 | Gallery Source Storage | 📋 Planned |

## Overview

This plan outlines how to refactor Tenrankai to support pluggable storage backends, enabling both local filesystem and S3 storage for galleries, caches, templates, posts, and static files.

---

## Completed Phases (Summary)

### Phase 1: Storage Traits ✅

Defined core `Storage` trait in `src/storage/mod.rs`:
- `read()`, `write()`, `exists()`, `metadata()`, `delete()`
- `list()`, `list_recursive()` for directory operations
- `read_stream()`, `read_range()` for efficient streaming
- `signed_url()` for S3 redirect support

### Phase 2: Storage Backends ✅

**Filesystem Storage** (`src/storage/filesystem.rs`):
- Wraps tokio async filesystem operations
- Uses `walkdir` for recursive listing

**S3 Storage** (`src/storage/s3.rs`):
- Uses AWS SDK for Rust
- Supports custom endpoints (MinIO), region override
- Presigned URL generation for redirects

### Phase 6: Cache Module Storage ✅

Refactored cache module to use storage abstraction:
- `Gallery` has `cache_storage: DynStorage` field
- Metadata cache (`metadata_cache.json`) via storage
- Processed images via `SyncStorageAdapter` in `spawn_blocking`
- `is_cache_valid_by_key()` uses storage metadata
- Optimized format coverage with single `list()` + HashSet

**Key commits:**
- `e400ff4` perf: Optimize format coverage analysis
- `9acb27b` refactor: Remove redundant exists() calls

### Phase 7: URL-Based Configuration ✅

Storage URLs parse filesystem paths or S3 URLs:
```
photos                              → Local filesystem
s3://bucket/prefix?region=us-west-2 → S3 with region
s3://bucket/prefix?endpoint=http://localhost:9000 → MinIO
```

### Phase 7B-7D: Static/Template/Posts Storage ✅

- Static files support S3 with signed URL redirects
- Templates load from storage (with multi-directory support)
- Posts markdown files load from storage

### Phase 8: Signed URL Redirects ✅

Static files can redirect to S3 presigned URLs for direct client download, reducing server bandwidth.

---

## Pending Phases

### Phase 3: Refactor LoadedImage

**Goal**: Enable `LoadedImage::load()` to work with storage backends.

**Challenge**: Image decoding requires `Read + Seek`. S3 doesn't support seeking natively.

**Solution**: `SyncStorageReader` with range-based seeking:
```rust
// src/storage/sync_adapter.rs - Already implemented
pub struct SyncStorageReader {
    storage: DynStorage,
    path: String,
    handle: Handle,
    position: u64,
    size: u64,
    cache: HashMap<u64, Bytes>,  // Chunk cache
    chunk_size: u64,
}

impl Read for SyncStorageReader { ... }
impl Seek for SyncStorageReader { ... }
```

**Files to modify:**
- `src/gallery/image_processing/types.rs` - `LoadedImage::load()` accepts storage
- `src/gallery/image_processing/formats/*.rs` - Use reader instead of path

### Phase 4: Refactor Gallery Module

**Goal**: Gallery source directories from storage.

**Changes needed:**
- `source_directory` becomes storage URL string
- `scan_directory()` uses `storage.list_recursive()`
- Image paths become storage keys
- Filesystem `mtime` → storage `last_modified`

### Phase 5: Refactor Image Serving

**Goal**: Serve original images from storage.

**Options:**
1. **Stream through server**: `storage.read_stream()` → axum Body
2. **Redirect to signed URL**: S3 presigned URLs for direct download

---

## Phase 10: Gallery Source Storage (Planned)

### Overview

Enable gallery source images to be served from remote storage (S3) in addition to local filesystem.

### Read Strategy Hint

The `SyncStorageReader` uses range reads with prefetching for efficient partial reads. However, when processing images for resize/watermark, we need the entire file.

**Proposed Enhancement:**

```rust
pub enum ReadStrategy {
    /// Use range reads with prefetching (metadata extraction, EXIF, ICC profiles)
    Streaming,
    /// Fetch entire file at once (image resize, tile generation)
    FullFetch,
}

// Usage:
fn open_with_strategy(
    storage: &DynStorage,
    path: &str,
    strategy: ReadStrategy,
    handle: &Handle,
) -> Result<SyncStorageReader, StorageError>
```

**Benefits:**
- `FullFetch` avoids multiple range requests when full content needed
- `Streaming` preserves efficient partial reads for metadata
- S3: Single GET cheaper/faster than multiple range GETs
- Filesystem: Can ignore hint (both fast locally)

### Use Cases

| Operation | Strategy | Reason |
|-----------|----------|--------|
| EXIF extraction | `Streaming` | Only need first few KB |
| Image resize | `FullFetch` | Need entire image |
| ICC profile read | `Streaming` | Profile near start of file |
| Tile generation | `FullFetch` | Need full image for tiles |
| AVIF gain map | `Streaming` | Gain map in headers |

### Implementation

1. Add `ReadStrategy` enum to `src/storage/mod.rs`
2. Add `storage_read_full_sync()` helper
3. Extend `SyncStorageReader::open()` with optional strategy
4. Update `LoadedImage::load()` to use `FullFetch`
5. Update metadata extraction to use `Streaming`

### Files to Modify

- `src/storage/mod.rs` - Add `ReadStrategy` enum
- `src/storage/sync_adapter.rs` - Strategy-aware functions
- `src/gallery/image_processing/types.rs` - Use strategy in `LoadedImage::load()`
- `src/gallery/metadata.rs` - Use `Streaming` for EXIF

---

## Configuration Examples

### Local Filesystem (Default)
```toml
[[galleries]]
name = "main"
source_directory = "photos"
cache_directory = "cache/main"
```

### S3 Storage
```toml
[[galleries]]
name = "main"
source_directory = "s3://my-bucket/photos?region=us-west-2"
cache_directory = "s3://my-bucket/cache/main?region=us-west-2"

[static_files]
directory = "s3://my-bucket/static?region=us-west-2"
use_redirects = true  # Redirect to presigned URLs
```

### Hybrid (Local Source, S3 Cache)
```toml
[[galleries]]
name = "main"
source_directory = "photos"  # Local for fast processing
cache_directory = "s3://my-bucket/cache/main?region=us-west-2"  # S3 for CDN
```

---

## Dependencies

```toml
[dependencies]
async-trait = "0.1"
bytes = "1.0"
futures = "0.3"
aws-sdk-s3 = "1.0"
aws-config = "1.0"
tokio = { version = "1", features = ["full"] }
```

---

## Testing

### Unit Tests
- `src/storage/mod.rs` - Storage trait tests
- `src/storage/filesystem.rs` - Filesystem backend tests
- `src/storage/sync_adapter.rs` - Sync adapter tests

### Integration Tests
- `tests/s3_storage_integration.rs` - S3 backend with localstack/MinIO
- End-to-end gallery tests with S3 source

### Running S3 Tests
```bash
# Start MinIO
docker run -p 9000:9000 -p 9001:9001 minio/minio server /data --console-address ":9001"

# Run S3 tests
TENRANKAI_TEST_S3_ENDPOINT=http://localhost:9000 cargo test s3
```
