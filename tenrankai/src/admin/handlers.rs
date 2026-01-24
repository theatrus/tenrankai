use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use super::error::AdminError;
use super::extractors::RequireAdmin;
use super::types::*;
use crate::cache::queue::{CacheCleanupRequest, CacheGenerationRequest};
use crate::gallery::path_utils::{self, SidecarPaths};
use crate::permissions::types::RolePermissions;
use crate::site::ResolvedState;
use crate::storage::DynStorage;

// ============================================================================
// Helper Functions for File Operations
// ============================================================================

/// Find all sidecar files that exist for a given image path.
/// Returns paths that actually exist in storage.
async fn find_existing_sidecars(storage: &DynStorage, image_path: &str) -> Vec<String> {
    let sidecars = SidecarPaths::for_image(image_path);
    let mut existing = Vec::new();

    // Check XMP sidecar
    if storage.exists(&sidecars.xmp).await.unwrap_or(false) {
        existing.push(sidecars.xmp);
    }

    // Check markdown sidecars
    if storage
        .exists(&sidecars.markdown_full)
        .await
        .unwrap_or(false)
    {
        existing.push(sidecars.markdown_full);
    }
    if storage
        .exists(&sidecars.markdown_replaced)
        .await
        .unwrap_or(false)
    {
        existing.push(sidecars.markdown_replaced);
    }

    existing
}

/// Generate destination sidecar paths from source sidecars.
/// Transforms source paths to destination folder while preserving sidecar type.
fn transform_sidecar_paths(
    source_sidecars: &[String],
    source_image: &str,
    dest_image: &str,
) -> Vec<(String, String)> {
    let source_sidecars_template = SidecarPaths::for_image(source_image);
    let dest_sidecars_template = SidecarPaths::for_image(dest_image);

    source_sidecars
        .iter()
        .filter_map(|src| {
            if *src == source_sidecars_template.xmp {
                Some((src.clone(), dest_sidecars_template.xmp.clone()))
            } else if *src == source_sidecars_template.markdown_full {
                Some((src.clone(), dest_sidecars_template.markdown_full.clone()))
            } else if *src == source_sidecars_template.markdown_replaced {
                Some((
                    src.clone(),
                    dest_sidecars_template.markdown_replaced.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Find version files for an image in the __versions folder.
/// Returns list of (source_path, filename) tuples.
async fn find_version_files(
    storage: &DynStorage,
    folder_path: &str,
    filename: &str,
) -> Vec<(String, String)> {
    let stem = path_utils::filename(filename);
    let base_name = crate::gallery::grouping::extract_base_name(stem);

    // Check __versions folder
    let versions_folder = if folder_path.is_empty() {
        "__versions".to_string()
    } else {
        format!("{}/__versions", folder_path)
    };

    let mut version_files = Vec::new();

    // List files in __versions folder
    if let Ok(entries) = storage.list(&versions_folder).await {
        for entry in entries {
            if !entry.is_dir {
                let entry_filename = path_utils::filename(&entry.path);
                let entry_base = crate::gallery::grouping::extract_base_name(entry_filename);

                // Check if this file belongs to the same image group
                if entry_base == base_name {
                    version_files.push((entry.path.clone(), entry_filename.to_string()));
                }
            }
        }
    }

    version_files
}

/// Serve the admin SPA HTML
pub async fn admin_spa_handler(ResolvedState(app_state): ResolvedState) -> Response {
    // Try to serve the admin index.html from static files
    let response = app_state
        .static_handler()
        .serve("admin/index.html", false)
        .await;

    // Check if file was found (not 404)
    if response.status() != StatusCode::NOT_FOUND {
        return response;
    }

    // Fallback HTML if admin app not built
    Html(
        r#"<!DOCTYPE html>
<html>
<head><title>Admin UI Not Found</title></head>
<body>
<h1>Admin UI Not Available</h1>
<p>The admin UI has not been built. Run <code>npm run build:admin</code> to build it.</p>
</body>
</html>"#,
    )
    .into_response()
}

// ============================================================================
// User Management
// ============================================================================

/// List all users
pub async fn list_users(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
) -> Result<Json<UserListResponse>, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    let users = user_storage
        .list_users()
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    let user_infos: Vec<UserInfo> = users
        .into_iter()
        .map(|(username, user)| UserInfo {
            username,
            email: user.email,
            passkey_count: user.passkeys.len(),
        })
        .collect();

    Ok(Json(UserListResponse { users: user_infos }))
}

/// Get a single user
pub async fn get_user(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(username): Path<String>,
) -> Result<Json<UserInfo>, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    let user = user_storage
        .get_user(&username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("User not found: {}", username)))?;

    Ok(Json(UserInfo {
        username,
        email: user.email,
        passkey_count: user.passkeys.len(),
    }))
}

/// Create a new user
pub async fn create_user(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserInfo>, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    // Validate username
    let username = request.username.to_lowercase();
    if username.len() < 3 || username.len() > 32 {
        return Err(AdminError::BadRequest(
            "Username must be 3-32 characters".into(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AdminError::BadRequest(
            "Username must contain only alphanumeric characters and underscores".into(),
        ));
    }

    // Create user
    let user = tenrankai_users::User {
        email: request.email.clone(),
        passkeys: vec![],
    };

    user_storage.add_user(&username, &user).await?;

    // Send invite if requested
    if request.send_invite {
        let token = {
            let mut login_state = app_state.login_state().write().await;
            login_state.create_invite_token(username.clone())
        };

        let base_url = app_state.base_url().unwrap_or("http://localhost:3000");
        let login_url = format!("{}/_login/verify?token={}", base_url, token);

        // Send the email if provider is configured
        if let Some(email_provider) = &app_state.email_provider
            && let Some(email_config) = app_state.email_config()
        {
            let mut email_message = crate::email::EmailMessage::new(
                &request.email,
                email_config.format_from(),
                format!("You've been invited to {}", app_state.app_name()),
            );

            if let Some(reply_to) = &email_config.reply_to {
                email_message = email_message.with_reply_to(reply_to);
            }

            email_message = email_message.with_both(
                format!(
                    "You've been invited to {}.\n\nClick this link to login:\n\n{}\n\nThis link will expire in 72 hours.",
                    app_state.app_name(), login_url
                ),
                format!(
                    r#"<p>You've been invited to {}.</p>
<p>Click this link to login:</p>
<p><a href="{}">{}</a></p>
<p>This link will expire in 72 hours.</p>"#,
                    app_state.app_name(), login_url, login_url
                ),
            );

            let _ = email_provider.send_email(email_message).await;
        }
    }

    Ok(Json(UserInfo {
        username,
        email: request.email,
        passkey_count: 0,
    }))
}

/// Update a user
pub async fn update_user(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(username): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserInfo>, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    let mut user = user_storage
        .get_user(&username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("User not found: {}", username)))?;

    if let Some(email) = request.email {
        user.email = email;
    }

    user_storage
        .update_user(&username, &user)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    Ok(Json(UserInfo {
        username,
        email: user.email,
        passkey_count: user.passkeys.len(),
    }))
}

/// Delete a user
pub async fn delete_user(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(username): Path<String>,
) -> Result<StatusCode, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    let removed = user_storage
        .remove_user(&username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AdminError::NotFound(format!(
            "User not found: {}",
            username
        )))
    }
}

/// Send a login invite to a user
pub async fn send_invite(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(username): Path<String>,
) -> Result<StatusCode, AdminError> {
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    let user = user_storage
        .get_user(&username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("User not found: {}", username)))?;

    let token = {
        let mut login_state = app_state.login_state().write().await;
        login_state.create_invite_token(username.clone())
    };

    let base_url = app_state.base_url().unwrap_or("http://localhost:3000");
    let login_url = format!("{}/_login/verify?token={}", base_url, token);

    // Send the email - require both provider and config
    let email_provider = app_state
        .email_provider
        .as_ref()
        .ok_or_else(|| AdminError::Internal("Email provider not configured".into()))?;
    let email_config = app_state
        .email_config()
        .ok_or_else(|| AdminError::Internal("Email not configured".into()))?;

    let mut email_message = crate::email::EmailMessage::new(
        &user.email,
        email_config.format_from(),
        format!("Login invitation for {}", app_state.app_name()),
    );

    if let Some(reply_to) = &email_config.reply_to {
        email_message = email_message.with_reply_to(reply_to);
    }

    email_message = email_message.with_both(
        format!(
            "Click this link to login to {}:\n\n{}\n\nThis link will expire in 72 hours.",
            app_state.app_name(),
            login_url
        ),
        format!(
            r#"<p>Click this link to login to {}:</p>
<p><a href="{}">{}</a></p>
<p>This link will expire in 72 hours.</p>"#,
            app_state.app_name(),
            login_url,
            login_url
        ),
    );

    email_provider
        .send_email(email_message)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to send email: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Gallery Management
// ============================================================================

/// List all galleries
pub async fn list_galleries(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
) -> Result<Json<GalleryListResponse>, AdminError> {
    let mut galleries = Vec::new();

    for (name, gallery) in app_state.galleries().iter() {
        let config = gallery.get_config();
        let permissions = &config.permissions;

        // Get size and count from root folder cache
        let (image_count, total_size) = gallery
            .folder_cache
            .get("")
            .await
            .map(|cache| (cache.recursive_image_count, cache.recursive_size))
            .unwrap_or((0, 0));

        galleries.push(GalleryInfo {
            name: name.clone(),
            url_prefix: config.url_prefix.clone(),
            permissions: PermissionConfigDto {
                site_admins: permissions.site_admins.clone(),
                public_role: permissions.public_role.clone(),
                default_authenticated_role: permissions.default_authenticated_role.clone(),
                roles: permissions
                    .roles
                    .iter()
                    .map(|(role_name, role)| {
                        (
                            role_name.clone(),
                            RoleDto {
                                name: role_name.clone(),
                                permissions: RolePermissionsDto::from(&role.permissions),
                                inherits: role.inherits.clone(),
                                is_builtin: false,
                            },
                        )
                    })
                    .collect(),
                user_roles: permissions
                    .user_roles
                    .iter()
                    .map(|user_role| UserRoleAssignment {
                        username: user_role.username.clone(),
                        roles: user_role.roles.clone(),
                    })
                    .collect(),
            },
            image_count,
            total_size,
            total_size_formatted: format_size(total_size),
        });
    }

    Ok(Json(GalleryListResponse { galleries }))
}

/// Get a single gallery
pub async fn get_gallery(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(name): Path<String>,
) -> Result<Json<GalleryInfo>, AdminError> {
    let gallery = app_state
        .galleries()
        .get(&name)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", name)))?;

    let config = gallery.get_config();
    let permissions = &config.permissions;

    // Get size and count from root folder cache
    let (image_count, total_size) = gallery
        .folder_cache
        .get("")
        .await
        .map(|cache| (cache.recursive_image_count, cache.recursive_size))
        .unwrap_or((0, 0));

    Ok(Json(GalleryInfo {
        name,
        url_prefix: config.url_prefix.clone(),
        permissions: PermissionConfigDto {
            site_admins: permissions.site_admins.clone(),
            public_role: permissions.public_role.clone(),
            default_authenticated_role: permissions.default_authenticated_role.clone(),
            roles: permissions
                .roles
                .iter()
                .map(|(role_name, role)| {
                    (
                        role_name.clone(),
                        RoleDto {
                            name: role_name.clone(),
                            permissions: RolePermissionsDto::from(&role.permissions),
                            inherits: role.inherits.clone(),
                            is_builtin: false,
                        },
                    )
                })
                .collect(),
            user_roles: permissions
                .user_roles
                .iter()
                .map(|user_role| UserRoleAssignment {
                    username: user_role.username.clone(),
                    roles: user_role.roles.clone(),
                })
                .collect(),
        },
        image_count,
        total_size,
        total_size_formatted: format_size(total_size),
    }))
}

// ============================================================================
// Role Management
// ============================================================================

/// Create viewer permissions (basic read-only access)
fn viewer_permissions() -> RolePermissions {
    RolePermissions {
        can_view: true,
        can_see_technical_details: true,
        can_see_exact_dates: true,
        can_read_metadata: true,
        can_use_zoom: true,
        ..Default::default()
    }
}

/// Create contributor permissions (view + some editing capabilities)
fn contributor_permissions() -> RolePermissions {
    RolePermissions {
        can_view: true,
        can_see_technical_details: true,
        can_see_exact_dates: true,
        can_see_location: true,
        can_download_medium: true,
        can_download_large: true,
        can_read_metadata: true,
        can_add_comments: true,
        can_edit_own_comments: true,
        can_delete_own_comments: true,
        can_set_picks: true,
        can_add_tags: true,
        can_use_zoom: true,
        can_see_ai_analysis: true,
        can_see_ai_alt_text: true,
        ..Default::default()
    }
}

/// List all available roles (built-in only)
/// Note: Custom roles are now managed through site permissions
pub async fn list_roles(
    ResolvedState(_app_state): ResolvedState,
    _admin: RequireAdmin,
) -> Result<Json<RoleListResponse>, AdminError> {
    // Built-in roles only - custom roles are now in site permissions
    let roles = vec![
        RoleDto {
            name: "viewer".to_string(),
            permissions: RolePermissionsDto::from(&viewer_permissions()),
            inherits: None,
            is_builtin: true,
        },
        RoleDto {
            name: "contributor".to_string(),
            permissions: RolePermissionsDto::from(&contributor_permissions()),
            inherits: None,
            is_builtin: true,
        },
        RoleDto {
            name: "admin".to_string(),
            permissions: RolePermissionsDto {
                owner_access: true,
                ..RolePermissionsDto::from(&RolePermissions::default())
            },
            inherits: None,
            is_builtin: true,
        },
    ];

    Ok(Json(RoleListResponse { roles }))
}

/// Get a specific role (built-in only)
/// Note: Custom roles are now managed through site permissions
pub async fn get_role(
    ResolvedState(_app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(name): Path<String>,
) -> Result<Json<RoleDto>, AdminError> {
    // Built-in roles only - custom roles are now in site permissions
    match name.as_str() {
        "viewer" => Ok(Json(RoleDto {
            name: "viewer".to_string(),
            permissions: RolePermissionsDto::from(&viewer_permissions()),
            inherits: None,
            is_builtin: true,
        })),
        "contributor" => Ok(Json(RoleDto {
            name: "contributor".to_string(),
            permissions: RolePermissionsDto::from(&contributor_permissions()),
            inherits: None,
            is_builtin: true,
        })),
        "admin" => Ok(Json(RoleDto {
            name: "admin".to_string(),
            permissions: RolePermissionsDto {
                owner_access: true,
                ..RolePermissionsDto::from(&RolePermissions::default())
            },
            inherits: None,
            is_builtin: true,
        })),
        _ => Err(AdminError::NotFound(format!(
            "Role not found: {} (custom roles are now managed through site permissions)",
            name
        ))),
    }
}

/// Create a new custom role
/// Note: Custom roles are now managed through site permissions (/_admin/api/sites/{site}/permissions)
pub async fn create_role(
    ResolvedState(_app_state): ResolvedState,
    _admin: RequireAdmin,
    Json(_request): Json<CreateRoleRequest>,
) -> Result<Json<RoleDto>, AdminError> {
    Err(AdminError::BadRequest(
        "Custom roles are now managed through site permissions. Use /_admin/api/sites/{site}/permissions instead.".into(),
    ))
}

/// Update an existing custom role
/// Note: Custom roles are now managed through site permissions (/_admin/api/sites/{site}/permissions)
pub async fn update_role(
    ResolvedState(_app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(name): Path<String>,
    Json(_request): Json<UpdateRoleRequest>,
) -> Result<Json<RoleDto>, AdminError> {
    // Don't allow modifying built-in roles
    if matches!(name.as_str(), "viewer" | "contributor" | "admin") {
        return Err(AdminError::BadRequest(
            "Cannot modify built-in roles".into(),
        ));
    }

    Err(AdminError::BadRequest(
        "Custom roles are now managed through site permissions. Use /_admin/api/sites/{site}/permissions instead.".into(),
    ))
}

/// Delete a custom role
/// Note: Custom roles are now managed through site permissions (/_admin/api/sites/{site}/permissions)
pub async fn delete_role(
    ResolvedState(_app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(name): Path<String>,
) -> Result<StatusCode, AdminError> {
    // Don't allow deleting built-in roles
    if matches!(name.as_str(), "viewer" | "contributor" | "admin") {
        return Err(AdminError::BadRequest(
            "Cannot delete built-in roles".into(),
        ));
    }

    Err(AdminError::BadRequest(
        "Custom roles are now managed through site permissions. Use /_admin/api/sites/{site}/permissions instead.".into(),
    ))
}

// ============================================================================
// Permission Groups (for frontend organization)
// ============================================================================

/// Get permission groups for UI organization
pub async fn list_permission_groups(_admin: RequireAdmin) -> Json<PermissionGroupsResponse> {
    let groups = vec![
        PermissionGroup {
            name: "Viewing".to_string(),
            description: "Control what content users can see".to_string(),
            permissions: vec![
                "can_view".to_string(),
                "can_see_technical_details".to_string(),
                "can_see_exact_dates".to_string(),
                "can_see_location".to_string(),
            ],
        },
        PermissionGroup {
            name: "Downloads".to_string(),
            description: "Control download access to images".to_string(),
            permissions: vec![
                "can_download_medium".to_string(),
                "can_download_large".to_string(),
                "can_download_original".to_string(),
                "can_download_gallery".to_string(),
                "can_download_raw".to_string(),
            ],
        },
        PermissionGroup {
            name: "Versions".to_string(),
            description: "Access to image versions".to_string(),
            permissions: vec!["can_see_versions".to_string()],
        },
        PermissionGroup {
            name: "Metadata".to_string(),
            description: "Access to metadata and editing".to_string(),
            permissions: vec![
                "can_read_metadata".to_string(),
                "can_edit_content".to_string(),
            ],
        },
        PermissionGroup {
            name: "Comments".to_string(),
            description: "Comment and annotation capabilities".to_string(),
            permissions: vec![
                "can_add_comments".to_string(),
                "can_edit_own_comments".to_string(),
                "can_delete_own_comments".to_string(),
                "can_edit_any_comments".to_string(),
                "can_delete_any_comments".to_string(),
            ],
        },
        PermissionGroup {
            name: "Features".to_string(),
            description: "Interactive features".to_string(),
            permissions: vec![
                "can_set_picks".to_string(),
                "can_add_tags".to_string(),
                "can_use_zoom".to_string(),
                "can_use_tile_zoom".to_string(),
            ],
        },
        PermissionGroup {
            name: "AI".to_string(),
            description: "AI-powered features".to_string(),
            permissions: vec![
                "can_analyze_images".to_string(),
                "can_see_ai_analysis".to_string(),
                "can_see_ai_alt_text".to_string(),
            ],
        },
        PermissionGroup {
            name: "Admin".to_string(),
            description: "Administrative access".to_string(),
            permissions: vec!["owner_access".to_string()],
        },
    ];

    Json(PermissionGroupsResponse { groups })
}

// ============================================================================
// Gallery Permission Management
// ============================================================================

/// Update gallery permissions
/// Note: Permissions are now managed at the site level. Use /_admin/api/sites/{site}/permissions instead.
pub async fn update_gallery_permissions(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(gallery_name): Path<String>,
    Json(_request): Json<UpdateGalleryPermissionsRequest>,
) -> Result<Json<GalleryInfo>, AdminError> {
    // Verify gallery exists
    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery_name)))?;

    // Return current gallery info but indicate permissions should be managed at site level
    let gallery_config = gallery.get_config();
    let permissions = &gallery_config.permissions;

    // Get size and count from root folder cache
    let (image_count, total_size) = gallery
        .folder_cache
        .get("")
        .await
        .map(|cache| (cache.recursive_image_count, cache.recursive_size))
        .unwrap_or((0, 0));

    Ok(Json(GalleryInfo {
        name: gallery_name,
        url_prefix: gallery_config.url_prefix.clone(),
        permissions: PermissionConfigDto {
            site_admins: permissions.site_admins.clone(),
            public_role: permissions.public_role.clone(),
            default_authenticated_role: permissions.default_authenticated_role.clone(),
            roles: permissions
                .roles
                .iter()
                .map(|(role_name, role)| {
                    (
                        role_name.clone(),
                        RoleDto {
                            name: role_name.clone(),
                            permissions: RolePermissionsDto::from(&role.permissions),
                            inherits: role.inherits.clone(),
                            is_builtin: false,
                        },
                    )
                })
                .collect(),
            user_roles: permissions
                .user_roles
                .iter()
                .map(|user_role| UserRoleAssignment {
                    username: user_role.username.clone(),
                    roles: user_role.roles.clone(),
                })
                .collect(),
        },
        image_count,
        total_size,
        total_size_formatted: format_size(total_size),
    }))
}

/// Assign roles to a user for a gallery
/// Note: User roles are now managed at the site level through site permissions.
pub async fn assign_user_roles(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((gallery_name, _username)): Path<(String, String)>,
    Json(_request): Json<AssignUserRolesRequest>,
) -> Result<StatusCode, AdminError> {
    // Verify gallery exists
    if !app_state.galleries().contains_key(&gallery_name) {
        return Err(AdminError::NotFound(format!(
            "Gallery not found: {}",
            gallery_name
        )));
    }

    Err(AdminError::BadRequest(
        "User roles are now managed through site permissions. Use /_admin/api/sites/{site}/permissions instead.".into(),
    ))
}

/// Get roles assigned to a user for a gallery
/// Note: User roles are now stored in site permissions. This returns the user's roles from the gallery's current config.
pub async fn get_user_gallery_roles(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((gallery_name, username)): Path<(String, String)>,
) -> Result<Json<UserRoleAssignment>, AdminError> {
    // Verify gallery exists and get its permissions
    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery_name)))?;

    let config = gallery.get_config();
    let roles = config
        .permissions
        .get_user_roles(&username)
        .map(|r| r.to_vec())
        .unwrap_or_default();

    Ok(Json(UserRoleAssignment { username, roles }))
}

