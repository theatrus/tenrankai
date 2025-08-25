use flate2::read::ZlibDecoder;
use image::{DynamicImage, codecs::png::PngEncoder};
use std::io::Read;
use std::path::Path;
use tracing::debug;

use crate::gallery::GalleryError;

/// Extract ICC profile from PNG file
pub fn extract_icc_profile(path: &Path) -> Option<Vec<u8>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return None;
    }

    // PNG signature check
    if buffer.len() < 8 || &buffer[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    let mut pos = 8; // Skip PNG signature

    while pos + 12 <= buffer.len() {
        // Read chunk length (4 bytes, big-endian)
        let chunk_length = u32::from_be_bytes([
            buffer[pos],
            buffer[pos + 1],
            buffer[pos + 2],
            buffer[pos + 3],
        ]) as usize;

        // Read chunk type (4 bytes)
        let chunk_type = &buffer[pos + 4..pos + 8];

        // Check for iCCP chunk (ICC profile)
        if chunk_type == b"iCCP" {
            let chunk_data_start = pos + 8;
            let chunk_data_end = chunk_data_start + chunk_length;

            if chunk_data_end <= buffer.len() {
                let chunk_data = &buffer[chunk_data_start..chunk_data_end];

                // iCCP chunk format:
                // - Profile name (null-terminated string)
                // - Compression method (1 byte, should be 0 for deflate)
                // - Compressed profile data

                // Find null terminator for profile name
                if let Some(null_pos) = chunk_data.iter().position(|&b| b == 0)
                    && null_pos + 2 < chunk_data.len()
                {
                    let compression_method = chunk_data[null_pos + 1];

                    if compression_method == 0 {
                        // Deflate compression
                        let compressed_data = &chunk_data[null_pos + 2..];

                        // Decompress using flate2
                        let mut decoder = ZlibDecoder::new(compressed_data);
                        let mut decompressed = Vec::new();

                        if decoder.read_to_end(&mut decompressed).is_ok() {
                            debug!(
                                "Found ICC profile in PNG: {} bytes (decompressed)",
                                decompressed.len()
                            );
                            return Some(decompressed);
                        }
                    }
                }
            }
        }

        // Move to next chunk (length + type + data + CRC)
        pos += 8 + chunk_length + 4;

        // Stop at IEND chunk
        if chunk_type == b"IEND" {
            break;
        }
    }

    None
}

/// Save image as PNG
pub fn save(image: &DynamicImage, path: &Path) -> Result<(), GalleryError> {
    let output = std::fs::File::create(path)?;
    let encoder = PngEncoder::new(output);
    image.write_with_encoder(encoder)?;
    Ok(())
}
