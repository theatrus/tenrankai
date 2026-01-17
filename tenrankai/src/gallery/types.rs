use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Image size variants with high-DPI support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageSize {
    Thumbnail,
    ThumbnailRetina,
    Gallery,
    GalleryRetina,
    Medium,
    MediumRetina,
    Large,
    LargeRetina,
    /// Tile at specific coordinates (x, y) for protected zoom
    Tile(u32, u32),
}

impl ImageSize {
    /// All supported image sizes
    pub const ALL: &'static [ImageSize] = &[
        ImageSize::Thumbnail,
        ImageSize::ThumbnailRetina,
        ImageSize::Gallery,
        ImageSize::GalleryRetina,
        ImageSize::Medium,
        ImageSize::MediumRetina,
        ImageSize::Large,
        ImageSize::LargeRetina,
    ];

    /// Base sizes (without retina variants)
    pub const BASE_SIZES: &'static [ImageSize] = &[
        ImageSize::Thumbnail,
        ImageSize::Gallery,
        ImageSize::Medium,
        ImageSize::Large,
    ];

    /// Sizes that require authentication
    pub const AUTH_REQUIRED: &'static [ImageSize] = &[ImageSize::Large, ImageSize::LargeRetina];

    /// Sizes that support watermarking
    pub const WATERMARK_SIZES: &'static [ImageSize] = &[ImageSize::Medium, ImageSize::MediumRetina];

    /// Get the base size (non-retina variant)
    pub fn base_size(&self) -> ImageSize {
        match self {
            ImageSize::Thumbnail | ImageSize::ThumbnailRetina => ImageSize::Thumbnail,
            ImageSize::Gallery | ImageSize::GalleryRetina => ImageSize::Gallery,
            ImageSize::Medium | ImageSize::MediumRetina => ImageSize::Medium,
            ImageSize::Large | ImageSize::LargeRetina => ImageSize::Large,
            ImageSize::Tile(x, y) => ImageSize::Tile(*x, *y),
        }
    }

    /// Check if this is a retina (@2x) variant
    pub fn is_retina(&self) -> bool {
        matches!(
            self,
            ImageSize::ThumbnailRetina
                | ImageSize::GalleryRetina
                | ImageSize::MediumRetina
                | ImageSize::LargeRetina
        )
    }

    /// Check if this size requires authentication
    pub fn requires_auth(&self) -> bool {
        Self::AUTH_REQUIRED.contains(self)
    }

    /// Check if this size supports watermarking
    pub fn supports_watermark(&self) -> bool {
        Self::WATERMARK_SIZES.contains(self)
    }

    /// Get the retina variant of this size
    pub fn retina_variant(&self) -> ImageSize {
        match self.base_size() {
            ImageSize::Thumbnail => ImageSize::ThumbnailRetina,
            ImageSize::Gallery => ImageSize::GalleryRetina,
            ImageSize::Medium => ImageSize::MediumRetina,
            ImageSize::Large => ImageSize::LargeRetina,
            // These are already base sizes, so they map to their retina variants
            _ => unreachable!(),
        }
    }

    /// Get the multiplier for dimensions (1.0 for normal, 2.0 for retina)
    pub fn multiplier(&self) -> f32 {
        if self.is_retina() { 2.0 } else { 1.0 }
    }

    /// Check if this is a tile variant
    pub fn is_tile(&self) -> bool {
        matches!(self, ImageSize::Tile(_, _))
    }

    /// Parse from string (e.g., "medium", "medium@2x", "tile_5_10")
    pub fn parse(s: &str) -> Option<ImageSize> {
        match s {
            "thumbnail" => Some(ImageSize::Thumbnail),
            "thumbnail@2x" => Some(ImageSize::ThumbnailRetina),
            "gallery" => Some(ImageSize::Gallery),
            "gallery@2x" => Some(ImageSize::GalleryRetina),
            "medium" => Some(ImageSize::Medium),
            "medium@2x" => Some(ImageSize::MediumRetina),
            "large" => Some(ImageSize::Large),
            "large@2x" => Some(ImageSize::LargeRetina),
            _ => {
                // Try to parse tile format "tile_x_y" or "tile_x_y@2x"
                if let Some(stripped) = s.strip_prefix("tile_") {
                    let (tile_part, _is_retina) = if s.ends_with("@2x") {
                        (&stripped[..stripped.len() - 3], true)
                    } else {
                        (stripped, false)
                    };

                    let parts: Vec<&str> = tile_part.split('_').collect();
                    if parts.len() == 2
                        && let (Ok(x), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
                        // For now, we handle retina tiles by returning the same tile
                        // The resize logic will handle the 2x scaling
                        return Some(ImageSize::Tile(x, y));
                    }
                }
                None
            }
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> String {
        match self {
            ImageSize::Thumbnail => "thumbnail".to_string(),
            ImageSize::ThumbnailRetina => "thumbnail@2x".to_string(),
            ImageSize::Gallery => "gallery".to_string(),
            ImageSize::GalleryRetina => "gallery@2x".to_string(),
            ImageSize::Medium => "medium".to_string(),
            ImageSize::MediumRetina => "medium@2x".to_string(),
            ImageSize::Large => "large".to_string(),
            ImageSize::LargeRetina => "large@2x".to_string(),
            ImageSize::Tile(x, y) => format!("tile_{}_{}", x, y),
        }
    }

    /// Get all base size strings for validation messages
    pub fn base_size_names() -> Vec<String> {
        Self::BASE_SIZES.iter().map(|s| s.as_str()).collect()
    }
}

impl std::fmt::Display for ImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ImageSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GalleryItem {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    /// URL path identifier (may be indexed/hashed, not the actual filename)
    pub path: String,
    /// Actual filesystem path relative to source directory (not serialized to frontend)
    #[serde(skip)]
    pub file_path: Option<String>,
    pub parent_path: Option<String>,
    pub is_directory: bool,
    pub thumbnail_url: Option<String>,
    pub gallery_url: Option<String>,
    pub preview_images: Option<Vec<String>>,
    pub item_count: Option<usize>,
    pub dimensions: Option<(u32, u32)>,
    pub capture_date: Option<SystemTime>,
    pub is_new: bool,
    /// User-editable metadata (comments, pick status, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<crate::metadata_storage::ImageUserMetadata>,
}

/// Information about an associated RAW file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFileInfo {
    /// Relative path to the RAW file
    pub path: String,
    /// RAW format extension (e.g., "dng", "arw", "nef")
    pub format: String,
    /// File size in bytes
    pub file_size: u64,
}

