use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};

use super::error::AdminError;
use super::extractors::RequireAdmin;
use super::types::*;
use crate::permissions::types::RolePermissions;
use crate::site::ResolvedState;

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
        Err(AdminError::NotFound(format!("User not found: {}", username)))
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
    let galleries: Vec<GalleryInfo> = app_state
        .galleries()
        .iter()
        .map(|(name, gallery)| {
            let config = gallery.get_config();
            let permissions = &config.permissions;

            GalleryInfo {
                name: name.clone(),
                url_prefix: config.url_prefix.clone(),
                permissions: PermissionConfigDto {
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
            }
        })
        .collect();

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

    Ok(Json(GalleryInfo {
        name,
        url_prefix: config.url_prefix.clone(),
        permissions: PermissionConfigDto {
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

    Ok(Json(GalleryInfo {
        name: gallery_name,
        url_prefix: gallery_config.url_prefix.clone(),
        permissions: PermissionConfigDto {
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
        gallery_count,
        posts_count,
    }))
}

/// Update a site (NOTE: storage_prefix cannot be edited via API)
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

    // Get the config_storage URL from the app config
    let config_storage_url = app_state
        .config
        .app
        .config_storage
        .as_ref()
        .ok_or(AdminError::Internal(
            "Config storage URL not configured".into(),
        ))?;

    // Create a ConfigStorageLoader
    let loader = crate::config::ConfigStorageLoader::new(
        config_storage.clone(),
        app_state.config.app.cookie_secret.clone(),
    );

    // Reload the site
    match site_manager.reload_site(&site, &loader, config_storage_url).await {
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