// ============================================================================
// Site Management
// ============================================================================

/// List all sites
pub async fn list_sites(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
) -> Result<Json<SiteListResponse>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    let site_names = config_storage
        .list_sites()
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    let mut sites = Vec::new();
    for name in site_names {
        if let Ok(Some(config)) = config_storage.get_site_config(&name).await {
            let gallery_count = config_storage
                .list_galleries(&name)
                .await
                .map(|g| g.len())
                .unwrap_or(0);
            let posts_count = config_storage
                .list_posts(&name)
                .await
                .map(|p| p.len())
                .unwrap_or(0);

            sites.push(SiteInfo {
                name,
                hostnames: config.hostnames,
                base_url: config.base_url,
                templates: config.templates,
                static_files: config.static_files,
                static_use_redirects: config.static_use_redirects,
                user_database: config.user_database,
                storage_prefix: config.storage_prefix,
                cache_prefix: config.cache_prefix,
                gallery_count,
                posts_count,
            });
        }
    }

    Ok(Json(SiteListResponse { sites }))
}

/// Get a single site
pub async fn get_site(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(name): Path<String>,
) -> Result<Json<SiteInfo>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    let config = config_storage
        .get_site_config(&name)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("Site not found: {}", name)))?;

    let gallery_count = config_storage
        .list_galleries(&name)
        .await
        .map(|g| g.len())
        .unwrap_or(0);
    let posts_count = config_storage
        .list_posts(&name)
        .await
        .map(|p| p.len())
        .unwrap_or(0);

    Ok(Json(SiteInfo {
        name,
        hostnames: config.hostnames,
        base_url: config.base_url,
        templates: config.templates,
        static_files: config.static_files,
        static_use_redirects: config.static_use_redirects,
        user_database: config.user_database,
        storage_prefix: config.storage_prefix,
        cache_prefix: config.cache_prefix,
        gallery_count,
        posts_count,
    }))
}

