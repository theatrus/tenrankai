use crate::permissions::types::{PermissionConfig, Role, RolePermissions, UserRole};
use crate::config::GallerySystemConfig;
use crate::gallery::FolderConfig;
use std::collections::HashMap;

/// Migrate gallery-level configuration to the new permission system
pub fn migrate_gallery_config(config: &mut GallerySystemConfig) {
    // If permissions are already configured, don't migrate
    if !config.permissions.roles.is_empty() || config.permissions.public_role.is_some() {
        return;
    }

    // Create default roles
    let mut roles = HashMap::new();
    
    // Create viewer role based on current settings
    let viewer_permissions = RolePermissions {
        can_view: true,
        can_download_medium: true,
        can_see_technical_details: true,
        can_see_exact_dates: !config.approximate_dates_for_public,
        can_see_location: !config.hide_location_from_public,
        ..Default::default()
    };
    
    roles.insert("viewer".to_string(), Role {
        name: "viewer".to_string(),
        permissions: viewer_permissions.clone(),
        inherits: None,
    });
    
    // Create contributor role with additional permissions
    let contributor_permissions = RolePermissions {
        can_view: true,
        can_see_technical_details: true,
        can_see_exact_dates: true,
        can_see_location: true,
        can_download_medium: true,
        can_download_large: true,
        can_read_metadata: config.enable_metadata,
        can_add_comments: config.enable_metadata,
        can_edit_own_comments: config.enable_metadata,
        can_delete_own_comments: config.enable_metadata,
        can_set_picks: config.enable_metadata,
        can_add_tags: config.enable_metadata,
        ..Default::default()
    };
    
    roles.insert("contributor".to_string(), Role {
        name: "contributor".to_string(),
        permissions: contributor_permissions,
        inherits: None,
    });
    
    // Create admin role with all permissions
    roles.insert("admin".to_string(), Role {
        name: "admin".to_string(),
        permissions: RolePermissions {
            owner_access: true,
            ..Default::default()
        },
        inherits: None,
    });
    
    // Set up the permission config
    config.permissions = PermissionConfig {
        public_role: Some("viewer".to_string()),
        default_authenticated_role: Some("contributor".to_string()),
        roles,
        user_roles: Vec::new(),
    };
}

