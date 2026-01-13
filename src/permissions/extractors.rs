#![allow(clippy::manual_async_fn)] // Required for axum 0.8 FromRequestParts trait

use crate::AppState;
use crate::permissions::{PermissionResolver, RolePermissions};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use std::future::Future;

/// User permissions resolved for the current request
#[derive(Debug, Clone)]
pub struct UserPermissions {
    pub username: Option<String>,
    pub permissions: RolePermissions,
}

impl UserPermissions {
    /// Create new UserPermissions instance
    pub fn new<S: Into<String>>(username: Option<S>, permissions: RolePermissions) -> Self {
        Self {
            username: username.map(|s| s.into()),
            permissions,
        }
    }
}

/// Optional permissions - always succeeds, provides permissions for user or public
#[derive(Debug, Clone)]
pub struct OptionalPermissions(pub UserPermissions);

/// Required view permission - returns 403 if user cannot view
#[derive(Debug, Clone)]
pub struct RequireView(pub UserPermissions);

/// Required metadata permission - returns 403 if user cannot read metadata
#[derive(Debug, Clone)]
pub struct RequireMetadata(pub UserPermissions);

/// Required owner permission - returns 403 if user is not an owner
#[derive(Debug, Clone)]
pub struct RequireOwner(pub UserPermissions);

// Helper to extract gallery name and path from request
async fn extract_gallery_and_path(parts: &Parts) -> Option<(String, String)> {
    // Try to extract from matched path
    if let Some(matched_path) = parts.extensions.get::<axum::extract::MatchedPath>() {
        let path = matched_path.as_str();

        // Parse gallery routes: /gallery_name/... or /api/gallery/gallery_name/...
        if let Some(stripped) = path.strip_prefix("/api/gallery/") {
            if let Some((gallery_name, rest)) = stripped.split_once('/') {
                return Some((gallery_name.to_string(), rest.to_string()));
            }
        } else if let Some(rest) = path.strip_prefix('/')
            && let Some((gallery_name, rest)) = rest.split_once('/')
        {
            return Some((gallery_name.to_string(), rest.to_string()));
        }
    }

    None
}

// Helper to resolve permissions for a request
async fn resolve_permissions(
    parts: &Parts,
    app_state: &AppState,
) -> Result<UserPermissions, (StatusCode, &'static str)> {
    // Get authenticated user if any
    let username = crate::login::get_authenticated_user_for_app(app_state, &parts.headers);

    // Extract gallery and path from request
    let (gallery_name, path) = extract_gallery_and_path(parts).await.ok_or((
        StatusCode::BAD_REQUEST,
        "Could not determine gallery context",
    ))?;

    // Get gallery
    let gallery = app_state
        .galleries
        .get(&gallery_name)
        .ok_or((StatusCode::NOT_FOUND, "Gallery not found"))?;

    // Get folder config if path is not empty
    let folder_config = if !path.is_empty() {
        gallery
            .read_folder_metadata_full(&path)
            .await
            .map(|meta| meta.config)
    } else {
        None
    };

    // Create resolver
    let resolver = PermissionResolver::new(
        &gallery.config.permissions,
        folder_config.as_ref().map(|fc| &fc.permissions),
    );

    // Resolve permissions
    let permissions = resolver
        .resolve_user_permissions(username.as_deref())
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resolve permissions",
            )
        })?;

    Ok(UserPermissions {
        username,
        permissions,
    })
}

// Implement extraction for OptionalPermissions
impl<S> FromRequestParts<S> for OptionalPermissions
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let app_state = AppState::from_ref(state);

            // Try to resolve permissions, but fall back to minimal permissions on error
            let user_perms = match resolve_permissions(parts, &app_state).await {
                Ok(perms) => perms,
                Err(_) => {
                    // Fall back to minimal permissions
                    UserPermissions {
                        username: None,
                        permissions: RolePermissions::default(),
                    }
                }
            };

            Ok(OptionalPermissions(user_perms))
        }
    }
}

