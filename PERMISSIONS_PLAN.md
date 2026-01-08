# Tenrankai Flexible Role-Based Permission System Plan

## Overview

This document outlines the implementation plan for a flexible role-based access control (RBAC) system for Tenrankai. The system will support arbitrary roles with configurable permissions at both gallery and folder levels.

## Goals

1. **Flexibility**: Support arbitrary roles with custom permission sets
2. **Granularity**: Fine-grained control over viewing, downloading, and metadata operations
3. **Simplicity**: Clean API using Axum extractors similar to current auth system
4. **Extensibility**: Easy to add new permissions without breaking existing code
5. **Performance**: Resolve permissions once per request
6. **Compatibility**: Smooth migration from current system

## Core Concepts

### Permission Flags

Each role will have a set of permission flags:

```rust
struct RolePermissions {
    // Viewing permissions
    can_view: bool,
    can_see_technical_details: bool,
    can_see_exact_dates: bool,
    can_see_location: bool,
    
    // Download permissions
    can_download_medium: bool,
    can_download_large: bool,
    can_download_original: bool,
    
    // Metadata permissions
    can_read_metadata: bool,
    can_add_comments: bool,
    can_edit_own_comments: bool,
    can_delete_own_comments: bool,
    can_set_picks: bool,
    can_add_tags: bool,
    
    // Moderation permissions
    can_edit_any_comments: bool,
    can_delete_any_comments: bool,
    
    // Special permissions
    owner_access: bool,  // Bypasses all restrictions
}
```

### Role Definition

Roles are defined by name with associated permissions:

```rust
pub struct Role {
    pub name: String,
    pub permissions: RolePermissions,
    pub inherits: Option<String>,  // Inherit from another role
}
```

### User Role Assignment

Users can be assigned multiple roles:

```rust
pub struct UserRole {
    pub username: String,
    pub roles: Vec<String>,  // Role names
}
```

## Configuration Format

### Gallery Level (config.toml)

```toml
[[galleries]]
name = "main"

[galleries.permissions]
# Role assigned to unauthenticated users (omit for no public access)
public_role = "viewer"

# Role assigned to authenticated users without specific roles
default_authenticated_role = "contributor"

# Define custom roles
[galleries.permissions.roles.viewer]
can_view = true
can_download_medium = true
# Omitted permissions default to false

[galleries.permissions.roles.contributor]
can_view = true
can_see_technical_details = true
can_see_exact_dates = true
can_see_location = true
can_download_medium = true
can_download_large = true
can_read_metadata = true
can_add_comments = true
can_edit_own_comments = true
can_delete_own_comments = true
can_set_picks = true
can_add_tags = true

[galleries.permissions.roles.trusted]
inherits = "contributor"  # Inherit all contributor permissions
can_download_original = true

[galleries.permissions.roles.moderator]
inherits = "trusted"
can_edit_any_comments = true
can_delete_any_comments = true

[galleries.permissions.roles.admin]
owner_access = true  # Has all permissions, bypasses all checks

# Assign roles to specific users
[[galleries.permissions.user_roles]]
username = "alice"
roles = ["admin"]

[[galleries.permissions.user_roles]]
username = "bob"
roles = ["contributor", "trusted"]  # Multiple roles, permissions are merged
```

### Folder Level (_folder.md)

```toml
+++
# Folder-specific permission overrides
[permissions]
# Override public access for this folder
public_role = "none"  # No public access to this folder

# Define folder-specific roles
[permissions.roles.family_member]
can_view = true
can_see_exact_dates = true
can_see_location = true
can_download_original = true
can_read_metadata = true
can_add_comments = true

# Folder-specific user role assignments
[[permissions.user_roles]]
username = "grandma"
roles = ["family_member"]

[[permissions.user_roles]]
username = "cousin_joe"
roles = ["family_member", "moderator"]
+++
```

## Permission Resolution

### Resolution Flow

1. **Determine base role(s)**:
   - For authenticated users: Check user_roles assignments
   - If no specific roles: Use default_authenticated_role
   - For unauthenticated users: Use public_role
   - If no applicable role: Deny access

2. **Merge multiple roles**:
   - If user has multiple roles, merge permissions (OR operation)
   - Most permissive permission wins

