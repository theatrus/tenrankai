use crate::gallery::GalleryError;
use image::{DynamicImage, GenericImageView, ImageBuffer};
use libavif::{decode_rgb, is_avif};
use libavif_sys as sys;
use std::path::Path;
use tracing::debug;

use super::avif_container;

// Helper functions for fraction conversion
fn signed_fraction_to_float(frac: &sys::avifSignedFraction) -> f32 {
    if frac.d == 0 {
        0.0
    } else {
        frac.n as f32 / frac.d as f32
    }
}

fn unsigned_fraction_to_float(frac: &sys::avifUnsignedFraction) -> f32 {
    if frac.d == 0 {
        0.0
    } else {
        frac.n as f32 / frac.d as f32
    }
}

fn float_to_signed_fraction(val: f32) -> sys::avifSignedFraction {
    if val == 0.0 {
        sys::avifSignedFraction { n: 0, d: 1 }
    } else {
        let d = 1000u32;
        let n = (val * d as f32).round() as i32;
        sys::avifSignedFraction { n, d }
    }
}

fn float_to_unsigned_fraction(val: f32) -> sys::avifUnsignedFraction {
    if val == 0.0 {
        sys::avifUnsignedFraction { n: 0, d: 1 }
    } else {
        let d = 1000u32;
        let n = (val * d as f32).round() as u32;
        sys::avifUnsignedFraction { n, d }
    }
}

// Common color space constants
const DISPLAY_P3_PRIMARIES: u16 = 12; // AVIF_COLOR_PRIMARIES_SMPTE432

impl AvifImageInfo {
    /// Check if this is a Display P3 image
    pub fn is_display_p3(&self) -> bool {
        self.color_primaries == DISPLAY_P3_PRIMARIES ||
        // Also check for unspecified with P3 ICC profile
        (self.color_primaries == 2 && self.has_p3_icc_profile()) // AVIF_COLOR_PRIMARIES_UNSPECIFIED = 2
    }

    /// Check if the ICC profile indicates Display P3
    fn has_p3_icc_profile(&self) -> bool {
        if let Some(ref icc) = self.icc_profile {
            // Simple check for Display P3 ICC profile by size and basic markers
            // Display P3 profiles are typically around 536-548 bytes
            icc.len() >= 500 && icc.len() < 600
        } else {
            false
        }
    }
}

/// AVIF specific image information
#[derive(Debug, Clone)]
pub struct AvifImageInfo {
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub is_hdr: bool,
    pub icc_profile: Option<Vec<u8>>,
    pub color_primaries: u16,
    pub transfer_characteristics: u16,
    pub matrix_coefficients: u16,
    pub max_cll: u16,                       // Maximum Content Light Level (cd/m²)
    pub max_pall: u16,                      // Maximum Picture Average Light Level (cd/m²)
    pub has_gain_map: bool,                 // Whether this AVIF contains a gain map
    pub gain_map_info: Option<GainMapInfo>, // Gain map metadata if available
    pub exif_data: Option<Vec<u8>>,         // Raw EXIF data if present
}

/// Gain map metadata information
#[derive(Debug, Clone)]
pub struct GainMapInfo {
    pub has_image: bool,             // Whether gain map image data is present
    pub gamma: [f32; 3],             // Gamma values for R,G,B channels
    pub min: [f32; 3],               // Minimum values for R,G,B channels
    pub max: [f32; 3],               // Maximum values for R,G,B channels
    pub base_offset: [f32; 3],       // Base offset for tone mapping
    pub alternate_offset: [f32; 3],  // Alternate offset for tone mapping
    pub base_hdr_headroom: f32,      // Base HDR headroom
    pub alternate_hdr_headroom: f32, // Alternate HDR headroom
    pub use_base_color_space: bool,  // Whether to use base color space
    pub gain_map_image: Option<DynamicImage>, // The actual gain map image data
}