/// Update a site (NOTE: storage_prefix and cache_prefix cannot be edited via API)
pub async fn update_site(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path(name): Path<String>,
    Json(request): Json<UpdateSiteRequest>,
) -> Result<Json<SiteInfo>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Get existing config
    let mut config = config_storage
        .get_site_config(&name)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("Site not found: {}", name)))?;

    // Update fields (storage_prefix is NOT updated)
    if let Some(hostnames) = request.hostnames {
        config.hostnames = hostnames;
    }
    if let Some(base_url) = request.base_url {
        config.base_url = Some(base_url);
    }
    if let Some(templates) = request.templates {
        config.templates = templates;
    }
    if let Some(static_files) = request.static_files {
        config.static_files = static_files;
    }
    if let Some(static_use_redirects) = request.static_use_redirects {
        config.static_use_redirects = static_use_redirects;
    }
    if let Some(user_database) = request.user_database {
        config.user_database = Some(user_database);
    }

    // Save the updated config
    config_storage
        .set_site_config(&name, &config, &admin.0.username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    let gallery_count = config_storage
        .list_galleries(&name)
        .await
        .map(|g| g.len())
        .unwrap_or(0);
    let posts_count = config_storage
        .list_posts(&name)
        .await
        .map(|p| p.len())
        .unwrap_or(0);

    Ok(Json(SiteInfo {
        name,
        hostnames: config.hostnames,
        base_url: config.base_url,
        templates: config.templates,
        static_files: config.static_files,
        static_use_redirects: config.static_use_redirects,
        user_database: config.user_database,
        storage_prefix: config.storage_prefix,
        cache_prefix: config.cache_prefix,
        gallery_count,
        posts_count,
    }))
}

/// List galleries for a site
pub async fn list_site_galleries(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(site): Path<String>,
) -> Result<Json<SiteGalleryListResponse>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    let gallery_names = config_storage
        .list_galleries(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    let mut galleries = Vec::new();
    for name in gallery_names {
        if let Ok(Some(config)) = config_storage.get_gallery_full_config(&site, &name).await {
            galleries.push(SiteGalleryInfo {
                name,
                url_prefix: config.url_prefix,
                source_directory: config.source_directory,
                cache_directory: config.cache_directory,
                images_per_page: config.images_per_page,
            });
        }
    }

    Ok(Json(SiteGalleryListResponse { galleries }))
}

/// Get a single gallery configuration
pub async fn get_site_gallery(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, name)): Path<(String, String)>,
) -> Result<Json<SiteGalleryInfo>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    let config = config_storage
        .get_gallery_full_config(&site, &name)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", name)))?;

    Ok(Json(SiteGalleryInfo {
        name,
        url_prefix: config.url_prefix,
        source_directory: config.source_directory,
        cache_directory: config.cache_directory,
        images_per_page: config.images_per_page,
    }))
}

/// Create or update a gallery
pub async fn upsert_site_gallery(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((site, name)): Path<(String, String)>,
    Json(request): Json<CreateGalleryRequest>,
) -> Result<Json<SiteGalleryInfo>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Verify site exists
    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Validate name matches URL parameter
    if request.name != name {
        return Err(AdminError::BadRequest(
            "Gallery name in URL must match name in request body".into(),
        ));
    }

    // Validate URL prefix starts with /
    if !request.url_prefix.starts_with('/') {
        return Err(AdminError::BadRequest(
            "URL prefix must start with /".into(),
        ));
    }

    // Create the stored config with sensible defaults
    let stored_config = tenrankai_config_storage::StoredGalleryConfig {
        name: name.clone(),
        url_prefix: request.url_prefix.clone(),
        source_directory: request.source_directory.clone(),
        cache_directory: request.cache_directory.clone(),
        images_per_page: request.images_per_page,
        gallery_template: "modules/gallery.html.liquid".to_string(),
        image_detail_template: "modules/image_detail.html.liquid".to_string(),
        thumbnail: tenrankai_config_storage::StoredImageSizeConfig {
            width: 300,
            height: 300,
        },
        gallery_size: tenrankai_config_storage::StoredImageSizeConfig {
            width: 800,
            height: 800,
        },
        medium: tenrankai_config_storage::StoredImageSizeConfig {
            width: 1200,
            height: 1200,
        },
        large: tenrankai_config_storage::StoredImageSizeConfig {
            width: 1600,
            height: 1600,
        },
        image_indexing: "filename".to_string(),
        metadata_cache_size: 1000,
        cache_refresh_interval_minutes: None,
        jpeg_quality: None,
        webp_quality: None,
        new_threshold_days: None,
        copyright_holder: None,
        tiles: None,
        pregenerate: None,
        preview: None,
    };

    // Save the gallery config
    config_storage
        .set_gallery_full_config(&site, &name, &stored_config, &admin.0.username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    Ok(Json(SiteGalleryInfo {
        name,
        url_prefix: request.url_prefix,
        source_directory: request.source_directory,
        cache_directory: request.cache_directory,
        images_per_page: request.images_per_page,
    }))
}

/// Delete a gallery
pub async fn delete_site_gallery(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((site, name)): Path<(String, String)>,
) -> Result<StatusCode, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Verify site exists
    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    let deleted = config_storage
        .delete_gallery(&site, &name, &admin.0.username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AdminError::NotFound(format!("Gallery not found: {}", name)))
    }
}

/// Trigger site reload
pub async fn reload_site(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(site): Path<String>,
) -> Result<Json<ReloadSiteResponse>, AdminError> {
    // Check if we have a site manager
    let Some(site_manager) = app_state.site_manager.as_ref() else {
        return Ok(Json(ReloadSiteResponse {
            success: false,
            message: "Site manager not available (single-site mode)".to_string(),
        }));
    };

    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Get the config_storage URL from the site
    let config_storage_url = app_state.config_storage_url().ok_or(AdminError::Internal(
        "Config storage URL not configured".into(),
    ))?;

    // Create a ConfigStorageLoader
    let loader = crate::config::ConfigStorageLoader::new(
        config_storage.clone(),
        app_state.cookie_secret().to_string(),
    );

    // Reload the site
    match site_manager
        .reload_site(&site, &loader, config_storage_url)
        .await
    {
        Ok(()) => Ok(Json(ReloadSiteResponse {
            success: true,
            message: format!("Site '{}' reloaded successfully", site),
        })),
        Err(e) => Ok(Json(ReloadSiteResponse {
            success: false,
            message: format!("Failed to reload site '{}': {}", site, e),
        })),
    }
}

// ============================================================================
// Site Permissions Management
// ============================================================================

/// Get site-level permissions
pub async fn get_site_permissions(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(site): Path<String>,
) -> Result<Json<PermissionConfigDto>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Verify site exists
    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    let permissions = config_storage
        .get_site_permissions(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    // Convert to DTO
    let dto = match permissions {
        Some(perms) => PermissionConfigDto {
            site_admins: perms.site_admins,
            public_role: perms.public_role,
            default_authenticated_role: perms.default_authenticated_role,
            roles: perms
                .roles
                .into_iter()
                .map(|(name, role)| {
                    let p = &role.permissions;
                    (
                        name.clone(),
                        RoleDto {
                            name,
                            permissions: RolePermissionsDto {
                                can_view: p.can_view,
                                can_see_hidden: p.can_see_hidden,
                                can_see_technical_details: p.can_see_technical_details,
                                can_see_exact_dates: p.can_see_exact_dates,
                                can_see_location: p.can_see_location,
                                can_download_medium: p.can_download_medium,
                                can_download_large: p.can_download_large,
                                can_download_original: p.can_download_original,
                                can_download_gallery: p.can_download_gallery,
                                can_download_raw: p.can_download_raw,
                                can_see_versions: p.can_see_versions,
                                can_read_metadata: p.can_read_metadata,
                                can_edit_content: p.can_edit_content,
                                can_add_comments: p.can_add_comments,
                                can_edit_own_comments: p.can_edit_own_comments,
                                can_delete_own_comments: p.can_delete_own_comments,
                                can_edit_any_comments: p.can_edit_any_comments,
                                can_delete_any_comments: p.can_delete_any_comments,
                                can_manage_images: p.can_manage_images,
                                can_set_picks: p.can_set_picks,
                                can_add_tags: p.can_add_tags,
                                can_use_zoom: p.can_use_zoom,
                                can_use_tile_zoom: p.can_use_tile_zoom,
                                can_analyze_images: p.can_analyze_images,
                                can_see_ai_analysis: p.can_see_ai_analysis,
                                can_see_ai_alt_text: p.can_see_ai_alt_text,
                                owner_access: p.owner_access,
                            },
                            inherits: role.inherits,
                            is_builtin: false,
                        },
                    )
                })
                .collect(),
            user_roles: perms
                .user_roles
                .into_iter()
                .map(|ur| UserRoleAssignment {
                    username: ur.username,
                    roles: ur.roles,
                })
                .collect(),
        },
        None => PermissionConfigDto {
            site_admins: Vec::new(),
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("viewer".to_string()),
            roles: std::collections::HashMap::new(),
            user_roles: Vec::new(),
        },
    };

    Ok(Json(dto))
}

/// Update site-level permissions
pub async fn update_site_permissions(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path(site): Path<String>,
    Json(request): Json<UpdateGalleryPermissionsRequest>,
) -> Result<Json<PermissionConfigDto>, AdminError> {
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    // Verify site exists
    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Convert DTO to storage format
    let permission_config = tenrankai_config_storage::GalleryPermissionConfig {
        site_admins: request.site_admins.clone(),
        public_role: request.public_role.clone(),
        default_authenticated_role: request.default_authenticated_role.clone(),
        roles: request
            .roles
            .iter()
            .map(|(name, role_dto)| {
                (
                    name.clone(),
                    tenrankai_config_storage::Role {
                        inherits: role_dto.inherits.clone(),
                        permissions: tenrankai_config_storage::RolePermissions {
                            can_view: role_dto.permissions.can_view,
                            can_see_hidden: role_dto.permissions.can_see_hidden,
                            can_see_technical_details: role_dto
                                .permissions
                                .can_see_technical_details,
                            can_see_exact_dates: role_dto.permissions.can_see_exact_dates,
                            can_see_location: role_dto.permissions.can_see_location,
                            can_download_medium: role_dto.permissions.can_download_medium,
                            can_download_large: role_dto.permissions.can_download_large,
                            can_download_original: role_dto.permissions.can_download_original,
                            can_download_gallery: role_dto.permissions.can_download_gallery,
                            can_download_raw: role_dto.permissions.can_download_raw,
                            can_see_versions: role_dto.permissions.can_see_versions,
                            can_read_metadata: role_dto.permissions.can_read_metadata,
                            can_edit_content: role_dto.permissions.can_edit_content,
                            can_add_comments: role_dto.permissions.can_add_comments,
                            can_edit_own_comments: role_dto.permissions.can_edit_own_comments,
                            can_delete_own_comments: role_dto.permissions.can_delete_own_comments,
                            can_edit_any_comments: role_dto.permissions.can_edit_any_comments,
                            can_delete_any_comments: role_dto.permissions.can_delete_any_comments,
                            can_manage_images: role_dto.permissions.can_manage_images,
                            can_set_picks: role_dto.permissions.can_set_picks,
                            can_add_tags: role_dto.permissions.can_add_tags,
                            can_use_zoom: role_dto.permissions.can_use_zoom,
                            can_use_tile_zoom: role_dto.permissions.can_use_tile_zoom,
                            can_analyze_images: role_dto.permissions.can_analyze_images,
                            can_see_ai_analysis: role_dto.permissions.can_see_ai_analysis,
                            can_see_ai_alt_text: role_dto.permissions.can_see_ai_alt_text,
                            owner_access: role_dto.permissions.owner_access,
                        },
                    },
                )
            })
            .collect(),
        user_roles: request
            .user_roles
            .iter()
            .map(|ur| tenrankai_config_storage::UserRole {
                username: ur.username.clone(),
                roles: ur.roles.clone(),
            })
            .collect(),
    };

    // Save permissions
    config_storage
        .set_site_permissions(&site, &permission_config, &admin.0.username)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    // Return the saved permissions
    Ok(Json(PermissionConfigDto {
        site_admins: request.site_admins,
        public_role: request.public_role,
        default_authenticated_role: request.default_authenticated_role,
        roles: request.roles,
        user_roles: request.user_roles,
    }))
}