// Implement extraction for RequireView
impl<S> FromRequestParts<S> for RequireView
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let app_state = AppState::from_ref(state);
            let user_perms = resolve_permissions(parts, &app_state).await?;

            if !user_perms.permissions.can_view {
                return Err((StatusCode::FORBIDDEN, "View permission required"));
            }

            Ok(RequireView(user_perms))
        }
    }
}

// Implement extraction for RequireMetadata
impl<S> FromRequestParts<S> for RequireMetadata
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let app_state = AppState::from_ref(state);
            let user_perms = resolve_permissions(parts, &app_state).await?;

            if !user_perms.permissions.can_read_metadata {
                return Err((StatusCode::FORBIDDEN, "Metadata read permission required"));
            }

            Ok(RequireMetadata(user_perms))
        }
    }
}

// Implement extraction for RequireOwner
impl<S> FromRequestParts<S> for RequireOwner
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let app_state = AppState::from_ref(state);
            let user_perms = resolve_permissions(parts, &app_state).await?;

            if !user_perms.permissions.owner_access {
                return Err((StatusCode::FORBIDDEN, "Owner access required"));
            }

            Ok(RequireOwner(user_perms))
        }
    }
}

impl OptionalPermissions {
    /// Get the username if authenticated
    pub fn username(&self) -> Option<&str> {
        self.0.username.as_deref()
    }

    /// Get the permissions
    pub fn permissions(&self) -> &RolePermissions {
        &self.0.permissions
    }

    /// Get the inner UserPermissions
    pub fn inner(&self) -> &UserPermissions {
        &self.0
    }
}

impl RequireView {
    /// Get the username
    pub fn username(&self) -> Option<&str> {
        self.0.username.as_deref()
    }

    /// Get the permissions
    pub fn permissions(&self) -> &RolePermissions {
        &self.0.permissions
    }

    /// Get the inner UserPermissions
    pub fn inner(&self) -> &UserPermissions {
        &self.0
    }
}

impl RequireMetadata {
    /// Get the username
    pub fn username(&self) -> Option<&str> {
        self.0.username.as_deref()
    }

    /// Get the permissions
    pub fn permissions(&self) -> &RolePermissions {
        &self.0.permissions
    }

    /// Get the inner UserPermissions
    pub fn inner(&self) -> &UserPermissions {
        &self.0
    }

    /// Check if user can edit their own comment
    pub fn can_edit_own_comment(&self, comment_author: &str) -> bool {
        self.0.permissions.can_edit_own_comments
            && self.0.username.as_deref() == Some(comment_author)
    }

    /// Check if user can delete their own comment
    pub fn can_delete_own_comment(&self, comment_author: &str) -> bool {
        self.0.permissions.can_delete_own_comments
            && self.0.username.as_deref() == Some(comment_author)
    }

    /// Check if user can edit any comment
    pub fn can_edit_any_comment(&self) -> bool {
        self.0.permissions.can_edit_any_comments
    }

    /// Check if user can delete any comment
    pub fn can_delete_any_comment(&self) -> bool {
        self.0.permissions.can_delete_any_comments
    }
}

impl RequireOwner {
    /// Get the username
    pub fn username(&self) -> Option<&str> {
        self.0.username.as_deref()
    }

    /// Get the permissions
    pub fn permissions(&self) -> &RolePermissions {
        &self.0.permissions
    }

    /// Get the inner UserPermissions
    pub fn inner(&self) -> &UserPermissions {
        &self.0
    }
}

impl UserPermissions {
    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.username.is_some()
    }

    /// Check if user can perform an action on their own content
    pub fn can_edit_own(&self, author: &str) -> bool {
        self.permissions.can_edit_own_comments && self.username.as_deref() == Some(author)
    }

    /// Check if user can delete their own content
    pub fn can_delete_own(&self, author: &str) -> bool {
        self.permissions.can_delete_own_comments && self.username.as_deref() == Some(author)
    }
}

