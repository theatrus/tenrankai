use std::ffi::c_void;

/// High-level wrapper around libwebp-sys for encoding WebP images with ICC profile support
pub struct WebPEncoder {
    width: u32,
    height: u32,
    rgb_data: Vec<u8>,
}

#[derive(Debug)]
pub enum WebPError {
    EncodingFailed,
    InvalidDimensions,
    InvalidData,
    MemoryError,
    MuxError,
}

impl std::fmt::Display for WebPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebPError::EncodingFailed => write!(f, "WebP encoding failed"),
            WebPError::InvalidDimensions => write!(f, "Invalid image dimensions"),
            WebPError::InvalidData => write!(f, "Invalid image data"),
            WebPError::MemoryError => write!(f, "Memory allocation error"),
            WebPError::MuxError => write!(f, "WebP mux error"),
        }
    }
}

impl std::error::Error for WebPError {}

impl WebPEncoder {
    /// Create a new WebP encoder with RGB image data
    pub fn new(width: u32, height: u32, rgb_data: Vec<u8>) -> Result<Self, WebPError> {
        if width == 0 || height == 0 {
            return Err(WebPError::InvalidDimensions);
        }

        let expected_size = (width * height * 3) as usize;
        if rgb_data.len() != expected_size {
            return Err(WebPError::InvalidData);
        }

        Ok(WebPEncoder {
            width,
            height,
            rgb_data,
        })
    }

    /// Encode to WebP with quality setting and optional ICC profile
    pub fn encode(&self, quality: f32, icc_profile: Option<&[u8]>) -> Result<Vec<u8>, WebPError> {
        unsafe {
            // First, encode without ICC profile using basic method
            let mut config =
                libwebp_sys::WebPConfig::new().map_err(|_| WebPError::EncodingFailed)?;

            // Set quality (0-100)
            config.quality = quality;
            config.method = 6; // Best compression method

            // Validate configuration
            if libwebp_sys::WebPValidateConfig(&config) == 0 {
                return Err(WebPError::EncodingFailed);
            }

            // Initialize picture structure
            let mut picture =
                libwebp_sys::WebPPicture::new().map_err(|_| WebPError::EncodingFailed)?;

            // Set picture properties
            picture.width = self.width as i32;
            picture.height = self.height as i32;
            picture.use_argb = 0; // Use YUV format

            // Import RGB data
            if libwebp_sys::WebPPictureImportRGB(
                &mut picture,
                self.rgb_data.as_ptr(),
                (self.width * 3) as i32,
            ) == 0
            {
                libwebp_sys::WebPPictureFree(&mut picture);
                return Err(WebPError::EncodingFailed);
            }

            // Set up memory writer
            let mut memory_writer = WebPMemoryWriter::new();
            picture.writer = Some(webp_memory_write);
            picture.custom_ptr = &mut memory_writer as *mut _ as *mut c_void;

            // Encode the image
            let encode_result = libwebp_sys::WebPEncode(&config, &mut picture);

            // Clean up picture
            libwebp_sys::WebPPictureFree(&mut picture);

            if encode_result == 0 {
                return Err(WebPError::EncodingFailed);
            }

            let mut webp_data = memory_writer.into_data();

            // If ICC profile is provided, add it using WebPMux
            if let Some(icc_data) = icc_profile {
                webp_data = self.add_icc_profile_with_mux(&webp_data, icc_data)?;
            }

            Ok(webp_data)
        }
    }

