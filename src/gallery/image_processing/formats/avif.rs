use crate::gallery::GalleryError;
use image::{DynamicImage, ImageReader, GenericImageView, ImageBuffer};
use libavif::{decode_rgb, is_avif};
use libavif_sys as sys;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::ptr;
use tracing::{debug, error};

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
}

/// Read an AVIF file using libavif with HDR support
pub fn read_avif_info(path: &Path) -> Result<(DynamicImage, AvifImageInfo), GalleryError> {
    // Read the file
    let data = std::fs::read(path)?;
    
    // Check if it's a valid AVIF file
    if !is_avif(&data) {
        return Err(GalleryError::ProcessingError("Not a valid AVIF file".to_string()));
    }
    
    // Use low-level API for HDR support
    unsafe {
        let decoder = sys::avifDecoderCreate();
        if decoder.is_null() {
            return Err(GalleryError::ProcessingError("Failed to create AVIF decoder".to_string()));
        }
        
        // Create an empty image to decode into
        let image = sys::avifImageCreateEmpty();
        if image.is_null() {
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError("Failed to create AVIF image".to_string()));
        }
        
        // Decode the image
        let result = sys::avifDecoderReadMemory(
            decoder,
            image,
            data.as_ptr(),
            data.len(),
        );
        
        if result != sys::AVIF_RESULT_OK as u32 {
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            error!("Failed to decode AVIF: error code {}", result);
            return read_with_fallback(path);
        }
        
        // Extract image properties
        let width = (*image).width;
        let height = (*image).height;
        let bit_depth = (*image).depth;
        let has_alpha = (*image).alphaPlane != ptr::null_mut();
        
        // Check if it's HDR - use more flexible logic
        // HDR can be indicated by:
        // 1. Traditional HDR: BT.2020 + PQ/HLG 
        // 2. Wide gamut HDR: Display P3 + high bit depth (common in camera output)
        // 3. Any high bit depth with HDR transfer function
        let has_hdr_transfer = (*image).transferCharacteristics == sys::AVIF_TRANSFER_CHARACTERISTICS_SMPTE2084 as u16 ||
                              (*image).transferCharacteristics == sys::AVIF_TRANSFER_CHARACTERISTICS_HLG as u16;
        
        let _has_wide_gamut = (*image).colorPrimaries == sys::AVIF_COLOR_PRIMARIES_BT2020 as u16 ||
                           (*image).colorPrimaries == 12; // Display P3
        
        let is_hdr = bit_depth > 8 && (
            // Traditional HDR: BT.2020 + PQ/HLG
            ((*image).colorPrimaries == sys::AVIF_COLOR_PRIMARIES_BT2020 as u16 && has_hdr_transfer) ||
            // Wide gamut HDR: Display P3 + high bit depth (preserve as HDR)
            ((*image).colorPrimaries == 12 && bit_depth >= 10) ||
            // Any high bit depth with HDR transfer function
            (bit_depth > 8 && has_hdr_transfer)
        );
        
        // Extract ICC profile if present
        let icc_profile = if (*image).icc.size > 0 && !(*image).icc.data.is_null() {
            let icc_slice = std::slice::from_raw_parts((*image).icc.data, (*image).icc.size);
            Some(icc_slice.to_vec())
        } else {
            None
        };
        
        let info = AvifImageInfo {
            bit_depth: bit_depth as u8,
            has_alpha,
            is_hdr,
            icc_profile: icc_profile.clone(),
            color_primaries: (*image).colorPrimaries,
            transfer_characteristics: (*image).transferCharacteristics,
            matrix_coefficients: (*image).matrixCoefficients,
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
            width, height, bit_depth, has_alpha, is_hdr, 
            color_space_name, (*image).colorPrimaries,
            transfer_name, (*image).transferCharacteristics
        );
        
        // Convert to RGB
        let mut rgb = sys::avifRGBImage::default();
        sys::avifRGBImageSetDefaults(&mut rgb, image);
        
        // Set format explicitly
        rgb.format = if has_alpha { 
            sys::AVIF_RGB_FORMAT_RGBA as u32 
        } else { 
            sys::AVIF_RGB_FORMAT_RGB as u32 
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
        if sys::avifRGBImageAllocatePixels(&mut rgb) != sys::AVIF_RESULT_OK as u32 {
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError("Failed to allocate RGB pixels".to_string()));
        }
        
        debug!(
            "RGB allocated: rowBytes={}, expected bytes per pixel={}",
            rgb.rowBytes,
            if has_alpha { 4 } else { 3 } * if bit_depth > 8 { 2 } else { 1 }
        );
        
        // Convert YUV to RGB
        if sys::avifImageYUVToRGB(image, &mut rgb) != sys::AVIF_RESULT_OK as u32 {
            sys::avifRGBImageFreePixels(&mut rgb);
            sys::avifImageDestroy(image);
            sys::avifDecoderDestroy(decoder);
            return Err(GalleryError::ProcessingError("Failed to convert YUV to RGB".to_string()));
        }
        
        // Create DynamicImage based on bit depth
        let dynamic_img = if bit_depth > 8 {
            // HDR image - convert to 16-bit
            if has_alpha {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 8; // 4 channels * 2 bytes
                
                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset = y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = unsafe { rgb.pixels.add(pixel_offset) as *const u16 };
                        
                        // Scale from bit_depth to 16-bit if needed
                        let shift = 16 - bit_depth;
                        let pixel = unsafe {
                            image::Rgba([
                                *pixel_ptr << shift,
                                *pixel_ptr.add(1) << shift,
                                *pixel_ptr.add(2) << shift,
                                *pixel_ptr.add(3) << shift,
                            ])
                        };
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgba16(img)
            } else {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 6; // 3 channels * 2 bytes
                
                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset = y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = unsafe { rgb.pixels.add(pixel_offset) as *const u16 };
                        
                        // Scale from bit_depth to 16-bit if needed
                        let shift = 16 - bit_depth;
                        let pixel = unsafe {
                            image::Rgb([
                                *pixel_ptr << shift,
                                *pixel_ptr.add(1) << shift,
                                *pixel_ptr.add(2) << shift,
                            ])
                        };
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
                        let pixel_offset = y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = unsafe { rgb.pixels.add(pixel_offset) };
                        
                        let pixel = unsafe {
                            image::Rgba([
                                *pixel_ptr,
                                *pixel_ptr.add(1),
                                *pixel_ptr.add(2),
                                *pixel_ptr.add(3),
                            ])
                        };
                        img.put_pixel(x, y, pixel);
                    }
                }
                DynamicImage::ImageRgba8(img)
            } else {
                let mut img = ImageBuffer::new(width, height);
                let bytes_per_pixel = 3; // RGB
                
                for y in 0..height {
                    for x in 0..width {
                        let pixel_offset = y as usize * rgb.rowBytes as usize + x as usize * bytes_per_pixel;
                        let pixel_ptr = unsafe { rgb.pixels.add(pixel_offset) };
                        
                        let pixel = unsafe {
                            image::Rgb([
                                *pixel_ptr,
                                *pixel_ptr.add(1),
                                *pixel_ptr.add(2),
                            ])
                        };
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

/// Fallback function to read AVIF with image crate
fn read_with_fallback(path: &Path) -> Result<(DynamicImage, AvifImageInfo), GalleryError> {
    let file = File::open(path)?;
    let reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()?;
    
    let img = reader.decode()?;
    
    let info = AvifImageInfo {
        bit_depth: 8,
        has_alpha: matches!(
            &img, 
            DynamicImage::ImageLumaA8(_) | 
            DynamicImage::ImageLumaA16(_) | 
            DynamicImage::ImageRgba8(_) | 
            DynamicImage::ImageRgba16(_)
        ),
        is_hdr: false, // Fallback doesn't preserve HDR
        icc_profile: None,
        // Default to sRGB/BT.709 for fallback
        color_primaries: sys::AVIF_COLOR_PRIMARIES_BT709 as u16,
        transfer_characteristics: sys::AVIF_TRANSFER_CHARACTERISTICS_SRGB as u16,
        matrix_coefficients: sys::AVIF_MATRIX_COEFFICIENTS_BT709 as u16,
    };
    
    Ok((img, info))
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

/// Save a DynamicImage as AVIF with HDR support (backward compatibility)
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
        DynamicImage::ImageLuma16(_) | 
        DynamicImage::ImageLumaA16(_) |
        DynamicImage::ImageRgb16(_) |
        DynamicImage::ImageRgba16(_)
    );
    
    let has_alpha = matches!(
        image,
        DynamicImage::ImageLumaA8(_) | 
        DynamicImage::ImageLumaA16(_) | 
        DynamicImage::ImageRgba8(_) | 
        DynamicImage::ImageRgba16(_)
    );
    
    // Decide bit depth based on input and preserve_hdr flag
    let bit_depth = if is_16bit && preserve_hdr { 10 } else { 8 };
    
    debug!("Encoding AVIF: {}x{}, quality={}, speed={}, bit_depth={}, has_alpha={}, preserve_hdr={}", 
           width, height, quality, speed, bit_depth, has_alpha, preserve_hdr);
    
    unsafe {
        // Create AVIF image with appropriate bit depth
        let avif_image = sys::avifImageCreate(
            width,
            height,
            bit_depth,
            if has_alpha { sys::AVIF_PIXEL_FORMAT_YUV444 as u32 } else { sys::AVIF_PIXEL_FORMAT_YUV420 as u32 }
        );
        
        if avif_image.is_null() {
            return Err(GalleryError::ProcessingError("Failed to create AVIF image".to_string()));
        }
        
        // Set color properties - preserve original if provided
        if let Some(info) = color_info {
            // Use the original color properties
            (*avif_image).colorPrimaries = info.color_primaries;
            (*avif_image).transferCharacteristics = info.transfer_characteristics;
            (*avif_image).matrixCoefficients = info.matrix_coefficients;
            (*avif_image).yuvRange = sys::AVIF_RANGE_FULL as u32;
            
            debug!(
                "Using provided color properties: primaries={}, transfer={}, matrix={}",
                info.color_primaries, info.transfer_characteristics, info.matrix_coefficients
            );
        } else {
            // Fallback to defaults based on whether we're preserving HDR
            if is_16bit && preserve_hdr {
                (*avif_image).colorPrimaries = sys::AVIF_COLOR_PRIMARIES_BT2020 as u16;
                (*avif_image).transferCharacteristics = sys::AVIF_TRANSFER_CHARACTERISTICS_SMPTE2084 as u16; // PQ
                (*avif_image).matrixCoefficients = sys::AVIF_MATRIX_COEFFICIENTS_BT2020_NCL as u16;
                (*avif_image).yuvRange = sys::AVIF_RANGE_FULL as u32;
            } else {
                (*avif_image).colorPrimaries = sys::AVIF_COLOR_PRIMARIES_BT709 as u16;
                (*avif_image).transferCharacteristics = sys::AVIF_TRANSFER_CHARACTERISTICS_SRGB as u16;
                (*avif_image).matrixCoefficients = sys::AVIF_MATRIX_COEFFICIENTS_BT709 as u16;
                (*avif_image).yuvRange = sys::AVIF_RANGE_FULL as u32;
            }
        }
        
        // Set ICC profile if provided
        if let Some(icc) = icc_profile {
            sys::avifImageSetProfileICC(avif_image, icc.as_ptr(), icc.len());
        }
        
        // Allocate planes
        sys::avifImageAllocatePlanes(avif_image, sys::AVIF_PLANES_YUV as u32);
        if has_alpha {
            sys::avifImageAllocatePlanes(avif_image, sys::AVIF_PLANES_A as u32);
        }
        
        // Create RGB image for conversion
        let mut rgb = sys::avifRGBImage::default();
        sys::avifRGBImageSetDefaults(&mut rgb, avif_image);
        rgb.depth = bit_depth;
        rgb.format = if has_alpha { sys::AVIF_RGB_FORMAT_RGBA as u32 } else { sys::AVIF_RGB_FORMAT_RGB as u32 };
        
        // Allocate RGB pixels
        if sys::avifRGBImageAllocatePixels(&mut rgb) != sys::AVIF_RESULT_OK as u32 {
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError("Failed to allocate RGB pixels".to_string()));
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
                            unsafe {
                                *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                            }
                        }
                    }
                },
                DynamicImage::ImageRgba16(img) => {
                    for y in 0..height {
                        for x in 0..width {
                            let src_pixel = img.get_pixel(x, y);
                            let dst_idx = y as usize * row_bytes / 2 + x as usize * 4;
                            // Scale from 16-bit to target bit depth
                            let shift = 16 - bit_depth;
                            unsafe {
                                *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                                *pixels.add(dst_idx + 3) = src_pixel[3] >> shift;
                            }
                        }
                    }
                },
                _ => {
                    // Convert to 16-bit first
                    if has_alpha {
                        let rgba16 = image.to_rgba16();
                        for y in 0..height {
                            for x in 0..width {
                                let src_pixel = rgba16.get_pixel(x, y);
                                let dst_idx = y as usize * row_bytes / 2 + x as usize * 4;
                                let shift = 16 - bit_depth;
                                unsafe {
                                    *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                    *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                    *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                                    *pixels.add(dst_idx + 3) = src_pixel[3] >> shift;
                                }
                            }
                        }
                    } else {
                        let rgb16 = image.to_rgb16();
                        for y in 0..height {
                            for x in 0..width {
                                let src_pixel = rgb16.get_pixel(x, y);
                                let dst_idx = y as usize * row_bytes / 2 + x as usize * 3;
                                let shift = 16 - bit_depth;
                                unsafe {
                                    *pixels.add(dst_idx) = src_pixel[0] >> shift;
                                    *pixels.add(dst_idx + 1) = src_pixel[1] >> shift;
                                    *pixels.add(dst_idx + 2) = src_pixel[2] >> shift;
                                }
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
                        unsafe {
                            *pixels.add(dst_idx) = src_pixel[0];
                            *pixels.add(dst_idx + 1) = src_pixel[1];
                            *pixels.add(dst_idx + 2) = src_pixel[2];
                            *pixels.add(dst_idx + 3) = src_pixel[3];
                        }
                    }
                }
            } else {
                let rgb_img = image.to_rgb8();
                for y in 0..height {
                    for x in 0..width {
                        let src_pixel = rgb_img.get_pixel(x, y);
                        let dst_idx = y as usize * row_bytes + x as usize * 3;
                        unsafe {
                            *pixels.add(dst_idx) = src_pixel[0];
                            *pixels.add(dst_idx + 1) = src_pixel[1];
                            *pixels.add(dst_idx + 2) = src_pixel[2];
                        }
                    }
                }
            }
        }
        
        // Convert RGB to YUV
        if sys::avifImageRGBToYUV(avif_image, &rgb) != sys::AVIF_RESULT_OK as u32 {
            sys::avifRGBImageFreePixels(&mut rgb);
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError("Failed to convert RGB to YUV".to_string()));
        }
        
        sys::avifRGBImageFreePixels(&mut rgb);
        
        // Create encoder
        let encoder = sys::avifEncoderCreate();
        if encoder.is_null() {
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError("Failed to create encoder".to_string()));
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
        
        if result != sys::AVIF_RESULT_OK as u32 {
            sys::avifEncoderDestroy(encoder);
            sys::avifImageDestroy(avif_image);
            return Err(GalleryError::ProcessingError(format!("Failed to encode AVIF: error {}", result)));
        }
        
        // Write to file
        let data = std::slice::from_raw_parts(output.data, output.size);
        std::fs::write(path, data)?;
        
        // Clean up
        sys::avifRWDataFree(&mut output);
        sys::avifEncoderDestroy(encoder);
        sys::avifImageDestroy(avif_image);
        
        debug!("Successfully saved {} AVIF to {:?}", if bit_depth > 8 { "HDR" } else { "SDR" }, path);
        Ok(())
    }
}