// ============================================================================
// Folder Management
// ============================================================================

/// List all folders in a gallery
pub async fn list_gallery_folders(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery)): Path<(String, String)>,
) -> Result<Json<FolderListResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Read all folders from the cache
    let folder_cache = gallery_obj.folder_cache.read_all().await;

    let mut folders: Vec<FolderInfo> = folder_cache
        .iter()
        .map(|(path, cached)| {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            let has_custom_permissions = cached.metadata.as_ref().is_some_and(|m| {
                let perms = &m.config.permissions;
                perms.public_role.is_some()
                    || perms.default_authenticated_role.is_some()
                    || !perms.roles.is_empty()
                    || !perms.user_roles.is_empty()
            });

            FolderInfo {
                path: path.clone(),
                name: if path.is_empty() {
                    "(root)".to_string()
                } else {
                    name
                },
                has_custom_permissions,
                image_count: cached.images.len(),
                size: cached.recursive_size,
                size_formatted: format_size(cached.recursive_size),
            }
        })
        .collect();

    // Sort by path
    folders.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Json(FolderListResponse { folders }))
}

/// Get folder permissions
pub async fn get_folder_permissions(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
) -> Result<Json<FolderPermissionsResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // URL decode the folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    // Handle special _root marker for root folder
    let folder_path = if folder_path == "_root" {
        String::new()
    } else {
        folder_path
    };

    // Read folder metadata from cache or storage
    // read_folder_metadata_full returns Option<FolderMetadata>
    let metadata = gallery_obj.read_folder_metadata_full(&folder_path).await;

    let (hidden, hidden_images, permissions, description) = match metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                meta.config.hidden_images.clone(),
                PermissionConfigDto {
                    site_admins: perms.site_admins.clone(),
                    public_role: perms.public_role.clone(),
                    default_authenticated_role: perms.default_authenticated_role.clone(),
                    roles: perms
                        .roles
                        .iter()
                        .map(|(name, role)| {
                            (
                                name.clone(),
                                RoleDto {
                                    name: name.clone(),
                                    permissions: RolePermissionsDto::from(&role.permissions),
                                    inherits: role.inherits.clone(),
                                    is_builtin: false,
                                },
                            )
                        })
                        .collect(),
                    user_roles: perms
                        .user_roles
                        .iter()
                        .map(|user_role| UserRoleAssignment {
                            username: user_role.username.clone(),
                            roles: user_role.roles.clone(),
                        })
                        .collect(),
                },
                meta.description_markdown,
            )
        }
        None => {
            // Folder exists in cache but has no _folder.md - return empty permissions
            // Check if the folder exists in the cache at all
            let folder_cache = gallery_obj.folder_cache.read_all().await;
            if !folder_cache.contains_key(&folder_path) {
                return Err(AdminError::NotFound(format!(
                    "Folder not found: {}",
                    folder_path
                )));
            }
            (
                false,
                vec![],
                PermissionConfigDto {
                    site_admins: Vec::new(),
                    public_role: None,
                    default_authenticated_role: None,
                    roles: std::collections::HashMap::new(),
                    user_roles: vec![],
                },
                String::new(),
            )
        }
    };

    Ok(Json(FolderPermissionsResponse {
        hidden,
        hidden_images,
        permissions,
        description,
    }))
}

/// Update folder permissions
pub async fn update_folder_permissions(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
    Json(request): Json<UpdateFolderPermissionsRequest>,
) -> Result<Json<FolderPermissionsResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // URL decode the folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    // Handle special _root marker for root folder
    let folder_path = if folder_path == "_root" {
        String::new()
    } else {
        folder_path
    };

    // Build the _folder.md content with TOML frontmatter
    let mut toml_content = toml_edit::DocumentMut::new();

    toml_content["hidden"] = toml_edit::value(request.hidden);

    // Add hidden_images array if non-empty
    if !request.hidden_images.is_empty() {
        let hidden_images_array: toml_edit::Array = request
            .hidden_images
            .iter()
            .map(|img| toml_edit::Value::from(img.clone()))
            .collect();
        toml_content["hidden_images"] = toml_edit::value(hidden_images_array);
    }

    // Build permissions section
    let mut permissions_table = toml_edit::Table::new();

    if let Some(ref public_role) = request.permissions.public_role {
        permissions_table["public_role"] = toml_edit::value(public_role.clone());
    }
    if let Some(ref default_auth_role) = request.permissions.default_authenticated_role {
        permissions_table["default_authenticated_role"] =
            toml_edit::value(default_auth_role.clone());
    }

    // Add roles
    if !request.permissions.roles.is_empty() {
        let mut roles_table = toml_edit::Table::new();
        for (role_name, role) in &request.permissions.roles {
            let mut role_table = toml_edit::Table::new();
            if let Some(ref inherits) = role.inherits {
                role_table["inherits"] = toml_edit::value(inherits.clone());
            }

            // Build permissions inline table
            let mut perms_table = toml_edit::InlineTable::new();
            if role.permissions.can_view {
                perms_table.insert("can_view", true.into());
            }
            if role.permissions.can_see_hidden {
                perms_table.insert("can_see_hidden", true.into());
            }
            if role.permissions.can_see_technical_details {
                perms_table.insert("can_see_technical_details", true.into());
            }
            if role.permissions.can_see_exact_dates {
                perms_table.insert("can_see_exact_dates", true.into());
            }
            if role.permissions.can_see_location {
                perms_table.insert("can_see_location", true.into());
            }
            if role.permissions.can_download_medium {
                perms_table.insert("can_download_medium", true.into());
            }
            if role.permissions.can_download_large {
                perms_table.insert("can_download_large", true.into());
            }
            if role.permissions.can_download_original {
                perms_table.insert("can_download_original", true.into());
            }
            if role.permissions.can_download_gallery {
                perms_table.insert("can_download_gallery", true.into());
            }
            if role.permissions.can_download_raw {
                perms_table.insert("can_download_raw", true.into());
            }
            if role.permissions.can_see_versions {
                perms_table.insert("can_see_versions", true.into());
            }
            if role.permissions.can_read_metadata {
                perms_table.insert("can_read_metadata", true.into());
            }
            if role.permissions.can_edit_content {
                perms_table.insert("can_edit_content", true.into());
            }
            if role.permissions.can_add_comments {
                perms_table.insert("can_add_comments", true.into());
            }
            if role.permissions.can_edit_own_comments {
                perms_table.insert("can_edit_own_comments", true.into());
            }
            if role.permissions.can_delete_own_comments {
                perms_table.insert("can_delete_own_comments", true.into());
            }
            if role.permissions.can_edit_any_comments {
                perms_table.insert("can_edit_any_comments", true.into());
            }
            if role.permissions.can_delete_any_comments {
                perms_table.insert("can_delete_any_comments", true.into());
            }
            if role.permissions.can_set_picks {
                perms_table.insert("can_set_picks", true.into());
            }
            if role.permissions.can_add_tags {
                perms_table.insert("can_add_tags", true.into());
            }
            if role.permissions.can_use_zoom {
                perms_table.insert("can_use_zoom", true.into());
            }
            if role.permissions.can_use_tile_zoom {
                perms_table.insert("can_use_tile_zoom", true.into());
            }
            if role.permissions.can_analyze_images {
                perms_table.insert("can_analyze_images", true.into());
            }
            if role.permissions.can_see_ai_analysis {
                perms_table.insert("can_see_ai_analysis", true.into());
            }
            if role.permissions.can_see_ai_alt_text {
                perms_table.insert("can_see_ai_alt_text", true.into());
            }
            if role.permissions.owner_access {
                perms_table.insert("owner_access", true.into());
            }

            role_table["permissions"] = toml_edit::value(perms_table);
            roles_table[role_name] = toml_edit::Item::Table(role_table);
        }
        permissions_table["roles"] = toml_edit::Item::Table(roles_table);
    }

    // Add user_roles as array of tables: [[permissions.user_roles]]
    if !request.permissions.user_roles.is_empty() {
        let mut user_roles_array = toml_edit::ArrayOfTables::new();
        for assignment in &request.permissions.user_roles {
            let mut user_role_table = toml_edit::Table::new();
            user_role_table["username"] = toml_edit::value(assignment.username.clone());
            let roles_array: toml_edit::Array = assignment
                .roles
                .iter()
                .map(|r| toml_edit::Value::from(r.clone()))
                .collect();
            user_role_table["roles"] = toml_edit::value(roles_array);
            user_roles_array.push(user_role_table);
        }
        permissions_table["user_roles"] = toml_edit::Item::ArrayOfTables(user_roles_array);
    }

    if !permissions_table.is_empty() {
        toml_content["permissions"] = toml_edit::Item::Table(permissions_table);
    }

    // Build the full _folder.md content
    let toml_str = toml_content.to_string();
    let content = if toml_str.trim().is_empty() && request.description.is_empty() {
        String::new()
    } else if toml_str.trim().is_empty() {
        request.description.clone()
    } else {
        format!("+++\n{}+++\n{}", toml_str, request.description)
    };

    // Write the _folder.md file
    let folder_md_path = if folder_path.is_empty() {
        "_folder.md".to_string()
    } else {
        format!("{}/_folder.md", folder_path)
    };

    gallery_obj
        .source_storage()
        .write(&folder_md_path, bytes::Bytes::from(content))
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to write _folder.md: {}", e)))?;

    // Refresh the folder cache to pick up changes
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh folder cache: {}", e)))?;

    Ok(Json(FolderPermissionsResponse {
        hidden: request.hidden,
        hidden_images: request.hidden_images,
        permissions: request.permissions,
        description: request.description,
    }))
}

/// List images in a folder (for admin management)
pub async fn list_folder_images(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
) -> Result<Json<FolderImagesResponse>, AdminError> {
    use crate::admin::types::{FolderImageInfo, FolderImagesResponse};

    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // URL decode the folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    // Handle special _root marker for root folder
    let folder_path = if folder_path == "_root" {
        String::new()
    } else {
        folder_path
    };

    // Get folder cache data
    let folder_cache = gallery_obj.folder_cache.read_all().await;
    let cached = folder_cache
        .get(&folder_path)
        .ok_or_else(|| AdminError::NotFound(format!("Folder not found: {}", folder_path)))?;

    // Get hidden images list
    let hidden_images: std::collections::HashSet<String> = cached
        .metadata
        .as_ref()
        .map(|m| m.config.hidden_images.iter().cloned().collect())
        .unwrap_or_default();

    // Build response from preview items
    let images: Vec<FolderImageInfo> = cached
        .preview_items
        .iter()
        .map(|item| {
            // Extract filename from path
            let filename = item
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&item.path)
                .to_string();
            FolderImageInfo {
                url_id: item.url_id.clone(),
                filename,
                thumbnail_url: item.thumbnail_url.clone(),
                is_hidden: hidden_images.contains(&item.path),
            }
        })
        .collect();

    Ok(Json(FolderImagesResponse { images }))
}

