use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::permissions::types::RolePermissions;

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub username: String,
    pub email: String,
    pub passkey_count: usize,
}

#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub send_invite: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePermissionsDto {
    #[serde(default)]
    pub can_view: bool,
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
    pub can_edit_any_comments: bool,
    #[serde(default)]
    pub can_delete_any_comments: bool,
    #[serde(default)]
    pub can_set_picks: bool,
    #[serde(default)]
    pub can_add_tags: bool,
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

impl From<&RolePermissions> for RolePermissionsDto {
    fn from(perms: &RolePermissions) -> Self {
        Self {
            can_view: perms.can_view,
            can_see_technical_details: perms.can_see_technical_details,
            can_see_exact_dates: perms.can_see_exact_dates,
            can_see_location: perms.can_see_location,
            can_download_medium: perms.can_download_medium,
            can_download_large: perms.can_download_large,
            can_download_original: perms.can_download_original,
            can_download_gallery: perms.can_download_gallery,
            can_download_raw: perms.can_download_raw,
            can_see_versions: perms.can_see_versions,
            can_read_metadata: perms.can_read_metadata,
            can_edit_content: perms.can_edit_content,
            can_add_comments: perms.can_add_comments,
            can_edit_own_comments: perms.can_edit_own_comments,
            can_delete_own_comments: perms.can_delete_own_comments,
            can_edit_any_comments: perms.can_edit_any_comments,
            can_delete_any_comments: perms.can_delete_any_comments,
            can_set_picks: perms.can_set_picks,
            can_add_tags: perms.can_add_tags,
            can_use_zoom: perms.can_use_zoom,
            can_use_tile_zoom: perms.can_use_tile_zoom,
            can_analyze_images: perms.can_analyze_images,
            can_see_ai_analysis: perms.can_see_ai_analysis,
            can_see_ai_alt_text: perms.can_see_ai_alt_text,
            owner_access: perms.owner_access,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDto {
    pub name: String,
    pub permissions: RolePermissionsDto,
    pub inherits: Option<String>,
    #[serde(default)]
    pub is_builtin: bool,
}

#[derive(Debug, Serialize)]
pub struct RoleListResponse {
    pub roles: Vec<RoleDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRoleAssignment {
    pub username: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfigDto {
    pub public_role: Option<String>,
    pub default_authenticated_role: Option<String>,
    pub roles: HashMap<String, RoleDto>,
    pub user_roles: Vec<UserRoleAssignment>,
}

#[derive(Debug, Serialize)]
pub struct GalleryInfo {
    pub name: String,
    pub url_prefix: String,
    pub permissions: PermissionConfigDto,
}

#[derive(Debug, Serialize)]
pub struct GalleryListResponse {
    pub galleries: Vec<GalleryInfo>,
}

#[derive(Debug, Serialize)]
pub struct PermissionGroup {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PermissionGroupsResponse {
    pub groups: Vec<PermissionGroup>,
}

// ============================================================================
// Request Types for Config Storage Operations
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub permissions: RolePermissionsDto,
    pub inherits: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub permissions: RolePermissionsDto,
    pub inherits: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGalleryPermissionsRequest {
    pub public_role: Option<String>,
    pub default_authenticated_role: Option<String>,
    #[serde(default)]
    pub roles: HashMap<String, RoleDto>,
    #[serde(default)]
    pub user_roles: Vec<UserRoleAssignment>,
}

#[derive(Debug, Deserialize)]
pub struct AssignUserRolesRequest {
    pub roles: Vec<String>,
}

// ============================================================================
// Site Management Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SiteInfo {
    pub name: String,
    pub hostnames: Vec<String>,
    pub base_url: Option<String>,
    pub templates: Vec<String>,
    pub static_files: Vec<String>,
    pub static_use_redirects: bool,
    pub user_database: Option<String>,
    pub storage_prefix: Option<String>,
    pub gallery_count: usize,
    pub posts_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SiteListResponse {
    pub sites: Vec<SiteInfo>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    pub hostnames: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub templates: Option<Vec<String>>,
    pub static_files: Option<Vec<String>>,
    pub static_use_redirects: Option<bool>,
    pub user_database: Option<String>,
    // NOTE: storage_prefix is intentionally NOT included - cannot be edited via API
}

#[derive(Debug, Serialize)]
pub struct SiteGalleryInfo {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: String,
    pub cache_directory: String,
    pub images_per_page: usize,
}

#[derive(Debug, Serialize)]
pub struct SiteGalleryListResponse {
    pub galleries: Vec<SiteGalleryInfo>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGalleryRequest {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: String,
    pub cache_directory: String,
    #[serde(default = "default_images_per_page")]
    pub images_per_page: usize,
}

fn default_images_per_page() -> usize {
    50
}

#[derive(Debug, Deserialize)]
pub struct UpdateGalleryRequest {
    pub url_prefix: Option<String>,
    pub source_directory: Option<String>,
    pub cache_directory: Option<String>,
    pub images_per_page: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ReloadSiteResponse {
    pub success: bool,
    pub message: String,
}
