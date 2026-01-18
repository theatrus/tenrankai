//! HEIF/HEIC image format support with HDR and gain map handling.

use crate::error::ImageError;
use four_cc::FourCC;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
use quick_xml::Reader;
use quick_xml::events::Event;
use tracing::{debug, trace};

#[cfg(feature = "avif")]
use super::avif::GainMapInfo;

/// Parsed XMP gain map metadata from Apple HDR photos
#[derive(Debug, Clone, Default)]
struct XmpGainMapMetadata {
    gain_map_min: f32,
    gain_map_max: f32,
    gamma: f32,
    offset_sdr: f32,
    offset_hdr: f32,
    hdr_capacity_min: f32,
    hdr_capacity_max: f32,
    base_rendition_is_hdr: bool,
}

/// HEIF/HEIC specific image information
#[derive(Debug, Clone)]
pub struct HeifImageInfo {
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub is_hdr: bool,
    pub icc_profile: Option<Vec<u8>>,
    pub color_primaries: u16,
    pub transfer_characteristics: u16,
    pub matrix_coefficients: u16,
    pub max_cll: u16,
    pub max_pall: u16,
    pub has_gain_map: bool,
    #[cfg(feature = "avif")]
    pub gain_map_info: Option<GainMapInfo>,
    pub exif_data: Option<Vec<u8>>,
}

impl HeifImageInfo {
    /// Convert to AvifImageInfo for encoding to AVIF output
    #[cfg(feature = "avif")]
    pub fn to_avif_info(&self) -> super::avif::AvifImageInfo {
        super::avif::AvifImageInfo {
            bit_depth: self.bit_depth,
            has_alpha: self.has_alpha,
            is_hdr: self.is_hdr,
            icc_profile: self.icc_profile.clone(),
            color_primaries: self.color_primaries,
            transfer_characteristics: self.transfer_characteristics,
            matrix_coefficients: self.matrix_coefficients,
            max_cll: self.max_cll,
            max_pall: self.max_pall,
            has_gain_map: self.has_gain_map,
            gain_map_info: self.gain_map_info.clone(),
            exif_data: self.exif_data.clone(),
        }
    }
}

/// Check if data appears to be HEIF/HEIC format
pub fn is_heif_format(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    // Check for ftyp box
    if &data[4..8] != b"ftyp" {
        return false;
    }
    // Check brand: heic, heix, mif1, msf1, heif
    let brand = &data[8..12];
    matches!(
        brand,
        b"heic" | b"heix" | b"mif1" | b"msf1" | b"heif" | b"heis" | b"hevc" | b"hevx"
    )
}

/// Read HEIF/HEIC data from memory and extract all metadata
pub fn read_heif_info_from_bytes(data: &[u8]) -> Result<(DynamicImage, HeifImageInfo), ImageError> {
    if !is_heif_format(data) {
        return Err(ImageError::Processing("Not a valid HEIF/HEIC file".into()));
    }

    let ctx = HeifContext::read_from_bytes(data)
        .map_err(|e| ImageError::Processing(format!("Failed to read HEIF context: {}", e)))?;

    let handle = ctx
        .primary_image_handle()
        .map_err(|e| ImageError::Processing(format!("Failed to get primary image: {}", e)))?;

    // Get basic image properties
    let width = handle.width();
    let height = handle.height();
    let has_alpha = handle.has_alpha_channel();
    let bit_depth = handle.luma_bits_per_pixel();

    debug!(
        "HEIF image: {}x{}, bit_depth={}, has_alpha={}",
        width, height, bit_depth, has_alpha
    );

    // Extract color profile information
    let (color_primaries, transfer_characteristics, matrix_coefficients, icc_profile) =
        extract_color_info(&handle);

    // Detect HDR
    let is_hdr = detect_hdr(bit_depth, transfer_characteristics);

    // Extract EXIF data
    let exif_data = extract_exif_from_handle(&handle);

    // Check for gain map (auxiliary images)
    #[cfg(feature = "avif")]
    let (has_gain_map, gain_map_info) = extract_gain_map_info(&ctx, &handle);
    #[cfg(not(feature = "avif"))]
    let has_gain_map = false;

    // Decode the image
    let lib_heif = LibHeif::new();
    let image = if has_alpha {
        lib_heif
            .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
            .map_err(|e| ImageError::Processing(format!("Failed to decode HEIF: {}", e)))?
    } else {
        lib_heif
            .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
            .map_err(|e| ImageError::Processing(format!("Failed to decode HEIF: {}", e)))?
    };

    // Convert to DynamicImage
    let dynamic_image = convert_to_dynamic_image(&image, width, height, has_alpha)?;

    let info = HeifImageInfo {
        bit_depth,
        has_alpha,
        is_hdr,
        icc_profile,
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        max_cll: 0, // HEIF doesn't have standard CLLI like AVIF
        max_pall: 0,
        has_gain_map,
        #[cfg(feature = "avif")]
        gain_map_info,
        exif_data,
    };

    Ok((dynamic_image, info))
}

