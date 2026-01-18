# Multi-File Image Support

## Overview

Add support for associating multiple files with a single gallery image:
- **RAW files**: Associate .dng, .arw, .crw, .cr2, .nef, .orf, .rw2, .pef, .raf, .srw files with their JPEG counterpart
- **Version variants**: Support `IMG_0001_v1.jpg`, `_v2.jpg` suffixes, with newest shown by default
- **`__versions` subfolder**: Versions can also live in a `__versions/` subfolder
- **Hidden `__` folders**: Folders prefixed with `__` are not shown in gallery browsing
- **RAW downloads**: New permission `can_download_raw` to allow downloading raw files

---

## Phase 1: Data Structures

### New Types (`tenrankai/src/gallery/types.rs`)

```rust
/// Information about an associated RAW file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFileInfo {
    pub path: String,           // Relative path to RAW file
    pub format: String,         // Extension (e.g., "dng", "arw")
    pub file_size: u64,
}

/// Information about a version variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageVersion {
    pub path: String,                       // Relative path
    pub version_number: Option<u32>,        // From _vN suffix
    pub modification_date: Option<SystemTime>,
    pub url_id: String,                     // For direct navigation
    pub thumbnail_url: String,              // For version picker UI
}

/// Group of related files (primary + versions + RAW)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGroup {
    pub primary_path: String,               // Newest version path
    pub all_paths: Vec<String>,             // All paths in group (for filtering)
    pub raw_files: Vec<RawFileInfo>,
    pub versions: Vec<ImageVersion>,        // Sorted oldest-first
    pub base_name: String,                  // Without version suffix
}
```

### Extend `ImageInfo`

```rust
pub struct ImageInfo {
    // ... existing fields ...

    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_files: Option<Vec<RawFileInfo>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<ImageVersion>>,

    #[serde(default)]
    pub is_primary: bool,
}
```

### Extend `CachedFolderMetadata`

```rust
pub(crate) struct CachedFolderMetadata {
    // ... existing fields ...

    /// Image groups (replaces flat image list for grouping)
    pub image_groups: Vec<ImageGroup>,
}
```

---

## Phase 2: Grouping Logic

### New Module (`tenrankai/src/gallery/grouping.rs`)

```rust
pub const RAW_EXTENSIONS: &[&str] = &[
    "dng", "arw", "crw", "cr2", "cr3", "nef",
    "orf", "rw2", "pef", "raf", "srw", "raw"
];

/// Check if extension is RAW format
pub fn is_raw_extension(ext: &str) -> bool;

/// Extract base name: "IMG_0001_v2.jpg" -> "IMG_0001"
pub fn extract_base_name(filename: &str) -> String;

/// Extract version: "IMG_0001_v2.jpg" -> Some(2)
pub fn extract_version_number(filename: &str) -> Option<u32>;

/// Check for __ prefix
pub fn is_hidden_folder(name: &str) -> bool;

/// Group files into ImageGroup structures
pub fn group_files(
    folder_path: &str,
    entries: &[StorageEntry],
) -> Vec<ImageGroup>;
```

**Version suffix pattern**: `_v\d+` before extension (case-insensitive)
- `IMG_0001.jpg` - base image (no version)
- `IMG_0001_v1.jpg` - version 1
- `IMG_0001_V2.jpg` - version 2 (uppercase V)

**Files in `__versions/` folder**:
- All files in `__versions/` are automatically treated as versions
- No `_vN` suffix required - the folder location implies versioning
- Files are matched to parent folder images by base name

**Primary selection logic**:
1. Explicit version numbers (`_vN`) are compared first - highest N wins
2. Files without version numbers use modification time - newest wins
3. `__versions/` files compete equally with parent folder files
4. The newest file overall (by version number or mod time) becomes primary

**Example 1**: `IMG_0001.jpg` + `IMG_0001_v1.jpg` + `IMG_0001_v2.jpg`
- `_v2` is primary (highest explicit version)
- `_v1` is an older version
- `IMG_0001.jpg` (base) is the original/oldest version

**Example 2**: `IMG_0001.jpg` + `__versions/IMG_0001.jpg` (no suffix)
- Compare modification times of both files
- Newer file becomes primary
- If `__versions/IMG_0001.jpg` is newer, it's the primary

**Example 3**: `IMG_0001.jpg` + `IMG_0001.crw` + `__versions/IMG_0001.jpg`
- `IMG_0001.crw` is associated as RAW (regardless of which jpg is primary)
- Compare mod times of the two .jpg files
- Newer .jpg becomes primary, older becomes a version

