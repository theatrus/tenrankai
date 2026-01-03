pub mod coverage;
pub mod operations;
/// Cache type definitions and utilities
pub mod types;

// Re-export main cache types
pub use coverage::FormatCoverage;
pub(crate) use operations::{
    load_cache_version_metadata, load_image_metadata_cache, save_cache_version_metadata,
    save_image_metadata_cache,
};
pub use types::CacheType;
