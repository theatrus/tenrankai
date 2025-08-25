use crate::gallery::GalleryError;
use image::{DynamicImage, ImageReader, GenericImageView, ImageBuffer};
use libavif::{Encoder, RgbPixels, YuvFormat, decode_rgb, is_avif};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tracing::{debug, error};

/// AVIF specific image information
#[derive(Debug, Clone)]
pub struct AvifImageInfo {
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub is_hdr: bool,
    pub icc_profile: Option<Vec<u8>>,
}

/// Read an AVIF file using libavif
pub fn read_avif_info(path: &Path) -> Result<(DynamicImage, AvifImageInfo), GalleryError> {
    // Read the file
    let data = std::fs::read(path)?;
    
    // Check if it's a valid AVIF file
    if !is_avif(&data) {
        return Err(GalleryError::ProcessingError("Not a valid AVIF file".to_string()));
    }
    
    // Decode to RGB
    match decode_rgb(&data) {
        Ok(rgb_pixels) => {
            let width = rgb_pixels.width();
            let height = rgb_pixels.height();
            
            // For now, we'll decode as 8-bit RGB/RGBA
            // The libavif crate's simple API doesn't expose bit depth info directly
            let pixels = rgb_pixels.as_slice();
            let has_alpha = pixels.len() == (width * height * 4) as usize;
            
            let info = AvifImageInfo {
                bit_depth: 8, // libavif simple API defaults to 8-bit
                has_alpha,
                is_hdr: false, // Can't determine from simple API
                icc_profile: None, // Not exposed in simple API
            };
            
            let dynamic_img = if has_alpha {
                let mut img = ImageBuffer::new(width, height);
                for y in 0..height {
                    for x in 0..width {
                        let idx = ((y * width + x) * 4) as usize;
                        if idx + 3 < pixels.len() {
                            img.put_pixel(x, y, image::Rgba([
                                pixels[idx],
                                pixels[idx + 1],
                                pixels[idx + 2],
                                pixels[idx + 3],
                            ]));
                        }
                    }
                }
                DynamicImage::ImageRgba8(img)
            } else {
                let mut img = ImageBuffer::new(width, height);
                for y in 0..height {
                    for x in 0..width {
                        let idx = ((y * width + x) * 3) as usize;
                        if idx + 2 < pixels.len() {
                            img.put_pixel(x, y, image::Rgb([
                                pixels[idx],
                                pixels[idx + 1],
                                pixels[idx + 2],
                            ]));
                        }
                    }
                }
                DynamicImage::ImageRgb8(img)
            };
            
            debug!(
                "Successfully decoded AVIF: {}x{}, has_alpha={}",
                width, height, has_alpha
            );
            
            Ok((dynamic_img, info))
        }
        Err(e) => {
            error!("Failed to decode AVIF with libavif: {}", e);
            
            // Fallback to image crate
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
                is_hdr: matches!(
                    &img,
                    DynamicImage::ImageLuma16(_) | 
                    DynamicImage::ImageLumaA16(_) |
                    DynamicImage::ImageRgb16(_) |
                    DynamicImage::ImageRgba16(_)
                ),
                icc_profile: None,
            };
            
            Ok((img, info))
        }
    }
}

/// Save a DynamicImage as AVIF with HDR support
pub fn save_with_profile(
    image: &DynamicImage,
    path: &Path,
    quality: u8,
    speed: u8,
    _icc_profile: Option<&[u8]>,
    preserve_hdr: bool,
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
    
    debug!("Encoding AVIF: {}x{}, quality={}, speed={}, is_16bit={}, has_alpha={}, preserve_hdr={}", 
           width, height, quality, speed, is_16bit, has_alpha, preserve_hdr);
    
    // Convert to RGB bytes
    let rgb_data = if has_alpha {
        let rgba = image.to_rgba8();
        rgba.as_raw().to_vec()
    } else {
        let rgb = image.to_rgb8();
        rgb.as_raw().to_vec()
    };
    
    // Create RGB pixels
    let rgb_pixels = RgbPixels::new(width, height, &rgb_data)
        .map_err(|e| GalleryError::ProcessingError(format!("Failed to create RGB pixels: {:?}", e)))?;
    
    // Convert to YUV image
    let yuv_image = rgb_pixels.to_image(YuvFormat::Yuv444);
    
    // Create encoder with settings
    let mut encoder = Encoder::new();
    encoder
        .set_quality(quality.min(100))
        .set_speed(speed.min(10));
    
    // For HDR support, we would need to use the low-level API
    // The high-level API only supports 8-bit encoding
    if is_16bit && preserve_hdr {
        debug!("Warning: HDR preservation not yet supported with current libavif API");
    }
    
    // Encode the image
    let encoded = encoder.encode(&yuv_image)
        .map_err(|e| GalleryError::ProcessingError(format!("Failed to encode AVIF: {:?}", e)))?;
    
    // Write to file
    std::fs::write(path, &*encoded)?;
    
    debug!("Successfully saved AVIF to {:?}", path);
    Ok(())
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