---

## Phase 3: Permission

### Add to `RolePermissions` (`tenrankai/src/permissions/types.rs`)

```rust
#[serde(default)]
pub can_download_raw: bool,
```

Update `merge()`:
```rust
self.can_download_raw |= other.can_download_raw;
```

Update `apply_owner_override()`:
```rust
self.can_download_raw = true;
```

Update default "contributor" role to include `can_download_raw: true`.

---

## Phase 4: Scanning Updates

### Update `metadata.rs`

**Hidden folder check** - modify `is_effectively_hidden()`:
```rust
// Check for __ prefix in any path component
for component in path.split('/') {
    if component.starts_with("__") {
        return true;
    }
}
```

**Scanning flow**:
1. `list_recursive("")` gets all files
2. Filter out `__` prefixed folders from display (but scan for versions)
3. Group files by base name within each folder
4. Check `__versions/` subfolder for additional versions
5. For each group: extract metadata for primary, size-only for RAW
6. Build `ImageGroup` structures in `CachedFolderMetadata`

### Update `mod.rs`

```rust
/// Check if file is RAW format (for association, not display)
pub(crate) fn is_raw_file(&self, file_name: &str) -> bool;

/// Check if file is any supported format
pub(crate) fn is_supported_file(&self, file_name: &str) -> bool;
```

---

## Phase 5: API Updates

### RAW Download Endpoint

**Route**: `GET /{gallery}/_raw/{path}`

**Handler** (`handlers.rs`):
```rust
pub async fn raw_download_handler(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, path)): Path<(String, String)>,
    auth: OptionalAuth,
) -> Response {
    // Check can_download_raw permission
    // Serve raw file from source storage
}
```

### Update `get_image_info_with_user()` (`core.rs`)

```rust
// Look up ImageGroup for this path
let group = self.find_image_group(relative_path).await;

// Include RAW files if permitted
let raw_files = if permissions.can_download_raw {
    group.map(|g| g.raw_files.clone())
} else {
    None
};

// Include versions
let versions = group.map(|g| g.versions.clone());
```

### Update `scan_directory_with_user()` (`core.rs`)

Iterate `cached.image_groups` instead of `cached.images`:
- Only create `GalleryItem` for primary image in each group
- Versions and RAW files not shown as separate items

---

## Phase 6: Cache Invalidation for Versioned Images

### Problem

Currently, cached images (thumbnails, gallery, medium, large, tiles) use the source file's modification date for staleness detection. When a new version becomes primary, we need to:
1. Detect the change in primary image
2. Invalidate cached previews that were generated from the old primary
3. Regenerate from the new primary

### Solution: Group-Aware Cache Keys

**Current cache key pattern**:
```
{image_path}/{size}.{format}
```

**New cache key pattern** (includes version identifier):
```
{base_name}/{primary_version}/{size}.{format}
```

Or use a hash of the primary path to keep keys shorter:
```
{base_name}/{hash(primary_path)}/{size}.{format}
```

### Cache Invalidation Strategy

**Option A: Version-aware cache paths** (Recommended)
- Cache path includes the primary path hash
- When primary changes, new cache path is used
- Old cached files become orphaned (cleaned up by periodic maintenance)

**Option B: Modification time tracking per group**
- Store `group_last_modified` = max(mod_time of all files in group)
- Compare against cached file timestamp
- Regenerate if source group is newer

### Implementation

**In `CachedFolderMetadata`**:
```rust
pub struct ImageGroup {
    // ... existing ...

    /// Hash of primary_path for cache key generation
    pub primary_hash: String,

    /// Latest modification time across all files in group
    pub group_modified: Option<SystemTime>,
}
```

**In cache generation** (`resizer.rs` or `handlers.rs`):
```rust
// Use group info for cache key
fn get_cache_key(group: &ImageGroup, size: ImageSize) -> String {
    format!("{}/{}/{}", group.base_name, group.primary_hash, size.as_str())
}
```

**During metadata refresh**:
- Compare previous `primary_path` with new `primary_path`
- If changed, the new cache key will be different
- Old cache entries remain but unused (garbage collected later)

### Cache Cleanup (Optional)

Add periodic cleanup of orphaned cache entries:
- Scan cache directory
- Compare against active `ImageGroup` cache keys
- Remove entries not matching any active group

---

## Phase 7: Frontend

### TypeScript Types (`frontend/react/types/index.ts`)

