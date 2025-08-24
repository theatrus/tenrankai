pub mod core;
pub mod error;
pub mod handlers;
pub mod types;

pub use core::PostsManager;
pub use error::PostsError;
pub use types::{Post, PostMetadata, PostSummary, PostsConfig};

#[cfg(test)]
mod tests;