#[cfg(feature = "avif")]
pub mod avif;
#[cfg(feature = "avif")]
pub mod avif_container;
pub mod jpeg;
pub mod png;
pub mod webp;

pub use super::types::OutputFormat;