/// Extract ICC profile from an AVIF file
pub fn extract_icc_profile(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    
    // The simple libavif API doesn't expose ICC profiles
    // Use fallback manual extraction
    extract_icc_profile_fallback(&data)
}

/// Fallback ICC profile extraction by parsing AVIF container
fn extract_icc_profile_fallback(data: &[u8]) -> Option<Vec<u8>> {
    // Parse AVIF boxes to find colr box
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let box_size = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        
        if box_size == 0 || box_size == 1 || pos + box_size > data.len() {
            break;
        }
        
        let box_type = &data[pos+4..pos+8];
        
        if box_type == b"meta" && pos + 12 < data.len() {
            // Search within meta box for colr
            return find_colr_in_meta(&data[pos+12..pos+box_size]);
        }
        
        pos += box_size;
    }
    
    None
}

/// Find colr box within meta box
fn find_colr_in_meta(meta_data: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    
    while pos + 8 <= meta_data.len() {
        let box_size = u32::from_be_bytes([meta_data[pos], meta_data[pos+1], meta_data[pos+2], meta_data[pos+3]]) as usize;
        
        if box_size == 0 || box_size == 1 || pos + box_size > meta_data.len() {
            break;
        }
        
        let box_type = &meta_data[pos+4..pos+8];
        
        if box_type == b"colr" && box_size > 12 {
            let colr_data = &meta_data[pos+8..pos+box_size];
            
            if colr_data.len() > 4 && &colr_data[0..4] == b"prof" {
                // ICC profile found
                return Some(colr_data[4..].to_vec());
            }
        }
        
        // Recurse into iprp (item properties) box
        if box_type == b"iprp" && box_size > 8 {
            if let Some(icc) = find_colr_in_meta(&meta_data[pos+8..pos+box_size]) {
                return Some(icc);
            }
        }
        
        // Recurse into ipco (item property container) box
        if box_type == b"ipco" && box_size > 8 {
            if let Some(icc) = find_colr_in_meta(&meta_data[pos+8..pos+box_size]) {
                return Some(icc);
            }
        }
        
        pos += box_size;
    }
    
    None
}