/// Read an AVIF file using libavif with HDR support
pub fn read_avif_info(path: &Path) -> Result<(DynamicImage, AvifImageInfo), GalleryError> {
    // Read the file
    let data = std::fs::read(path)?;

    // Check if it's a valid AVIF file
    if !is_avif(&data) {
        return Err(GalleryError::ProcessingError(
            "Not a valid AVIF file".to_string(),
        ));
    }

    // First do a quick gain map detection using our container parser
    // This ensures we can detect gain maps even if libavif decoding fails
    let (container_has_gain_map, container_gain_map_info) =
        avif_container::detect_gain_map_in_container(&data);
    debug!(
        "Container gain map detection: has_gain_map={}",
        container_has_gain_map
    );

    // Use low-level API for HDR support
    unsafe {
        let decoder = sys::avifDecoderCreate();
        if decoder.is_null() {
            return Err(GalleryError::ProcessingError(
                "Failed to create AVIF decoder".to_string(),
            ));
        }

        // Set codec choice to ensure we use AOM
        (*decoder).codecChoice = sys::AVIF_CODEC_CHOICE_AOM;

        // Enable gain map decoding - requires libavif 1.1.0+
        // Set to decode all content (color, alpha, and gain maps)
        (*decoder).imageContentToDecode = sys::AVIF_IMAGE_CONTENT_ALL;

        // Create an empty image to decode into
        let image = sys::avifImageCreateEmpty();
        if image.is_null() {
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError(
                "Failed to create AVIF image".to_string(),
            ));
        }

        // Decode the image
        let result = sys::avifDecoderReadMemory(decoder, image, data.as_ptr(), data.len());

        if result != sys::AVIF_RESULT_OK {
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            // For now, just report the error code. In the future we could use
            // avifResultToString when it's available in the bindings
            return Err(GalleryError::ProcessingError(format!(
                "Failed to decode AVIF: error code {}",
                result
            )));
        }

        // Extract image properties
        let width = (*image).width;
        let height = (*image).height;
        let bit_depth = (*image).depth;
        let has_alpha = !(*image).alphaPlane.is_null();

        // Check if it's HDR - use more flexible logic
        // HDR can be indicated by:
        // 1. Traditional HDR: BT.2020 + PQ/HLG
        // 2. Wide gamut HDR: Display P3 + high bit depth (common in camera output)
        // 3. Any high bit depth with HDR transfer function
        let has_hdr_transfer = (*image).transferCharacteristics
            == sys::AVIF_TRANSFER_CHARACTERISTICS_SMPTE2084 as u16
            || (*image).transferCharacteristics == sys::AVIF_TRANSFER_CHARACTERISTICS_HLG as u16;

        let _has_wide_gamut = (*image).colorPrimaries == sys::AVIF_COLOR_PRIMARIES_BT2020 as u16
            || (*image).colorPrimaries == 12; // Display P3

        // Extract CLLI (Content Light Level Information) - HDR metadata
        let clli = (*image).clli;
        let has_clli = clli.maxCLL > 0 || clli.maxPALL > 0;

        // More accurate HDR detection logic
        // Wide gamut alone doesn't make an image HDR - it needs HDR signaling
        let is_hdr = if bit_depth <= 8 {
            false
        } else {
            // High bit depth (>8) AND any of:
            // 1. HDR transfer function (PQ/HLG)
            // 2. BT.2020 with HDR transfer
            // 3. CLLI metadata present (indicates HDR mastering)
            // 4. Display P3 with PQ/HLG transfer
            // Note: Display P3 + sRGB transfer is wide gamut SDR, not HDR
            has_hdr_transfer || has_clli
        };

        // Extract ICC profile if present
        let icc_profile = if (*image).icc.size > 0 && !(*image).icc.data.is_null() {
            let icc_slice = std::slice::from_raw_parts((*image).icc.data, (*image).icc.size);
            Some(icc_slice.to_vec())
        } else {
            None
        };

        // Extract EXIF data if present
        let exif_data = if (*image).exif.size > 0 && !(*image).exif.data.is_null() {
            let exif_slice = std::slice::from_raw_parts((*image).exif.data, (*image).exif.size);
            debug!("Found EXIF data in AVIF: {} bytes", exif_slice.len());
            Some(exif_slice.to_vec())
        } else {
            None
        };

        // Check for gain map presence using libavif 1.2.1 API
        let libavif_has_gain_map = !(*image).gainMap.is_null();
        debug!(
            "Libavif gain map detection: has_gain_map={}",
            libavif_has_gain_map
        );

        // Use container detection info if libavif didn't provide gain map
        let has_gain_map = libavif_has_gain_map || container_has_gain_map;

        let gain_map_info = if libavif_has_gain_map {
            // Use libavif's gain map data if available
            let gain_map = (*image).gainMap;
            let gm = &*gain_map;

            // Extract gain map image if available
            let gain_map_image = if !gm.image.is_null() {
                let gm_img = &*gm.image;
                debug!(
                    "Gain map image found: {}x{}, depth={}",
                    gm_img.width, gm_img.height, gm_img.depth
                );
                if gm_img.width > 0 && gm_img.height > 0 {
                    // Convert gain map image to DynamicImage
                    let mut gm_rgb = sys::avifRGBImage::default();
                    sys::avifRGBImageSetDefaults(&mut gm_rgb, gm.image);

                    // Gain maps are typically single channel (grayscale)
                    gm_rgb.format = sys::AVIF_RGB_FORMAT_RGB;
                    gm_rgb.depth = gm_img.depth;

                    if sys::avifRGBImageAllocatePixels(&mut gm_rgb) == sys::AVIF_RESULT_OK {
                        if sys::avifImageYUVToRGB(gm.image, &mut gm_rgb) == sys::AVIF_RESULT_OK {
                            // Convert to DynamicImage based on bit depth
                            let gain_img = if gm_img.depth > 8 {
                                // 16-bit gain map
                                let mut img = ImageBuffer::new(gm_img.width, gm_img.height);
                                let bytes_per_pixel = 6; // RGB16

                                for y in 0..gm_img.height {
                                    for x in 0..gm_img.width {
                                        let pixel_offset = y as usize * gm_rgb.rowBytes as usize
                                            + x as usize * bytes_per_pixel;
                                        let pixel_ptr =
                                            gm_rgb.pixels.add(pixel_offset) as *const u16;

                                        let shift = 16 - gm_img.depth;
                                        let pixel = image::Rgb([
                                            *pixel_ptr << shift,
                                            *pixel_ptr.add(1) << shift,
                                            *pixel_ptr.add(2) << shift,
                                        ]);
                                        img.put_pixel(x, y, pixel);
                                    }
                                }
                                Some(DynamicImage::ImageRgb16(img))
                            } else {
                                // 8-bit gain map
                                let mut img = ImageBuffer::new(gm_img.width, gm_img.height);
                                let bytes_per_pixel = 3; // RGB8

                                for y in 0..gm_img.height {
                                    for x in 0..gm_img.width {
                                        let pixel_offset = y as usize * gm_rgb.rowBytes as usize
                                            + x as usize * bytes_per_pixel;
                                        let pixel_ptr = gm_rgb.pixels.add(pixel_offset);

                                        let pixel = image::Rgb([
                                            *pixel_ptr,
                                            *pixel_ptr.add(1),
                                            *pixel_ptr.add(2),
                                        ]);
                                        img.put_pixel(x, y, pixel);
                                    }
                                }
                                Some(DynamicImage::ImageRgb8(img))
                            };

                            sys::avifRGBImageFreePixels(&mut gm_rgb);
                            gain_img
                        } else {
                            sys::avifRGBImageFreePixels(&mut gm_rgb);
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    debug!("Gain map image has zero dimensions");
                    None
                }
            } else {
                debug!("Gain map image pointer is null");
                None
            };

            Some(GainMapInfo {
                has_image: !gm.image.is_null(),
                gamma: [
                    unsigned_fraction_to_float(&gm.gainMapGamma[0]),
                    unsigned_fraction_to_float(&gm.gainMapGamma[1]),
                    unsigned_fraction_to_float(&gm.gainMapGamma[2]),
                ],
                min: [
                    signed_fraction_to_float(&gm.gainMapMin[0]),
                    signed_fraction_to_float(&gm.gainMapMin[1]),
                    signed_fraction_to_float(&gm.gainMapMin[2]),
                ],
                max: [
                    signed_fraction_to_float(&gm.gainMapMax[0]),
                    signed_fraction_to_float(&gm.gainMapMax[1]),
                    signed_fraction_to_float(&gm.gainMapMax[2]),
                ],
                base_offset: [
                    signed_fraction_to_float(&gm.baseOffset[0]),
                    signed_fraction_to_float(&gm.baseOffset[1]),
                    signed_fraction_to_float(&gm.baseOffset[2]),
                ],
                alternate_offset: [
                    signed_fraction_to_float(&gm.alternateOffset[0]),
                    signed_fraction_to_float(&gm.alternateOffset[1]),
                    signed_fraction_to_float(&gm.alternateOffset[2]),
                ],
                base_hdr_headroom: unsigned_fraction_to_float(&gm.baseHdrHeadroom),
                alternate_hdr_headroom: unsigned_fraction_to_float(&gm.alternateHdrHeadroom),
                use_base_color_space: gm.useBaseColorSpace != 0,
                gain_map_image,
            })
        } else if container_has_gain_map {
            // Use container detection info if libavif didn't detect
            if let Some(mut info) = container_gain_map_info {
                info.gain_map_image = None; // We can't extract the image without libavif API
                Some(info)
            } else {
                None
            }
        } else {
            None
        };

        // Update HDR detection to account for gain maps
        // Images with gain maps should be considered HDR even if they appear as SDR base images
        let is_hdr_with_gainmap = is_hdr || has_gain_map;

        let info = AvifImageInfo {
            bit_depth: bit_depth as u8,
            has_alpha,
            is_hdr: is_hdr_with_gainmap, // Include gain map in HDR detection
            icc_profile: icc_profile.clone(),
            color_primaries: (*image).colorPrimaries,
            transfer_characteristics: (*image).transferCharacteristics,
            matrix_coefficients: (*image).matrixCoefficients,
            max_cll: clli.maxCLL,
            max_pall: clli.maxPALL,
            has_gain_map,
            gain_map_info,
            exif_data: exif_data.clone(),
        };

        // Identify color space name for debugging
        let color_space_name = match (*image).colorPrimaries {
            1 => "BT.709",
            9 => "BT.2020",
            12 => "Display P3",
            2 => "Unspecified",
            _ => "Unknown",
        };

        let transfer_name = match (*image).transferCharacteristics {
            1 => "BT.709",
            13 => "sRGB",
            16 => "PQ (SMPTE 2084)",
            18 => "HLG",
            2 => "Unspecified",
            _ => "Unknown",
        };

        debug!(
            "AVIF properties: {}x{}, depth={}, has_alpha={}, is_hdr={}, colorSpace={} ({}), transfer={} ({})",
            width,
            height,
            bit_depth,
            has_alpha,
            is_hdr,
            color_space_name,
            (*image).colorPrimaries,
            transfer_name,
            (*image).transferCharacteristics
        );

        // Convert to RGB
        let mut rgb = sys::avifRGBImage::default();
        sys::avifRGBImageSetDefaults(&mut rgb, image);

        // Set format explicitly
        rgb.format = if has_alpha {
            sys::AVIF_RGB_FORMAT_RGBA
        } else {
            sys::AVIF_RGB_FORMAT_RGB
        };

        // For HDR images, preserve bit depth
        if bit_depth > 8 {
            rgb.depth = bit_depth;
        }

        debug!(
            "RGB conversion settings: format={}, depth={}, rowBytes will be calculated by libavif",
            rgb.format, rgb.depth
        );

        // Allocate RGB pixels
        if sys::avifRGBImageAllocatePixels(&mut rgb) != sys::AVIF_RESULT_OK {
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError(
                "Failed to allocate RGB pixels".to_string(),
            ));
        }

        debug!(
            "RGB allocated: rowBytes={}, expected bytes per pixel={}",
            rgb.rowBytes,
            if has_alpha { 4 } else { 3 } * if bit_depth > 8 { 2 } else { 1 }
        );

        // Convert YUV to RGB
        if sys::avifImageYUVToRGB(image, &mut rgb) != sys::AVIF_RESULT_OK {
            sys::avifRGBImageFreePixels(&mut rgb);
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError(
                "Failed to convert YUV to RGB".to_string(),
            ));
        }

        // Create DynamicImage based on bit depth
        let dynamic_img = if bit_depth > 8 {
            // HDR image - convert to 16-bit
            if has_alpha {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 8; // 4 channels * 2 bytes

                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset =
                            y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = rgb.pixels.add(pixel_offset) as *const u16;

                        // Scale from bit_depth to 16-bit if needed
                        let shift = 16 - bit_depth;
                        let pixel = image::Rgba([
                            *pixel_ptr << shift,
                            *pixel_ptr.add(1) << shift,
                            *pixel_ptr.add(2) << shift,
                            *pixel_ptr.add(3) << shift,
                        ]);
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgba16(img)
            } else {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 6; // 3 channels * 2 bytes

                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset =
                            y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = rgb.pixels.add(pixel_offset) as *const u16;

                        // Scale from bit_depth to 16-bit if needed
                        let shift = 16 - bit_depth;
                        let pixel = image::Rgb([
                            *pixel_ptr << shift,
                            *pixel_ptr.add(1) << shift,
                            *pixel_ptr.add(2) << shift,
                        ]);
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgb16(img)
            }
        } else {
            // 8-bit image
            if has_alpha {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 4; // RGBA

                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset =
                            y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = rgb.pixels.add(pixel_offset);

                        let pixel = image::Rgba([
                            *pixel_ptr,
                            *pixel_ptr.add(1),
                            *pixel_ptr.add(2),
                            *pixel_ptr.add(3),
                        ]);
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgba8(img)
            } else {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 3; // RGB

                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset =
                            y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = rgb.pixels.add(pixel_offset);

                        let pixel = image::Rgb([*pixel_ptr, *pixel_ptr.add(1), *pixel_ptr.add(2)]);
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgb8(img)
            }
        };

        // Clean up
        sys::avifRGBImageFreePixels(&mut rgb);
        sys::avifImageDestroy(image);
        sys::avifDecoderDestroy(decoder);

        Ok((dynamic_img, info))
    }
}