```typescript
export interface RawFileInfo {
  path: string;
  format: string;
  file_size: number;
}

export interface ImageVersion {
  path: string;
  version_number?: number;
  modification_date?: string;
  url_id: string;
  thumbnail_url: string;
}

export interface RolePermissions {
  // ... existing ...
  can_download_raw: boolean;
}

export interface ImageInfo {
  // ... existing ...
  raw_files?: RawFileInfo[];
  versions?: ImageVersion[];
  is_primary?: boolean;
}
```

### Version Picker Component

New `VersionPicker.tsx`:
- Shows thumbnail strip of all versions
- Current version highlighted
- Click to navigate to different version

### RAW Download UI

In `ImageControls.tsx`:
- Show RAW download buttons when `permissions.can_download_raw && raw_files?.length`
- Display format and file size

---

## Files to Modify

| File | Changes |
|------|---------|
| `tenrankai/src/gallery/types.rs` | Add RawFileInfo, ImageVersion, ImageGroup with primary_hash |
| `tenrankai/src/gallery/mod.rs` | Add is_raw_file(), is_supported_file() |
| `tenrankai/src/gallery/grouping.rs` | NEW - grouping logic |
| `tenrankai/src/gallery/metadata.rs` | __ folder filtering, group-aware scanning |
| `tenrankai/src/gallery/core.rs` | Use image_groups, include versions/raw in ImageInfo |
| `tenrankai/src/gallery/handlers.rs` | Add raw_download_handler, use group cache keys |
| `tenrankai/src/gallery/indexing.rs` | Index primary + versions, skip RAW |
| `tenrankai/src/gallery/resizer.rs` | Use group-aware cache key generation |
| `tenrankai/src/permissions/types.rs` | Add can_download_raw |
| `tenrankai/src/lib.rs` | Add /_raw route |
| `frontend/react/types/index.ts` | Add new types |
| `frontend/react/components/ImageDetail/` | Version picker, RAW downloads |

---

## Testing

### Unit Tests

- `grouping.rs`: base name extraction, version parsing, file grouping
- `permissions`: can_download_raw merging and override

### Integration Tests

```rust
#[tokio::test]
async fn test_version_grouping_same_folder();

#[tokio::test]
async fn test_version_grouping_versions_subfolder();

#[tokio::test]
async fn test_raw_file_association();

#[tokio::test]
async fn test_double_underscore_folder_hidden();

#[tokio::test]
async fn test_raw_download_with_permission();

#[tokio::test]
async fn test_raw_download_denied();

#[tokio::test]
async fn test_cache_invalidation_on_version_change();

#[tokio::test]
async fn test_versions_folder_no_suffix_uses_mod_time();
```

### Test Fixtures

```
test_photos/
├── IMG_0001.jpg           # Primary (no versions, has RAW)
├── IMG_0001.dng           # Associated RAW
├── IMG_0002.jpg           # Original/oldest version
├── IMG_0002_v1.jpg        # Version 1 (primary if no _v2)
├── IMG_0002_v2.jpg        # Version 2 (primary - highest explicit version)
├── IMG_0003.jpg           # Original, versions in subfolder with _vN suffix
├── IMG_0004.jpg           # Base image (older mod time)
├── IMG_0004.crw           # Associated RAW
├── __versions/
│   ├── IMG_0003_v1.jpg    # Version 1 (explicit suffix)
│   ├── IMG_0003_v2.jpg    # Primary (highest explicit version)
│   └── IMG_0004.jpg       # No suffix - primary if newer than parent IMG_0004.jpg
└── __hidden/
    └── secret.jpg         # Should not appear in gallery
```

**Test case for mod-time based versioning**:
- Set `IMG_0004.jpg` mod time to `2024-01-01`
- Set `__versions/IMG_0004.jpg` mod time to `2024-06-01`
- Expected: `__versions/IMG_0004.jpg` is primary (newer)
- `IMG_0004.crw` associated with whichever jpg is primary

---

## Verification

```bash
# Build and test
cargo clippy --no-default-features -- -D warnings
cargo test --no-default-features
npm run type-check && npm run build

# Manual testing
cargo run --no-default-features -- serve

# Test cases:
# 1. Upload IMG_0001.jpg + IMG_0001.dng - verify grouped
# 2. Upload IMG_0002.jpg + IMG_0002_v1.jpg - verify _v1 is primary, base is older version
# 3. Create __versions/ folder with versions - verify grouping
# 4. Create __hidden/ folder - verify not displayed
# 5. Test RAW download with/without permission
# 6. Add IMG_0002_v2.jpg - verify thumbnails regenerate with _v2 as new primary
```
