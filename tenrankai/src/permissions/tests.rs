use super::*;
use crate::config::GallerySystemConfig;
use crate::gallery::FolderConfig;
use crate::permissions::PermissionResolver;
use std::collections::HashMap;

// Helper function to create a test gallery config
fn create_test_gallery_config(name: &str) -> GallerySystemConfig {
    GallerySystemConfig {
        name: name.to_string(),
        url_prefix: format!("/{}", name),
        source_directory: format!("test/{}", name),
        cache_directory: format!("cache/{}", name),
        ..Default::default()
    }
}

#[cfg(test)]
mod scenario_tests {
    use super::*;

    #[test]
    fn test_public_portfolio_scenario() {
        // Scenario: Public portfolio where anyone can view but only clients can download
        let mut gallery_config = create_test_gallery_config("portfolio");

        // Set up permissions
        let mut roles = HashMap::new();

        // Public viewer - can only view images
        roles.insert(
            "public_viewer".to_string(),
            Role {
                name: "public_viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: false,
                    can_see_technical_details: false,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Client - can view and download
        roles.insert(
            "client".to_string(),
            Role {
                name: "client".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_download_large: true,
                    can_see_technical_details: true,
                    can_see_exact_dates: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Photographer - full access
        roles.insert(
            "photographer".to_string(),
            Role {
                name: "photographer".to_string(),
                permissions: RolePermissions {
                    owner_access: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let mut user_roles = Vec::new();
        user_roles.push(UserRole {
            username: "alice".to_string(),
            roles: vec!["client".to_string()],
        });
        user_roles.push(UserRole {
            username: "bob".to_string(),
            roles: vec!["photographer".to_string()],
        });

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("public_viewer".to_string()),
            default_authenticated_role: Some("public_viewer".to_string()),
            roles,
            user_roles,
        };

        let resolver = PermissionResolver::new(&gallery_config.permissions, None);

        // Test public user
        let public_perms = resolver.resolve_user_permissions(None).unwrap();
        assert!(public_perms.can_view);
        assert!(!public_perms.can_download_medium);
        assert!(!public_perms.can_see_technical_details);

        // Test client
        let client_perms = resolver.resolve_user_permissions(Some("alice")).unwrap();
        assert!(client_perms.can_view);
        assert!(client_perms.can_download_medium);
        assert!(client_perms.can_download_large);
        assert!(client_perms.can_see_technical_details);

        // Test photographer
        let photog_perms = resolver.resolve_user_permissions(Some("bob")).unwrap();
        assert!(photog_perms.owner_access);
        assert!(photog_perms.can_view);
        assert!(photog_perms.can_download_original);
    }

    #[test]
    fn test_family_album_scenario() {
        // Scenario: Private family album with different permission levels
        let mut gallery_config = create_test_gallery_config("family");

        let mut roles = HashMap::new();

        // Family member - can view everything but not edit
        roles.insert(
            "family".to_string(),
            Role {
                name: "family".to_string(),
                permissions: RolePermissions {
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
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Extended family - limited access
        roles.insert(
            "extended_family".to_string(),
            Role {
                name: "extended_family".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_see_exact_dates: false, // Only see month/year
                    can_see_location: false,    // No location info
                    can_download_medium: true,
                    can_read_metadata: true,
                    can_add_comments: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Kids - very limited access
        roles.insert(
            "kids".to_string(),
            Role {
                name: "kids".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_see_location: false,
                    can_download_medium: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let mut user_roles = Vec::new();
        user_roles.push(UserRole {
            username: "mom".to_string(),
            roles: vec!["family".to_string()],
        });
        user_roles.push(UserRole {
            username: "uncle_joe".to_string(),
            roles: vec!["extended_family".to_string()],
        });
        user_roles.push(UserRole {
            username: "timmy".to_string(),
            roles: vec!["kids".to_string()],
        });

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("none".to_string()), // Explicitly deny public access
            default_authenticated_role: Some("extended_family".to_string()),
            roles,
            user_roles,
        };

        let resolver = PermissionResolver::new(&gallery_config.permissions, None);

        // Test public user - no access
        let public_perms = resolver.resolve_user_permissions(None).unwrap();
        assert!(!public_perms.can_view);

        // Test family member
        let family_perms = resolver.resolve_user_permissions(Some("mom")).unwrap();
        assert!(family_perms.can_view);
        assert!(family_perms.can_see_location);
        assert!(family_perms.can_download_original);
        assert!(family_perms.can_add_comments);

        // Test extended family
        let extended_perms = resolver
            .resolve_user_permissions(Some("uncle_joe"))
            .unwrap();
        assert!(extended_perms.can_view);
        assert!(!extended_perms.can_see_location);
        assert!(!extended_perms.can_see_exact_dates);
        assert!(extended_perms.can_add_comments);

        // Test kids
        let kids_perms = resolver.resolve_user_permissions(Some("timmy")).unwrap();
        assert!(kids_perms.can_view);
        assert!(!kids_perms.can_see_location);
        assert!(!kids_perms.can_add_comments);
    }

    #[test]
    fn test_team_workspace_scenario() {
        // Scenario: Team workspace with different roles
        let mut gallery_config = create_test_gallery_config("team");

        let mut roles = HashMap::new();

        // Viewer - read-only access
        roles.insert(
            "viewer".to_string(),
            Role {
                name: "viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Team member - can contribute
        roles.insert(
            "team_member".to_string(),
            Role {
                name: "team_member".to_string(),
                permissions: RolePermissions {
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
                },
                inherits: None,
            },
        );

        // Team lead - can moderate
        roles.insert(
            "team_lead".to_string(),
            Role {
                name: "team_lead".to_string(),
                permissions: RolePermissions {
                    can_edit_any_comments: true,
                    can_delete_any_comments: true,
                    ..Default::default()
                },
                inherits: Some("team_member".to_string()),
            },
        );

        // Admin - full access
        roles.insert(
            "admin".to_string(),
            Role {
                name: "admin".to_string(),
                permissions: RolePermissions {
                    owner_access: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let mut user_roles = Vec::new();
        user_roles.push(UserRole {
            username: "alice".to_string(),
            roles: vec!["team_member".to_string()],
        });
        user_roles.push(UserRole {
            username: "bob".to_string(),
            roles: vec!["team_lead".to_string()],
        });
        user_roles.push(UserRole {
            username: "carol".to_string(),
            roles: vec!["admin".to_string()],
        });

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("team_member".to_string()),
            roles,
            user_roles,
        };

        let resolver = PermissionResolver::new(&gallery_config.permissions, None);

        // Test public viewer
        let viewer_perms = resolver.resolve_user_permissions(None).unwrap();
        assert!(viewer_perms.can_view);
        assert!(viewer_perms.can_download_medium);
        assert!(!viewer_perms.can_add_comments);

        // Test team member
        let member_perms = resolver.resolve_user_permissions(Some("alice")).unwrap();
        assert!(member_perms.can_view);
        assert!(member_perms.can_download_original);
        assert!(member_perms.can_add_comments);
        assert!(member_perms.can_edit_own_comments);
        assert!(!member_perms.can_edit_any_comments);

        // Test team lead (inherits from team_member)
        let lead_perms = resolver.resolve_user_permissions(Some("bob")).unwrap();
        assert!(lead_perms.can_view);
        assert!(lead_perms.can_download_original);
        assert!(lead_perms.can_add_comments);
        assert!(lead_perms.can_edit_any_comments);
        assert!(lead_perms.can_delete_any_comments);

        // Test admin
        let admin_perms = resolver.resolve_user_permissions(Some("carol")).unwrap();
        assert!(admin_perms.owner_access);
    }

    #[test]
    fn test_folder_override_scenario() {
        // Scenario: Gallery with public access but specific folders have restrictions
        let mut gallery_config = create_test_gallery_config("mixed");

        // Gallery level permissions
        let mut roles = HashMap::new();
        roles.insert(
            "viewer".to_string(),
            Role {
                name: "viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_see_technical_details: true,
                    can_see_exact_dates: true,
                    can_see_location: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("viewer".to_string()),
            roles,
            user_roles: Vec::new(),
        };

        // Folder level permissions - private folder
        let mut folder_config = FolderConfig {
            hidden: false,
            hidden_images: vec![],
            permissions: Default::default(),
            grid_mode: None,
            max_columns: None,
            sort_order: None,
            sort_direction: None,
            custom_order: vec![],
        };

        // Override viewer role for this folder
        let mut folder_roles = HashMap::new();
        folder_roles.insert(
            "viewer".to_string(),
            Role {
                name: "viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_see_technical_details: false, // Hide technical details
                    can_see_exact_dates: false,       // Hide exact dates
                    can_see_location: false,          // Hide location
                    ..Default::default()
                },
                inherits: None,
            },
        );

        folder_roles.insert(
            "trusted".to_string(),
            Role {
                name: "trusted".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_download_large: true,
                    can_download_original: true,
                    can_see_technical_details: true,
                    can_see_exact_dates: true,
                    can_see_location: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let mut folder_user_roles = Vec::new();
        folder_user_roles.push(UserRole {
            username: "trusted_friend".to_string(),
            roles: vec!["trusted".to_string()],
        });

        folder_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("viewer".to_string()), // Still allow public viewing
            default_authenticated_role: Some("viewer".to_string()),
            roles: folder_roles,
            user_roles: folder_user_roles,
        };

        // Test with gallery resolver (no folder override)
        let gallery_resolver = PermissionResolver::new(&gallery_config.permissions, None);
        let gallery_perms = gallery_resolver.resolve_user_permissions(None).unwrap();
        assert!(gallery_perms.can_see_location);
        assert!(gallery_perms.can_see_exact_dates);

        // Test with folder resolver
        let folder_resolver = PermissionResolver::new(
            &gallery_config.permissions,
            Some(&folder_config.permissions),
        );

        // Public user sees limited info in private folder
        let folder_public_perms = folder_resolver.resolve_user_permissions(None).unwrap();
        assert!(folder_public_perms.can_view);
        assert!(!folder_public_perms.can_see_location);
        assert!(!folder_public_perms.can_see_exact_dates);
        assert!(!folder_public_perms.can_see_technical_details);

        // Trusted friend sees everything
        let trusted_perms = folder_resolver
            .resolve_user_permissions(Some("trusted_friend"))
            .unwrap();
        assert!(trusted_perms.can_view);
        assert!(trusted_perms.can_see_location);
        assert!(trusted_perms.can_see_exact_dates);
        assert!(trusted_perms.can_download_original);
    }

    #[test]
    fn test_multiple_roles_scenario() {
        // Scenario: User with multiple roles, permissions should be merged
        let mut gallery_config = create_test_gallery_config("multi_role");

        let mut roles = HashMap::new();

        // Contributor role
        roles.insert(
            "contributor".to_string(),
            Role {
                name: "contributor".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_add_comments: true,
                    can_edit_own_comments: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Moderator role
        roles.insert(
            "moderator".to_string(),
            Role {
                name: "moderator".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_edit_any_comments: true,
                    can_delete_any_comments: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        // Downloader role
        roles.insert(
            "downloader".to_string(),
            Role {
                name: "downloader".to_string(),
                permissions: RolePermissions {
                    can_download_large: true,
                    can_download_original: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let mut user_roles = Vec::new();
        // User with multiple roles
        user_roles.push(UserRole {
            username: "power_user".to_string(),
            roles: vec![
                "contributor".to_string(),
                "moderator".to_string(),
                "downloader".to_string(),
            ],
        });

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: None,
            default_authenticated_role: None,
            roles,
            user_roles,
        };

        let resolver = PermissionResolver::new(&gallery_config.permissions, None);

        // Test merged permissions
        let merged_perms = resolver
            .resolve_user_permissions(Some("power_user"))
            .unwrap();
        assert!(merged_perms.can_view);
        assert!(merged_perms.can_download_medium); // from contributor
        assert!(merged_perms.can_add_comments); // from contributor
        assert!(merged_perms.can_edit_own_comments); // from contributor
        assert!(merged_perms.can_edit_any_comments); // from moderator
        assert!(merged_perms.can_delete_any_comments); // from moderator
        assert!(merged_perms.can_download_large); // from downloader
        assert!(merged_perms.can_download_original); // from downloader
    }

    #[test]
    fn test_event_photography_scenario() {
        // Scenario: Event photography with different access levels for different events
        let mut gallery_config = create_test_gallery_config("events");

        // Gallery has no public access by default
        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("none".to_string()),
            default_authenticated_role: None,
            roles: HashMap::new(),
            user_roles: Vec::new(),
        };

        // Wedding folder - guests can view and download
        let mut wedding_folder = FolderConfig {
            hidden: false,
            hidden_images: vec![],
            permissions: Default::default(),
            grid_mode: None,
            max_columns: None,
            sort_order: None,
            sort_direction: None,
            custom_order: vec![],
        };

        let mut wedding_roles = HashMap::new();
        wedding_roles.insert(
            "wedding_guest".to_string(),
            Role {
                name: "wedding_guest".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_download_large: true,
                    can_add_comments: true,
                    can_add_tags: true, // Can tag themselves
                    ..Default::default()
                },
                inherits: None,
            },
        );

        wedding_roles.insert(
            "bride_groom".to_string(),
            Role {
                name: "bride_groom".to_string(),
                permissions: RolePermissions {
                    can_download_original: true,
                    can_set_picks: true, // Can mark favorites
                    ..Default::default()
                },
                inherits: Some("wedding_guest".to_string()),
            },
        );

        let mut wedding_users = Vec::new();
        wedding_users.push(UserRole {
            username: "john_smith".to_string(),
            roles: vec!["bride_groom".to_string()],
        });
        wedding_users.push(UserRole {
            username: "jane_smith".to_string(),
            roles: vec!["bride_groom".to_string()],
        });
        // All wedding guests would be added here

        wedding_folder.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("wedding_guest".to_string()), // Anyone with link can view
            default_authenticated_role: Some("wedding_guest".to_string()),
            roles: wedding_roles,
            user_roles: wedding_users,
        };

        // Corporate event folder - restricted access
        let mut corporate_folder = FolderConfig {
            hidden: true, // Hidden from gallery listings
            hidden_images: vec![],
            permissions: Default::default(),
            grid_mode: None,
            max_columns: None,
            sort_order: None,
            sort_direction: None,
            custom_order: vec![],
        };

        let mut corp_roles = HashMap::new();
        corp_roles.insert(
            "attendee".to_string(),
            Role {
                name: "attendee".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        corp_roles.insert(
            "corp_pr".to_string(),
            Role {
                name: "corp_pr".to_string(),
                permissions: RolePermissions {
                    can_download_original: true,
                    can_set_picks: true,
                    can_add_tags: true,
                    ..Default::default()
                },
                inherits: Some("attendee".to_string()),
            },
        );

        let mut corp_users = Vec::new();
        corp_users.push(UserRole {
            username: "pr_manager".to_string(),
            roles: vec!["corp_pr".to_string()],
        });

        corporate_folder.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("none".to_string()), // No public access
            default_authenticated_role: Some("attendee".to_string()),
            roles: corp_roles,
            user_roles: corp_users,
        };

        // Test wedding folder
        let wedding_resolver = PermissionResolver::new(
            &gallery_config.permissions,
            Some(&wedding_folder.permissions),
        );

        // Public guest
        let guest_perms = wedding_resolver.resolve_user_permissions(None).unwrap();
        assert!(guest_perms.can_view);
        assert!(guest_perms.can_download_large);
        assert!(guest_perms.can_add_comments);
        assert!(!guest_perms.can_download_original);

        // Bride/Groom
        let bride_perms = wedding_resolver
            .resolve_user_permissions(Some("jane_smith"))
            .unwrap();
        assert!(bride_perms.can_download_original);
        assert!(bride_perms.can_set_picks);

        // Test corporate folder
        let corp_resolver = PermissionResolver::new(
            &gallery_config.permissions,
            Some(&corporate_folder.permissions),
        );

        // No public access
        let public_perms = corp_resolver.resolve_user_permissions(None).unwrap();
        assert!(!public_perms.can_view);

        // PR Manager
        let pr_perms = corp_resolver
            .resolve_user_permissions(Some("pr_manager"))
            .unwrap();
        assert!(pr_perms.can_view);
        assert!(pr_perms.can_download_original);
        assert!(pr_perms.can_set_picks);
    }

    #[test]
    fn test_folder_grants_location_via_public_role() {
        // Scenario: Gallery hides location by default, but a specific folder
        // defines a new role that grants location and sets it as the public role
        let mut gallery_config = create_test_gallery_config("travel");

        let mut gallery_roles = HashMap::new();
        gallery_roles.insert(
            "viewer".to_string(),
            Role {
                name: "viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_see_location: false,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        gallery_config.permissions = PermissionConfig {
            site_admins: Vec::new(),
            public_role: Some("viewer".to_string()),
            default_authenticated_role: Some("viewer".to_string()),
            roles: gallery_roles,
            user_roles: Vec::new(),
        };

        // Verify gallery-level public user cannot see location
        let gallery_resolver = PermissionResolver::new(&gallery_config.permissions, None);
        let gallery_public = gallery_resolver.resolve_user_permissions(None).unwrap();
        assert!(gallery_public.can_view);
        assert!(!gallery_public.can_see_location);

        // Folder defines a new role that grants location and uses it as public_role
        let mut folder_roles = HashMap::new();
        folder_roles.insert(
            "location_viewer".to_string(),
            Role {
                name: "location_viewer".to_string(),
                permissions: RolePermissions {
                    can_view: true,
                    can_download_medium: true,
                    can_see_location: true,
                    ..Default::default()
                },
                inherits: None,
            },
        );

        let folder_config = FolderConfig {
            hidden: false,
            hidden_images: vec![],
            permissions: PermissionConfig {
                site_admins: Vec::new(),
                public_role: Some("location_viewer".to_string()),
                default_authenticated_role: Some("location_viewer".to_string()),
                roles: folder_roles,
                user_roles: Vec::new(),
            },
            grid_mode: None,
            max_columns: None,
        };

        let folder_resolver = PermissionResolver::new(
            &gallery_config.permissions,
            Some(&folder_config.permissions),
        );

        // Public (unauthenticated) user in this folder should see location
        let folder_public = folder_resolver.resolve_user_permissions(None).unwrap();
        assert!(folder_public.can_view);
        assert!(folder_public.can_see_location);
        assert!(folder_public.can_download_medium);
        assert!(!folder_public.can_download_original);
    }
}
