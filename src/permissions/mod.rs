pub mod error;
pub mod extractors;
pub mod resolver;
pub mod types;

pub use error::PermissionError;
pub use resolver::PermissionResolver;
pub use types::{PermissionConfig, Role, RolePermissions, UserRole};
// pub use extractors::{UserPermissions, RequireView, RequireMetadata, RequireOwner};  // TODO: Implement in step 3
