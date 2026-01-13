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

## Gallery Source File Access Analysis

Before implementing gallery source storage, we need to understand all file access patterns:

### 1. Directory Scanning (Currently WalkDir - Sync)

| Location | Function | Current API |
|----------|----------|-------------|
| `core.rs:286` | `get_all_images()` | `WalkDir::new()` |
| `core.rs:324` | `collect_hidden_folders()` | `WalkDir::new()` |
| `core.rs:364` | `get_all_images_for_user()` | `WalkDir::new()` |
| `metadata.rs:349` | `background_refresh_metadata()` | `WalkDir::new()` |
| `metadata.rs:395` | `refresh_all_metadata()` | `WalkDir::new()` |

**Migration**: Replace with `storage.list_recursive()` (async).

### 2. Folder Metadata (`_folder.md`)

| Location | Function | Current API |
|----------|----------|-------------|
| `core.rs:732` | `read_folder_metadata_full()` | `tokio::fs::read_to_string` |
| `core.rs:333` | `collect_hidden_folders()` | `std::fs::read_to_string` |

Parses TOML front matter for: `hidden`, `title`, `permissions`.

**Migration**: `storage.read()` - small text files, async.

### 3. Image Markdown Sidecars (`image.jpg.md` or `image.md`)

| Location | Function | Current API |
|----------|----------|-------------|
| `metadata_sources.rs:131` | `read_image_markdown_metadata()` | `tokio::fs::read_to_string` |
| `metadata.rs:557-564` | `is_metadata_stale()` | `tokio::fs::metadata` |

**Migration**: `storage.read()` for content, `storage.metadata()` for mtime.

### 4. XMP Sidecar Files (`image.jpg.xmp`)

| Location | Function | Current API |
|----------|----------|-------------|
| `metadata_sources.rs:10` | `read_xmp_metadata()` | `tokio::fs::read_to_string` |
| `metadata.rs:544-547` | `is_metadata_stale()` | `tokio::fs::metadata` |
| `core.rs:463-470` | `get_item_modification_time()` | `tokio::fs::metadata` |

**Migration**: `storage.read()` for content, `storage.metadata()` for mtime.

### 5. EXIF Extraction (Sync - in spawn_blocking)

| Location | Function | Current API |
|----------|----------|-------------|
| `metadata.rs:54` | `extract_all_exif_data()` | `rexif::parse_file(path)` |
| `avif.rs:1062` | `extract_exif_data()` | `std::fs::read(path)` |

**Migration**: Use `SyncStorageReader` with `Streaming` strategy - only needs ~64KB from start.

### 6. Image Loading (Sync - in spawn_blocking)

| Location | Function | Current API |
|----------|----------|-------------|
| `types.rs:40` | `LoadedImage::load()` | `std::fs::File::open()` |
| `jpeg.rs:10` | `extract_icc_profile()` | `std::fs::File::open()` |
| `png.rs:11` | `extract_icc_profile()` | `std::fs::File::open()` |
| `avif.rs:105` | `read_avif_info()` | `std::fs::read()` |

**Migration**: Use `SyncStorageReader` with `FullFetch` strategy for processing.

### 7. File Metadata (mtime for staleness)

| Location | Function | Current API |
|----------|----------|-------------|
| `core.rs:781,797` | `get_image_metadata_cached()` | `tokio::fs::metadata` |
| `metadata.rs:527` | `is_metadata_stale()` | `tokio::fs::metadata` |
| `resize.rs:464` | `is_cache_valid_by_key()` | `tokio::fs::metadata` |

**Migration**: `storage.metadata()` returns `last_modified`.

---

## Pending Phases

### Phase 3: Refactor LoadedImage

**Goal**: Enable `LoadedImage::load()` to work with storage backends.

**Challenge**: Image decoding requires `Read + Seek`. S3 doesn't support seeking natively.

**Solution**: `SyncStorageReader` with range-based seeking (already implemented):
```rust
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

**Changes:**
1. `LoadedImage::load()` → `LoadedImage::load_from_storage(storage, path, handle)`
2. ICC extractors take `impl Read + Seek` instead of `&Path`
3. AVIF loader uses storage reader

**Files to modify:**
- `src/gallery/image_processing/types.rs`
- `src/gallery/image_processing/formats/jpeg.rs`
- `src/gallery/image_processing/formats/png.rs`
- `src/gallery/image_processing/formats/avif.rs`

### Phase 4: Refactor Gallery Scanning

**Goal**: Gallery source directories from storage.

**Changes:**
1. `source_directory: PathBuf` → `source_directory: String` (storage URL)
2. Add `source_storage: DynStorage` field to `Gallery`
3. Replace `WalkDir` with `storage.list_recursive()`
4. `Path::exists()` → `storage.exists()`
5. `fs::metadata().modified()` → `storage.metadata().last_modified`

**Migration for WalkDir:**
```rust
// Before (sync)
for entry in WalkDir::new(&full_path).into_iter().flatten() {
    if entry.file_type().is_file() { ... }
}

// After (async)
let entries = storage.list_recursive("").await?;
for entry in entries.iter().filter(|e| !e.is_dir) {
    ...
}
```

**Files to modify:**
- `src/config/types.rs` - `source_directory` type
- `src/gallery/mod.rs` - Add `source_storage` field
- `src/gallery/core.rs` - All WalkDir usages
- `src/gallery/metadata.rs` - Refresh functions

### Phase 5: Refactor Metadata & Sidecars

**Goal**: Load XMP, markdown, and folder metadata from storage.

**Changes:**
1. `metadata_sources.rs` functions take storage parameter
2. `read_folder_metadata_full()` uses storage
3. `is_metadata_stale()` uses `storage.metadata()`

**Files to modify:**
- `src/gallery/metadata_sources.rs`
- `src/gallery/core.rs`
- `src/gallery/metadata.rs`

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