/// Share a folder with a user by email
pub async fn share_folder(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
    Json(request): Json<ShareFolderRequest>,
) -> Result<Json<ShareFolderResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    let gallery_config = gallery_obj.get_config();

    // URL decode the folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    // Handle special _root marker for root folder
    let folder_path = if folder_path == "_root" {
        String::new()
    } else {
        folder_path
    };

    // Verify folder exists in cache
    {
        let folder_cache = gallery_obj.folder_cache.read_all().await;
        if !folder_cache.contains_key(&folder_path) {
            return Err(AdminError::NotFound(format!(
                "Folder not found: {}",
                folder_path
            )));
        }
    }

    // Validate email format (basic check)
    let email = request.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 {
        return Err(AdminError::BadRequest("Invalid email address".into()));
    }

    // Get site permissions to check for custom roles
    let site_permissions = config_storage
        .get_site_permissions(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?;

    // Validate role (must be built-in, defined in site permissions, or in gallery)
    let valid_roles = ["viewer", "contributor", "admin"];
    let is_valid_role = valid_roles.contains(&request.role.as_str())
        || gallery_config.permissions.roles.contains_key(&request.role)
        || site_permissions
            .as_ref()
            .map(|p| p.roles.contains_key(&request.role))
            .unwrap_or(false);
    if !is_valid_role {
        return Err(AdminError::BadRequest(format!(
            "Invalid role: {}. Define custom roles in Site Permissions first.",
            request.role
        )));
    }

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(AdminError::Internal("User storage not configured".into()))?;

    // Find or create user
    let (username, user_created) = match user_storage.get_user_by_email(&email).await {
        Ok(Some((existing_username, _))) => (existing_username, false),
        Ok(None) => {
            // Create new user with email-based username
            let username = email
                .split('@')
                .next()
                .unwrap_or("user")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(32)
                .collect::<String>()
                .to_lowercase();

            // Ensure username is at least 3 chars
            let username = if username.len() < 3 {
                format!(
                    "user_{}",
                    &email.replace(['@', '.'], "_")[..8.min(email.len())]
                )
            } else {
                username
            };

            // Check if username already exists, add suffix if needed
            let mut final_username = username.clone();
            let mut suffix = 1;
            while user_storage
                .get_user(&final_username)
                .await
                .map_err(|e| AdminError::Internal(e.to_string()))?
                .is_some()
            {
                final_username = format!("{}_{}", username, suffix);
                suffix += 1;
            }

            // Create the user
            let user = tenrankai_users::User {
                email: email.clone(),
                passkeys: vec![],
            };
            user_storage
                .add_user(&final_username, &user)
                .await
                .map_err(|e| AdminError::Internal(format!("Failed to create user: {}", e)))?;

            (final_username, true)
        }
        Err(e) => {
            return Err(AdminError::Internal(format!(
                "Failed to lookup user: {}",
                e
            )));
        }
    };

    // Add user to folder's user_roles
    // Read current folder metadata
    let current_metadata = gallery_obj.read_folder_metadata_full(&folder_path).await;

    let (hidden, hidden_images, mut permissions, description) = match current_metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                meta.config.hidden_images.clone(),
                PermissionConfigDto {
                    site_admins: perms.site_admins.clone(),
                    public_role: perms.public_role.clone(),
                    default_authenticated_role: perms.default_authenticated_role.clone(),
                    roles: perms
                        .roles
                        .iter()
                        .map(|(name, role)| {
                            (
                                name.clone(),
                                RoleDto {
                                    name: name.clone(),
                                    permissions: RolePermissionsDto::from(&role.permissions),
                                    inherits: role.inherits.clone(),
                                    is_builtin: false,
                                },
                            )
                        })
                        .collect(),
                    user_roles: perms
                        .user_roles
                        .iter()
                        .map(|user_role| UserRoleAssignment {
                            username: user_role.username.clone(),
                            roles: user_role.roles.clone(),
                        })
                        .collect(),
                },
                meta.description_markdown,
            )
        }
        None => (
            false,
            vec![],
            PermissionConfigDto {
                site_admins: Vec::new(),
                public_role: None,
                default_authenticated_role: None,
                roles: std::collections::HashMap::new(),
                user_roles: vec![],
            },
            String::new(),
        ),
    };

    // Update or add user role assignment
    let mut found = false;
    for assignment in &mut permissions.user_roles {
        if assignment.username == username {
            // Add role if not already present
            if !assignment.roles.contains(&request.role) {
                assignment.roles.push(request.role.clone());
            }
            found = true;
            break;
        }
    }
    if !found {
        permissions.user_roles.push(UserRoleAssignment {
            username: username.clone(),
            roles: vec![request.role.clone()],
        });
    }

    // Write updated _folder.md
    let update_request = UpdateFolderPermissionsRequest {
        hidden,
        hidden_images,
        permissions: permissions.clone(),
        description,
    };

    // Build the _folder.md content (reuse logic from update_folder_permissions)
    let mut toml_content = toml_edit::DocumentMut::new();

    toml_content["hidden"] = toml_edit::value(update_request.hidden);

    let mut permissions_table = toml_edit::Table::new();

    if let Some(ref public_role) = update_request.permissions.public_role {
        permissions_table["public_role"] = toml_edit::value(public_role.clone());
    }
    if let Some(ref default_auth_role) = update_request.permissions.default_authenticated_role {
        permissions_table["default_authenticated_role"] =
            toml_edit::value(default_auth_role.clone());
    }

    // Add roles
    if !update_request.permissions.roles.is_empty() {
        let mut roles_table = toml_edit::Table::new();
        for (role_name, role) in &update_request.permissions.roles {
            let mut role_table = toml_edit::Table::new();
            if let Some(ref inherits) = role.inherits {
                role_table["inherits"] = toml_edit::value(inherits.clone());
            }

            let mut perms_table = toml_edit::InlineTable::new();
            if role.permissions.can_view {
                perms_table.insert("can_view", true.into());
            }
            if role.permissions.can_see_technical_details {
                perms_table.insert("can_see_technical_details", true.into());
            }
            if role.permissions.can_see_exact_dates {
                perms_table.insert("can_see_exact_dates", true.into());
            }
            if role.permissions.can_see_location {
                perms_table.insert("can_see_location", true.into());
            }
            if role.permissions.can_download_medium {
                perms_table.insert("can_download_medium", true.into());
            }
            if role.permissions.can_download_large {
                perms_table.insert("can_download_large", true.into());
            }
            if role.permissions.can_download_original {
                perms_table.insert("can_download_original", true.into());
            }
            if role.permissions.can_download_gallery {
                perms_table.insert("can_download_gallery", true.into());
            }
            if role.permissions.can_download_raw {
                perms_table.insert("can_download_raw", true.into());
            }
            if role.permissions.can_see_versions {
                perms_table.insert("can_see_versions", true.into());
            }
            if role.permissions.can_read_metadata {
                perms_table.insert("can_read_metadata", true.into());
            }
            if role.permissions.can_edit_content {
                perms_table.insert("can_edit_content", true.into());
            }
            if role.permissions.can_add_comments {
                perms_table.insert("can_add_comments", true.into());
            }
            if role.permissions.can_edit_own_comments {
                perms_table.insert("can_edit_own_comments", true.into());
            }
            if role.permissions.can_delete_own_comments {
                perms_table.insert("can_delete_own_comments", true.into());
            }
            if role.permissions.can_edit_any_comments {
                perms_table.insert("can_edit_any_comments", true.into());
            }
            if role.permissions.can_delete_any_comments {
                perms_table.insert("can_delete_any_comments", true.into());
            }
            if role.permissions.can_set_picks {
                perms_table.insert("can_set_picks", true.into());
            }
            if role.permissions.can_add_tags {
                perms_table.insert("can_add_tags", true.into());
            }
            if role.permissions.can_use_zoom {
                perms_table.insert("can_use_zoom", true.into());
            }
            if role.permissions.can_use_tile_zoom {
                perms_table.insert("can_use_tile_zoom", true.into());
            }
            if role.permissions.can_analyze_images {
                perms_table.insert("can_analyze_images", true.into());
            }
            if role.permissions.can_see_ai_analysis {
                perms_table.insert("can_see_ai_analysis", true.into());
            }
            if role.permissions.can_see_ai_alt_text {
                perms_table.insert("can_see_ai_alt_text", true.into());
            }
            if role.permissions.owner_access {
                perms_table.insert("owner_access", true.into());
            }

            role_table["permissions"] = toml_edit::value(perms_table);
            roles_table[role_name] = toml_edit::Item::Table(role_table);
        }
        permissions_table["roles"] = toml_edit::Item::Table(roles_table);
    }

    // Add user_roles as array of tables: [[permissions.user_roles]]
    if !update_request.permissions.user_roles.is_empty() {
        let mut user_roles_array = toml_edit::ArrayOfTables::new();
        for assignment in &update_request.permissions.user_roles {
            let mut user_role_table = toml_edit::Table::new();
            user_role_table["username"] = toml_edit::value(assignment.username.clone());
            let roles_array: toml_edit::Array = assignment
                .roles
                .iter()
                .map(|r| toml_edit::Value::from(r.clone()))
                .collect();
            user_role_table["roles"] = toml_edit::value(roles_array);
            user_roles_array.push(user_role_table);
        }
        permissions_table["user_roles"] = toml_edit::Item::ArrayOfTables(user_roles_array);
    }

    if !permissions_table.is_empty() {
        toml_content["permissions"] = toml_edit::Item::Table(permissions_table);
    }

    let toml_str = toml_content.to_string();
    let content = if toml_str.trim().is_empty() && update_request.description.is_empty() {
        String::new()
    } else if toml_str.trim().is_empty() {
        update_request.description.clone()
    } else {
        format!("+++\n{}+++\n{}", toml_str, update_request.description)
    };

    // Write the _folder.md file
    let folder_md_path = if folder_path.is_empty() {
        "_folder.md".to_string()
    } else {
        format!("{}/_folder.md", folder_path)
    };

    gallery_obj
        .source_storage()
        .write(&folder_md_path, bytes::Bytes::from(content))
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to write _folder.md: {}", e)))?;

    // Refresh the folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh folder cache: {}", e)))?;

    // Create invite token and send email
    let token = {
        let mut login_state = app_state.login_state().write().await;
        login_state.create_invite_token(username.clone())
    };

    // Build folder URL for redirect after login
    let folder_url = if folder_path.is_empty() {
        gallery_config.url_prefix.clone()
    } else {
        format!("{}/{}", gallery_config.url_prefix, folder_path)
    };

    let base_url = app_state.base_url().unwrap_or("http://localhost:3000");
    let login_url = format!(
        "{}/_login/verify?token={}&return={}",
        base_url,
        token,
        urlencoding::encode(&folder_url)
    );

    // Send the email if provider is configured
    if let Some(email_provider) = &app_state.email_provider
        && let Some(email_config) = app_state.email_config()
    {
        let folder_display = if folder_path.is_empty() {
            gallery.clone()
        } else {
            format!("{}/{}", gallery, folder_path)
        };

        let mut email_message = crate::email::EmailMessage::new(
            &email,
            email_config.format_from(),
            format!(
                "{} has shared '{}' with you",
                admin.0.username, folder_display
            ),
        );

        if let Some(reply_to) = &email_config.reply_to {
            email_message = email_message.with_reply_to(reply_to);
        }

        email_message = email_message.with_both(
            format!(
                "{} has shared the folder '{}' with you on {}.\n\nClick this link to view it:\n\n{}\n\nThis link will expire in 72 hours.",
                admin.0.username, folder_display, app_state.app_name(), login_url
            ),
            format!(
                r#"<p><strong>{}</strong> has shared the folder <strong>{}</strong> with you on {}.</p>
<p>Click this link to view it:</p>
<p><a href="{}">{}</a></p>
<p>This link will expire in 72 hours.</p>"#,
                admin.0.username, folder_display, app_state.app_name(), login_url, login_url
            ),
        );

        if let Err(e) = email_provider.send_email(email_message).await {
            tracing::error!("Failed to send share invitation email: {}", e);
            // Don't fail the request - user is already added to folder
        }
    } else {
        // Log the URL if no email provider
        tracing::info!("Share invitation URL for {}: {}", email, login_url);
    }

    let message = if user_created {
        format!(
            "Created account for {} and shared folder with {} role. Invitation email sent.",
            email, request.role
        )
    } else {
        format!(
            "Shared folder with {} ({} role). Invitation email sent.",
            email, request.role
        )
    };

    Ok(Json(ShareFolderResponse {
        success: true,
        message,
        user_created,
    }))
}

