/// Cache type definitions and utilities
pub mod types;
pub mod coverage;
pub mod operations;

// Re-export main cache types
pub use types::CacheType;
pub use coverage::FormatCoverage;
pub(crate) use operations::{load_image_metadata_cache, load_cache_version_metadata};