/// Extract color profile information from image handle
fn extract_color_info(handle: &libheif_rs::ImageHandle) -> (u16, u16, u16, Option<Vec<u8>>) {
    let mut color_primaries: u16 = 2; // Unspecified
    let mut transfer_characteristics: u16 = 2;
    let mut matrix_coefficients: u16 = 2;
    let mut icc_profile: Option<Vec<u8>> = None;

    // Try to get NCLX color profile
    if let Some(nclx) = handle.color_profile_nclx() {
        color_primaries = nclx.color_primaries() as u16;
        transfer_characteristics = nclx.transfer_characteristics() as u16;
        matrix_coefficients = nclx.matrix_coefficients() as u16;
        debug!(
            "HEIF NCLX: primaries={}, transfer={}, matrix={}",
            color_primaries, transfer_characteristics, matrix_coefficients
        );
    }

    // Try to get raw ICC profile
    if let Some(raw_profile) = handle.color_profile_raw() {
        let profile_data = raw_profile.data.to_vec();
        debug!("HEIF ICC profile: {} bytes", profile_data.len());
        icc_profile = Some(profile_data);
    }

    (
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        icc_profile,
    )
}

/// Detect if image is HDR based on metadata
fn detect_hdr(bit_depth: u8, transfer_characteristics: u16) -> bool {
    // Must be > 8-bit and have HDR transfer function: PQ (16) or HLG (18)
    bit_depth > 8 && matches!(transfer_characteristics, 16 | 18)
}

/// Extract EXIF data from image handle
fn extract_exif_from_handle(handle: &libheif_rs::ImageHandle) -> Option<Vec<u8>> {
    // FourCC for Exif metadata type
    let exif_type = FourCC(*b"Exif");

    // Get number of metadata blocks
    let num_blocks = handle.number_of_metadata_blocks(exif_type);
    if num_blocks <= 0 {
        trace!("No EXIF metadata blocks found in HEIF");
        return None;
    }

    // Get metadata block IDs
    let mut block_ids = vec![0u32; num_blocks as usize];
    let count = handle.metadata_block_ids(&mut block_ids, exif_type);

    if count == 0 || block_ids.is_empty() {
        return None;
    }

    // Get the first EXIF block
    if let Ok(exif_data) = handle.metadata(block_ids[0]) {
        // HEIF EXIF data starts with 4-byte offset, skip it for rexif compatibility
        if exif_data.len() > 4 {
            debug!("HEIF EXIF data: {} bytes", exif_data.len());
            // The first 4 bytes are the Exif header offset, skip them
            return Some(exif_data[4..].to_vec());
        }
    }

    None
}

/// Extract XMP metadata from image handle
fn extract_xmp_from_handle(handle: &libheif_rs::ImageHandle) -> Option<Vec<u8>> {
    // FourCC for XMP/MIME metadata type
    let mime_type = FourCC(*b"mime");

    let num_blocks = handle.number_of_metadata_blocks(mime_type);
    if num_blocks <= 0 {
        trace!("No XMP/MIME metadata blocks found in HEIF");
        return None;
    }

    let mut block_ids = vec![0u32; num_blocks as usize];
    let count = handle.metadata_block_ids(&mut block_ids, mime_type);

    if count == 0 || block_ids.is_empty() {
        return None;
    }

    // Check each block for XMP content
    for block_id in block_ids {
        if let Ok(data) = handle.metadata(block_id)
            && let Ok(text) = std::str::from_utf8(&data)
            && (text.contains("xmpmeta") || text.contains("rdf:RDF"))
        {
            debug!("Found XMP metadata: {} bytes", data.len());
            return Some(data);
        }
    }

    None
}

