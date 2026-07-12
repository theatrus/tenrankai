// Image processing module - handles image resizing, format conversion, and serving
pub mod formats;
mod icc;
mod resize;
mod serve;
mod types;
mod watermark;

// Re-export public items
pub use types::{LoadedImage, OutputFormat};

// Re-export format-specific ICC profile extraction functions (path-based)
#[cfg(feature = "avif")]
pub use formats::avif::extract_icc_profile as extract_icc_profile_from_avif;

pub use formats::{
    jpeg::extract_icc_profile as extract_icc_profile_from_jpeg,
    png::extract_icc_profile as extract_icc_profile_from_png,
};
pub use icc::extract_icc_profile_name;
pub use serve::{pending_generation_response, serve_image_with_generation_queue};

// Re-export byte-based extraction functions for storage abstraction
pub use formats::{
    jpeg::extract_icc_profile_from_bytes as extract_icc_profile_from_jpeg_bytes,
    png::extract_icc_profile_from_bytes as extract_icc_profile_from_png_bytes,
};

#[cfg(feature = "avif")]
pub use formats::avif::{
    extract_color_description_from_bytes as extract_avif_color_description_from_bytes,
    extract_dimensions_from_bytes as extract_avif_dimensions_from_bytes,
    extract_exif_data_from_bytes as extract_avif_exif_from_bytes,
};

// Note: Gallery methods like serve_image, get_resized_image, etc. are implemented
// as impl blocks in the respective modules

#[cfg(test)]
mod tests {
    #[cfg(feature = "avif")]
    mod avif_tests;
    mod cache_tests;
    mod composite_tests;
    mod icc_profile_tests;
    mod jpeg_tests;
    mod png_tests;
    mod watermark_tests;
}
