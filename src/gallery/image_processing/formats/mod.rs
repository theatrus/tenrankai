pub mod avif;
pub mod jpeg;
pub mod png;
pub mod webp;

pub use super::types::OutputFormat;

use crate::gallery::Gallery;

impl Gallery {
    pub fn determine_output_format(&self, accept_header: &str, source_path: &str) -> OutputFormat {
        // PNG sources should always output as PNG to preserve transparency and quality
        if source_path.to_lowercase().ends_with(".png") {
            return OutputFormat::Png;
        }

        // AVIF sources should output as AVIF when supported to preserve HDR
        if source_path.to_lowercase().ends_with(".avif") && accept_header.contains("image/avif") {
            return OutputFormat::Avif;
        }

        // For other formats, check browser support in priority order
        if accept_header.contains("image/avif") {
            OutputFormat::Avif
        } else if accept_header.contains("image/webp") {
            OutputFormat::WebP
        } else {
            OutputFormat::Jpeg
        }
    }
}
