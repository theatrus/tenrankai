use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Permissions that can be assigned to a role
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RolePermissions {
    // Viewing permissions
    #[serde(default)]
    pub can_view: bool,
    #[serde(default)]
    pub can_see_technical_details: bool,
    #[serde(default)]
    pub can_see_exact_dates: bool,
    #[serde(default)]
    pub can_see_location: bool,

    // Download permissions
    #[serde(default)]
    pub can_download_medium: bool,
    #[serde(default)]
    pub can_download_large: bool,
    #[serde(default)]
    pub can_download_original: bool,
    #[serde(default)]
    pub can_download_gallery: bool, // Download entire gallery/folder as zip
    #[serde(default)]
    pub can_download_raw: bool, // Download associated RAW files

    // Metadata permissions
    #[serde(default)]
    pub can_read_metadata: bool,
    #[serde(default)]
    pub can_edit_content: bool, // Edit folder/image descriptions
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

    // Moderation permissions
    #[serde(default)]
    pub can_edit_any_comments: bool,
    #[serde(default)]
    pub can_delete_any_comments: bool,

    // Interactive permissions
    #[serde(default)]
    pub can_use_zoom: bool,
    #[serde(default)]
    pub can_use_tile_zoom: bool,

    // AI permissions
    #[serde(default)]
    pub can_analyze_images: bool,
    #[serde(default)]
    pub can_see_ai_analysis: bool,
    #[serde(default)]
    pub can_see_ai_alt_text: bool,

    // Special permissions
    #[serde(default)]
    pub owner_access: bool, // Bypasses all restrictions
}

impl RolePermissions {
    /// Create a new empty permission set
    pub fn new() -> Self {
        Self::default()
    }

    /// Create permissions with owner access (all permissions)
    pub fn owner() -> Self {
        Self {
            owner_access: true,
            ..Default::default()
        }
    }

    /// Merge permissions from another role (OR operation)
    /// Most permissive wins
    pub fn merge(&mut self, other: &RolePermissions) {
        self.can_view |= other.can_view;
        self.can_see_technical_details |= other.can_see_technical_details;
        self.can_see_exact_dates |= other.can_see_exact_dates;
        self.can_see_location |= other.can_see_location;

        self.can_download_medium |= other.can_download_medium;
        self.can_download_large |= other.can_download_large;
        self.can_download_original |= other.can_download_original;
        self.can_download_gallery |= other.can_download_gallery;
        self.can_download_raw |= other.can_download_raw;

        self.can_read_metadata |= other.can_read_metadata;
        self.can_edit_content |= other.can_edit_content;
        self.can_add_comments |= other.can_add_comments;
        self.can_edit_own_comments |= other.can_edit_own_comments;
        self.can_delete_own_comments |= other.can_delete_own_comments;
        self.can_set_picks |= other.can_set_picks;
        self.can_add_tags |= other.can_add_tags;

        self.can_edit_any_comments |= other.can_edit_any_comments;
        self.can_delete_any_comments |= other.can_delete_any_comments;

        self.can_use_zoom |= other.can_use_zoom;
        self.can_use_tile_zoom |= other.can_use_tile_zoom;

        self.can_analyze_images |= other.can_analyze_images;
        self.can_see_ai_analysis |= other.can_see_ai_analysis;
        self.can_see_ai_alt_text |= other.can_see_ai_alt_text;

        self.owner_access |= other.owner_access;
    }

    /// Check if these permissions grant owner access
    pub fn is_owner(&self) -> bool {
        self.owner_access
    }

    /// Override all permissions if owner_access is true
    pub fn apply_owner_override(&mut self) {
        if self.owner_access {
            self.can_view = true;
            self.can_see_technical_details = true;
            self.can_see_exact_dates = true;
            self.can_see_location = true;

            self.can_download_medium = true;
            self.can_download_large = true;
            self.can_download_original = true;
            self.can_download_gallery = true;
            self.can_download_raw = true;

            self.can_read_metadata = true;
            self.can_edit_content = true;
            self.can_add_comments = true;
            self.can_edit_own_comments = true;
            self.can_delete_own_comments = true;
            self.can_set_picks = true;
            self.can_add_tags = true;

            self.can_edit_any_comments = true;
            self.can_delete_any_comments = true;

            self.can_use_zoom = true;
            self.can_use_tile_zoom = true;

            self.can_analyze_images = true;
            self.can_see_ai_analysis = true;
            self.can_see_ai_alt_text = true;
        }
    }
}

