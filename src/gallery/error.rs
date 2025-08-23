use thiserror::Error;

#[derive(Debug, Error)]
pub enum GalleryError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("Invalid path")]
    InvalidPath,

    #[error("Not found")]
    NotFound,
}
