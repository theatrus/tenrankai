#![allow(clippy::let_and_return)]

pub mod error;
pub mod formats;
pub mod icc;
pub mod types;
pub mod watermark;
pub mod webp_encoder;

pub use error::ImageError;
pub use types::{ImageSize, OutputFormat};
pub use watermark::{CopyrightConfig, add_copyright_notice};

#[cfg(feature = "avif")]
pub use formats::avif::{AvifImageInfo, GainMapInfo};
