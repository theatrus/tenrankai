pub mod coverage;
pub mod operations;
pub mod persistent_cache;
/// Cache generation queue for background processing
pub mod queue;
/// Cache type definitions and utilities
pub mod types;

// Re-export main cache types
pub use coverage::FormatCoverage;
pub(crate) use operations::{load_cache_version_metadata, save_cache_version_metadata};
pub use persistent_cache::{CacheError, PersistentCache};
pub use types::CacheType;
