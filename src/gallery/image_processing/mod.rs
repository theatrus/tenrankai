// Image processing module - handles image resizing, format conversion, and serving
pub mod formats;
mod icc;
mod resize;
mod serve;
mod types;
mod watermark;

// Re-export public items
pub use types::OutputFormat;

// Re-export format-specific ICC profile extraction functions
pub use formats::{
    avif::extract_icc_profile as extract_icc_profile_from_avif,
    jpeg::extract_icc_profile as extract_icc_profile_from_jpeg,
    png::extract_icc_profile as extract_icc_profile_from_png,
};
pub use icc::extract_icc_profile_name;

// Note: Gallery methods like serve_image, get_resized_image, etc. are implemented
// as impl blocks in the respective modules

#[cfg(test)]
mod tests {
    mod avif_tests;
    mod cache_tests;
    mod composite_tests;
    mod icc_profile_tests;
    mod jpeg_tests;
    mod png_tests;
    mod watermark_tests;
}
