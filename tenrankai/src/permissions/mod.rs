pub mod error;
pub mod extractors;
pub mod resolver;
pub mod types;

#[cfg(test)]
mod tests;

pub use error::PermissionError;
pub use extractors::{
    OptionalPermissions, RequireMetadata, RequireOwner, RequireView, UserPermissions,
    resolve_permissions_for_path,
};
pub use resolver::PermissionResolver;
pub use types::{PermissionConfig, Role, RolePermissions, UserRole};