3. **Apply inheritance**:
   - Resolve role inheritance before merging
   - Child role permissions override parent

4. **Folder overrides**:
   - Folder roles extend/override gallery roles
   - Folder-specific user assignments take precedence

5. **Owner bypass**:
   - If any role has `owner_access = true`, grant all permissions

## Implementation Details

### Permission Extractor

```rust
// Main permission struct injected into handlers
pub struct UserPermissions {
    username: Option<String>,
    permissions: RolePermissions,
}

impl UserPermissions {
    // Core permission checks
    pub fn can_view(&self) -> bool { self.permissions.can_view }
    pub fn is_owner(&self) -> bool { self.permissions.owner_access }
    
    // Download permission by size
    pub fn can_download(&self, size: ImageSize) -> bool {
        match size {
            ImageSize::Thumbnail | ImageSize::Gallery => self.can_view(),
            ImageSize::Medium => self.permissions.can_download_medium,
            ImageSize::Large => self.permissions.can_download_large,
            ImageSize::Original => self.permissions.can_download_original,
        }
    }
    
    // Metadata operation permissions
    pub fn can_edit_comment(&self, author: &str) -> bool {
        self.permissions.owner_access || 
        (self.permissions.can_edit_own_comments && 
         self.username.as_deref() == Some(author))
    }
    
    pub fn can_delete_comment(&self, author: &str) -> bool {
        self.permissions.owner_access ||
        self.permissions.can_delete_any_comments ||
        (self.permissions.can_delete_own_comments && 
         self.username.as_deref() == Some(author))
    }
    
    // Filter data based on permissions
    pub fn filter_image_info(&self, mut info: ImageInfo) -> ImageInfo {
        if !self.permissions.can_see_technical_details {
            info.camera_info = None;
            info.color_profile = None;
        }
        if !self.permissions.can_see_location {
            info.location_info = None;
        }
        if !self.permissions.can_see_exact_dates {
            // Convert to approximate date
            info.capture_date = approximate_date(info.capture_date);
        }
        info
    }
}
```

### Axum Extractors

```rust
// Extractor that just loads permissions
#[async_trait]
impl<S> FromRequestParts<S> for UserPermissions { ... }

// Extractor that requires view permission
pub struct RequireView(pub UserPermissions);

#[async_trait]
impl<S> FromRequestParts<S> for RequireView {
    async fn from_request_parts(...) -> Result<Self, Self::Rejection> {
        let perms = UserPermissions::from_request_parts(parts, state).await?;
        if !perms.can_view() {
            return Err(ApiResponse::Forbidden.into_response());
        }
        Ok(RequireView(perms))
    }
}

// Similar extractors for other common requirements
pub struct RequireMetadata(pub UserPermissions);
pub struct RequireDownload(pub UserPermissions);
pub struct RequireOwner(pub UserPermissions);
```

### Handler Examples

```rust
// Gallery handler with view requirement
pub async fn gallery_handler(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    RequireView(perms): RequireView,  // Automatically ensures can_view
) -> Result<impl IntoResponse> {
    // Use perms for filtering
    let items = gallery.list_directory(&path)
        .map(|item| perms.filter_image_info(item));
    // ...
}

// Image download with size-based permission check
pub async fn image_handler(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    perms: UserPermissions,  // No automatic requirement
) -> Result<impl IntoResponse> {
    if !perms.can_download(params.size) {
        return Err(ApiResponse::Forbidden);
    }
    // ...
}

// Metadata update with edit check
pub async fn edit_comment_handler(
    State(state): State<AppState>,
    Path((name, path, comment_id)): Path<(String, String, String)>,
    RequireMetadata(perms): RequireMetadata,
    Json(req): Json<EditCommentRequest>,
) -> Result<impl IntoResponse> {
    let comment = load_comment(&comment_id)?;
    if !perms.can_edit_comment(&comment.author) {
        return Err(ApiResponse::Forbidden);
    }
    // ...
}
```

### Template Integration

```liquid
<!-- In templates -->
{% if permissions.can_add_comments %}
  <button>Add Comment</button>
{% endif %}

{% if permissions.can_see_technical_details %}
  <div class="camera-info">{{ image.camera_info }}</div>
{% endif %}

<!-- Show role indicator -->
{% if permissions.is_owner %}
  <span class="role-badge">Owner</span>
{% endif %}
```

