/// Configuration type definitions
pub mod types;

/// Custom serialization/deserialization functions
pub mod serialization;

/// Default implementations and helper functions
pub mod defaults;

/// Root configuration types
pub mod multi_site;

/// ConfigStorage-based site loading
pub mod storage_loader;

// Re-export all public types for easier access
pub use multi_site::{MultiSiteConfig, RootConfig};
pub use storage_loader::ConfigStorageLoader;
pub use types::*;
