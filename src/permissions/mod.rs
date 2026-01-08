pub mod error;
pub mod extractors;
pub mod migration;
pub mod resolver;
pub mod types;

#[cfg(test)]
mod tests;

pub use error::PermissionError;
pub use resolver::PermissionResolver;
pub use types::{PermissionConfig, Role, RolePermissions, UserRole};
pub use extractors::{
    UserPermissions, OptionalPermissions, RequireView, RequireMetadata, RequireOwner,
    resolve_permissions_for_path
};
pub use migration::{migrate_gallery_config, migrate_folder_config};