// ============================================================================
// Image Management
// ============================================================================

/// Delete images from a gallery
pub async fn delete_gallery_images(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery)): Path<(String, String)>,
    Json(request): Json<DeleteImagesRequest>,
) -> Result<Json<DeleteImagesResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    if request.paths.is_empty() {
        return Ok(Json(DeleteImagesResponse {
            success: true,
            deleted_count: 0,
            errors: vec![],
        }));
    }

    let mut deleted_count = 0;
    let mut errors = Vec::new();

    // Resolve URL paths to actual file paths via indexer
    let resolved_paths: Vec<String> = {
        let indexer = gallery_obj.image_indexer.read().await;
        request
            .paths
            .iter()
            .filter_map(|p| {
                // URL decode the path first
                let decoded_path = urlencoding::decode(p).ok()?.to_string();

                // Basic path validation - no traversal
                if decoded_path.contains("..") || decoded_path.starts_with('/') {
                    return None;
                }

                // Try to resolve through indexer (handles unique_id/sequence modes)
                if let Some(resolved) = indexer.get_path(&decoded_path) {
                    Some(resolved.to_string())
                } else {
                    // Fallback for filename indexing mode - use path as-is
                    Some(decoded_path)
                }
            })
            .collect()
    };

    // Delete each resolved image from source storage
    for file_path in &resolved_paths {
        match gallery_obj.source_storage().delete(file_path).await {
            Ok(()) => {
                deleted_count += 1;
                tracing::info!("Deleted image: {} from gallery {}", file_path, gallery);
            }
            Err(e) => {
                errors.push(format!("Failed to delete {}: {}", file_path, e));
                tracing::error!("Failed to delete image {}: {}", file_path, e);
            }
        }
    }

    // Refresh the folder cache to update image listings
    if deleted_count > 0
        && let Err(e) = gallery_obj.refresh_folder_cache().await
    {
        tracing::error!("Failed to refresh folder cache: {}", e);
        errors.push(format!("Failed to refresh cache: {}", e));
    }

    Ok(Json(DeleteImagesResponse {
        success: errors.is_empty(),
        deleted_count,
        errors,
    }))
}

/// Delete images from a gallery (site-resolved version)
/// Uses the current site from the resolved state (determined by host)
pub async fn delete_gallery_images_resolved(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path(gallery): Path<String>,
    Json(request): Json<DeleteImagesRequest>,
) -> Result<Json<DeleteImagesResponse>, AdminError> {
    let site = app_state.site.name.clone();
    delete_gallery_images(
        ResolvedState(app_state),
        _admin,
        Path((site, gallery)),
        Json(request),
    )
    .await
}

/// Hide or unhide images in a gallery folder
pub async fn hide_gallery_images(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
    Json(request): Json<HideImagesRequest>,
) -> Result<Json<HideImagesResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get the runtime gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // URL decode the folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    // Handle special _root marker for root folder
    let folder_path = if folder_path == "_root" {
        String::new()
    } else {
        folder_path
    };

    // Get current folder metadata
    let current_metadata = gallery_obj.read_folder_metadata_full(&folder_path).await;

    // Resolve URL paths through the indexer and extract actual filenames
    // This handles unique_id indexing where URL has "3f601x" but file is "CRW_1978.jpg"
    let filenames: Vec<String> = {
        let indexer = gallery_obj.image_indexer.read().await;
        request
            .paths
            .iter()
            .filter_map(|p| {
                // Get the URL identifier from the path
                let url_id = p.rsplit('/').next()?;
                // Resolve through indexer to get actual path
                if let Some(resolved) = indexer.get_path(p) {
                    // Extract filename from resolved path
                    resolved.rsplit('/').next().map(String::from)
                } else {
                    // Fallback: use the URL id as-is (for filename indexing mode)
                    Some(url_id.to_string())
                }
            })
            .collect()
    };

    // Build the updated hidden_images list
    let mut hidden_images: Vec<String> = current_metadata
        .as_ref()
        .map(|m| m.config.hidden_images.clone())
        .unwrap_or_default();

    if request.hide {
        // Add to hidden list using actual filenames
        for filename in &filenames {
            if !hidden_images.contains(filename) {
                hidden_images.push(filename.clone());
            }
        }
    } else {
        // Remove from hidden list
        hidden_images.retain(|f| !filenames.contains(f));
    }

    // Get other metadata values to preserve
    let (hidden, permissions, description) = match &current_metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                PermissionConfigDto {
                    site_admins: perms.site_admins.clone(),
                    public_role: perms.public_role.clone(),
                    default_authenticated_role: perms.default_authenticated_role.clone(),
                    roles: perms
                        .roles
                        .iter()
                        .map(|(name, role)| {
                            (
                                name.clone(),
                                RoleDto {
                                    name: name.clone(),
                                    permissions: RolePermissionsDto::from(&role.permissions),
                                    inherits: role.inherits.clone(),
                                    is_builtin: false,
                                },
                            )
                        })
                        .collect(),
                    user_roles: perms
                        .user_roles
                        .iter()
                        .map(|user_role| UserRoleAssignment {
                            username: user_role.username.clone(),
                            roles: user_role.roles.clone(),
                        })
                        .collect(),
                },
                meta.description_markdown.clone(),
            )
        }
        None => (
            false,
            PermissionConfigDto {
                site_admins: Vec::new(),
                public_role: None,
                default_authenticated_role: None,
                roles: std::collections::HashMap::new(),
                user_roles: vec![],
            },
            String::new(),
        ),
    };

    // Build the _folder.md content
    let update_request = UpdateFolderPermissionsRequest {
        hidden,
        hidden_images: hidden_images.clone(),
        permissions,
        description,
    };

    // Build the TOML content
    let mut toml_content = toml_edit::DocumentMut::new();
    toml_content["hidden"] = toml_edit::value(update_request.hidden);

    // Add hidden_images array if non-empty
    if !update_request.hidden_images.is_empty() {
        let hidden_images_array: toml_edit::Array = update_request
            .hidden_images
            .iter()
            .map(|img| toml_edit::Value::from(img.clone()))
            .collect();
        toml_content["hidden_images"] = toml_edit::value(hidden_images_array);
    }

    // Build permissions section
    let mut permissions_table = toml_edit::Table::new();

    if let Some(ref public_role) = update_request.permissions.public_role {
        permissions_table["public_role"] = toml_edit::value(public_role.clone());
    }
    if let Some(ref default_auth_role) = update_request.permissions.default_authenticated_role {
        permissions_table["default_authenticated_role"] =
            toml_edit::value(default_auth_role.clone());
    }

    // Add roles if any
    if !update_request.permissions.roles.is_empty() {
        let mut roles_table = toml_edit::Table::new();
        for (role_name, role) in &update_request.permissions.roles {
            let mut role_table = toml_edit::Table::new();
            if let Some(ref inherits) = role.inherits {
                role_table["inherits"] = toml_edit::value(inherits.clone());
            }

            let mut perms_table = toml_edit::InlineTable::new();
            if role.permissions.can_view {
                perms_table.insert("can_view", true.into());
            }
            if role.permissions.can_see_hidden {
                perms_table.insert("can_see_hidden", true.into());
            }
            if role.permissions.owner_access {
                perms_table.insert("owner_access", true.into());
            }

            role_table["permissions"] = toml_edit::value(perms_table);
            roles_table[role_name] = toml_edit::Item::Table(role_table);
        }
        permissions_table["roles"] = toml_edit::Item::Table(roles_table);
    }

    // Add user_roles if any
    if !update_request.permissions.user_roles.is_empty() {
        let mut user_roles_array = toml_edit::ArrayOfTables::new();
        for assignment in &update_request.permissions.user_roles {
            let mut user_role_table = toml_edit::Table::new();
            user_role_table["username"] = toml_edit::value(assignment.username.clone());
            let roles_array: toml_edit::Array = assignment
                .roles
                .iter()
                .map(|r| toml_edit::Value::from(r.clone()))
                .collect();
            user_role_table["roles"] = toml_edit::value(roles_array);
            user_roles_array.push(user_role_table);
        }
        permissions_table["user_roles"] = toml_edit::Item::ArrayOfTables(user_roles_array);
    }

    if !permissions_table.is_empty() {
        toml_content["permissions"] = toml_edit::Item::Table(permissions_table);
    }

    // Build the full _folder.md content
    let toml_str = toml_content.to_string();
    let content = if toml_str.trim().is_empty() && update_request.description.is_empty() {
        String::new()
    } else if toml_str.trim().is_empty() {
        update_request.description.clone()
    } else {
        format!("+++\n{}+++\n{}", toml_str, update_request.description)
    };

    // Write the _folder.md file
    let folder_md_path = if folder_path.is_empty() {
        "_folder.md".to_string()
    } else {
        format!("{}/_folder.md", folder_path)
    };

    gallery_obj
        .source_storage()
        .write(&folder_md_path, bytes::Bytes::from(content))
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to write _folder.md: {}", e)))?;

    // Refresh the folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh folder cache: {}", e)))?;

    Ok(Json(HideImagesResponse {
        success: true,
        hidden_images,
    }))
}

/// Hide or unhide images in a gallery folder (site-resolved version)
/// Uses the current site from the resolved state (determined by host)
pub async fn hide_gallery_images_resolved(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((gallery, folder_path)): Path<(String, String)>,
    Json(request): Json<HideImagesRequest>,
) -> Result<Json<HideImagesResponse>, AdminError> {
    // Get site name from resolved state
    let site = app_state.site.name.clone();

    // Call the existing handler logic with the resolved site
    hide_gallery_images(
        ResolvedState(app_state),
        _admin,
        Path((site, gallery, folder_path)),
        Json(request),
    )
    .await
}

// ============================================================================
// Folder Management
// ============================================================================

/// Validate folder name - no path traversal or special characters
fn is_valid_folder_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    // No path separators or parent directory references
    if name.contains('/') || name.contains('\\') || name == ".." || name == "." {
        return false;
    }
    // Only allow alphanumeric, hyphen, underscore, space, and common chars
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' || c == '.')
}

/// Create a new folder in a gallery
pub async fn create_gallery_folder(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, parent_folder)): Path<(String, String, String)>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<Json<CreateFolderResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to check site: {}", e)))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Validate folder name
    if !is_valid_folder_name(&request.name) {
        return Err(AdminError::BadRequest(
            "Invalid folder name. Use only letters, numbers, hyphens, underscores, and spaces."
                .into(),
        ));
    }

    // Decode parent folder path
    let parent_folder = urlencoding::decode(&parent_folder)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    let parent_folder = if parent_folder == "_root" {
        String::new()
    } else {
        parent_folder
    };

    // Build new folder path
    let new_folder_path = if parent_folder.is_empty() {
        request.name.clone()
    } else {
        format!("{}/{}", parent_folder, request.name)
    };

    // Check if folder already exists
    let folder_md_path = format!("{}/_folder.md", new_folder_path);
    if gallery_obj
        .source_storage()
        .exists(&folder_md_path)
        .await
        .unwrap_or(false)
    {
        return Err(AdminError::AlreadyExists(format!(
            "Folder already exists: {}",
            new_folder_path
        )));
    }

    // Also check if there are any files in that path (folder exists without _folder.md)
    let entries = gallery_obj
        .source_storage()
        .list(&new_folder_path)
        .await
        .unwrap_or_default();
    if !entries.is_empty() {
        return Err(AdminError::AlreadyExists(format!(
            "Folder already exists: {}",
            new_folder_path
        )));
    }

    // Create _folder.md with default config
    let description = request.description.clone().unwrap_or_default();
    let content = if description.is_empty() {
        "+++\nhidden = false\n+++\n".to_string()
    } else {
        format!("+++\nhidden = false\n+++\n\n{}", description)
    };

    gallery_obj
        .source_storage()
        .write(&folder_md_path, bytes::Bytes::from(content))
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to create folder: {}", e)))?;

    // Refresh folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh cache: {}", e)))?;

    Ok(Json(CreateFolderResponse {
        success: true,
        folder_path: new_folder_path,
    }))
}

