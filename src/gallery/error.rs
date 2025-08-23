use std::fmt;

#[derive(Debug)]
pub enum GalleryError {
    IoError(std::io::Error),
    ImageError(image::ImageError),
    SerdeError(serde_json::Error),
    InvalidPath,
}

impl fmt::Display for GalleryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GalleryError::IoError(e) => write!(f, "IO error: {}", e),
            GalleryError::ImageError(e) => write!(f, "Image error: {}", e),
            GalleryError::SerdeError(e) => write!(f, "Serialization error: {}", e),
            GalleryError::InvalidPath => write!(f, "Invalid path"),
        }
    }
}

impl std::error::Error for GalleryError {}

impl From<std::io::Error> for GalleryError {
    fn from(error: std::io::Error) -> Self {
        GalleryError::IoError(error)
    }
}

impl From<image::ImageError> for GalleryError {
    fn from(error: image::ImageError) -> Self {
        GalleryError::ImageError(error)
    }
}

impl From<serde_json::Error> for GalleryError {
    fn from(error: serde_json::Error) -> Self {
        GalleryError::SerdeError(error)
    }
}