use super::error::PermissionError;
use super::types::{PermissionConfig, Role, RolePermissions};
use std::collections::{HashMap, HashSet};

/// Resolves permissions by merging roles with inheritance
pub struct PermissionResolver<'a> {
    gallery_config: &'a PermissionConfig,
    folder_config: Option<&'a PermissionConfig>,
}

impl<'a> PermissionResolver<'a> {
    /// Create a new resolver with gallery config and optional folder config
    pub fn new(gallery_config: &'a PermissionConfig, folder_config: Option<&'a PermissionConfig>) -> Self {
        Self {
            gallery_config,
            folder_config,
        }
    }

    /// Resolve permissions for a user (authenticated or not)
    pub fn resolve_user_permissions(&self, username: Option<&str>) -> Result<RolePermissions, PermissionError> {
        match username {
            Some(user) => self.resolve_authenticated_user_permissions(user),
            None => self.resolve_public_permissions(),
        }
    }

    /// Resolve permissions for an unauthenticated user
    fn resolve_public_permissions(&self) -> Result<RolePermissions, PermissionError> {
        // Check folder-level public role first
        if let Some(folder_config) = self.folder_config {
            if let Some(public_role) = &folder_config.public_role {
                if public_role == "none" {
                    return Ok(RolePermissions::default()); // No access
                }
                // Try to resolve from folder roles first
                if let Ok(perms) = self.resolve_role_permissions(public_role, folder_config) {
                    return Ok(perms);
                }
                // Fall back to gallery roles
                return self.resolve_role_permissions(public_role, self.gallery_config);
            }
        }

        // Fall back to gallery-level public role
        if let Some(public_role) = &self.gallery_config.public_role {
            if public_role == "none" {
                return Ok(RolePermissions::default()); // No access
            }
            return self.resolve_role_permissions(public_role, self.gallery_config);
        }

        // No public role defined - default to viewer
        self.resolve_role_permissions("viewer", self.gallery_config)
            .or_else(|_| Ok(self.default_viewer_permissions()))
    }

    /// Resolve permissions for an authenticated user
    fn resolve_authenticated_user_permissions(&self, username: &str) -> Result<RolePermissions, PermissionError> {
        let mut final_permissions = RolePermissions::default();
        let mut found_roles = false;

        // Check folder-level user roles
        if let Some(folder_config) = self.folder_config {
            if let Some(role_names) = folder_config.get_user_roles(username) {
                found_roles = true;
                for role_name in role_names {
                    // Try folder roles first, then gallery roles
                    let role_perms = self.resolve_role_permissions(role_name, folder_config)
                        .or_else(|_| self.resolve_role_permissions(role_name, self.gallery_config))?;
                    final_permissions.merge(&role_perms);
                }
            }
        }

        // Check gallery-level user roles
        if let Some(role_names) = self.gallery_config.get_user_roles(username) {
            found_roles = true;
            for role_name in role_names {
                let role_perms = self.resolve_role_permissions(role_name, self.gallery_config)?;
                final_permissions.merge(&role_perms);
            }
        }

        // If no specific roles found, use default authenticated role
        if !found_roles {
            // Check folder default first
            if let Some(folder_config) = self.folder_config {
                if let Some(default_role) = &folder_config.default_authenticated_role {
                    let default_perms = self.resolve_role_permissions(default_role, folder_config)
                        .or_else(|_| self.resolve_role_permissions(default_role, self.gallery_config))?;
                    final_permissions.merge(&default_perms);
                    found_roles = true;
                }
            }

            // Fall back to gallery default
            if !found_roles {
                if let Some(default_role) = &self.gallery_config.default_authenticated_role {
                    let default_perms = self.resolve_role_permissions(default_role, self.gallery_config)?;
                    final_permissions.merge(&default_perms);
                } else {
                    // Default to contributor if nothing specified
                    let default_perms = self.default_contributor_permissions();
                    final_permissions.merge(&default_perms);
                }
            }
        }

        // Apply owner override if needed
        final_permissions.apply_owner_override();

        Ok(final_permissions)
    }

    /// Resolve a single role with inheritance
    fn resolve_role_permissions(
        &self,
        role_name: &str,
        config: &PermissionConfig,
    ) -> Result<RolePermissions, PermissionError> {
        let mut visited = HashSet::new();
        self.resolve_role_with_inheritance(role_name, config, &mut visited)
    }

    /// Recursively resolve role permissions with inheritance
    fn resolve_role_with_inheritance(
        &self,
        role_name: &str,
        config: &PermissionConfig,
        visited: &mut HashSet<String>,
    ) -> Result<RolePermissions, PermissionError> {
        // Check for circular inheritance
        if !visited.insert(role_name.to_string()) {
            return Err(PermissionError::CircularInheritance(role_name.to_string()));
        }

        // Get all available roles (merge default and configured)
        let all_roles = self.get_all_roles(config);

        // Find the role
        let role = all_roles
            .get(role_name)
            .ok_or_else(|| PermissionError::RoleNotFound(role_name.to_string()))?;

        let mut permissions = role.permissions.clone();

        // Handle inheritance
        if let Some(parent_name) = &role.inherits {
            // Try to find parent in current config, then in gallery config
            let parent_perms = self.resolve_role_with_inheritance(parent_name, config, visited)
                .or_else(|e| {
                    // If it's a circular inheritance error, propagate it
                    if matches!(e, PermissionError::CircularInheritance(_)) {
                        return Err(e);
                    }
                    
                    if std::ptr::eq(config, self.gallery_config) {
                        // Already at gallery level, can't go higher
                        Err(PermissionError::RoleNotFound(parent_name.clone()))
                    } else {
                        // Try gallery config
                        self.resolve_role_with_inheritance(parent_name, self.gallery_config, visited)
                    }
                })?;

            // Child permissions override parent
            let child_perms = permissions.clone();
            permissions = parent_perms;
            permissions.merge(&child_perms);
        }

        Ok(permissions)
    }

