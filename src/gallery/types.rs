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

    /// Parse from string (e.g., "medium", "medium@2x")
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
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageSize::Thumbnail => "thumbnail",
            ImageSize::ThumbnailRetina => "thumbnail@2x",
            ImageSize::Gallery => "gallery",
            ImageSize::GalleryRetina => "gallery@2x",
            ImageSize::Medium => "medium",
            ImageSize::MediumRetina => "medium@2x",
            ImageSize::Large => "large",
            ImageSize::LargeRetina => "large@2x",
        }
    }

    /// Get all base size strings for validation messages
    pub fn base_size_names() -> Vec<&'static str> {
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
    pub path: String,
    pub parent_path: Option<String>,
    pub is_directory: bool,
    pub thumbnail_url: Option<String>,
    pub gallery_url: Option<String>,
    pub preview_images: Option<Vec<String>>,
    pub item_count: Option<usize>,
    pub dimensions: Option<(u32, u32)>,
    pub capture_date: Option<SystemTime>,
    pub is_new: bool,
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

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FolderConfig {
    #[serde(default)]
    pub hidden: bool,
    pub title: Option<String>,

    // Access control fields
    #[serde(default)]
    pub require_auth: bool,
    pub allowed_users: Option<Vec<String>>,

    // Location privacy fields
    /// When true, hide location/GPS information from non-authenticated users for images in this folder
    #[serde(default)]
    pub hide_location_from_public: Option<bool>,

    // Technical details privacy field
    /// When true, hide technical details (camera info, metadata, etc.) from the image detail page
    #[serde(default)]
    pub hide_technical_details: bool,

    // Image indexing mode for this folder (overrides gallery default)
    #[allow(dead_code)]
    pub image_indexing: Option<crate::config::ImageIndexingMode>,

    // Metadata features control
    /// Override gallery-level metadata setting for this folder
    pub enable_metadata: Option<bool>,

    // Permission configuration for this folder
    /// Folder-specific permission overrides
    #[serde(default)]
    pub permissions: crate::permissions::types::PermissionConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct FolderMetadata {
    pub config: FolderConfig,
    pub description_markdown: String,
}
