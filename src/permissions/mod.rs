pub mod types;
pub mod resolver;
pub mod extractors;
pub mod error;

pub use types::{RolePermissions, Role, UserRole, PermissionConfig};
// pub use resolver::PermissionResolver;  // TODO: Implement in step 2
// pub use extractors::{UserPermissions, RequireView, RequireMetadata, RequireOwner};  // TODO: Implement in step 3
pub use error::PermissionError;