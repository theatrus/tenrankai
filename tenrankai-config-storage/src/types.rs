use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Permissions that can be assigned to a role
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RolePermissions {
    #[serde(default)]
    pub can_view: bool,
    #[serde(default)]
    pub can_see_hidden: bool,
    #[serde(default)]
    pub can_see_technical_details: bool,
    #[serde(default)]
    pub can_see_exact_dates: bool,
    #[serde(default)]
    pub can_see_location: bool,

    #[serde(default)]
    pub can_download_medium: bool,
    #[serde(default)]
    pub can_download_large: bool,
    #[serde(default)]
    pub can_download_original: bool,
    #[serde(default)]
    pub can_download_gallery: bool,
    #[serde(default)]
    pub can_download_raw: bool,

    #[serde(default)]
    pub can_see_versions: bool,

    #[serde(default)]
    pub can_read_metadata: bool,
    #[serde(default)]
    pub can_edit_content: bool,
    #[serde(default)]
    pub can_add_comments: bool,
    #[serde(default)]
    pub can_edit_own_comments: bool,
    #[serde(default)]
    pub can_delete_own_comments: bool,
    #[serde(default)]
    pub can_set_picks: bool,
    #[serde(default)]
    pub can_add_tags: bool,

    #[serde(default)]
    pub can_edit_any_comments: bool,
    #[serde(default)]
    pub can_delete_any_comments: bool,

    #[serde(default)]
    pub can_manage_images: bool,

    #[serde(default)]
    pub can_use_zoom: bool,
    #[serde(default)]
    pub can_use_tile_zoom: bool,

    #[serde(default)]
    pub can_analyze_images: bool,
    #[serde(default)]
    pub can_see_ai_analysis: bool,
    #[serde(default)]
    pub can_see_ai_alt_text: bool,

    #[serde(default)]
    pub owner_access: bool,
}

/// A role definition with permissions and optional inheritance
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Role {
    pub permissions: RolePermissions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>,
}

/// User to roles mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserRole {
    pub username: String,
    pub roles: Vec<String>,
}

/// Gallery-level permission configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GalleryPermissionConfig {
    /// Site-level administrators (bypass gallery permission checks for admin access)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub site_admins: Vec<String>,

    /// Role for unauthenticated users (or "none" for no access)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_role: Option<String>,

    /// Default role for authenticated users without explicit assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_authenticated_role: Option<String>,

    /// Custom role definitions
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<String, Role>,

    /// User-role assignments
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_roles: Vec<UserRole>,
}

// ============================================================================
// Site Configuration Types
// ============================================================================

/// Stored site configuration
/// The storage_prefix field is NOT editable via admin API for security
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StoredSiteConfig {
    /// Hostnames that route to this site
    /// Special values: "*" (catch-all), "*.example.com" (wildcard)
    #[serde(default)]
    pub hostnames: Vec<String>,

    /// Template directories for this site
    #[serde(default)]
    pub templates: Vec<String>,

    /// Static file directories for this site
    #[serde(default)]
    pub static_files: Vec<String>,

    /// Whether to use signed URL redirects for S3 static files
    #[serde(default)]
    pub static_use_redirects: bool,

    /// User database URL/path for this site
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_database: Option<String>,

    /// Base URL for this site
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Cookie secret override for this site
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie_secret: Option<String>,

    /// Storage prefix for gallery/posts source paths
    /// All source directories are relative to this prefix
    /// NOT editable via admin API - must be set manually in site.toml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_prefix: Option<String>,

    /// Cache prefix for gallery cache paths
    /// All cache directories are relative to this prefix
    /// Defaults to current working directory if not set
    /// NOT editable via admin API - must be set manually in site.toml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_prefix: Option<String>,

    /// Per-site email sender configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<StoredSiteEmailConfig>,
}

/// Per-site email sender configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredSiteEmailConfig {
    pub from_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// Stored gallery configuration (full gallery settings)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredGalleryConfig {
    pub name: String,

    #[serde(default)]
    pub url_prefix: String,

    /// Source directory (relative to site's storage_prefix)
    pub source_directory: String,

    /// Cache directory (relative to site's storage_prefix)
    pub cache_directory: String,

    #[serde(default = "default_gallery_template")]
    pub gallery_template: String,

    #[serde(default = "default_image_detail_template")]
    pub image_detail_template: String,

    #[serde(default = "default_thumbnail_size")]
    pub thumbnail: StoredImageSizeConfig,

    #[serde(default = "default_gallery_size")]
    pub gallery_size: StoredImageSizeConfig,

    #[serde(default = "default_medium_size")]
    pub medium: StoredImageSizeConfig,

    #[serde(default = "default_large_size")]
    pub large: StoredImageSizeConfig,

    #[serde(default)]
    pub cache_refresh_interval_minutes: Option<u64>,

    #[serde(default)]
    pub jpeg_quality: Option<u8>,

    #[serde(default)]
    pub webp_quality: Option<f32>,

    #[serde(default)]
    pub new_threshold_days: Option<u32>,

    #[serde(default)]
    pub copyright_holder: Option<String>,

    #[serde(default = "default_image_indexing")]
    pub image_indexing: String,

    #[serde(default = "default_metadata_cache_size")]
    pub metadata_cache_size: usize,

    /// Tile configuration for protected zoom
    #[serde(default)]
    pub tiles: Option<StoredTileConfig>,

    /// Pre-generation configuration
    #[serde(default)]
    pub pregenerate: Option<StoredPregenerateConfig>,

    /// Preview configuration
    #[serde(default)]
    pub preview: Option<StoredPreviewConfig>,
}

