#![allow(clippy::let_and_return)]

pub mod error;
pub mod formats;
pub mod icc;
pub mod image_watermark;
pub mod luminance;
pub mod types;
pub mod watermark;
pub mod webp_encoder;

pub use error::ImageError;
pub use image_watermark::{ImageWatermark, WatermarkConfig, WatermarkPosition};
pub use types::{ImageSize, OutputFormat};
pub use watermark::{CopyrightConfig, add_copyright_notice};

#[cfg(feature = "avif")]
pub use formats::avif::{AvifImageInfo, GainMapInfo};

#[cfg(feature = "heif")]
pub use formats::heif::{HeifImageInfo, is_heif_format, read_heif_info_from_bytes};
