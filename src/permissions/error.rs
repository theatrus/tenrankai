use thiserror::Error;

#[derive(Error, Debug)]
pub enum PermissionError {
    #[error("Access denied")]
    AccessDenied,
    
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    
    #[error("Role not found: {0}")]
    RoleNotFound(String),
    
    #[error("Circular role inheritance detected: {0}")]
    CircularInheritance(String),
    
    #[error("Authentication required")]
    AuthenticationRequired,
    
    #[error("Permission check failed: {0}")]
    PermissionCheckFailed(String),
}