/// Alternative: resolve permissions given explicit gallery and path
/// This is useful for handlers that already have gallery/path extracted
pub async fn resolve_permissions_for_path(
    app_state: &AppState,
    gallery_name: &str,
    path: &str,
    username: Option<&str>,
) -> Result<UserPermissions, crate::permissions::PermissionError> {
    // Get gallery
    let gallery = app_state.galleries.get(gallery_name).ok_or(
        crate::permissions::PermissionError::RoleNotFound("Gallery not found".to_string()),
    )?;

    // Get folder config if path is not empty
    let folder_config = if !path.is_empty() {
        gallery
            .read_folder_metadata_full(path)
            .await
            .map(|meta| meta.config)
    } else {
        None
    };

    // Create resolver
    let resolver = PermissionResolver::new(
        &gallery.config.permissions,
        folder_config.as_ref().map(|fc| &fc.permissions),
    );

    // Resolve permissions
    let permissions = resolver.resolve_user_permissions(username)?;

    Ok(UserPermissions {
        username: username.map(String::from),
        permissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FilesystemStorage;

    fn create_test_storage(cache_dir: &str) -> crate::storage::DynStorage {
        let path = std::path::PathBuf::from(cache_dir);
        std::fs::create_dir_all(&path).ok();
        std::sync::Arc::new(FilesystemStorage::new(path))
    }

    #[tokio::test]
    async fn test_resolve_permissions_for_path_public() {
        // Create minimal app state for testing
        let config = crate::Config::default();
        let mut gallery_config = crate::GallerySystemConfig::default();
        gallery_config.name = "test".to_string();
        gallery_config.source_directory = std::path::PathBuf::from(".");
        gallery_config.cache_directory = ".".to_string();

        let cache_storage = create_test_storage(&gallery_config.cache_directory);
        let gallery =
            std::sync::Arc::new(crate::gallery::Gallery::new(gallery_config, cache_storage));
        let mut galleries = std::collections::HashMap::new();
        galleries.insert("test".to_string(), gallery);

        let app_state = AppState {
            template_engine: std::sync::Arc::new(crate::templating::TemplateEngine::new(vec![])),
            static_handler: crate::static_files::StaticFileHandler::new(vec![]),
            galleries: std::sync::Arc::new(galleries),
            favicon_renderer: crate::favicon::FaviconRenderer::new(vec![]),
            posts_managers: std::sync::Arc::new(std::collections::HashMap::new()),
            login_state: std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::login::LoginState::new(),
            )),
            user_database_manager: None,
            email_provider: None,
            webauthn: None,
            openai_client: None,
            config,
        };

        // Test public user permissions
        let result = resolve_permissions_for_path(&app_state, "test", "", None).await;
        assert!(result.is_ok());
        let perms = result.unwrap();
        assert!(perms.username.is_none());
        assert!(perms.permissions.can_view); // Default viewer role allows viewing
        assert!(!perms.permissions.can_add_comments); // But not adding comments
    }

    #[tokio::test]
    async fn test_user_permissions_helpers() {
        let perms = UserPermissions {
            username: Some("testuser".to_string()),
            permissions: RolePermissions {
                can_edit_own_comments: true,
                can_delete_own_comments: true,
                ..Default::default()
            },
        };

        assert!(perms.is_authenticated());
        assert!(perms.can_edit_own("testuser"));
        assert!(!perms.can_edit_own("otheruser"));
        assert!(perms.can_delete_own("testuser"));
        assert!(!perms.can_delete_own("otheruser"));
    }

    #[tokio::test]
    async fn test_require_metadata_helpers() {
        let metadata_perms = RequireMetadata(UserPermissions {
            username: Some("testuser".to_string()),
            permissions: RolePermissions {
                can_read_metadata: true,
                can_edit_own_comments: true,
                can_delete_own_comments: true,
                can_edit_any_comments: false,
                can_delete_any_comments: false,
                ..Default::default()
            },
        });

        assert_eq!(metadata_perms.username(), Some("testuser"));
        assert!(metadata_perms.can_edit_own_comment("testuser"));
        assert!(!metadata_perms.can_edit_own_comment("otheruser"));
        assert!(metadata_perms.can_delete_own_comment("testuser"));
        assert!(!metadata_perms.can_delete_own_comment("otheruser"));
        assert!(!metadata_perms.can_edit_any_comment());
        assert!(!metadata_perms.can_delete_any_comment());
    }
}