/// Save a DynamicImage as AVIF preserving color properties
pub fn save_with_info(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    speed: u8,
    info: Option<&AvifImageInfo>,
) -> Result<(), GalleryError> {
    // For backward compatibility
    let icc_profile = info.and_then(|i| i.icc_profile.as_deref());
    let preserve_hdr = info.map(|i| i.is_hdr).unwrap_or(false);
    save_with_profile_and_color(image, path, quality, speed, icc_profile, preserve_hdr, info)
}

/// Save a DynamicImage as AVIF with HDR support
pub fn save_with_profile(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    speed: u8,
    icc_profile: Option<&[u8]>,
    preserve_hdr: bool,
) -> Result<(), GalleryError> {
    save_with_profile_and_color(image, path, quality, speed, icc_profile, preserve_hdr, None)
}

/// Internal save function with full color property support
fn save_with_profile_and_color(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    speed: u8,
    icc_profile: Option<&[u8]>,
    preserve_hdr: bool,
    color_info: Option<&AvifImageInfo>,
) -> Result<(), GalleryError> {
    let (width, height) = image.dimensions();

    // Check if we have an HDR image
    let is_16bit = matches!(
        image,
        DynamicImage::ImageLuma16(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageRgb16(_)
            | DynamicImage::ImageRgba16(_)
    );

    let has_alpha = matches!(
        image,
        DynamicImage::ImageLumaA8(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageRgba8(_)
            | DynamicImage::ImageRgba16(_)
    );

    // Decide bit depth based on input and preserve_hdr flag
    let bit_depth = if is_16bit && preserve_hdr { 10 } else { 8 };

    debug!(
        "Encoding AVIF: {}x{}, quality={}, speed={}, bit_depth={}, has_alpha={}, preserve_hdr={}",
        width, height, quality, speed, bit_depth, has_alpha, preserve_hdr
    );

    unsafe {
        // Create AVIF image with appropriate bit depth
        let avif_image = sys::avifImageCreate(
            width,
            height,
            bit_depth,
            if has_alpha {
                sys::AVIF_PIXEL_FORMAT_YUV444
            } else {
                sys::AVIF_PIXEL_FORMAT_YUV420
            },
        );

        if avif_image.is_null() {
            return Err(GalleryError::ProcessingError(
                "Failed to create AVIF image".to_string(),
            ));
        }

        // Set color properties - preserve original exactly
        if let Some(info) = color_info {
            // Preserve the original color properties exactly as they were
            (*avif_image).colorPrimaries = info.color_primaries;
            (*avif_image).transferCharacteristics = info.transfer_characteristics;
            (*avif_image).matrixCoefficients = info.matrix_coefficients;
            (*avif_image).yuvRange = sys::AVIF_RANGE_FULL;

            debug!(
                "Preserving original color properties: primaries={}, transfer={}, matrix={}",
                info.color_primaries, info.transfer_characteristics, info.matrix_coefficients
            );
        } else {
            // Fallback to defaults based on whether we're preserving HDR
            if is_16bit && preserve_hdr {
                (*avif_image).colorPrimaries = sys::AVIF_COLOR_PRIMARIES_BT2020 as u16;
                (*avif_image).transferCharacteristics =
                    sys::AVIF_TRANSFER_CHARACTERISTICS_SMPTE2084 as u16; // PQ
                (*avif_image).matrixCoefficients = sys::AVIF_MATRIX_COEFFICIENTS_BT2020_NCL as u16;
                (*avif_image).yuvRange = sys::AVIF_RANGE_FULL;
            } else {
                (*avif_image).colorPrimaries = sys::AVIF_COLOR_PRIMARIES_BT709 as u16;
                (*avif_image).transferCharacteristics =
                    sys::AVIF_TRANSFER_CHARACTERISTICS_SRGB as u16;
                (*avif_image).matrixCoefficients = sys::AVIF_MATRIX_COEFFICIENTS_BT709 as u16;
                (*avif_image).yuvRange = sys::AVIF_RANGE_FULL;
            }
        }

        // Set ICC profile if provided
        if let Some(icc) = icc_profile {
            sys::avifImageSetProfileICC(avif_image, icc.as_ptr(), icc.len());
        }

        // Set CLLI data if provided in color_info
        if let Some(info) = color_info
            && (info.max_cll > 0 || info.max_pall > 0)
        {
            (*avif_image).clli.maxCLL = info.max_cll;
            (*avif_image).clli.maxPALL = info.max_pall;
            debug!(
                "Set CLLI: maxCLL={} cd/m², maxPALL={} cd/m²",
                info.max_cll, info.max_pall
            );
        }

        // Allocate planes
        sys::avifImageAllocatePlanes(avif_image, sys::AVIF_PLANES_YUV);
        if has_alpha {
            sys::avifImageAllocatePlanes(avif_image, sys::AVIF_PLANES_A);
        }

        // Create RGB image for conversion
        let mut rgb = sys::avifRGBImage::default();
        sys::avifRGBImageSetDefaults(&mut rgb, avif_image);
        rgb.depth = bit_depth;
        rgb.format = if has_alpha {
            sys::AVIF_RGB_FORMAT_RGBA
        } else {
            sys::AVIF_RGB_FORMAT_RGB
        };

        // Allocate RGB pixels
        if sys::avifRGBImageAllocatePixels(&mut rgb) != sys::AVIF_RESULT_OK {
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError(
                "Failed to allocate RGB pixels".to_string(),
            ));
        }

        // Copy image data to RGB buffer
        if bit_depth > 8 && is_16bit {
            // HDR path - copy 16-bit data
            let pixels = rgb.pixels as *mut u16;
            let row_bytes = rgb.rowBytes as usize;

            match image {
                DynamicImage::ImageRgb16(img) => {
                    for y in 0..height {
                        for x in 0..width {
                            let src_pixel = img.get_pixel(x, y);
                            let dst_idx = y as usize * row_bytes / 2 + x as usize * 3;
                            // Scale from 16-bit to target bit depth
                            let shift = 16 - bit_depth;
                            *pixels.add(dst_idx) = src_pixel[0] >> shift;
                            *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                            *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                        }
                    }
                }
                DynamicImage::ImageRgba16(img) => {
                    for y in 0..height {
                        for x in 0..width {
                            let src_pixel = img.get_pixel(x, y);
                            let dst_idx = y as usize * row_bytes / 2 + x as usize * 4;
                            // Scale from 16-bit to target bit depth
                            let shift = 16 - bit_depth;
                            *pixels.add(dst_idx) = src_pixel[0] >> shift;
                            *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                            *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                            *pixels.add(dst_idx + 3) = src_pixel[3] >> shift;
                        }
                    }
                }
                _ => {
                    // Convert to 16-bit first
                    if has_alpha {
                        let rgba16 = image.to_rgba16();
                        for y in 0..height {
                            for x in 0..width {
                                let src_pixel = rgba16.get_pixel(x, y);
                                let dst_idx = y as usize * row_bytes / 2 + x as usize * 4;
                                let shift = 16 - bit_depth;
                                *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                                *pixels.add(dst_idx + 3) = src_pixel[3] >> shift;
                            }
                        }
                    } else {
                        let rgb16 = image.to_rgb16();
                        for y in 0..height {
                            for x in 0..width {
                                let src_pixel = rgb16.get_pixel(x, y);
                                let dst_idx = y as usize * row_bytes / 2 + x as usize * 3;
                                let shift = 16 - bit_depth;
                                *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                            }
                        }
                    }
                }
            }
        } else {
            // 8-bit path
            let pixels = rgb.pixels;
            let row_bytes = rgb.rowBytes as usize;

            if has_alpha {
                let rgba = image.to_rgba8();
                for y in 0..height {
                    for x in 0..width {
                        let src_pixel = rgba.get_pixel(x, y);
                        let dst_idx = y as usize * row_bytes + x as usize * 4;
                        *pixels.add(dst_idx) = src_pixel[0];
                        *pixels.add(dst_idx + 1) = src_pixel[1];
                        *pixels.add(dst_idx + 2) = src_pixel[2];
                        *pixels.add(dst_idx + 3) = src_pixel[3];
                    }
                }
            } else {
                let rgb_img = image.to_rgb8();
                for y in 0..height {
                    for x in 0..width {
                        let src_pixel = rgb_img.get_pixel(x, y);
                        let dst_idx = y as usize * row_bytes + x as usize * 3;
                        *pixels.add(dst_idx) = src_pixel[0];
                        *pixels.add(dst_idx + 1) = src_pixel[1];
                        *pixels.add(dst_idx + 2) = src_pixel[2];
                    }
                }
            }
        }

        // Convert RGB to YUV
        if sys::avifImageRGBToYUV(avif_image, &rgb) != sys::AVIF_RESULT_OK {
            sys::avifRGBImageFreePixels(&mut rgb);
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError(
                "Failed to convert RGB to YUV".to_string(),
            ));
        }

        sys::avifRGBImageFreePixels(&mut rgb);

        // If we have gain map info, attach it to the image
        if let Some(info) = color_info
            && info.has_gain_map
            && let Some(ref gm_info) = info.gain_map_info
            && let Some(ref gm_image) = gm_info.gain_map_image
        {
            debug!("Attaching gain map to output AVIF");

            // Create gain map structure
            let gain_map = sys::avifGainMapCreate();
            if !gain_map.is_null() {
                let gm = &mut *gain_map;

                // Set gain map metadata

                // Set metadata
                for i in 0..3 {
                    gm.gainMapGamma[i] = float_to_unsigned_fraction(gm_info.gamma[i]);
                    gm.gainMapMin[i] = float_to_signed_fraction(gm_info.min[i]);
                    gm.gainMapMax[i] = float_to_signed_fraction(gm_info.max[i]);
                    gm.baseOffset[i] = float_to_signed_fraction(gm_info.base_offset[i]);
                    gm.alternateOffset[i] = float_to_signed_fraction(gm_info.alternate_offset[i]);
                }
                gm.baseHdrHeadroom = float_to_unsigned_fraction(gm_info.base_hdr_headroom);
                gm.alternateHdrHeadroom =
                    float_to_unsigned_fraction(gm_info.alternate_hdr_headroom);
                gm.useBaseColorSpace = if gm_info.use_base_color_space { 1 } else { 0 };

                // Create gain map image
                let (gm_width, gm_height) = gm_image.dimensions();
                let gm_bit_depth = if matches!(
                    gm_image,
                    DynamicImage::ImageRgb16(_)
                        | DynamicImage::ImageRgba16(_)
                        | DynamicImage::ImageLuma16(_)
                        | DynamicImage::ImageLumaA16(_)
                ) {
                    10
                } else {
                    8
                };

                gm.image = sys::avifImageCreate(
                    gm_width,
                    gm_height,
                    gm_bit_depth,
                    sys::AVIF_PIXEL_FORMAT_YUV420,
                );

                if !gm.image.is_null() {
                    // Allocate planes for gain map
                    sys::avifImageAllocatePlanes(gm.image, sys::AVIF_PLANES_YUV);

                    // Convert gain map image to YUV
                    let mut gm_rgb = sys::avifRGBImage::default();
                    sys::avifRGBImageSetDefaults(&mut gm_rgb, gm.image);
                    gm_rgb.depth = gm_bit_depth;
                    gm_rgb.format = sys::AVIF_RGB_FORMAT_RGB;

                    if sys::avifRGBImageAllocatePixels(&mut gm_rgb) == sys::AVIF_RESULT_OK {
                        // Copy gain map image data
                        let gm_pixels = gm_rgb.pixels;
                        let gm_row_bytes = gm_rgb.rowBytes as usize;

                        if gm_bit_depth > 8 {
                            // 16-bit gain map
                            let rgb16 = gm_image.to_rgb16();
                            let pixels_u16 = gm_pixels as *mut u16;
                            for y in 0..gm_height {
                                for x in 0..gm_width {
                                    let src_pixel = rgb16.get_pixel(x, y);
                                    let dst_idx = y as usize * gm_row_bytes / 2 + x as usize * 3;
                                    let shift = 16 - gm_bit_depth;
                                    *pixels_u16.add(dst_idx) = src_pixel[0] >> shift;
                                    *pixels_u16.add(dst_idx + 1) = src_pixel[1] >> shift;
                                    *pixels_u16.add(dst_idx + 2) = src_pixel[2] >> shift;
                                }
                            }
                        } else {
                            // 8-bit gain map
                            let rgb8 = gm_image.to_rgb8();
                            for y in 0..gm_height {
                                for x in 0..gm_width {
                                    let src_pixel = rgb8.get_pixel(x, y);
                                    let dst_idx = y as usize * gm_row_bytes + x as usize * 3;
                                    *gm_pixels.add(dst_idx) = src_pixel[0];
                                    *gm_pixels.add(dst_idx + 1) = src_pixel[1];
                                    *gm_pixels.add(dst_idx + 2) = src_pixel[2];
                                }
                            }
                        }

                        // Convert RGB to YUV for gain map
                        if sys::avifImageRGBToYUV(gm.image, &gm_rgb) == sys::AVIF_RESULT_OK {
                            // Attach gain map to main image
                            (*avif_image).gainMap = gain_map;
                            debug!("Successfully attached gain map to AVIF image");
                        } else {
                            debug!("Failed to convert gain map RGB to YUV");
                            sys::avifGainMapDestroy(gain_map);
                        }

                        sys::avifRGBImageFreePixels(&mut gm_rgb);
                    } else {
                        debug!("Failed to allocate gain map RGB pixels");
                        sys::avifGainMapDestroy(gain_map);
                    }
                } else {
                    debug!("Failed to create gain map image");
                    sys::avifGainMapDestroy(gain_map);
                }
            }
        }

        // Create encoder
        let encoder = sys::avifEncoderCreate();
        if encoder.is_null() {
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError(
                "Failed to create encoder".to_string(),
            ));
        }

        // Set encoder options
        (*encoder).quality = quality as i32;
        (*encoder).qualityAlpha = quality as i32;
        (*encoder).speed = speed as i32;
        (*encoder).maxThreads = 1;

        // For HDR, ensure we use appropriate settings
        if bit_depth > 8 {
            (*encoder).minQuantizer = 0;
            (*encoder).maxQuantizer = 63; // Allow full quality range for HDR
        }

        // Encode the image
        let mut output = sys::avifRWData::default();
        let result = sys::avifEncoderWrite(encoder, avif_image, &mut output);

        if result != sys::AVIF_RESULT_OK {
            sys::avifEncoderDestroy(encoder);
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError(format!(
                "Failed to encode AVIF: error {}",
                result
            )));
        }

        // Write to file
        let data = std::slice::from_raw_parts(output.data, output.size);
        std::fs::write(path, data)?;

        // Clean up
        sys::avifRWDataFree(&mut output);
        sys::avifEncoderDestroy(encoder);
        sys::avifImageDestroy(avif_image);

        debug!(
            "Successfully saved {} AVIF to {:?}",
            if bit_depth > 8 { "HDR" } else { "SDR" },
            path
        );
        Ok(())
    }
}

