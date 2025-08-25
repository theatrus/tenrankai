use image::DynamicImage;
use std::path::Path;
use tracing::{debug, error};

use crate::gallery::GalleryError;
use crate::webp_encoder::{WebPEncoder, WebPError};

/// Save image as WebP with optional ICC profile
pub fn save_with_profile(
    image: &DynamicImage,
    path: &Path,
    quality: f32,
    icc_profile: Option<&[u8]>,
) -> Result<(), GalleryError> {
    let rgb_image = image.to_rgb8();
    let (width, height) = rgb_image.dimensions();
    let rgb_data = rgb_image.into_raw();

    match WebPEncoder::new(width, height, rgb_data) {
        Ok(encoder) => match encoder.encode(quality, icc_profile) {
            Ok(webp_data) => {
                std::fs::write(path, webp_data)?;
                if let Some(profile_data) = icc_profile {
                    debug!(
                        "WebP written with ICC profile: {} bytes",
                        profile_data.len()
                    );
                } else {
                    debug!("WebP written without ICC profile");
                }
                Ok(())
            }
            Err(WebPError::EncodingFailed) => {
                error!("WebP encoding failed, falling back to basic webp crate");
                save_fallback(image, path, quality)
            }
            Err(e) => {
                error!(
                    "WebP encoding error: {}, falling back to basic webp crate",
                    e
                );
                save_fallback(image, path, quality)
            }
        },
        Err(e) => {
            error!(
                "Failed to create WebP encoder: {}, falling back to basic webp crate",
                e
            );
            save_fallback(image, path, quality)
        }
    }
}

/// Fallback WebP encoder using the basic webp crate (no ICC profile support)
fn save_fallback(image: &DynamicImage, path: &Path, quality: f32) -> Result<(), GalleryError> {
    let rgb_image = image.to_rgb8();
    let (width, height) = rgb_image.dimensions();
    let rgb_data = rgb_image.into_raw();

    let encoder = webp::Encoder::from_rgb(&rgb_data, width, height);
    let encoded_webp = encoder.encode(quality);
    std::fs::write(path, &*encoded_webp)?;
    Ok(())
}