/// Parse XMP data for Apple HDR gain map metadata
fn parse_xmp_gain_map_metadata(xmp_data: &[u8]) -> Option<XmpGainMapMetadata> {
    let text = std::str::from_utf8(xmp_data).ok()?;

    // Check if this contains Apple HDR gain map data
    if !text.contains("HDRGainMap") && !text.contains("hdrgm") {
        return None;
    }

    let mut metadata = XmpGainMapMetadata {
        gamma: 1.0, // Default values
        ..Default::default()
    };

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                // Check for rdf:Description with hdrgm attributes
                if e.name().as_ref() == b"rdf:Description" {
                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                        let value = std::str::from_utf8(&attr.value).unwrap_or("");

                        match key {
                            "hdrgm:GainMapMin" => {
                                metadata.gain_map_min = value.parse().unwrap_or(0.0);
                            }
                            "hdrgm:GainMapMax" => {
                                metadata.gain_map_max = value.parse().unwrap_or(1.0);
                            }
                            "hdrgm:Gamma" => {
                                metadata.gamma = value.parse().unwrap_or(1.0);
                            }
                            "hdrgm:OffsetSDR" => {
                                metadata.offset_sdr = value.parse().unwrap_or(0.0);
                            }
                            "hdrgm:OffsetHDR" => {
                                metadata.offset_hdr = value.parse().unwrap_or(0.0);
                            }
                            "hdrgm:HDRCapacityMin" => {
                                metadata.hdr_capacity_min = value.parse().unwrap_or(0.0);
                            }
                            "hdrgm:HDRCapacityMax" => {
                                metadata.hdr_capacity_max = value.parse().unwrap_or(1.0);
                            }
                            "hdrgm:BaseRenditionIsHDR" => {
                                metadata.base_rendition_is_hdr =
                                    value.eq_ignore_ascii_case("true") || value == "1";
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                trace!("XMP parsing error: {}", e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    // Only return if we found meaningful gain map data
    if metadata.gain_map_max > 0.0 || metadata.hdr_capacity_max > 0.0 {
        debug!(
            "Parsed XMP gain map: min={}, max={}, gamma={}, headroom={}",
            metadata.gain_map_min, metadata.gain_map_max, metadata.gamma, metadata.hdr_capacity_max
        );
        Some(metadata)
    } else {
        None
    }
}

/// Extract gain map information from auxiliary images
#[cfg(feature = "avif")]
fn extract_gain_map_info(
    _ctx: &HeifContext,
    handle: &libheif_rs::ImageHandle,
) -> (bool, Option<GainMapInfo>) {
    // Get auxiliary images (None filter = get all auxiliary images)
    let aux_handles = handle.auxiliary_images(None);

    if aux_handles.is_empty() {
        return (false, None);
    }

    // Try to extract XMP metadata for gain map parameters
    let xmp_metadata =
        extract_xmp_from_handle(handle).and_then(|xmp| parse_xmp_gain_map_metadata(&xmp));

    let lib_heif = LibHeif::new();

    for aux_handle in aux_handles {
        // Check auxiliary type for gain map indicators
        if let Ok(aux_type) = aux_handle.auxiliary_type() {
            let type_str = aux_type.to_lowercase();
            if type_str.contains("gainmap")
                || type_str.contains("hdrgainmap")
                || type_str.contains("urn:com:apple:photo:2020:aux:hdrgainmap")
            {
                debug!("Found HEIF gain map auxiliary image: {}", aux_type);

                // Try to decode the gain map image
                let gain_map_image = decode_auxiliary_image(&lib_heif, &aux_handle);

                if let Some(ref gm_img) = gain_map_image {
                    debug!(
                        "Successfully decoded HEIF gain map image: {}x{}",
                        gm_img.width(),
                        gm_img.height()
                    );
                }

                // Use XMP metadata if available, otherwise use defaults
                let (
                    gamma,
                    min,
                    max,
                    base_offset,
                    alternate_offset,
                    base_headroom,
                    alt_headroom,
                    use_base,
                ) = if let Some(ref xmp) = xmp_metadata {
                    (
                        [xmp.gamma, xmp.gamma, xmp.gamma],
                        [xmp.gain_map_min, xmp.gain_map_min, xmp.gain_map_min],
                        [xmp.gain_map_max, xmp.gain_map_max, xmp.gain_map_max],
                        [xmp.offset_sdr, xmp.offset_sdr, xmp.offset_sdr],
                        [xmp.offset_hdr, xmp.offset_hdr, xmp.offset_hdr],
                        xmp.hdr_capacity_min.max(1.0), // headroom must be >= 1.0
                        xmp.hdr_capacity_max.max(1.0),
                        !xmp.base_rendition_is_hdr,
                    )
                } else {
                    (
                        [1.0, 1.0, 1.0],
                        [0.0, 0.0, 0.0],
                        [1.0, 1.0, 1.0],
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 0.0],
                        1.0,
                        1.0,
                        true,
                    )
                };

                return (
                    true,
                    Some(GainMapInfo {
                        has_image: gain_map_image.is_some(),
                        gamma,
                        min,
                        max,
                        base_offset,
                        alternate_offset,
                        base_hdr_headroom: base_headroom,
                        alternate_hdr_headroom: alt_headroom,
                        use_base_color_space: use_base,
                        gain_map_image,
                    }),
                );
            }
        }
    }

    (false, None)
}

/// Decode an auxiliary image handle to DynamicImage
#[cfg(feature = "avif")]
fn decode_auxiliary_image(
    lib_heif: &LibHeif,
    aux_handle: &libheif_rs::ImageHandle,
) -> Option<DynamicImage> {
    let width = aux_handle.width();
    let height = aux_handle.height();
    let has_alpha = aux_handle.has_alpha_channel();

    // Decode the auxiliary image (gain maps are typically grayscale or RGB)
    let image = if has_alpha {
        lib_heif
            .decode(aux_handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
            .ok()?
    } else {
        lib_heif
            .decode(aux_handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
            .ok()?
    };

    // Convert to DynamicImage
    match convert_to_dynamic_image(&image, width, height, has_alpha) {
        Ok(img) => Some(img),
        Err(e) => {
            trace!("Failed to convert auxiliary image: {}", e);
            None
        }
    }
}

/// Convert libheif Image to DynamicImage
fn convert_to_dynamic_image(
    image: &libheif_rs::Image,
    width: u32,
    height: u32,
    has_alpha: bool,
) -> Result<DynamicImage, ImageError> {
    let planes = image.planes();
    let interleaved = planes
        .interleaved
        .ok_or_else(|| ImageError::Processing("No interleaved plane data".into()))?;

    let data = interleaved.data;
    let stride = interleaved.stride;

    if has_alpha {
        // RGBA
        let mut img_buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);

        for y in 0..height {
            let row_start = (y as usize) * stride;
            for x in 0..width {
                let px_start = row_start + (x as usize) * 4;
                if px_start + 3 < data.len() {
                    img_buf.put_pixel(
                        x,
                        y,
                        Rgba([
                            data[px_start],
                            data[px_start + 1],
                            data[px_start + 2],
                            data[px_start + 3],
                        ]),
                    );
                }
            }
        }

        Ok(DynamicImage::ImageRgba8(img_buf))
    } else {
        // RGB
        let mut img_buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(width, height);

        for y in 0..height {
            let row_start = (y as usize) * stride;
            for x in 0..width {
                let px_start = row_start + (x as usize) * 3;
                if px_start + 2 < data.len() {
                    img_buf.put_pixel(
                        x,
                        y,
                        Rgb([data[px_start], data[px_start + 1], data[px_start + 2]]),
                    );
                }
            }
        }

        Ok(DynamicImage::ImageRgb8(img_buf))
    }
}

/// Extract raw EXIF bytes from HEIF data (standalone function for metadata extraction)
pub fn extract_exif_data_from_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if !is_heif_format(data) {
        return None;
    }

    let ctx = HeifContext::read_from_bytes(data).ok()?;
    let handle = ctx.primary_image_handle().ok()?;

    extract_exif_from_handle(&handle)
}

/// Extract ICC profile from HEIF data
pub fn extract_icc_profile_from_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if !is_heif_format(data) {
        return None;
    }

    let ctx = HeifContext::read_from_bytes(data).ok()?;
    let handle = ctx.primary_image_handle().ok()?;

    handle.color_profile_raw().map(|p| p.data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heif_format_detection() {
        // Valid HEIC magic bytes
        let heic_data = b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00";
        assert!(is_heif_format(heic_data));

        // Valid HEIF magic bytes
        let heif_data = b"\x00\x00\x00\x18ftypheif\x00\x00\x00\x00";
        assert!(is_heif_format(heif_data));

        // Valid mif1 brand
        let mif1_data = b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00";
        assert!(is_heif_format(mif1_data));

        // Invalid data - JPEG
        let jpeg_data = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
        assert!(!is_heif_format(jpeg_data));

        // Too short
        let short_data = b"short";
        assert!(!is_heif_format(short_data));
    }

    #[test]
    fn test_hdr_detection() {
        // 10-bit with PQ transfer = HDR
        assert!(detect_hdr(10, 16));

        // 10-bit with HLG transfer = HDR
        assert!(detect_hdr(10, 18));

        // 8-bit is never HDR
        assert!(!detect_hdr(8, 16));

        // 10-bit with sRGB transfer = not HDR
        assert!(!detect_hdr(10, 13));
    }
}
