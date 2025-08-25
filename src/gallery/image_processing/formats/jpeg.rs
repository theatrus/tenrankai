use image::{DynamicImage, ImageEncoder, codecs::jpeg::JpegEncoder};
use std::io::Read;
use std::path::Path;
use tracing::debug;

use crate::gallery::GalleryError;

/// Extract ICC profile from JPEG file
pub fn extract_icc_profile(path: &Path) -> Option<Vec<u8>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return None;
    }

    // Look for ICC profile in JPEG APP2 segments
    // ICC profiles in JPEG are stored in APP2 markers with ICC_PROFILE identifier
    let mut pos = 0;
    while pos < buffer.len() - 1 {
        if buffer[pos] == 0xFF {
            let marker = buffer[pos + 1];
            if marker == 0xE2 {
                // APP2 marker
                if pos + 4 < buffer.len() {
                    let segment_length =
                        u16::from_be_bytes([buffer[pos + 2], buffer[pos + 3]]) as usize;
                    if pos + 2 + segment_length <= buffer.len() {
                        let segment_start = pos + 4;
                        let segment_end = pos + 2 + segment_length;
                        let segment_data = &buffer[segment_start..segment_end];

                        // Check for ICC_PROFILE identifier
                        if segment_data.len() > 12 && segment_data.starts_with(b"ICC_PROFILE\0") {
                            // ICC profile data starts after the identifier and 2 sequence bytes
                            let icc_data = &segment_data[14..];
                            if !icc_data.is_empty() {
                                debug!("Found ICC profile in JPEG: {} bytes", icc_data.len());
                                return Some(icc_data.to_vec());
                            }
                        }
                        pos = segment_end;
                    } else {
                        pos += 2;
                    }
                } else {
                    pos += 2;
                }
            } else {
                pos += 2;
            }
        } else {
            pos += 1;
        }
    }

    None
}

/// Save image as JPEG with optional ICC profile
pub fn save_with_profile(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    icc_profile: Option<&[u8]>,
) -> Result<(), GalleryError> {
    // JPEG doesn't support alpha channel, so convert to RGB
    let rgb_image = image.to_rgb8();
    let output = std::fs::File::create(path)?;

    if let Some(profile_data) = icc_profile {
        // Create encoder with ICC profile
        let mut encoder = JpegEncoder::new_with_quality(output, quality);
        match encoder.set_icc_profile(profile_data.to_vec()) {
            Ok(()) => {
                encoder.write_image(
                    &rgb_image,
                    rgb_image.width(),
                    rgb_image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
                debug!(
                    "JPEG written with ICC profile: {} bytes",
                    profile_data.len()
                );
            }
            Err(e) => {
                debug!(
                    "Failed to set ICC profile on JPEG encoder ({}), using standard JPEG",
                    e
                );
                let encoder = JpegEncoder::new_with_quality(std::fs::File::create(path)?, quality);
                encoder.write_image(
                    &rgb_image,
                    rgb_image.width(),
                    rgb_image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
            }
        }
    } else {
        // No ICC profile, use standard encoder
        let encoder = JpegEncoder::new_with_quality(output, quality);
        encoder.write_image(
            &rgb_image,
            rgb_image.width(),
            rgb_image.height(),
            image::ExtendedColorType::Rgb8,
        )?;
    }

    Ok(())
}
