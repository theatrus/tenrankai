/// Configuration type definitions
pub mod types;

/// Custom serialization/deserialization functions
pub mod serialization;

/// Default implementations and helper functions
pub mod defaults;

/// Multi-site configuration types
pub mod multi_site;

// Re-export all public types for easier access
pub use multi_site::{MultiSiteConfig, SiteConfigSection};
pub use types::*;