/// Extract ICC profile from an AVIF file
pub fn extract_icc_profile(path: &Path) -> Option<Vec<u8>> {
    avif_container::extract_icc_profile(path)
}

/// Generate a descriptive color space string for an AVIF file
pub fn get_color_space_description(info: &AvifImageInfo) -> String {
    let primaries = match info.color_primaries {
        1 => "BT.709",
        9 => "BT.2020",
        12 => "Display P3",
        2 => "Unspecified",
        _ => "Unknown",
    };

    let transfer = match info.transfer_characteristics {
        1 => "BT.709",
        13 => "sRGB",
        16 => "PQ",
        18 => "HLG",
        2 => "Unspecified",
        _ => "Unknown",
    };

    let mut description = primaries.to_string();

    // Add transfer function if it's notable (HDR transfers or if needed for clarity)
    if info.transfer_characteristics == 16 || info.transfer_characteristics == 18 {
        description.push_str(&format!(" {}", transfer));
    } else if info.color_primaries == 12
        && info.transfer_characteristics == 13
        && info.bit_depth > 8
    {
        // For Display P3 with sRGB transfer, explicitly note it's wide gamut
        description.push_str(" sRGB");
    }

    // Add bit depth if high
    if info.bit_depth > 8 {
        description.push_str(&format!(" {}-bit", info.bit_depth));
    }

    // Add gain map indicator
    if info.has_gain_map {
        description.push_str(" Gain Map");
    }

    // Add HDR indicator only if truly HDR
    if info.is_hdr {
        description.push_str(" HDR");
    } else if info.color_primaries == 12 && info.bit_depth > 8 {
        // Explicitly note wide gamut for clarity
        description.push_str(" Wide Gamut");
    }

    description
}