/// Information about a version variant of an image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageVersion {
    /// Relative path to this version
    pub path: String,
    /// Version number (extracted from _vN suffix, if present)
    pub version_number: Option<u32>,
    /// Modification date of this version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modification_date: Option<SystemTime>,
    /// Pre-built URL identifier for this version
    pub url_id: String,
    /// Thumbnail URL for version switcher UI
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub name: String,
    pub title: Option<String>,
    pub path: String,
    pub url: String,
    pub thumbnail_url: String,
    pub gallery_url: String,
    pub medium_url: String,
    pub description: Option<String>,
    pub camera_info: Option<CameraInfo>,
    pub location_info: Option<LocationInfo>,
    pub file_size: u64,
    pub dimensions: (u32, u32),
    pub capture_date: Option<String>,
    pub is_new: bool,
    pub color_profile: Option<String>,
    /// User-editable metadata (comments, pick status, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<crate::metadata_storage::ImageUserMetadata>,
    /// Associated RAW files available for download
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_files: Option<Vec<RawFileInfo>>,
    /// Previous versions of this image (sorted oldest-first)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<ImageVersion>>,
    /// Whether this is the primary/current version
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,

    // Astronomical imaging fields
    pub telescope: Option<String>,
    pub mount: Option<String>,
    pub filters: Option<String>,
    pub total_exposure_time: Option<f32>, // in hours
    pub ra: Option<String>,               // Right Ascension
    pub dec: Option<String>,              // Declination

    // Additional technical details that can be set via markdown
    pub additional_details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub google_maps_url: String,
    pub apple_maps_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationImage {
    pub path: String,
    pub name: String,
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GalleryQuery {
    pub page: Option<usize>,
    pub size: Option<String>,
}

// Internal types
#[derive(Serialize, Deserialize)]
pub(crate) struct CacheMetadata {
    pub version: String,
    pub last_full_refresh: SystemTime,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ImageMetadata {
    pub dimensions: (u32, u32),
    pub capture_date: Option<SystemTime>,
    pub camera_info: Option<CameraInfo>,
    pub location_info: Option<LocationInfo>,
    pub modification_date: Option<SystemTime>,
    pub color_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FolderConfig {
    #[serde(default)]
    pub hidden: bool,

    /// Folder-specific permission overrides
    #[serde(default)]
    pub permissions: crate::permissions::types::PermissionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FolderMetadata {
    pub config: FolderConfig,
    pub description_markdown: String,
}

/// Cached folder data with metadata, contents, and pre-computed values
/// Single source of truth for all folder-related data to avoid repeated S3 calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedFolderMetadata {
    // === From _folder.md ===
    /// The parsed folder metadata, or None if no _folder.md exists
    pub metadata: Option<FolderMetadata>,
    /// Last modified time of the _folder.md file (for staleness checks)
    pub metadata_last_modified: Option<std::time::SystemTime>,

    // === Directory contents (direct children only) ===
    /// Names of visible subdirectories (excludes hidden)
    pub subdirectories: Vec<String>,
    /// Paths of images in this folder (relative to gallery root)
    pub images: Vec<String>,

    // === Pre-computed data ===
    /// Total image count including all subdirectories (recursive)
    pub recursive_image_count: usize,
    /// Pre-computed preview items for fast gallery preview API
    pub preview_items: Vec<CachedPreviewItem>,
    /// Image groups (primary + versions + RAW files)
    /// When populated, use this instead of `images` for grouped display
    #[serde(default)]
    pub image_groups: Vec<ImageGroup>,
}

/// Minimal preview item with pre-computed URLs and dimensions
/// Used for fast gallery preview without expensive lookups at request time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedPreviewItem {
    /// Image path relative to gallery root
    pub path: String,
    /// URL identifier (indexed or encoded path)
    pub url_id: String,
    /// Pre-built thumbnail URL
    pub thumbnail_url: String,
    /// Pre-built gallery URL
    pub gallery_url: String,
    /// Image dimensions (for layout)
    pub dimensions: Option<(u32, u32)>,
}

/// A group of related files: primary image + versions + associated RAW files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImageGroup {
    /// The primary image path (newest version, shown in gallery)
    pub primary_path: String,
    /// All image paths in this group (for filtering during listing)
    pub all_image_paths: Vec<String>,
    /// Associated RAW files
    pub raw_files: Vec<RawFileInfo>,
    /// Previous versions (sorted oldest-first, excludes primary)
    pub versions: Vec<ImageVersion>,
    /// Base filename without version suffix (for grouping)
    pub base_name: String,
    /// Hash of primary_path for cache key generation
    pub primary_hash: String,
    /// Latest modification time across all files in group
    pub group_modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ImageMarkdownConfig {
    pub title: Option<String>,

    // Camera/technical info overrides
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,

    // Astronomical fields
    pub telescope: Option<String>,
    pub mount: Option<String>,
    pub filters: Option<String>,
    pub total_exposure_time: Option<f32>,
    pub ra: Option<String>,
    pub dec: Option<String>,

    // Location overrides
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    // Additional info
    pub additional_details: Option<String>,
    pub capture_date: Option<String>, // ISO 8601 format
}

#[derive(Debug, Clone)]
pub struct ImageMarkdownMetadata {
    pub config: ImageMarkdownConfig,
    pub description_markdown: String,
}
