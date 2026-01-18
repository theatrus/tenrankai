use image::{DynamicImage, ImageEncoder, codecs::jpeg::JpegEncoder};
use std::io::Read;
use std::path::Path;
use tracing::{debug, warn};

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

    extract_icc_profile_from_bytes_with_path(&buffer, Some(path))
}

/// Extract ICC profile from JPEG data in memory
pub fn extract_icc_profile_from_bytes(buffer: &[u8]) -> Option<Vec<u8>> {
    extract_icc_profile_from_bytes_with_path(buffer, None)
}

/// Extract ICC profile from JPEG data with optional path for error logging
fn extract_icc_profile_from_bytes_with_path(buffer: &[u8], path: Option<&Path>) -> Option<Vec<u8>> {
    // Look for ICC profile in JPEG APP2 segments
    // ICC profiles in JPEG are stored in APP2 markers with ICC_PROFILE identifier
    let mut pos = 0;
    while pos < buffer.len().saturating_sub(1) {
        if buffer[pos] == 0xFF {
            let marker = buffer[pos + 1];
            if marker == 0xE2 {
                // APP2 marker
                if pos + 4 < buffer.len() {
                    let segment_length =
                        u16::from_be_bytes([buffer[pos + 2], buffer[pos + 3]]) as usize;
                    // segment_length includes the 2 length bytes, so must be >= 2 for valid data
                    if segment_length < 2 {
                        if let Some(p) = path {
                            warn!(
                                "Malformed JPEG APP2 segment in {}: segment_length={} at position {}",
                                p.display(),
                                segment_length,
                                pos
                            );
                        }
                        pos += 2;
                        continue;
                    }
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

/// Encode image as JPEG with optional ICC profile, returning bytes
pub fn encode_with_profile(
    image: &DynamicImage,
    quality: u8,
    icc_profile: Option<&[u8]>,
) -> Result<Vec<u8>, GalleryError> {
    let rgb_image = image.to_rgb8();
    let mut buffer = Vec::new();

    if let Some(profile_data) = icc_profile {
        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
        match encoder.set_icc_profile(profile_data.to_vec()) {
            Ok(()) => {
                encoder.write_image(
                    &rgb_image,
                    rgb_image.width(),
                    rgb_image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
                debug!(
                    "JPEG encoded with ICC profile: {} bytes",
                    profile_data.len()
                );
            }
            Err(e) => {
                debug!(
                    "Failed to set ICC profile on JPEG encoder ({}), encoding without profile",
                    e
                );
                buffer.clear();
                let encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
                encoder.write_image(
                    &rgb_image,
                    rgb_image.width(),
                    rgb_image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
            }
        }
    } else {
        let encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
        encoder.write_image(
            &rgb_image,
            rgb_image.width(),
            rgb_image.height(),
            image::ExtendedColorType::Rgb8,
        )?;
    }

    Ok(buffer)
}

/// Save image as JPEG with optional ICC profile
pub fn save_with_profile(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    icc_profile: Option<&[u8]>,
) -> Result<(), GalleryError> {
    let data = encode_with_profile(image, quality, icc_profile)?;
    std::fs::write(path, data)?;
    Ok(())
}
