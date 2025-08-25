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

        // For other formats, check if browser accepts WebP
        if accept_header.contains("image/webp") {
            OutputFormat::WebP
        } else {
            OutputFormat::Jpeg
        }
    }
}
