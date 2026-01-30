use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::permissions::types::RolePermissions;

/// Format bytes as human-readable size (KiB, MiB, GiB, TiB)
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;

    if bytes >= TIB {
        format!("{:.2} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

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
    pub can_edit_any_comments: bool,
    #[serde(default)]
    pub can_delete_any_comments: bool,
    #[serde(default)]
    pub can_manage_images: bool,
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
            can_see_hidden: perms.can_see_hidden,
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
            can_manage_images: perms.can_manage_images,
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
    #[serde(default)]
    pub site_admins: Vec<String>,
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
    pub image_count: usize,
    pub total_size: u64,
    pub total_size_formatted: String,
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
    #[serde(default)]
    pub site_admins: Vec<String>,
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
    pub cache_prefix: Option<String>,
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

/// Watermark position for image watermarks
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkPositionDto {
    BottomLeft,
    #[default]
    BottomRight,
    TopLeft,
    TopRight,
    Center,
    Tiled,
}

impl From<tenrankai_config_storage::StoredWatermarkPosition> for WatermarkPositionDto {
    fn from(pos: tenrankai_config_storage::StoredWatermarkPosition) -> Self {
        match pos {
            tenrankai_config_storage::StoredWatermarkPosition::BottomLeft => Self::BottomLeft,
            tenrankai_config_storage::StoredWatermarkPosition::BottomRight => Self::BottomRight,
            tenrankai_config_storage::StoredWatermarkPosition::TopLeft => Self::TopLeft,
            tenrankai_config_storage::StoredWatermarkPosition::TopRight => Self::TopRight,
            tenrankai_config_storage::StoredWatermarkPosition::Center => Self::Center,
            tenrankai_config_storage::StoredWatermarkPosition::Tiled => Self::Tiled,
        }
    }
}

impl From<WatermarkPositionDto> for tenrankai_config_storage::StoredWatermarkPosition {
    fn from(dto: WatermarkPositionDto) -> Self {
        match dto {
            WatermarkPositionDto::BottomLeft => Self::BottomLeft,
            WatermarkPositionDto::BottomRight => Self::BottomRight,
            WatermarkPositionDto::TopLeft => Self::TopLeft,
            WatermarkPositionDto::TopRight => Self::TopRight,
            WatermarkPositionDto::Center => Self::Center,
            WatermarkPositionDto::Tiled => Self::Tiled,
        }
    }
}

/// Image watermark configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageWatermarkConfigDto {
    pub image: String,
    #[serde(default)]
    pub position: WatermarkPositionDto,
    #[serde(default = "default_watermark_opacity")]
    pub opacity: f32,
    #[serde(default = "default_watermark_scale")]
    pub scale: f32,
    #[serde(default = "default_watermark_padding")]
    pub padding: u32,
    #[serde(default = "default_true")]
    pub adaptive: bool,
    #[serde(default)]
    pub apply_to_gallery: bool,
    #[serde(default = "default_true")]
    pub apply_to_medium: bool,
    #[serde(default)]
    pub apply_to_large: bool,
}

fn default_watermark_opacity() -> f32 {
    0.5
}
fn default_watermark_scale() -> f32 {
    15.0
}
fn default_watermark_padding() -> u32 {
    10
}
fn default_true() -> bool {
    true
}

impl From<tenrankai_config_storage::StoredImageWatermarkConfig> for ImageWatermarkConfigDto {
    fn from(config: tenrankai_config_storage::StoredImageWatermarkConfig) -> Self {
        Self {
            image: config.image,
            position: config.position.into(),
            opacity: config.opacity,
            scale: config.scale,
            padding: config.padding,
            adaptive: config.adaptive,
            apply_to_gallery: config.apply_to_gallery,
            apply_to_medium: config.apply_to_medium,
            apply_to_large: config.apply_to_large,
        }
    }
}

impl From<ImageWatermarkConfigDto> for tenrankai_config_storage::StoredImageWatermarkConfig {
    fn from(dto: ImageWatermarkConfigDto) -> Self {
        Self {
            image: dto.image,
            position: dto.position.into(),
            opacity: dto.opacity,
            scale: dto.scale,
            padding: dto.padding,
            adaptive: dto.adaptive,
            apply_to_gallery: dto.apply_to_gallery,
            apply_to_medium: dto.apply_to_medium,
            apply_to_large: dto.apply_to_large,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SiteGalleryInfo {
    pub name: String,
    pub url_prefix: String,
    pub source_directory: String,
    pub cache_directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copyright_holder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_watermark: Option<ImageWatermarkConfigDto>,
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
    #[serde(default)]
    pub copyright_holder: Option<String>,
    #[serde(default)]
    pub image_watermark: Option<ImageWatermarkConfigDto>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGalleryRequest {
    pub url_prefix: Option<String>,
    pub source_directory: Option<String>,
    pub cache_directory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReloadSiteResponse {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// Theme Management Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeColorSetDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_hover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_muted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_hover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btn_danger_bg: Option<String>,
}

impl From<tenrankai_config_storage::ThemeColorSet> for ThemeColorSetDto {
    fn from(colors: tenrankai_config_storage::ThemeColorSet) -> Self {
        Self {
            bg_primary: colors.bg_primary,
            bg_secondary: colors.bg_secondary,
            bg_card: colors.bg_card,
            bg_hover: colors.bg_hover,
            header_bg: colors.header_bg,
            text_primary: colors.text_primary,
            text_secondary: colors.text_secondary,
            text_muted: colors.text_muted,
            link_color: colors.link_color,
            link_hover: colors.link_hover,
            border_color: colors.border_color,
            accent_color: colors.accent_color,
            btn_danger_bg: colors.btn_danger_bg,
        }
    }
}

impl From<ThemeColorSetDto> for tenrankai_config_storage::ThemeColorSet {
    fn from(dto: ThemeColorSetDto) -> Self {
        Self {
            bg_primary: dto.bg_primary,
            bg_secondary: dto.bg_secondary,
            bg_card: dto.bg_card,
            bg_hover: dto.bg_hover,
            header_bg: dto.header_bg,
            text_primary: dto.text_primary,
            text_secondary: dto.text_secondary,
            text_muted: dto.text_muted,
            link_color: dto.link_color,
            link_hover: dto.link_hover,
            border_color: dto.border_color,
            accent_color: dto.accent_color,
            btn_danger_bg: dto.btn_danger_bg,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleFontConfigDto {
    pub family: String,
    pub weights: Vec<String>,
}

impl From<tenrankai_config_storage::GoogleFontConfig> for GoogleFontConfigDto {
    fn from(font: tenrankai_config_storage::GoogleFontConfig) -> Self {
        Self {
            family: font.family,
            weights: font.weights,
        }
    }
}

impl From<GoogleFontConfigDto> for tenrankai_config_storage::GoogleFontConfig {
    fn from(dto: GoogleFontConfigDto) -> Self {
        Self {
            family: dto.family,
            weights: dto.weights,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeConfigDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_color_scheme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark: Option<ThemeColorSetDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<ThemeColorSetDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_heading: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_mono: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub google_fonts: Vec<GoogleFontConfigDto>,
}

impl From<tenrankai_config_storage::StoredThemeConfig> for ThemeConfigDto {
    fn from(theme: tenrankai_config_storage::StoredThemeConfig) -> Self {
        Self {
            force_color_scheme: theme.force_color_scheme,
            dark: theme.dark.map(Into::into),
            light: theme.light.map(Into::into),
            font_body: theme.font_body,
            font_heading: theme.font_heading,
            font_mono: theme.font_mono,
            google_fonts: theme.google_fonts.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ThemeConfigDto> for tenrankai_config_storage::StoredThemeConfig {
    fn from(dto: ThemeConfigDto) -> Self {
        Self {
            force_color_scheme: dto.force_color_scheme,
            dark: dto.dark.map(Into::into),
            light: dto.light.map(Into::into),
            font_body: dto.font_body,
            font_heading: dto.font_heading,
            font_mono: dto.font_mono,
            google_fonts: dto.google_fonts.into_iter().map(Into::into).collect(),
        }
    }
}

// ============================================================================
// Folder Management Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct FolderInfo {
    pub path: String,
    pub name: String,
    pub has_custom_permissions: bool,
    pub image_count: usize,
    pub size: u64,
    pub size_formatted: String,
}

#[derive(Debug, Serialize)]
pub struct FolderListResponse {
    pub folders: Vec<FolderInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderPermissionsResponse {
    pub hidden: bool,
    #[serde(default)]
    pub hidden_images: Vec<String>,
    pub permissions: PermissionConfigDto,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFolderPermissionsRequest {
    pub hidden: bool,
    #[serde(default)]
    pub hidden_images: Vec<String>,
    pub permissions: PermissionConfigDto,
    pub description: String,
}

// ============================================================================
// Folder Sharing Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ShareFolderRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct ShareFolderResponse {
    pub success: bool,
    pub message: String,
    pub user_created: bool,
}

// ============================================================================
// Image Management Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct DeleteImagesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteImagesResponse {
    pub success: bool,
    pub deleted_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct HideImagesRequest {
    pub paths: Vec<String>,
    pub hide: bool,
}

#[derive(Debug, Serialize)]
pub struct HideImagesResponse {
    pub success: bool,
    pub hidden_images: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateFolderResponse {
    pub success: bool,
    pub folder_path: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveImagesRequest {
    pub paths: Vec<String>,
    pub target_folder: String,
}

#[derive(Debug, Serialize)]
pub struct MoveImagesResponse {
    pub success: bool,
    pub moved_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CopyImagesRequest {
    pub paths: Vec<String>,
    pub target_folder: String,
}

#[derive(Debug, Serialize)]
pub struct CopyImagesResponse {
    pub success: bool,
    pub copied_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FolderImageInfo {
    pub url_id: String,
    pub filename: String,
    pub thumbnail_url: String,
    pub is_hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct FolderImagesResponse {
    pub images: Vec<FolderImageInfo>,
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub new_name: String,
}

#[derive(Debug, Serialize)]
pub struct RenameFolderResponse {
    pub success: bool,
    pub new_path: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteFolderResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct EnsureWatermarkFolderResponse {
    pub folder_path: String,
    pub created: bool,
    pub images: Vec<WatermarkImageInfo>,
}

#[derive(Debug, Serialize)]
pub struct WatermarkImageInfo {
    pub filename: String,
    pub path: String,
}