## Migration Strategy

### Automatic Mapping

Current configuration will map to the new system:

1. **No permissions configured**:
   - Public users get "viewer" role (full read access)
   - Authenticated users get "contributor" role (add metadata)

2. **Gallery with `user_database` but no permissions**:
   - No public_role (authentication required)
   - Authenticated users get "contributor" role

3. **Current folder settings**:
   - `hide_technical_details = true` → viewer role lacks `can_see_technical_details`
   - `hide_location_from_public = true` → viewer role lacks `can_see_location`
   - `approximate_dates_for_public = true` → viewer role lacks `can_see_exact_dates`

### Built-in Default Roles

If not defined in configuration, these roles are available:

```toml
[roles.viewer]
can_view = true
can_download_medium = true

[roles.contributor]
# Everything viewer has plus:
can_see_technical_details = true
can_see_exact_dates = true
can_see_location = true
can_download_large = true
can_read_metadata = true
can_add_comments = true
can_edit_own_comments = true
can_delete_own_comments = true
can_set_picks = true
can_add_tags = true

[roles.admin]
owner_access = true
```

## Use Case Examples

### Public Portfolio
```toml
[galleries.permissions]
public_role = "portfolio_viewer"
default_authenticated_role = "client"

[galleries.permissions.roles.portfolio_viewer]
can_view = true
# No downloads, no technical details

[galleries.permissions.roles.client]
can_view = true
can_download_medium = true
can_see_exact_dates = true
```

### Private Family Album
```toml
[galleries.permissions]
# No public_role = no public access
default_authenticated_role = "family"

[galleries.permissions.roles.family]
can_view = true
can_see_everything = true
can_download_original = true
can_add_comments = true

[galleries.permissions.roles.kid]
can_view = true
can_download_medium = true
# No location info for safety
```

### Team Workspace
```toml
[galleries.permissions]
public_role = "viewer"
default_authenticated_role = "team_member"

[galleries.permissions.roles.team_member]
inherits = "contributor"
can_download_original = true

[galleries.permissions.roles.team_lead]
inherits = "team_member"
can_delete_any_comments = true
can_edit_any_comments = true
```

## Benefits

1. **Flexible**: Define any permission model needed
2. **Secure**: Fail-closed design, explicit grants only
3. **Maintainable**: Clear permission checks in code
4. **Performant**: Single permission resolution per request
5. **User-friendly**: Easy to understand configuration
6. **Future-proof**: Easy to add new permissions

## Implementation Status

### ✅ Step 1: Core Permission Types and Structures (COMPLETED)

**What we implemented:**
- Created `src/permissions/mod.rs` with module structure
- Created `src/permissions/types.rs` with:
  - `RolePermissions` struct with all permission flags
  - `Role` struct with name, permissions, and inheritance
  - `UserRole` struct for user-to-role assignments
  - `PermissionConfig` struct for gallery/folder configuration
  - Helper methods: `merge()`, `apply_owner_override()`, `default_roles()`
- Created `src/permissions/error.rs` with permission-specific error types
- Added `permissions` field to `GallerySystemConfig` in `src/config/types.rs`
- Added `permissions` field to `FolderConfig` in `src/gallery/types.rs`

**Learnings:**
- The config types are in `src/config/types.rs`, not in a single config.rs file
- Gallery types including `FolderConfig` are in `src/gallery/types.rs`
- We're using `#[serde(default)]` extensively for optional fields
- The inheritance field on Role will be important for permission resolution

**Notes:**
- We included a `default_roles()` method that provides viewer, contributor, and admin roles
- The `owner_access` flag properly bypasses all permission checks when true
- Permissions merge with OR logic (most permissive wins)

### ✅ Step 2: Create Permission Resolution System (COMPLETED)

**What we implemented:**
- Created `src/permissions/resolver.rs` with `PermissionResolver` struct
- Implemented `resolve_user_permissions()` for authenticated and public users
- Added role inheritance with circular dependency detection
- Implemented permission merging for users with multiple roles
- Handled folder-level permission overrides
- Created comprehensive test suite for all resolver scenarios