/// Create a new folder in a gallery (site-resolved version)
pub async fn create_gallery_folder_resolved(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((gallery, parent_folder)): Path<(String, String)>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<Json<CreateFolderResponse>, AdminError> {
    let site = app_state.site.name.clone();
    create_gallery_folder(
        ResolvedState(app_state),
        admin,
        Path((site, gallery, parent_folder)),
        Json(request),
    )
    .await
}

/// Delete an empty folder from a gallery
pub async fn delete_gallery_folder(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
) -> Result<Json<DeleteFolderResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to check site: {}", e)))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Decode folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    let folder_path = if folder_path == "_root" {
        return Err(AdminError::BadRequest("Cannot delete root folder".into()));
    } else {
        folder_path
    };

    let storage = gallery_obj.source_storage();

    // Check if folder exists by looking for _folder.md or any content
    let folder_metadata_path = if folder_path.is_empty() {
        "_folder.md".to_string()
    } else {
        format!("{}/_folder.md", folder_path)
    };

    // List contents of the folder to check if empty
    let contents = storage
        .list(&folder_path)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to list folder: {}", e)))?;

    // Filter out _folder.md from the count
    let non_metadata_contents: Vec<_> = contents
        .iter()
        .filter(|entry| {
            let name = path_utils::filename(&entry.path);
            name != "_folder.md"
        })
        .collect();

    if !non_metadata_contents.is_empty() {
        return Err(AdminError::BadRequest(format!(
            "Folder is not empty. Contains {} items. Delete all images first.",
            non_metadata_contents.len()
        )));
    }

    // Delete _folder.md if it exists
    if storage.exists(&folder_metadata_path).await.unwrap_or(false) {
        storage.delete(&folder_metadata_path).await.map_err(|e| {
            AdminError::Internal(format!("Failed to delete folder metadata: {}", e))
        })?;
    }

    // Try to delete the empty directory itself (for filesystem storage)
    // This is a no-op for S3 since directories don't really exist there
    let _ = storage.delete_directory(&folder_path).await;

    // Refresh folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh cache: {}", e)))?;

    Ok(Json(DeleteFolderResponse {
        success: true,
        message: format!("Folder '{}' deleted", folder_path),
    }))
}

/// Delete an empty folder (site-resolved version)
pub async fn delete_gallery_folder_resolved(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((gallery, folder_path)): Path<(String, String)>,
) -> Result<Json<DeleteFolderResponse>, AdminError> {
    let site = app_state.site.name.clone();
    delete_gallery_folder(
        ResolvedState(app_state),
        admin,
        Path((site, gallery, folder_path)),
    )
    .await
}

/// Rename a folder in a gallery
pub async fn rename_gallery_folder(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, folder_path)): Path<(String, String, String)>,
    Json(request): Json<RenameFolderRequest>,
) -> Result<Json<RenameFolderResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to check site: {}", e)))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Decode folder path
    let folder_path = urlencoding::decode(&folder_path)
        .map_err(|e| AdminError::BadRequest(format!("Invalid folder path encoding: {}", e)))?
        .to_string();

    if folder_path == "_root" || folder_path.is_empty() {
        return Err(AdminError::BadRequest("Cannot rename root folder".into()));
    }

    // Validate new name
    let new_name = request.new_name.trim();
    if new_name.is_empty() {
        return Err(AdminError::BadRequest("Folder name cannot be empty".into()));
    }

    // Check for invalid characters
    if !new_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(AdminError::BadRequest(
            "Folder name can only contain letters, numbers, hyphens, underscores, and spaces"
                .into(),
        ));
    }

    // Cannot use reserved names
    if new_name.starts_with("__") || new_name == "_folder.md" {
        return Err(AdminError::BadRequest(
            "Folder name cannot start with '__' or be '_folder.md'".into(),
        ));
    }

    let storage = gallery_obj.source_storage();

    // Calculate new path
    let new_path = if let Some(parent) = path_utils::parent(&folder_path) {
        format!("{}/{}", parent, new_name)
    } else {
        new_name.to_string()
    };

    // Check if destination already exists
    let new_folder_metadata = format!("{}/_folder.md", new_path);
    if storage.exists(&new_folder_metadata).await.unwrap_or(false) {
        return Err(AdminError::BadRequest(format!(
            "A folder named '{}' already exists",
            new_name
        )));
    }

    // List all files in the source folder recursively
    let all_files = storage
        .list_recursive(&folder_path)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to list folder contents: {}", e)))?;

    // Move each file
    for entry in &all_files {
        if !entry.is_dir {
            // Calculate new path by replacing the folder prefix
            let relative_path = entry
                .path
                .strip_prefix(&folder_path)
                .unwrap_or(&entry.path)
                .trim_start_matches('/');
            let dest_path = if relative_path.is_empty() {
                new_path.clone()
            } else {
                format!("{}/{}", new_path, relative_path)
            };

            // Move the file
            if let Ok(data) = storage.read(&entry.path).await
                && storage.write(&dest_path, data).await.is_ok()
            {
                let _ = storage.delete(&entry.path).await;
            }
        }
    }

    // Queue cache cleanup for old paths and generation for new paths
    if let Some(cache_queue) = app_state.cache_queue() {
        for entry in &all_files {
            if !entry.is_dir
                && crate::gallery::grouping::is_image_extension(
                    path_utils::filename(&entry.path)
                        .rsplit('.')
                        .next()
                        .unwrap_or(""),
                )
            {
                let relative_path = entry
                    .path
                    .strip_prefix(&folder_path)
                    .unwrap_or(&entry.path)
                    .trim_start_matches('/');
                let dest_path = if relative_path.is_empty() {
                    new_path.clone()
                } else {
                    format!("{}/{}", new_path, relative_path)
                };

                // Cleanup old cache
                let _ = cache_queue
                    .submit_cleanup(CacheCleanupRequest::new(&site, &gallery, &entry.path))
                    .await;

                // Generate new cache
                let _ = cache_queue
                    .submit(CacheGenerationRequest::new(&site, &gallery, &dest_path))
                    .await;
            }
        }
    }

    // Refresh folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh cache: {}", e)))?;

    Ok(Json(RenameFolderResponse {
        success: true,
        new_path,
    }))
}

/// Rename a folder (site-resolved version)
pub async fn rename_gallery_folder_resolved(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((gallery, folder_path)): Path<(String, String)>,
    Json(request): Json<RenameFolderRequest>,
) -> Result<Json<RenameFolderResponse>, AdminError> {
    let site = app_state.site.name.clone();
    rename_gallery_folder(
        ResolvedState(app_state),
        admin,
        Path((site, gallery, folder_path)),
        Json(request),
    )
    .await
}

/// Move images from one folder to another
pub async fn move_gallery_images(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, source_folder)): Path<(String, String, String)>,
    Json(request): Json<MoveImagesRequest>,
) -> Result<Json<MoveImagesResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to check site: {}", e)))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Decode folder paths
    let source_folder = urlencoding::decode(&source_folder)
        .map_err(|e| AdminError::BadRequest(format!("Invalid source folder encoding: {}", e)))?
        .to_string();

    let source_folder = if source_folder == "_root" {
        String::new()
    } else {
        source_folder
    };

    let target_folder = if request.target_folder == "_root" {
        String::new()
    } else {
        request.target_folder.clone()
    };

    // Can't move to same folder
    if source_folder == target_folder {
        return Err(AdminError::BadRequest(
            "Cannot move images to the same folder".into(),
        ));
    }

    // Get source folder metadata for hidden images
    let source_metadata = gallery_obj.read_folder_metadata_full(&source_folder).await;
    let mut source_hidden_images: Vec<String> = source_metadata
        .as_ref()
        .map(|m| m.config.hidden_images.clone())
        .unwrap_or_default();

    // Get target folder metadata
    let target_metadata = gallery_obj.read_folder_metadata_full(&target_folder).await;
    let mut target_hidden_images: Vec<String> = target_metadata
        .as_ref()
        .map(|m| m.config.hidden_images.clone())
        .unwrap_or_default();

    // URL decode and resolve paths to actual filenames via indexer
    let filenames: Vec<(String, String)> = {
        let indexer = gallery_obj.image_indexer.read().await;
        request
            .paths
            .iter()
            .filter_map(|p| {
                // URL decode the path first
                let decoded_path = urlencoding::decode(p).ok()?.to_string();
                let url_id = decoded_path.rsplit('/').next()?.to_string();
                if let Some(resolved) = indexer.get_path(&decoded_path) {
                    let filename = resolved.rsplit('/').next()?.to_string();
                    Some((decoded_path, filename))
                } else {
                    // Fallback for filename indexing mode
                    Some((decoded_path, url_id))
                }
            })
            .collect()
    };

    let mut moved_count = 0;
    let mut errors = Vec::new();
    let storage = gallery_obj.source_storage();

    for (_url_path, filename) in &filenames {
        // Build source and destination paths
        let source_path = if source_folder.is_empty() {
            filename.clone()
        } else {
            format!("{}/{}", source_folder, filename)
        };

        let dest_path = if target_folder.is_empty() {
            filename.clone()
        } else {
            format!("{}/{}", target_folder, filename)
        };

        // Check if destination already has this file
        if storage.exists(&dest_path).await.unwrap_or(false) {
            errors.push(format!("File already exists at destination: {}", filename));
            continue;
        }

        // Move main image file
        let data = match storage.read(&source_path).await {
            Ok(data) => data,
            Err(e) => {
                errors.push(format!("Failed to read {}: {}", filename, e));
                continue;
            }
        };

        if let Err(e) = storage.write(&dest_path, data).await {
            errors.push(format!("Failed to write {}: {}", filename, e));
            continue;
        }

        if let Err(e) = storage.delete(&source_path).await {
            errors.push(format!(
                "Failed to delete source {}: {} (file was copied)",
                filename, e
            ));
        }

        // Move sidecar files (XMP, MD)
        let source_sidecars = find_existing_sidecars(storage, &source_path).await;
        let sidecar_transforms =
            transform_sidecar_paths(&source_sidecars, &source_path, &dest_path);

        for (src_sidecar, dest_sidecar) in sidecar_transforms {
            if let Ok(sidecar_data) = storage.read(&src_sidecar).await
                && storage.write(&dest_sidecar, sidecar_data).await.is_ok()
            {
                let _ = storage.delete(&src_sidecar).await;
            }
        }

        // Move version files from __versions folder
        let version_files = find_version_files(storage, &source_folder, filename).await;
        let dest_versions_folder = if target_folder.is_empty() {
            "__versions".to_string()
        } else {
            format!("{}/__versions", target_folder)
        };

        for (version_src, version_filename) in &version_files {
            let version_dest = format!("{}/{}", dest_versions_folder, version_filename);

            if let Ok(version_data) = storage.read(version_src).await
                && storage.write(&version_dest, version_data).await.is_ok()
            {
                let _ = storage.delete(version_src).await;

                // Also move sidecars for version files
                let version_sidecars = find_existing_sidecars(storage, version_src).await;
                let version_sidecar_transforms =
                    transform_sidecar_paths(&version_sidecars, version_src, &version_dest);
                for (src_sc, dest_sc) in version_sidecar_transforms {
                    if let Ok(sc_data) = storage.read(&src_sc).await
                        && storage.write(&dest_sc, sc_data).await.is_ok()
                    {
                        let _ = storage.delete(&src_sc).await;
                    }
                }

                // Queue cache cleanup/generation for version files
                if let Some(cache_queue) = app_state.cache_queue() {
                    let _ = cache_queue
                        .submit_cleanup(CacheCleanupRequest::new(
                            &site,
                            &gallery,
                            version_src.clone(),
                        ))
                        .await;
                    let _ = cache_queue
                        .submit(CacheGenerationRequest::new(
                            &site,
                            &gallery,
                            version_dest.clone(),
                        ))
                        .await;
                }
            }
        }

        // Handle hidden status - preserve it in the target folder
        if source_hidden_images.contains(filename) {
            source_hidden_images.retain(|f| f != filename);
            if !target_hidden_images.contains(filename) {
                target_hidden_images.push(filename.clone());
            }
        }

        // Queue cache cleanup for old path and generation for new path
        if let Some(cache_queue) = app_state.cache_queue() {
            // Cleanup old cache files
            let _ = cache_queue
                .submit_cleanup(CacheCleanupRequest::new(&site, &gallery, &source_path))
                .await;

            // Generate cache for new path
            let _ = cache_queue
                .submit(CacheGenerationRequest::new(&site, &gallery, &dest_path))
                .await;
        }

        moved_count += 1;
    }

    // Update source folder's hidden_images if changed
    if source_metadata.is_some() {
        let original_hidden = source_metadata
            .as_ref()
            .map(|m| m.config.hidden_images.clone())
            .unwrap_or_default();
        if source_hidden_images != original_hidden {
            update_folder_hidden_images(&gallery_obj, &source_folder, &source_hidden_images)
                .await?;
        }
    }

    // Update target folder's hidden_images if changed
    if !target_hidden_images.is_empty() {
        let original_hidden = target_metadata
            .as_ref()
            .map(|m| m.config.hidden_images.clone())
            .unwrap_or_default();
        if target_hidden_images != original_hidden {
            update_folder_hidden_images(&gallery_obj, &target_folder, &target_hidden_images)
                .await?;
        }
    }

    // Refresh folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh cache: {}", e)))?;

    Ok(Json(MoveImagesResponse {
        success: errors.is_empty(),
        moved_count,
        errors,
    }))
}