/// Extract color space description from an AVIF file
pub fn extract_color_description(path: &Path) -> Option<String> {
    match read_avif_info(path) {
        Ok((_, info)) => Some(get_color_space_description(&info)),
        Err(_) => None,
    }
}

/// Extract EXIF data from an AVIF file
pub fn extract_exif_data(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;

    unsafe {
        let decoder = sys::avifDecoderCreate();
        if decoder.is_null() {
            return None;
        }

        let image = sys::avifImageCreateEmpty();
        if image.is_null() {
            sys::avifDecoderDestroy(decoder);
            return None;
        }

        // Decode the image (we need to decode to access metadata)
        let result = sys::avifDecoderReadMemory(decoder, image, data.as_ptr(), data.len());

        let exif_data = if result == sys::AVIF_RESULT_OK {
            if (*image).exif.size > 0 && !(*image).exif.data.is_null() {
                let exif_slice = std::slice::from_raw_parts((*image).exif.data, (*image).exif.size);
                debug!("Extracted EXIF data from AVIF: {} bytes", exif_slice.len());
                Some(exif_slice.to_vec())
            } else {
                None
            }
        } else {
            None
        };

        sys::avifImageDestroy(image);
        sys::avifDecoderDestroy(decoder);

        exif_data
    }
}

/// Extract dimensions from AVIF file without full decode
pub fn extract_dimensions(path: &Path) -> Option<(u32, u32)> {
    // Try using libavif first
    if let Ok(data) = std::fs::read(path)
        && is_avif(&data)
        && let Ok(rgb_pixels) = decode_rgb(&data)
    {
        return Some((rgb_pixels.width(), rgb_pixels.height()));
    }

    // Try using image crate
    if let Ok(dimensions) = image::image_dimensions(path) {
        return Some(dimensions);
    }

    // Parse AVIF container directly for dimensions
    avif_container::extract_dimensions(path)
}

/// Check if a browser supports AVIF based on Accept header
pub fn browser_supports_avif(accept_header: &str) -> bool {
    accept_header.contains("image/avif")
}