/// Image size configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredImageSizeConfig {
    pub width: u32,
    pub height: u32,
}

/// Tile configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredTileConfig {
    #[serde(default = "default_tile_size")]
    pub tile_size: u32,
}

/// Pre-generation configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredPregenerateConfig {
    #[serde(default)]
    pub formats: StoredPregenerateFormats,
    #[serde(default)]
    pub sizes: StoredPregenerateSizes,
    #[serde(default)]
    pub tiles: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StoredPregenerateFormats {
    #[serde(default = "default_true")]
    pub jpeg: bool,
    #[serde(default = "default_true")]
    pub webp: bool,
    #[serde(default)]
    pub avif: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StoredPregenerateSizes {
    #[serde(default = "default_true")]
    pub thumbnail: bool,
    #[serde(default = "default_true")]
    pub gallery: bool,
    #[serde(default = "default_true")]
    pub medium: bool,
    #[serde(default)]
    pub large: bool,
}

/// Preview configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredPreviewConfig {
    #[serde(default = "default_preview_max_images")]
    pub max_images: usize,
    #[serde(default = "default_preview_max_depth")]
    pub max_depth: usize,
    #[serde(default = "default_preview_max_per_folder")]
    pub max_per_folder: usize,
}

/// Stored posts configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredPostsConfig {
    pub name: String,

    /// Source directory (relative to site's storage_prefix)
    pub source_directory: String,

    pub url_prefix: String,

    #[serde(default = "default_posts_index_template")]
    pub index_template: String,

    #[serde(default = "default_posts_detail_template")]
    pub post_template: String,

    #[serde(default = "default_posts_per_page")]
    pub posts_per_page: usize,

    #[serde(default)]
    pub refresh_interval_minutes: Option<u64>,
}

// Default value functions
fn default_gallery_template() -> String {
    "modules/gallery.html.liquid".to_string()
}
fn default_image_detail_template() -> String {
    "modules/image_detail.html.liquid".to_string()
}
fn default_thumbnail_size() -> StoredImageSizeConfig {
    StoredImageSizeConfig {
        width: 300,
        height: 300,
    }
}
fn default_gallery_size() -> StoredImageSizeConfig {
    StoredImageSizeConfig {
        width: 800,
        height: 800,
    }
}
fn default_medium_size() -> StoredImageSizeConfig {
    StoredImageSizeConfig {
        width: 1200,
        height: 1200,
    }
}
fn default_large_size() -> StoredImageSizeConfig {
    StoredImageSizeConfig {
        width: 1600,
        height: 1600,
    }
}
fn default_image_indexing() -> String {
    "filename".to_string()
}
fn default_metadata_cache_size() -> usize {
    1000
}
fn default_tile_size() -> u32 {
    1024
}
fn default_true() -> bool {
    true
}
fn default_preview_max_images() -> usize {
    4
}
fn default_preview_max_depth() -> usize {
    2
}
fn default_preview_max_per_folder() -> usize {
    2
}
fn default_posts_index_template() -> String {
    "modules/posts_index.html.liquid".to_string()
}
fn default_posts_detail_template() -> String {
    "modules/post_detail.html.liquid".to_string()
}
fn default_posts_per_page() -> usize {
    10
}

// ============================================================================
// Audit Types
// ============================================================================

/// Audit log entry for tracking changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO 8601 timestamp
    pub ts: String,

    /// Username who made the change
    pub user: String,

    /// Action performed
    pub action: AuditAction,

    /// Target of the action (gallery name, role name, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Change details (old value -> new value for modified fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // Permission actions (existing)
    SetGalleryConfig,
    DeleteGalleryConfig,
    SetRole,
    DeleteRole,
    SetUserRoles,
    // Site management actions (new)
    SetSiteConfig,
    DeleteSite,
    SetGalleryFullConfig,
    DeleteGallery,
    SetPostsConfig,
    DeletePosts,
}

impl AuditEntry {
    pub fn new(user: impl Into<String>, action: AuditAction) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339(),
            user: user.into(),
            action,
            target: None,
            changes: None,
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_changes(mut self, changes: serde_json::Value) -> Self {
        self.changes = Some(changes);
        self
    }
}