**Learnings:**
- Circular inheritance detection requires propagating the error through the recursion
- The resolver uses a visited set to track and detect circular dependencies
- Permission merging uses OR logic - most permissive wins
- Tests need a test-only constructor for OptionalAuth (added `#[cfg(test)] pub fn new()`)

### ✅ Step 3: Implement Axum Extractors (COMPLETED)

**What we implemented:**
- Created `src/permissions/extractors.rs` with permission-aware extractors
- `OptionalPermissions` - Always succeeds, provides permissions for user or public
- `RequireView` - Returns 403 if user cannot view
- `RequireMetadata` - Returns 403 if user cannot read metadata
- `RequireOwner` - Returns 403 if user is not an owner
- `UserPermissions` struct with helper methods for checking permissions
- `resolve_permissions_for_path()` helper for handlers with explicit gallery/path
- Comprehensive helper methods for checking edit/delete permissions on own content

**Learnings:**
- Extractors need to determine gallery and path from the request context
- Path extraction from MatchedPath works for most routes
- Alternative `resolve_permissions_for_path` function useful for handlers with explicit params
- Helper methods on extractors make permission checks cleaner in handlers
- Tests confirmed the extractors work correctly with proper role resolution

### ✅ Step 4: Update Handlers to Use New Extractors (COMPLETED)

**What we updated:**
- `add_comment_handler` - Uses permissions to check `can_add_comments`
- `edit_comment_handler` - Checks `can_edit_own_comments` or `can_edit_any_comments`
- `delete_comment_handler` - Checks `can_delete_own_comments` or `can_delete_any_comments`
- `update_metadata_handler` - Checks permissions for picks, tags, and metadata
- `get_metadata_handler` - Checks `can_read_metadata` permission
- `image_handler_for_named` - Maps image sizes to download permissions:
  - Thumbnail/Gallery sizes: `can_view`
  - Medium size: `can_download_medium`
  - Large size: `can_download_large`
  - Original (no size): `can_download_original`

**Learnings:**
- All handlers now use `OptionalAuth` instead of `RequireAuth`
- Permissions are resolved using `resolve_permissions_for_path()`
- Fine-grained permission checks replace simple authentication checks
- Image download permissions are mapped to specific size requests
- Permission denied returns appropriate HTTP status codes (403 Forbidden)

### ✅ Step 5: Update Templates with Permission Checks (COMPLETED)

**What we implemented:**
- Updated `gallery_handler_for_named` to resolve permissions and add them to liquid context
- Updated `image_detail_handler_for_named` to include permissions in template context
- Modified `gallery.html.liquid` template to pass permissions to React components
- Modified `image_detail.html.liquid` template to include permissions data attribute
- Updated API response structures to include permissions:
  - `GalleryApiResponse` now includes `permissions` field
  - `ImageDetailApiResponse` now includes `permissions` field
- Updated API handlers to resolve and include permissions in responses

**Learnings:**
- Permissions are passed to React components via data attributes
- API responses need to include permissions for dynamic client-side rendering
- Template context includes full permission object for conditional rendering
- Consistent permission resolution across both server-side rendering and API

### ✅ Step 6: Create Migration Code for Existing Configs (COMPLETED)

**What we implemented:**
- Created `src/permissions/migration.rs` with migration functions
- `migrate_gallery_config()` - Migrates gallery-level configuration to new permission system
- `migrate_folder_config()` - Migrates folder-level configuration to new permission system
- Migration maps old settings to new permission flags:
  - `approximate_dates_for_public` → viewer role lacks `can_see_exact_dates`
  - `hide_location_from_public` → viewer role lacks `can_see_location`
  - `hide_technical_details` → viewer role lacks `can_see_technical_details`
  - `require_auth` → removes public_role (no public access)
  - `allowed_users` → creates "allowed" role and assigns users
  - `enable_metadata` → controls metadata permissions for contributor role
- Migration is called automatically when galleries are loaded in `create_app()`
- Migration is called automatically when folder metadata is read
- Created comprehensive tests for migration scenarios

**Learnings:**
- Migration needs to handle both gallery and folder levels separately
- The generic `new` method required type annotations for None values
- Test configs can be simplified to focus on just the migration logic
- Migration only runs if permissions are not already configured
- Folder config is more private than public API but that's OK for internal use

### 📝 Remaining Steps

7. Test with various permission scenarios
8. Document permission system for users