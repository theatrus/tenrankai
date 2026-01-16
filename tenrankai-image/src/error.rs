use thiserror::Error;

/// Error type for image processing operations
#[derive(Error, Debug)]
pub enum ImageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image decoding/encoding error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Processing error: {0}")]
    Processing(String),

    #[error("WebP encoding error: {0}")]
    WebP(#[from] crate::webp_encoder::WebPError),

    #[error("Invalid format")]
    InvalidFormat,

    #[error("Unsupported operation")]
    Unsupported,
}