    /// Add ICC profile to existing WebP data using WebPMux
    unsafe fn add_icc_profile_with_mux(
        &self,
        webp_data: &[u8],
        icc_profile: &[u8],
    ) -> Result<Vec<u8>, WebPError> {
        unsafe {
            // Create WebPData structure for input
            let input_data = libwebp_sys::WebPData {
                bytes: webp_data.as_ptr(),
                size: webp_data.len(),
            };

            // Create new mux
            let mux = libwebp_sys::WebPMuxNew();
            if mux.is_null() {
                return Err(WebPError::MuxError);
            }

            // Set the image data
            let set_image_result = libwebp_sys::WebPMuxSetImage(mux, &input_data, 1);
            if set_image_result != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
                libwebp_sys::WebPMuxDelete(mux);
                return Err(WebPError::MuxError);
            }

            // Create WebPData for ICC profile
            let icc_data_struct = libwebp_sys::WebPData {
                bytes: icc_profile.as_ptr(),
                size: icc_profile.len(),
            };

            // Add ICC profile chunk ("ICCP" fourcc)
            let iccp_fourcc = b"ICCP\0".as_ptr() as *const i8;
            let set_result = libwebp_sys::WebPMuxSetChunk(mux, iccp_fourcc, &icc_data_struct, 1);

            if set_result != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
                libwebp_sys::WebPMuxDelete(mux);
                return Err(WebPError::MuxError);
            }

            // Assemble the final WebP data
            let mut assembled_data = libwebp_sys::WebPData {
                bytes: std::ptr::null(),
                size: 0,
            };

            let assemble_result = libwebp_sys::WebPMuxAssemble(mux, &mut assembled_data);

            if assemble_result != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
                libwebp_sys::WebPMuxDelete(mux);
                return Err(WebPError::MuxError);
            }

            // Copy the assembled data to a Vec
            let final_data = if assembled_data.size > 0 && !assembled_data.bytes.is_null() {
                let slice = std::slice::from_raw_parts(assembled_data.bytes, assembled_data.size);
                slice.to_vec()
            } else {
                libwebp_sys::WebPMuxDelete(mux);
                return Err(WebPError::MuxError);
            };

            // Clean up
            libwebp_sys::WebPDataClear(&mut assembled_data);
            libwebp_sys::WebPMuxDelete(mux);

            Ok(final_data)
        }
    }
}

/// Custom memory writer for capturing WebP output
struct WebPMemoryWriter {
    data: Vec<u8>,
}

impl WebPMemoryWriter {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn write(&mut self, data: &[u8]) -> bool {
        self.data.extend_from_slice(data);
        true
    }

    fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// C callback function for writing WebP data to memory
unsafe extern "C" fn webp_memory_write(
    data: *const u8,
    data_size: usize,
    picture: *const libwebp_sys::WebPPicture,
) -> i32 {
    unsafe {
        if data.is_null() || picture.is_null() {
            return 0;
        }

        let writer = (*picture).custom_ptr as *mut WebPMemoryWriter;
        if writer.is_null() {
            return 0;
        }

        let slice = std::slice::from_raw_parts(data, data_size);
        if (*writer).write(slice) { 1 } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webp_encoder_creation() {
        let rgb_data = vec![255u8; 10 * 10 * 3]; // 10x10 white image
        let encoder = WebPEncoder::new(10, 10, rgb_data);
        assert!(encoder.is_ok());
    }

    #[test]
    fn test_invalid_dimensions() {
        let rgb_data = vec![255u8; 10 * 10 * 3];
        let encoder = WebPEncoder::new(0, 10, rgb_data);
        assert!(matches!(encoder, Err(WebPError::InvalidDimensions)));
    }

    #[test]
    fn test_invalid_data_size() {
        let rgb_data = vec![255u8; 100]; // Too small for 10x10 RGB
        let encoder = WebPEncoder::new(10, 10, rgb_data);
        assert!(matches!(encoder, Err(WebPError::InvalidData)));
    }

    #[test]
    fn test_basic_encoding() {
        let rgb_data = vec![128u8; 10 * 10 * 3]; // 10x10 gray image
        let encoder = WebPEncoder::new(10, 10, rgb_data).unwrap();

        let result = encoder.encode(80.0, None);
        assert!(result.is_ok());

        let webp_data = result.unwrap();
        assert!(!webp_data.is_empty());

        // Check that it starts with WebP signature
        assert!(webp_data.len() >= 12);
        assert_eq!(&webp_data[0..4], b"RIFF");
        assert_eq!(&webp_data[8..12], b"WEBP");
    }

    #[test]
    fn test_encoding_with_icc_profile() {
        let rgb_data = vec![200u8; 10 * 10 * 3]; // 10x10 light gray image
        let encoder = WebPEncoder::new(10, 10, rgb_data).unwrap();

        // Create a minimal ICC profile for testing
        let icc_profile = create_test_icc_profile();

        let result = encoder.encode(85.0, Some(&icc_profile));
        assert!(result.is_ok());

        let webp_data = result.unwrap();
        assert!(!webp_data.is_empty());

        // The WebP should be larger than without ICC profile
        let without_icc = encoder.encode(85.0, None).unwrap();
        assert!(webp_data.len() > without_icc.len());

        // Verify the WebP contains the ICCP chunk
        // Look for VP8X format and ICCP chunk
        assert!(webp_data.len() >= 12);
        assert_eq!(&webp_data[0..4], b"RIFF");
        assert_eq!(&webp_data[8..12], b"WEBP");

        // Check for VP8X chunk which indicates extended format
        let mut pos = 12;
        let mut found_vp8x = false;
        let mut found_iccp = false;

        while pos + 8 <= webp_data.len() {
            let chunk_fourcc = &webp_data[pos..pos + 4];
            let chunk_size = u32::from_le_bytes([
                webp_data[pos + 4],
                webp_data[pos + 5],
                webp_data[pos + 6],
                webp_data[pos + 7],
            ]) as usize;

            if chunk_fourcc == b"VP8X" {
                found_vp8x = true;
            } else if chunk_fourcc == b"ICCP" {
                found_iccp = true;
                // Verify the ICC profile data is present
                assert!(chunk_size > 0);
                assert!(pos + 8 + chunk_size <= webp_data.len());
            }

            // Move to next chunk (with padding)
            pos += 8 + chunk_size + (chunk_size % 2);
        }

        assert!(found_vp8x, "VP8X chunk not found - not in extended format");
        assert!(found_iccp, "ICCP chunk not found in WebP data");
    }

    fn create_test_icc_profile() -> Vec<u8> {
        // Minimal valid ICC profile for testing
        vec![
            // Profile header (first 128 bytes)
            0x00, 0x00, 0x01, 0x90, // Profile size (400 bytes)
            b'A', b'D', b'B', b'E', // CMM type
            0x02, 0x40, 0x00, 0x00, // Profile version
            b'd', b'i', b's', b'p', // Device class (display)
            b'R', b'G', b'B', b' ', // Color space (RGB)
            b'X', b'Y', b'Z', b' ', // PCS (XYZ)
            // Creation date/time (12 bytes)
            0x07, 0xe7, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'A', b'D',
            b'B', b'E', // Platform signature
            0x00, 0x00, 0x00, 0x00, // Profile flags
            b'T', b'E', b'S', b'T', // Device manufacturer
            0x00, 0x00, 0x00, 0x00, // Device model
            0x00, 0x00, 0x00, 0x00, // Device attributes (8 bytes)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Rendering intent
            // PCS illuminant (12 bytes) - D50
            0x00, 0x00, 0xf6, 0xd6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xd3, 0x2d, b'T', b'E',
            b'S', b'T', // Profile creator
            // Reserved (44 bytes)
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            // Tag table (starts at byte 128)
            0x00, 0x00, 0x00, 0x01, // Tag count (1 tag)
            // Tag entry for 'desc' tag
            b'd', b'e', b's', b'c', // Tag signature
            0x00, 0x00, 0x00, 0x90, // Offset (144)
            0x00, 0x00, 0x00, 0x40, // Size (64 bytes)
            // Description tag data at offset 144
            b'd', b'e', b's', b'c', // desc type signature
            0x00, 0x00, 0x00, 0x00, // Reserved
            0x00, 0x00, 0x00, 0x0D, // ASCII count (13 chars)
            // ASCII string: "Test Profile\0"
            b'T', b'e', b's', b't', b' ', b'P', b'r', b'o', b'f', b'i', b'l', b'e', 0x00, 0x00,
            0x00, 0x00, // Padding to 4-byte boundary
            0x00, 0x00, 0x00, 0x00, // Unicode code and count (0)
            0x00, 0x00, // ScriptCode count (0)
            // Padding to fill 64 bytes
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]
    }
}
