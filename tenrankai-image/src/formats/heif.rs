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
    let (mut color_primaries, mut transfer_characteristics, mut matrix_coefficients, icc_profile) =
        extract_color_info(&handle);

    // Extract EXIF data
    let exif_data = extract_exif_from_handle(&handle);

    // Check for gain map (auxiliary images)
    #[cfg(feature = "avif")]
    let (has_gain_map, gain_map_info) = extract_gain_map_info(&ctx, &handle);
    #[cfg(not(feature = "avif"))]
    let has_gain_map = false;

    // For Apple HDR photos with gain maps but unspecified color properties,
    // default to Display P3 with sRGB transfer (typical for iPhone photos)
    if has_gain_map && color_primaries == 2 && transfer_characteristics == 2 {
        // Check if ICC profile looks like Display P3 (around 536-560 bytes)
        let is_likely_p3 = icc_profile.as_ref().is_some_and(|p| p.len() >= 500 && p.len() < 600);
        if is_likely_p3 {
            color_primaries = 12; // Display P3
            transfer_characteristics = 13; // sRGB
            matrix_coefficients = 1; // BT.709
            debug!(
                "Defaulting to Display P3/sRGB for Apple HDR photo (unspecified NCLX with gain map)"
            );
        } else {
            // Default to BT.709/sRGB for standard SDR base
            color_primaries = 1; // BT.709
            transfer_characteristics = 13; // sRGB
            matrix_coefficients = 1; // BT.709
            debug!(
                "Defaulting to BT.709/sRGB for Apple HDR photo (unspecified NCLX with gain map)"
            );
        }
    }

    // Detect HDR: either via transfer characteristics OR presence of gain map
    // Apple HDR photos use 8-bit SDR base layer with gain map, so we can't rely
    // solely on bit depth and transfer function
    let is_hdr = detect_hdr(bit_depth, transfer_characteristics) || has_gain_map;

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
        // HEIF EXIF data format:
        // - Bytes 0-3: 4-byte offset (usually 0)
        // - Bytes 4-9: "Exif\0\0" header (6 bytes)
        // - Bytes 10+: TIFF header (starts with "MM" or "II")
        // rexif expects just the TIFF data, so we need to skip both prefixes
        if exif_data.len() > 10 {
            let data = &exif_data[4..]; // Skip 4-byte offset first

            // Check for and skip "Exif\0\0" header
            let tiff_data = if data.starts_with(b"Exif\x00\x00") {
                debug!("HEIF EXIF data: {} bytes (stripping Exif header)", exif_data.len());
                &data[6..]
            } else {
                debug!("HEIF EXIF data: {} bytes", exif_data.len());
                data
            };

            // Verify we have a valid TIFF header (MM or II)
            if tiff_data.len() >= 4
                && (tiff_data.starts_with(b"MM") || tiff_data.starts_with(b"II"))
            {
                return Some(tiff_data.to_vec());
            }
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
    let mut current_element: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let name_bytes = e.name();
                let name = std::str::from_utf8(name_bytes.as_ref()).unwrap_or("");

                // Check for rdf:Description with hdrgm attributes (ISO 21496-1 format)
                if name == "rdf:Description" {
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

                // Track element name for Apple's nested element format
                current_element = Some(name.to_string());
            }
            Ok(Event::Text(e)) => {
                // Handle Apple's HDRGainMap format where values are element content
                if let Some(ref elem) = current_element {
                    // Convert BytesText to string (quick-xml 0.39 API)
                    let text_bytes = e.as_ref();
                    if let Ok(value_str) = std::str::from_utf8(text_bytes) {
                        let value = value_str.trim();
                        match elem.as_str() {
                            "HDRGainMap:HDRGainMapHeadroom" => {
                                // Apple uses a single headroom value for both min and max
                                let headroom: f32 = value.parse().unwrap_or(1.0);
                                metadata.hdr_capacity_max = headroom;
                                debug!("Found Apple HDRGainMapHeadroom: {}", headroom);
                            }
                            "HDRGainMap:HDRGainMapVersion" => {
                                trace!("Apple HDRGainMap version: {}", value);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                current_element = None;
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

/// Extract XMP metadata from an auxiliary image handle
fn extract_xmp_from_aux_handle(aux_handle: &libheif_rs::ImageHandle) -> Option<Vec<u8>> {
    let mime_type = FourCC(*b"mime");

    let num_blocks = aux_handle.number_of_metadata_blocks(mime_type);
    if num_blocks <= 0 {
        return None;
    }

    let mut block_ids = vec![0u32; num_blocks as usize];
    let count = aux_handle.metadata_block_ids(&mut block_ids, mime_type);

    if count == 0 || block_ids.is_empty() {
        return None;
    }

    for block_id in block_ids {
        if let Ok(data) = aux_handle.metadata(block_id)
            && let Ok(text) = std::str::from_utf8(&data)
            && (text.contains("xmpmeta") || text.contains("rdf:RDF"))
        {
            debug!("Found XMP metadata on auxiliary image: {} bytes", data.len());
            return Some(data);
        }
    }

    None
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

                // Extract XMP metadata from the AUXILIARY image (not primary)
                // Apple stores HDRGainMapHeadroom on the gain map image itself
                let xmp_metadata = extract_xmp_from_aux_handle(&aux_handle)
                    .and_then(|xmp| parse_xmp_gain_map_metadata(&xmp));

                // Try to decode the gain map image
                let raw_gain_map = decode_auxiliary_image(&lib_heif, &aux_handle);

                // Check if this is Apple's simple format
                let is_apple_format = xmp_metadata
                    .as_ref()
                    .is_some_and(|xmp| xmp.gamma == 1.0 && xmp.gain_map_min == 0.0 && xmp.gain_map_max == 0.0);

                // Transform Apple's gain map from 128-255 range to 0-255 range
                // Apple uses 128 as neutral (SDR), ISO 21496-1 uses 0 as neutral
                let gain_map_image = if is_apple_format {
                    raw_gain_map.map(|img| {
                        debug!("Transforming Apple gain map from 128-255 to 0-255 range");
                        transform_apple_gain_map(&img)
                    })
                } else {
                    raw_gain_map
                };

                if let Some(ref gm_img) = gain_map_image {
                    debug!(
                        "Successfully decoded HEIF gain map image: {}x{}",
                        gm_img.width(),
                        gm_img.height()
                    );
                }

                // Compute gain map parameters for ISO 21496-1 AVIF format
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
                    // Check if this is Apple's simple format (only headroom, no explicit min/max/gamma)
                    let is_apple_simple_format = xmp.gamma == 1.0
                        && xmp.gain_map_min == 0.0
                        && xmp.gain_map_max == 0.0
                        && xmp.hdr_capacity_max > 0.0;

                    if is_apple_simple_format {
                        // Apple HDR format conversion to ISO 21496-1
                        //
                        // Apple's actual formula (from apple-hdr-heic library):
                        //   gainmap_linear = sRGB_EOTF(gainmap)  // gamma ~2.4
                        //   scale = 1.0 + (headroom - 1.0) * gainmap_linear
                        //   HDR_linear = SDR_linear * scale
                        //
                        // ISO 21496-1 formula:
                        //   recovery = pow(gainmap, gamma)
                        //   log_boost = min + (max - min) * recovery
                        //   HDR = SDR * exp2(log_boost)
                        //
                        // To approximate Apple's formula:
                        // - gamma = 2.4 (matches sRGB EOTF applied to gain map)
                        // - min = 0 (scale = 1 when gainmap = 0)
                        // - max = log2(headroom) (scale = headroom when gainmap = 1)
                        //
                        // After our pixel transform (128-255 → 0-255), the gain map
                        // now has 0 = SDR and 255 = full HDR, matching ISO expectations.

                        let headroom = xmp.hdr_capacity_max;
                        // ISO 21496-1 uses: recovery = pow(gainmap, 1/gamma)
                        // Apple uses sRGB EOTF which is approximately pow(value, 2.4)
                        // So we need: 1/gamma = 2.4, therefore gamma = 1/2.4 ≈ 0.417
                        // The good reference file uses gamma ≈ 0.4, confirming this
                        let gamma_val = 1.0 / 2.4; // ≈ 0.417, close to good file's 0.4
                        let offset = 1.0 / 64.0;
                        let min_val = 0.0; // log2(1) = 0, no boost
                        let max_val = headroom.log2(); // log2(headroom) for exp2 to give headroom

                        debug!(
                            "Apple HDR format: headroom={}, log2(headroom)={}, gamma={}, min={}, max={}",
                            headroom, max_val, gamma_val, min_val, max_val
                        );

                        (
                            [gamma_val, gamma_val, gamma_val],
                            [min_val, min_val, min_val],
                            [max_val, max_val, max_val],
                            [offset, offset, offset],
                            [offset, offset, offset],
                            0.0, // Base is SDR (headroom 0)
                            max_val, // Use log2(headroom) as the headroom value
                            true, // Use base (SDR) color space
                        )
                    } else {
                        // Full ISO 21496-1 format with explicit parameters
                        (
                            [xmp.gamma, xmp.gamma, xmp.gamma],
                            [xmp.gain_map_min, xmp.gain_map_min, xmp.gain_map_min],
                            [xmp.gain_map_max, xmp.gain_map_max, xmp.gain_map_max],
                            [xmp.offset_sdr, xmp.offset_sdr, xmp.offset_sdr],
                            [xmp.offset_hdr, xmp.offset_hdr, xmp.offset_hdr],
                            xmp.hdr_capacity_min,
                            xmp.hdr_capacity_max,
                            !xmp.base_rendition_is_hdr,
                        )
                    }
                } else {
                    // No XMP metadata - use sensible defaults
                    (
                        [1.0, 1.0, 1.0],
                        [0.0, 0.0, 0.0],
                        [1.0, 1.0, 1.0],
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 0.0],
                        0.0,
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

/// Transform Apple's gain map from 128-255 range to 0-255 range
///
/// Apple HDR gain maps use 128 as the neutral point (no boost):
/// - 128 = SDR (no change)
/// - 255 = maximum HDR boost
///
/// ISO 21496-1 expects 0 as the neutral point:
/// - 0 = minimum boost (usually SDR)
/// - 255 = maximum boost
///
/// This function remaps: new = clamp((old - 128) * 2, 0, 255)
#[cfg(feature = "avif")]
fn transform_apple_gain_map(image: &DynamicImage) -> DynamicImage {
    use image::{GenericImageView, Pixel};

    let (width, height) = image.dimensions();

    match image {
        DynamicImage::ImageRgb8(img) => {
            let mut out = ImageBuffer::new(width, height);
            for (x, y, pixel) in img.enumerate_pixels() {
                let channels = pixel.channels();
                let new_pixel = Rgb([
                    transform_apple_pixel(channels[0]),
                    transform_apple_pixel(channels[1]),
                    transform_apple_pixel(channels[2]),
                ]);
                out.put_pixel(x, y, new_pixel);
            }
            DynamicImage::ImageRgb8(out)
        }
        DynamicImage::ImageRgba8(img) => {
            let mut out: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
            for (x, y, pixel) in img.enumerate_pixels() {
                let channels = pixel.channels();
                let new_pixel = Rgba([
                    transform_apple_pixel(channels[0]),
                    transform_apple_pixel(channels[1]),
                    transform_apple_pixel(channels[2]),
                    channels[3], // Keep alpha unchanged
                ]);
                out.put_pixel(x, y, new_pixel);
            }
            DynamicImage::ImageRgba8(out)
        }
        _ => {
            // For other formats, convert to RGB8 first
            let rgb8 = image.to_rgb8();
            let mut out = ImageBuffer::new(width, height);
            for (x, y, pixel) in rgb8.enumerate_pixels() {
                let channels = pixel.channels();
                let new_pixel = Rgb([
                    transform_apple_pixel(channels[0]),
                    transform_apple_pixel(channels[1]),
                    transform_apple_pixel(channels[2]),
                ]);
                out.put_pixel(x, y, new_pixel);
            }
            DynamicImage::ImageRgb8(out)
        }
    }
}

/// Transform a single pixel value from Apple's 128-255 range to 0-255
#[cfg(feature = "avif")]
#[inline]
fn transform_apple_pixel(value: u8) -> u8 {
    // Remap: 128 -> 0, 255 -> 254 (approximately)
    // Formula: (value - 128) * 2, clamped to 0-255
    if value <= 128 {
        0
    } else {
        ((value as u16 - 128) * 2).min(255) as u8
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

/// Extract image dimensions from HEIF data without full decode
pub fn extract_dimensions_from_bytes(data: &[u8]) -> Option<(u32, u32)> {
    if !is_heif_format(data) {
        return None;
    }

    let ctx = HeifContext::read_from_bytes(data).ok()?;
    let handle = ctx.primary_image_handle().ok()?;

    Some((handle.width(), handle.height()))
}

/// Extract color description from HEIF data
/// Returns a human-readable string describing the color space
pub fn extract_color_description_from_bytes(data: &[u8]) -> Option<String> {
    if !is_heif_format(data) {
        return None;
    }

    let ctx = HeifContext::read_from_bytes(data).ok()?;
    let handle = ctx.primary_image_handle().ok()?;

    // Try to get ICC profile name first
    if let Some(raw_profile) = handle.color_profile_raw() {
        let profile_data = raw_profile.data.to_vec();
        // Extract ICC profile name (description tag)
        if let Some(name) = extract_icc_profile_name(&profile_data) {
            return Some(name);
        }
    }

    // Fall back to NCLX color info
    if let Some(nclx) = handle.color_profile_nclx() {
        let primaries = nclx.color_primaries() as u16;
        let transfer = nclx.transfer_characteristics() as u16;

        let primaries_str = match primaries {
            1 => "BT.709",
            9 => "BT.2020",
            12 => "Display P3",
            _ => return None,
        };

        let transfer_str = match transfer {
            1 | 6 => "SDR",
            13 => "sRGB",
            16 => "PQ (HDR)",
            18 => "HLG (HDR)",
            _ => "SDR",
        };

        // Check for gain map (indicates HDR capability)
        let aux_handles = handle.auxiliary_images(None);
        let has_gain_map = aux_handles.iter().any(|aux| {
            aux.auxiliary_type()
                .map(|t| t.to_lowercase().contains("gainmap"))
                .unwrap_or(false)
        });

        if has_gain_map {
            return Some(format!("{} + Gain Map (HDR)", primaries_str));
        }

        return Some(format!("{} / {}", primaries_str, transfer_str));
    }

    None
}

/// Extract ICC profile name from raw profile data
fn extract_icc_profile_name(profile_data: &[u8]) -> Option<String> {
    // ICC profile description is in the 'desc' tag
    // Look for tag table and find 'desc' tag
    if profile_data.len() < 132 {
        return None;
    }

    // Tag count is at offset 128
    let tag_count = u32::from_be_bytes([
        profile_data[128],
        profile_data[129],
        profile_data[130],
        profile_data[131],
    ]) as usize;

    // Each tag entry is 12 bytes starting at offset 132
    for i in 0..tag_count {
        let offset = 132 + i * 12;
        if offset + 12 > profile_data.len() {
            break;
        }

        let sig = &profile_data[offset..offset + 4];
        if sig == b"desc" {
            let tag_offset = u32::from_be_bytes([
                profile_data[offset + 4],
                profile_data[offset + 5],
                profile_data[offset + 6],
                profile_data[offset + 7],
            ]) as usize;
            let tag_size = u32::from_be_bytes([
                profile_data[offset + 8],
                profile_data[offset + 9],
                profile_data[offset + 10],
                profile_data[offset + 11],
            ]) as usize;

            if tag_offset + tag_size <= profile_data.len() && tag_size > 12 {
                let tag_data = &profile_data[tag_offset..tag_offset + tag_size];
                let tag_type = &tag_data[0..4];

                // 'mluc' type (multi-localized Unicode)
                if tag_type == b"mluc" && tag_size > 28 {
                    let str_offset = u32::from_be_bytes([
                        tag_data[24],
                        tag_data[25],
                        tag_data[26],
                        tag_data[27],
                    ]) as usize;
                    let str_len = u32::from_be_bytes([
                        tag_data[20],
                        tag_data[21],
                        tag_data[22],
                        tag_data[23],
                    ]) as usize;

                    if str_offset + str_len <= tag_size {
                        // UTF-16BE string
                        let utf16_data = &tag_data[str_offset..str_offset + str_len];
                        let utf16: Vec<u16> = utf16_data
                            .chunks_exact(2)
                            .map(|c| u16::from_be_bytes([c[0], c[1]]))
                            .collect();
                        if let Ok(s) = String::from_utf16(&utf16) {
                            let trimmed = s.trim_matches('\0').trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }
                // 'desc' type (textDescription - older ICC v2)
                else if tag_type == b"desc" && tag_size > 12 {
                    let ascii_len = u32::from_be_bytes([
                        tag_data[8],
                        tag_data[9],
                        tag_data[10],
                        tag_data[11],
                    ]) as usize;
                    if ascii_len > 0
                        && 12 + ascii_len <= tag_size
                        && let Ok(s) = std::str::from_utf8(&tag_data[12..12 + ascii_len - 1])
                    {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }

    None
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

        // 8-bit is never HDR (via transfer function alone)
        assert!(!detect_hdr(8, 16));

        // 10-bit with sRGB transfer = not HDR
        assert!(!detect_hdr(10, 13));
    }
}