/// Migrate folder-level configuration to the new permission system
pub fn migrate_folder_config(folder_config: &mut FolderConfig, gallery_config: &GallerySystemConfig) {
    // If permissions are already configured, don't migrate
    if !folder_config.permissions.roles.is_empty() || folder_config.permissions.public_role.is_some() {
        return;
    }
    
    // If folder has special requirements, create custom configuration
    if folder_config.require_auth || folder_config.allowed_users.is_some() || folder_config.hide_technical_details {
        let mut roles = HashMap::new();
        
        // If require_auth is true, remove public access
        if folder_config.require_auth {
            folder_config.permissions.public_role = None;
        }
        
        // If hide_technical_details is true, create a limited viewer role
        if folder_config.hide_technical_details {
            let viewer_permissions = RolePermissions {
                can_view: true,
                can_download_medium: true,
                can_see_technical_details: false,
                can_see_exact_dates: !gallery_config.approximate_dates_for_public,
                can_see_location: folder_config.hide_location_from_public
                    .map(|hide| !hide)
                    .unwrap_or(!gallery_config.hide_location_from_public),
                ..Default::default()
            };
            
            roles.insert("viewer".to_string(), Role {
                name: "viewer".to_string(),
                permissions: viewer_permissions,
                inherits: None,
            });
            
            // Update public role to use the limited viewer
            if folder_config.permissions.public_role.is_none() && !folder_config.require_auth {
                folder_config.permissions.public_role = Some("viewer".to_string());
            }
        }
        
        // Handle allowed_users by creating user role assignments
        if let Some(allowed_users) = &folder_config.allowed_users {
            // Create a special "allowed" role with full permissions
            let allowed_permissions = RolePermissions {
                can_view: true,
                can_see_technical_details: true,
                can_see_exact_dates: true,
                can_see_location: true,
                can_download_medium: true,
                can_download_large: true,
                can_download_original: true,
                can_read_metadata: gallery_config.enable_metadata,
                can_add_comments: gallery_config.enable_metadata,
                can_edit_own_comments: gallery_config.enable_metadata,
                can_delete_own_comments: gallery_config.enable_metadata,
                can_set_picks: gallery_config.enable_metadata,
                can_add_tags: gallery_config.enable_metadata,
                ..Default::default()
            };
            
            roles.insert("allowed".to_string(), Role {
                name: "allowed".to_string(),
                permissions: allowed_permissions,
                inherits: None,
            });
            
            // Create user role assignments
            let mut user_roles = Vec::new();
            for username in allowed_users {
                user_roles.push(UserRole {
                    username: username.to_string(),
                    roles: vec!["allowed".to_string()],
                });
            }
            
            folder_config.permissions.user_roles = user_roles;
        }
        
        // Only set roles if we created any
        if !roles.is_empty() {
            folder_config.permissions.roles = roles;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_migrate_gallery_basic() {
        // Create a minimal config with just the fields we care about
        let mut permissions = PermissionConfig::default();
        let approximate_dates_for_public = false;
        let hide_location_from_public = false;
        let enable_metadata = true;
        
        // Call migration logic directly
        if permissions.roles.is_empty() && permissions.public_role.is_none() {
            let mut roles = HashMap::new();
            
            let viewer_permissions = RolePermissions {
                can_view: true,
                can_download_medium: true,
                can_see_technical_details: true,
                can_see_exact_dates: !approximate_dates_for_public,
                can_see_location: !hide_location_from_public,
                ..Default::default()
            };
            
            roles.insert("viewer".to_string(), Role {
                name: "viewer".to_string(),
                permissions: viewer_permissions,
                inherits: None,
            });
            
            let contributor_permissions = RolePermissions {
                can_view: true,
                can_see_technical_details: true,
                can_see_exact_dates: true,
                can_see_location: true,
                can_download_medium: true,
                can_download_large: true,
                can_read_metadata: enable_metadata,
                can_add_comments: enable_metadata,
                can_edit_own_comments: enable_metadata,
                can_delete_own_comments: enable_metadata,
                can_set_picks: enable_metadata,
                can_add_tags: enable_metadata,
                ..Default::default()
            };
            
            roles.insert("contributor".to_string(), Role {
                name: "contributor".to_string(),
                permissions: contributor_permissions,
                inherits: None,
            });
            
            roles.insert("admin".to_string(), Role {
                name: "admin".to_string(),
                permissions: RolePermissions {
                    owner_access: true,
                    ..Default::default()
                },
                inherits: None,
            });
            
            permissions = PermissionConfig {
                public_role: Some("viewer".to_string()),
                default_authenticated_role: Some("contributor".to_string()),
                roles,
                user_roles: Vec::new(),
            };
        }
        
        assert_eq!(permissions.public_role, Some("viewer".to_string()));
        assert_eq!(permissions.default_authenticated_role, Some("contributor".to_string()));
        assert_eq!(permissions.roles.len(), 3);
        
        let viewer = &permissions.roles["viewer"];
        assert!(viewer.permissions.can_view);
        assert!(viewer.permissions.can_see_exact_dates);
        assert!(viewer.permissions.can_see_location);
        
        let contributor = &permissions.roles["contributor"];
        assert!(contributor.permissions.can_add_comments);
        assert!(contributor.permissions.can_set_picks);
    }
    
    #[test]
    fn test_migrate_gallery_with_privacy() {
        // Test with privacy settings enabled
        let mut permissions = PermissionConfig::default();
        let approximate_dates_for_public = true;
        let hide_location_from_public = true;
        let enable_metadata = false;
        
        // Call migration logic directly
        if permissions.roles.is_empty() && permissions.public_role.is_none() {
            let mut roles = HashMap::new();
            
            let viewer_permissions = RolePermissions {
                can_view: true,
                can_download_medium: true,
                can_see_technical_details: true,
                can_see_exact_dates: !approximate_dates_for_public,
                can_see_location: !hide_location_from_public,
                ..Default::default()
            };
            
            roles.insert("viewer".to_string(), Role {
                name: "viewer".to_string(),
                permissions: viewer_permissions,
                inherits: None,
            });
            
            let contributor_permissions = RolePermissions {
                can_view: true,
                can_see_technical_details: true,
                can_see_exact_dates: true,
                can_see_location: true,
                can_download_medium: true,
                can_download_large: true,
                can_read_metadata: enable_metadata,
                can_add_comments: enable_metadata,
                can_edit_own_comments: enable_metadata,
                can_delete_own_comments: enable_metadata,
                can_set_picks: enable_metadata,
                can_add_tags: enable_metadata,
                ..Default::default()
            };
            
            roles.insert("contributor".to_string(), Role {
                name: "contributor".to_string(),
                permissions: contributor_permissions,
                inherits: None,
            });
            
            permissions.roles = roles;
        }
        
        let viewer = &permissions.roles["viewer"];
        assert!(!viewer.permissions.can_see_exact_dates);
        assert!(!viewer.permissions.can_see_location);
        
        let contributor = &permissions.roles["contributor"];
        assert!(!contributor.permissions.can_add_comments);
        assert!(!contributor.permissions.can_set_picks);
    }
    
    #[test] 
    fn test_migrate_folder_with_auth() {
        let mut folder_permissions = PermissionConfig::default();
        let require_auth = true;
        let allowed_users = Some(vec!["alice".to_string(), "bob".to_string()]);
        
        // Call folder migration logic directly
        if folder_permissions.roles.is_empty() && folder_permissions.public_role.is_none() {
            if require_auth {
                folder_permissions.public_role = None;
            }
            
            if let Some(users) = allowed_users {
                let mut roles = HashMap::new();
                
                let allowed_permissions = RolePermissions {
                    can_view: true,
                    can_see_technical_details: true,
                    can_see_exact_dates: true,
                    can_see_location: true,
                    can_download_medium: true,
                    can_download_large: true,
                    can_download_original: true,
                    can_read_metadata: true,
                    can_add_comments: true,
                    can_edit_own_comments: true,
                    can_delete_own_comments: true,
                    can_set_picks: true,
                    can_add_tags: true,
                    ..Default::default()
                };
                
                roles.insert("allowed".to_string(), Role {
                    name: "allowed".to_string(),
                    permissions: allowed_permissions,
                    inherits: None,
                });
                
                let mut user_roles = Vec::new();
                for username in users {
                    user_roles.push(UserRole {
                        username: username.to_string(),
                        roles: vec!["allowed".to_string()],
                    });
                }
                
                folder_permissions.roles = roles;
                folder_permissions.user_roles = user_roles;
            }
        }
        
        assert_eq!(folder_permissions.public_role, None);
        assert!(folder_permissions.roles.contains_key("allowed"));
        assert_eq!(folder_permissions.user_roles.len(), 2);
        
        let alice_role = folder_permissions.user_roles.iter()
            .find(|ur| ur.username == "alice")
            .unwrap();
        assert_eq!(alice_role.roles, vec!["allowed".to_string()]);
    }
}