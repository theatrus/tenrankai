#[cfg(feature = "avif")]
pub mod avif;
#[cfg(feature = "avif")]
pub mod avif_container;
pub mod jpeg;
pub mod png;
pub mod webp;

pub use super::types::OutputFormat;

use crate::gallery::Gallery;

impl Gallery {
    /// Determine output format for resized images based on browser support and source format
    /// Note: This is only used for resized images. Original images are always served as-is.
    pub fn determine_output_format(&self, accept_header: &str, source_path: &str) -> OutputFormat {
        // PNG sources should always output as PNG to preserve transparency and quality
        if OutputFormat::should_preserve_source_format(source_path) {
            return OutputFormat::Png;
        }

        // For all other sources (including AVIF), use browser capability negotiation
        // This allows AVIF sources to be served as WebP/JPEG when browser doesn't support AVIF
        OutputFormat::from_accept_header(accept_header)
    }
}