/// Extract dimensions from AVIF file without full decode
pub fn extract_dimensions(path: &Path) -> Option<(u32, u32)> {
    // Try using libavif first
    if let Ok(data) = std::fs::read(path) {
        if is_avif(&data) {
            if let Ok(rgb_pixels) = decode_rgb(&data) {
                return Some((rgb_pixels.width(), rgb_pixels.height()));
            }
        }
    }
    
    // Try using image crate
    if let Ok(dimensions) = image::image_dimensions(path) {
        return Some(dimensions);
    }
    
    // Fallback to parsing AVIF container
    extract_dimensions_fallback(path)
}

/// Fallback dimension extraction by parsing AVIF container
fn extract_dimensions_fallback(path: &Path) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    
    // Look for ispe (image spatial extents) box
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let box_size = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        
        if box_size == 0 || box_size == 1 || pos + box_size > data.len() {
            break;
        }
        
        let box_type = &data[pos+4..pos+8];
        
        if box_type == b"meta" && pos + 12 < data.len() {
            // Search within meta box for ispe
            if let Some((w, h)) = find_ispe_in_meta(&data[pos+12..pos+box_size]) {
                return Some((w, h));
            }
        }
        
        pos += box_size;
    }
    
    None
}

/// Find ispe box within meta box structure
fn find_ispe_in_meta(meta_data: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 0;
    
    while pos + 8 <= meta_data.len() {
        let box_size = u32::from_be_bytes([meta_data[pos], meta_data[pos+1], meta_data[pos+2], meta_data[pos+3]]) as usize;
        
        if box_size == 0 || box_size == 1 || pos + box_size > meta_data.len() {
            break;
        }
        
        let box_type = &meta_data[pos+4..pos+8];
        
        // ispe box contains image dimensions
        if box_type == b"ispe" && pos + 20 <= meta_data.len() {
            let width = u32::from_be_bytes([meta_data[pos+12], meta_data[pos+13], meta_data[pos+14], meta_data[pos+15]]);
            let height = u32::from_be_bytes([meta_data[pos+16], meta_data[pos+17], meta_data[pos+18], meta_data[pos+19]]);
            return Some((width, height));
        }
        
        // Recurse into iprp (item properties) box
        if box_type == b"iprp" && box_size > 8 {
            if let Some(dims) = find_ispe_in_meta(&meta_data[pos+8..pos+box_size]) {
                return Some(dims);
            }
        }
        
        // Recurse into ipco (item property container) box
        if box_type == b"ipco" && box_size > 8 {
            if let Some(dims) = find_ispe_in_meta(&meta_data[pos+8..pos+box_size]) {
                return Some(dims);
            }
        }
        
        pos += box_size;
    }
    
    None
}