    /// Get all roles including defaults and configured ones
    fn get_all_roles<'b>(&self, config: &'b PermissionConfig) -> HashMap<String, Role> {
        let mut all_roles = HashMap::new();

        // Add default roles
        for (name, role) in PermissionConfig::default_roles() {
            all_roles.insert(name.clone(), role);
        }

        // Add/override with configured roles
        for (name, role) in &config.roles {
            all_roles.insert(name.clone(), role.clone());
        }

        all_roles
    }

    /// Default viewer permissions if no role is found
    fn default_viewer_permissions(&self) -> RolePermissions {
        RolePermissions {
            can_view: true,
            can_download_medium: true,
            ..Default::default()
        }
    }

    /// Default contributor permissions for authenticated users
    fn default_contributor_permissions(&self) -> RolePermissions {
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
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::UserRole;

    #[test]
    fn test_public_user_default() {
        let config = PermissionConfig::default();
        let resolver = PermissionResolver::new(&config, None);
        
        let perms = resolver.resolve_user_permissions(None).unwrap();
        assert!(perms.can_view);
        assert!(perms.can_download_medium);
        assert!(!perms.can_see_technical_details);
    }

    #[test]
    fn test_authenticated_user_default() {
        let config = PermissionConfig::default();
        let resolver = PermissionResolver::new(&config, None);
        
        let perms = resolver.resolve_user_permissions(Some("testuser")).unwrap();
        assert!(perms.can_view);
        assert!(perms.can_see_technical_details);
        assert!(perms.can_add_comments);
    }

    #[test]
    fn test_role_inheritance() {
        let mut config = PermissionConfig::default();
        
        // Create base role
        config.roles.insert("base".to_string(), Role::new(
            "base".to_string(),
            RolePermissions {
                can_view: true,
                can_download_medium: true,
                ..Default::default()
            }
        ));

        // Create child role that inherits from base
        config.roles.insert("child".to_string(), Role::with_inheritance(
            "child".to_string(),
            RolePermissions {
                can_see_technical_details: true,
                ..Default::default()
            },
            "base".to_string()
        ));

        config.user_roles.push(UserRole::new("testuser".to_string(), vec!["child".to_string()]));

        let resolver = PermissionResolver::new(&config, None);
        let perms = resolver.resolve_user_permissions(Some("testuser")).unwrap();

        // Should have both base and child permissions
        assert!(perms.can_view);
        assert!(perms.can_download_medium);
        assert!(perms.can_see_technical_details);
    }

    #[test]
    fn test_folder_override() {
        let gallery_config = PermissionConfig {
            public_role: Some("viewer".to_string()),
            ..Default::default()
        };

        let folder_config = PermissionConfig {
            public_role: Some("none".to_string()),
            ..Default::default()
        };

        let resolver = PermissionResolver::new(&gallery_config, Some(&folder_config));
        let perms = resolver.resolve_user_permissions(None).unwrap();

        // Folder says no public access
        assert!(!perms.can_view);
    }

    #[test]
    fn test_owner_access() {
        let mut config = PermissionConfig::default();
        config.roles.insert("owner".to_string(), Role::new(
            "owner".to_string(),
            RolePermissions {
                owner_access: true,
                ..Default::default()
            }
        ));
        config.user_roles.push(UserRole::new("admin".to_string(), vec!["owner".to_string()]));

        let resolver = PermissionResolver::new(&config, None);
        let perms = resolver.resolve_user_permissions(Some("admin")).unwrap();

        // Owner should have all permissions
        assert!(perms.can_view);
        assert!(perms.can_download_original);
        assert!(perms.can_delete_any_comments);
        assert!(perms.owner_access);
    }

    #[test]
    fn test_multiple_roles() {
        let mut config = PermissionConfig::default();
        config.user_roles.push(UserRole::new(
            "testuser".to_string(),
            vec!["viewer".to_string(), "contributor".to_string()]
        ));

        let resolver = PermissionResolver::new(&config, None);
        let perms = resolver.resolve_user_permissions(Some("testuser")).unwrap();

        // Should have combined permissions
        assert!(perms.can_view);
        assert!(perms.can_see_technical_details); // from contributor
        assert!(perms.can_add_comments); // from contributor
    }

    #[test]
    fn test_circular_inheritance() {
        let mut config = PermissionConfig::default();
        
        // Create circular inheritance
        config.roles.insert("role_a".to_string(), Role::with_inheritance(
            "role_a".to_string(),
            RolePermissions::default(),
            "role_b".to_string()
        ));
        config.roles.insert("role_b".to_string(), Role::with_inheritance(
            "role_b".to_string(),
            RolePermissions::default(),
            "role_a".to_string()
        ));
        config.user_roles.push(UserRole::new("testuser".to_string(), vec!["role_a".to_string()]));

        let resolver = PermissionResolver::new(&config, None);
        let result = resolver.resolve_user_permissions(Some("testuser"));
        
        assert!(matches!(result, Err(PermissionError::CircularInheritance(_))));
    }
}