/// A named role with permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub name: String,
    pub permissions: RolePermissions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>, // Name of parent role to inherit from
}

impl Role {
    pub fn new(name: String, permissions: RolePermissions) -> Self {
        Self {
            name,
            permissions,
            inherits: None,
        }
    }

    pub fn with_inheritance(name: String, permissions: RolePermissions, inherits: String) -> Self {
        Self {
            name,
            permissions,
            inherits: Some(inherits),
        }
    }
}

/// User to role assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserRole {
    pub username: String,
    pub roles: Vec<String>, // Role names assigned to this user
}

impl UserRole {
    pub fn new(username: String, roles: Vec<String>) -> Self {
        Self { username, roles }
    }
}

/// Gallery or folder permission configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionConfig {
    /// Role assigned to unauthenticated users
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_role: Option<String>,

    /// Role assigned to authenticated users without specific roles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_authenticated_role: Option<String>,

    /// Defined roles
    #[serde(default)]
    pub roles: HashMap<String, Role>,

    /// User role assignments
    #[serde(default)]
    pub user_roles: Vec<UserRole>,
}

impl PermissionConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the default roles that are always available
    pub fn default_roles() -> HashMap<String, Role> {
        let mut roles = HashMap::new();

        // Basic viewer role
        roles.insert(
            "viewer".to_string(),
            Role::new(
                "viewer".to_string(),
                RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    ..Default::default()
                },
            ),
        );

        // Contributor role (authenticated users default)
        roles.insert(
            "contributor".to_string(),
            Role::new(
                "contributor".to_string(),
                RolePermissions {
                    can_view: true,
                    can_see_technical_details: true,
                    can_see_exact_dates: true,
                    can_see_location: true,
                    can_download_medium: true,
                    can_download_large: true,
                    can_download_gallery: true,
                    can_download_raw: true,
                    can_read_metadata: true,
                    can_add_comments: true,
                    can_edit_own_comments: true,
                    can_delete_own_comments: true,
                    can_set_picks: true,
                    can_add_tags: true,
                    ..Default::default()
                },
            ),
        );

        // Admin role with full access
        roles.insert(
            "admin".to_string(),
            Role::new("admin".to_string(), RolePermissions::owner()),
        );

        roles
    }

    /// Find roles assigned to a specific user
    pub fn get_user_roles(&self, username: &str) -> Option<&[String]> {
        self.user_roles
            .iter()
            .find(|ur| ur.username == username)
            .map(|ur| ur.roles.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_merge() {
        let mut base = RolePermissions {
            can_view: true,
            can_download_medium: true,
            ..Default::default()
        };

        let additional = RolePermissions {
            can_see_technical_details: true,
            can_download_large: true,
            ..Default::default()
        };

        base.merge(&additional);

        assert!(base.can_view);
        assert!(base.can_download_medium);
        assert!(base.can_see_technical_details);
        assert!(base.can_download_large);
        assert!(!base.can_download_original);
    }

    #[test]
    fn test_owner_override() {
        let mut perms = RolePermissions {
            can_view: true,
            owner_access: true,
            ..Default::default()
        };

        perms.apply_owner_override();

        // All permissions should be true
        assert!(perms.can_view);
        assert!(perms.can_see_technical_details);
        assert!(perms.can_download_original);
        assert!(perms.can_delete_any_comments);
    }

    #[test]
    fn test_default_roles() {
        let roles = PermissionConfig::default_roles();

        // Check viewer role
        let viewer = roles.get("viewer").unwrap();
        assert!(viewer.permissions.can_view);
        assert!(viewer.permissions.can_download_medium);
        assert!(!viewer.permissions.can_see_technical_details);

        // Check contributor role
        let contributor = roles.get("contributor").unwrap();
        assert!(contributor.permissions.can_view);
        assert!(contributor.permissions.can_add_comments);
        assert!(!contributor.permissions.can_download_original);

        // Check admin role
        let admin = roles.get("admin").unwrap();
        assert!(admin.permissions.owner_access);
    }
}
