use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
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

    let (hidden, permissions, description) = match metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                PermissionConfigDto {
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
                PermissionConfigDto {
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
        permissions: request.permissions,
        description: request.description,
    }))
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

    let (hidden, mut permissions, description) = match current_metadata {
        Some(meta) => {
            let perms = &meta.config.permissions;
            (
                meta.config.hidden,
                PermissionConfigDto {
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
            PermissionConfigDto {
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