/// Move images (site-resolved version)
pub async fn move_gallery_images_resolved(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((gallery, source_folder)): Path<(String, String)>,
    Json(request): Json<MoveImagesRequest>,
) -> Result<Json<MoveImagesResponse>, AdminError> {
    let site = app_state.site.name.clone();
    move_gallery_images(
        ResolvedState(app_state),
        admin,
        Path((site, gallery, source_folder)),
        Json(request),
    )
    .await
}

/// Copy images from one folder to another
pub async fn copy_gallery_images(
    ResolvedState(app_state): ResolvedState,
    _admin: RequireAdmin,
    Path((site, gallery, source_folder)): Path<(String, String, String)>,
    Json(request): Json<CopyImagesRequest>,
) -> Result<Json<CopyImagesResponse>, AdminError> {
    // Verify site exists
    let config_storage = app_state
        .config_storage()
        .as_ref()
        .ok_or(AdminError::Internal("Config storage not configured".into()))?;

    if config_storage
        .get_site_config(&site)
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to check site: {}", e)))?
        .is_none()
    {
        return Err(AdminError::NotFound(format!("Site not found: {}", site)));
    }

    // Get gallery
    let gallery_obj = app_state
        .galleries()
        .get(&gallery)
        .ok_or_else(|| AdminError::NotFound(format!("Gallery not found: {}", gallery)))?
        .clone();

    // Decode folder paths
    let source_folder = urlencoding::decode(&source_folder)
        .map_err(|e| AdminError::BadRequest(format!("Invalid source folder encoding: {}", e)))?
        .to_string();

    let source_folder = if source_folder == "_root" {
        String::new()
    } else {
        source_folder
    };

    let target_folder = if request.target_folder == "_root" {
        String::new()
    } else {
        request.target_folder.clone()
    };

    // Can't copy to same folder
    if source_folder == target_folder {
        return Err(AdminError::BadRequest(
            "Cannot copy images to the same folder".into(),
        ));
    }

    // URL decode and resolve paths to actual filenames via indexer
    let filenames: Vec<(String, String)> = {
        let indexer = gallery_obj.image_indexer.read().await;
        request
            .paths
            .iter()
            .filter_map(|p| {
                // URL decode the path first
                let decoded_path = urlencoding::decode(p).ok()?.to_string();
                let url_id = decoded_path.rsplit('/').next()?.to_string();
                if let Some(resolved) = indexer.get_path(&decoded_path) {
                    let filename = resolved.rsplit('/').next()?.to_string();
                    Some((decoded_path, filename))
                } else {
                    // Fallback for filename indexing mode
                    Some((decoded_path, url_id))
                }
            })
            .collect()
    };

    let mut copied_count = 0;
    let mut errors = Vec::new();
    let storage = gallery_obj.source_storage();

    for (_url_path, filename) in &filenames {
        // Build source and destination paths
        let source_path = if source_folder.is_empty() {
            filename.clone()
        } else {
            format!("{}/{}", source_folder, filename)
        };

        let dest_path = if target_folder.is_empty() {
            filename.clone()
        } else {
            format!("{}/{}", target_folder, filename)
        };

        // Check if destination already has this file
        if storage.exists(&dest_path).await.unwrap_or(false) {
            errors.push(format!("File already exists at destination: {}", filename));
            continue;
        }

        // Copy main image file
        let data = match storage.read(&source_path).await {
            Ok(data) => data,
            Err(e) => {
                errors.push(format!("Failed to read {}: {}", filename, e));
                continue;
            }
        };

        if let Err(e) = storage.write(&dest_path, data).await {
            errors.push(format!("Failed to write {}: {}", filename, e));
            continue;
        }

        // Copy sidecar files (XMP, MD)
        let source_sidecars = find_existing_sidecars(storage, &source_path).await;
        let sidecar_transforms =
            transform_sidecar_paths(&source_sidecars, &source_path, &dest_path);

        for (src_sidecar, dest_sidecar) in sidecar_transforms {
            if let Ok(sidecar_data) = storage.read(&src_sidecar).await {
                let _ = storage.write(&dest_sidecar, sidecar_data).await;
            }
        }

        // Copy version files from __versions folder
        let version_files = find_version_files(storage, &source_folder, filename).await;
        let dest_versions_folder = if target_folder.is_empty() {
            "__versions".to_string()
        } else {
            format!("{}/__versions", target_folder)
        };

        for (version_src, version_filename) in &version_files {
            let version_dest = format!("{}/{}", dest_versions_folder, version_filename);

            if let Ok(version_data) = storage.read(version_src).await
                && storage.write(&version_dest, version_data).await.is_ok()
            {
                // Also copy sidecars for version files
                let version_sidecars = find_existing_sidecars(storage, version_src).await;
                let version_sidecar_transforms =
                    transform_sidecar_paths(&version_sidecars, version_src, &version_dest);
                for (src_sc, dest_sc) in version_sidecar_transforms {
                    if let Ok(sc_data) = storage.read(&src_sc).await {
                        let _ = storage.write(&dest_sc, sc_data).await;
                    }
                }

                // Queue cache generation for version files
                if let Some(cache_queue) = app_state.cache_queue() {
                    let _ = cache_queue
                        .submit(CacheGenerationRequest::new(&site, &gallery, &version_dest))
                        .await;
                }
            }
        }

        // Queue cache generation for new path
        if let Some(cache_queue) = app_state.cache_queue() {
            let _ = cache_queue
                .submit(CacheGenerationRequest::new(&site, &gallery, &dest_path))
                .await;
        }

        copied_count += 1;
    }

    // Refresh folder cache
    gallery_obj
        .refresh_folder_cache()
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to refresh cache: {}", e)))?;

    Ok(Json(CopyImagesResponse {
        success: errors.is_empty(),
        copied_count,
        errors,
    }))
}

/// Copy images (site-resolved version)
pub async fn copy_gallery_images_resolved(
    ResolvedState(app_state): ResolvedState,
    admin: RequireAdmin,
    Path((gallery, source_folder)): Path<(String, String)>,
    Json(request): Json<CopyImagesRequest>,
) -> Result<Json<CopyImagesResponse>, AdminError> {
    let site = app_state.site.name.clone();
    copy_gallery_images(
        ResolvedState(app_state),
        admin,
        Path((site, gallery, source_folder)),
        Json(request),
    )
    .await
}

/// Helper to update a folder's hidden_images list
async fn update_folder_hidden_images(
    gallery: &crate::gallery::SharedGallery,
    folder_path: &str,
    hidden_images: &[String],
) -> Result<(), AdminError> {
    // Read current metadata
    let current_metadata = gallery.read_folder_metadata_full(folder_path).await;

    let (hidden, permissions, description) = match &current_metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                PermissionConfigDto {
                    site_admins: perms.site_admins.clone(),
                    public_role: perms.public_role.clone(),
                    default_authenticated_role: perms.default_authenticated_role.clone(),
                    roles: perms
                        .roles
                        .iter()
                        .map(|(name, role)| {
                            (
                                name.clone(),
                                RoleDto {
                                    name: name.clone(),
                                    permissions: RolePermissionsDto::from(&role.permissions),
                                    inherits: role.inherits.clone(),
                                    is_builtin: false,
                                },
                            )
                        })
                        .collect(),
                    user_roles: perms
                        .user_roles
                        .iter()
                        .map(|ur| UserRoleAssignment {
                            username: ur.username.clone(),
                            roles: ur.roles.clone(),
                        })
                        .collect(),
                },
                meta.description_markdown.clone(),
            )
        }
        None => (
            false,
            PermissionConfigDto {
                site_admins: Vec::new(),
                public_role: None,
                default_authenticated_role: None,
                roles: std::collections::HashMap::new(),
                user_roles: vec![],
            },
            String::new(),
        ),
    };

    // Build TOML content
    let mut toml_content = toml_edit::DocumentMut::new();
    toml_content["hidden"] = toml_edit::value(hidden);

    // Add hidden_images array
    if !hidden_images.is_empty() {
        let hidden_images_array: toml_edit::Array = hidden_images
            .iter()
            .map(|img| toml_edit::Value::from(img.clone()))
            .collect();
        toml_content["hidden_images"] = toml_edit::value(hidden_images_array);
    }

    // Add permissions section if non-empty
    let mut permissions_table = toml_edit::Table::new();
    if let Some(ref public_role) = permissions.public_role {
        permissions_table["public_role"] = toml_edit::value(public_role.clone());
    }
    if let Some(ref default_authenticated_role) = permissions.default_authenticated_role {
        permissions_table["default_authenticated_role"] =
            toml_edit::value(default_authenticated_role.clone());
    }
    if !permissions_table.is_empty() {
        toml_content["permissions"] = toml_edit::Item::Table(permissions_table);
    }

    // Build full content
    let toml_str = toml_content.to_string();
    let content = if toml_str.trim().is_empty() && description.is_empty() {
        String::new()
    } else if toml_str.trim().is_empty() {
        description
    } else {
        format!("+++\n{}+++\n{}", toml_str, description)
    };

    // Determine file path
    let folder_md_path = if folder_path.is_empty() {
        "_folder.md".to_string()
    } else {
        format!("{}/_folder.md", folder_path)
    };

    // Write file
    gallery
        .source_storage()
        .write(&folder_md_path, bytes::Bytes::from(content))
        .await
        .map_err(|e| AdminError::Internal(format!("Failed to write _folder.md: {}", e)))?;

    Ok(())
}
