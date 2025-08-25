use crate::GallerySystemConfig;
use crate::gallery::Gallery;
use image::{ImageBuffer, ImageEncoder};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

// Helper function to create a test gallery
async fn create_test_gallery() -> (Gallery, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");

    let config = GallerySystemConfig {
        name: "test".to_string(),
        url_prefix: "gallery".to_string(),
        gallery_template: "gallery.html.liquid".to_string(),
        image_detail_template: "image_detail.html.liquid".to_string(),
        source_directory: temp_dir.path().to_path_buf(),
        cache_directory: cache_dir,
        images_per_page: 50,
        thumbnail: crate::ImageSizeConfig {
            width: 300,
            height: 300,
        },
        gallery_size: crate::ImageSizeConfig {
            width: 800,
            height: 800,
        },
        medium: crate::ImageSizeConfig {
            width: 1200,
            height: 1200,
        },
        large: crate::ImageSizeConfig {
            width: 1600,
            height: 1600,
        },
        preview: crate::PreviewConfig {
            max_images: 6,
            max_depth: 3,
            max_per_folder: 3,
        },
        cache_refresh_interval_minutes: None,
        jpeg_quality: Some(85),
        webp_quality: Some(85.0),
        pregenerate_cache: false,
        new_threshold_days: None,
        approximate_dates_for_public: false,
        copyright_holder: None,
    };

    let gallery = Gallery {
        config,
        metadata_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        cache_metadata: Arc::new(RwLock::new(crate::gallery::CacheMetadata {
            version: String::new(),
            last_full_refresh: std::time::SystemTime::UNIX_EPOCH,
        })),
        metadata_cache_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        metadata_updates_since_save: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };

    (gallery, temp_dir)
}

// Create a minimal but valid Display P3 ICC profile for testing
fn create_test_display_p3_profile() -> Vec<u8> {
    // This is a simplified Display P3 profile for testing
    // Real Display P3 profiles are typically 548 bytes
    vec![
        // Profile header (128 bytes)
        0x00, 0x00, 0x02, 0x24, // Profile size (548 bytes)
        b'A', b'P', b'P', b'L', // Preferred CMM type
        0x04, 0x30, 0x00, 0x00, // Profile version 4.3.0
        b'm', b'n', b't', b'r', // Monitor device class
        b'R', b'G', b'B', b' ', // RGB color space
        b'X', b'Y', b'Z', b' ', // PCS (XYZ)
        // Creation date/time (12 bytes)
        0x07, 0xe7, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', b'c', b's',
        b'p', // Profile signature
        0x00, 0x00, 0x00, 0x00, // Platform signature
        0x00, 0x00, 0x00, 0x00, // Profile flags
        b'A', b'P', b'P', b'L', // Device manufacturer
        0x00, 0x00, 0x00, 0x00, // Device model
        0x00, 0x00, 0x00, 0x00, // Device attributes
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // Rendering intent: perceptual
        // PCS illuminant (12 bytes) - D65
        0x00, 0x00, 0xf6, 0xd6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xd3, 0x2d, b'A', b'P', b'P',
        b'L', // Profile creator
        // MD5 fingerprint (16 bytes) - zeros for simplicity
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // Reserved (28 bytes)
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // Tag table
        0x00, 0x00, 0x00, 0x0A, // Tag count (10 tags for a basic Display P3)
        // Tag directory entries (12 bytes each)
        // 1. 'desc' tag
        b'd', b'e', b's', b'c', 0x00, 0x00, 0x01, 0x4C, // Offset
        0x00, 0x00, 0x00, 0x6E, // Size
        // 2. 'cprt' tag
        b'c', b'p', b'r', b't', 0x00, 0x00, 0x01, 0xBA, // Offset
        0x00, 0x00, 0x00, 0x2C, // Size
        // 3. 'wtpt' tag (white point)
        b'w', b't', b'p', b't', 0x00, 0x00, 0x01, 0xE8, // Offset
        0x00, 0x00, 0x00, 0x14, // Size
        // 4. 'rXYZ' tag (red colorant)
        b'r', b'X', b'Y', b'Z', 0x00, 0x00, 0x01, 0xFC, // Offset
        0x00, 0x00, 0x00, 0x14, // Size
        // 5. 'gXYZ' tag (green colorant)
        b'g', b'X', b'Y', b'Z', 0x00, 0x00, 0x02, 0x10, // Offset
        0x00, 0x00, 0x00, 0x14, // Size
        // 6. 'bXYZ' tag (blue colorant)
        b'b', b'X', b'Y', b'Z', 0x00, 0x00, 0x02, 0x24, // Offset (end of profile)
        0x00, 0x00, 0x00, 0x14, // Size (but this extends past declared size)
        // Padding to reach minimum tag offset
        0, 0, 0, 0, 0, 0, 0, 0, // 'desc' tag data (mluc type)
        b'm', b'l', b'u', b'c', 0x00, 0x00, 0x00, 0x00, // mluc signature
        0x00, 0x00, 0x00, 0x01, // Number of records
        0x00, 0x00, 0x00, 0x0C, // Record size
        b'e', b'n', b'U', b'S', // Language code
        0x00, 0x00, 0x00, 0x1C, // String length
        0x00, 0x00, 0x00, 0x1C, // String offset
        // The actual description string
        0x00, b'D', 0x00, b'i', 0x00, b's', 0x00, b'p', 0x00, b'l', 0x00, b'a', 0x00, b'y', 0x00,
        b' ', 0x00, b'P', 0x00, b'3', 0x00, 0x00, // Padding
        0x00, 0x00,
    ]
}

// Helper function to create a test JPEG with ICC profile
fn create_test_jpeg_with_icc_profile(
    path: &std::path::Path,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Create a simple RGB image
    let img = ImageBuffer::from_pixel(width, height, image::Rgb([255u8, 128, 64]));

    // Create a minimal Display P3 ICC profile for testing
    let icc_profile = create_test_display_p3_profile();

    // First save as regular JPEG
    img.save(path)?;

    // Now read it back and inject the ICC profile
    let mut jpeg_data = std::fs::read(path)?;

    // Verify it's a valid JPEG
    if jpeg_data.len() < 4 || jpeg_data[0..2] != [0xFF, 0xD8] {
        return Err("Not a valid JPEG file".into());
    }

    // Find where to insert the APP2 segment (after SOI, before other segments)
    let mut insert_pos = 2; // After SOI marker (0xFF 0xD8)

    // Look for the first segment after SOI - usually APP0 (JFIF) or APP1 (EXIF)
    if jpeg_data.len() > 4 && jpeg_data[2] == 0xFF {
        // There's already a segment here, skip past it
        if jpeg_data[3] >= 0xE0 && jpeg_data[3] <= 0xEF {
            // It's an APP segment, get its length
            if jpeg_data.len() > 6 {
                let segment_length = u16::from_be_bytes([jpeg_data[4], jpeg_data[5]]) as usize;
                insert_pos = 4 + segment_length;
            }
        }
    }

    // Create APP2 segment with ICC profile
    let mut app2_segment = vec![0xFF, 0xE2]; // APP2 marker
    let icc_identifier = b"ICC_PROFILE\0\x01\x01"; // ICC profile identifier
    let segment_data_length = icc_identifier.len() + icc_profile.len();
    let segment_length = segment_data_length + 2; // +2 for length bytes
    app2_segment.extend_from_slice(&(segment_length as u16).to_be_bytes());
    app2_segment.extend_from_slice(icc_identifier);
    app2_segment.extend_from_slice(&icc_profile);

    // Insert the APP2 segment
    jpeg_data.splice(insert_pos..insert_pos, app2_segment);

    // Write back to file
    std::fs::write(path, &jpeg_data)?;

    Ok(icc_profile)
}

#[tokio::test]
async fn test_jpeg_icc_profile_preservation() {
    let (gallery, temp_dir) = create_test_gallery().await;

    // Create a simple JPEG first
    let img = ImageBuffer::from_pixel(100, 100, image::Rgb([255u8, 128, 64]));
    let source_path = temp_dir.path().join("test_source.jpg");
    img.save(&source_path).unwrap();

    // Now add ICC profile to it
    let icc_profile = create_test_display_p3_profile();

    // Use the actual JPEG encoder with ICC profile to create a proper test file
    use image::codecs::jpeg::JpegEncoder;
    let output_path = gallery.config.source_directory.join("test_icc.jpg");
    let output_file = std::fs::File::create(&output_path).unwrap();
    let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

    // Try to set ICC profile - if it fails, skip the test
    if encoder.set_icc_profile(icc_profile.clone()).is_err() {
        eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
        return;
    }

    img.write_with_encoder(encoder).unwrap();

    // Test thumbnail generation preserves ICC profile
    let relative_path = "test_icc.jpg";
    let result = gallery
        .get_resized_image(
            &output_path,
            relative_path,
            "thumbnail",
            crate::gallery::image_processing::OutputFormat::Jpeg,
        )
        .await;

    assert!(result.is_ok(), "Failed to resize JPEG: {:?}", result);

    let cache_path = result.unwrap();

    // Check if ICC profile was preserved
    let preserved_icc =
        crate::gallery::image_processing::extract_icc_profile_from_jpeg(&cache_path);
    assert!(
        preserved_icc.is_some(),
        "ICC profile should be preserved in resized JPEG"
    );

    // The profile might be modified by the encoder, so just check it exists and has reasonable size
    let preserved_icc = preserved_icc.unwrap();
    assert!(
        preserved_icc.len() > 100,
        "Preserved ICC profile seems too small: {} bytes",
        preserved_icc.len()
    );
}

#[tokio::test]
async fn test_jpeg_icc_profile_preservation_with_watermark() {
    let (mut gallery, _temp_dir) = create_test_gallery().await;

    // Enable watermarking
    gallery.config.copyright_holder = Some("Test Copyright".to_string());

    // Create a test image
    let img = ImageBuffer::from_pixel(200, 200, image::Rgb([255u8, 128, 64]));
    let icc_profile = create_test_display_p3_profile();

    // Save with ICC profile
    use image::codecs::jpeg::JpegEncoder;
    let output_path = gallery.config.source_directory.join("test_watermark.jpg");
    let output_file = std::fs::File::create(&output_path).unwrap();
    let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

    // Try to set ICC profile - if it fails, skip the test
    if encoder.set_icc_profile(icc_profile.clone()).is_err() {
        eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
        return;
    }

    img.write_with_encoder(encoder).unwrap();

    // Test medium size with watermark
    let relative_path = "test_watermark.jpg";
    let result = gallery
        .get_resized_image(
            &output_path,
            relative_path,
            "medium",
            crate::gallery::image_processing::OutputFormat::Jpeg,
        )
        .await;

    assert!(
        result.is_ok(),
        "Failed to resize JPEG with watermark: {:?}",
        result
    );

    let cache_path = result.unwrap();

    // Check if ICC profile was preserved despite watermarking
    let preserved_icc =
        crate::gallery::image_processing::extract_icc_profile_from_jpeg(&cache_path);
    assert!(
        preserved_icc.is_some(),
        "ICC profile should be preserved even with watermark"
    );
}

#[tokio::test]
async fn test_icc_profile_extraction_from_jpeg() {
    let temp_dir = TempDir::new().unwrap();
    let jpeg_path = temp_dir.path().join("test_extract.jpg");

    // Create JPEG with ICC profile
    let original_icc =
        create_test_jpeg_with_icc_profile(&jpeg_path, 50, 50).expect("Failed to create test JPEG");

    // Test extraction
    let extracted_icc = crate::gallery::image_processing::extract_icc_profile_from_jpeg(&jpeg_path);

    assert!(extracted_icc.is_some(), "Failed to extract ICC profile");
    let extracted_icc = extracted_icc.unwrap();

    assert_eq!(
        extracted_icc.len(),
        original_icc.len(),
        "Extracted ICC profile size doesn't match: {} vs {}",
        extracted_icc.len(),
        original_icc.len()
    );

    // Also test profile name extraction
    let profile_name = crate::gallery::image_processing::extract_icc_profile_name(&extracted_icc);
    // Our test profile might not have a proper desc tag, so just verify extraction doesn't crash
    // Real profiles would have names
    if profile_name.is_some() {
        println!("Extracted profile name: {:?}", profile_name);
    }
}

#[tokio::test]
async fn test_icc_profile_preservation_all_sizes() {
    let (gallery, _temp_dir) = create_test_gallery().await;

    // Create a smaller test image for faster testing (500x500 instead of 2000x2000)
    let img = ImageBuffer::from_pixel(500, 500, image::Rgb([255u8, 128, 64]));
    let icc_profile = create_test_display_p3_profile();

    // Save as JPEG with ICC profile
    use image::codecs::jpeg::JpegEncoder;
    let source_path = gallery.config.source_directory.join("test_all_sizes.jpg");
    let output_file = std::fs::File::create(&source_path).unwrap();
    let mut encoder = JpegEncoder::new_with_quality(output_file, 90);

    // Try to set ICC profile - if it fails, skip the test
    if encoder.set_icc_profile(icc_profile.clone()).is_err() {
        eprintln!("Skipping test - JPEG encoder doesn't support ICC profiles");
        return;
    }

    img.write_with_encoder(encoder).unwrap();

    // Test a subset of sizes to speed up the test - thumbnail and medium cover the key paths
    let sizes = ["thumbnail", "medium"];
    let formats = [
        crate::gallery::image_processing::OutputFormat::Jpeg,
        crate::gallery::image_processing::OutputFormat::WebP,
    ];

    for size in &sizes {
        for format in &formats {
            tracing::debug!("Testing {} size with {:?} format", size, format);

            let result = gallery
                .get_resized_image(&source_path, "test_all_sizes.jpg", size, *format)
                .await;

            assert!(
                result.is_ok(),
                "Failed to process {} as {:?}: {:?}",
                size,
                format,
                result
            );

            let cache_path = result.unwrap();

            match format {
                crate::gallery::image_processing::OutputFormat::Jpeg => {
                    // Check JPEG has ICC profile
                    let processed_icc =
                        crate::gallery::image_processing::extract_icc_profile_from_jpeg(
                            &cache_path,
                        );
                    assert!(
                        processed_icc.is_some(),
                        "ICC profile missing in {} JPEG",
                        size
                    );
                    assert!(
                        !processed_icc.unwrap().is_empty(),
                        "ICC profile empty in {} JPEG",
                        size
                    );
                }
                crate::gallery::image_processing::OutputFormat::WebP => {
                    // Check WebP has ICC profile
                    let webp_data = std::fs::read(&cache_path).unwrap();
                    let mut found_iccp = false;
                    let mut pos = 12;

                    while pos + 8 <= webp_data.len() {
                        let chunk_fourcc = &webp_data[pos..pos + 4];
                        if chunk_fourcc == b"ICCP" {
                            found_iccp = true;
                            break;
                        }
                        let chunk_size = u32::from_le_bytes([
                            webp_data[pos + 4],
                            webp_data[pos + 5],
                            webp_data[pos + 6],
                            webp_data[pos + 7],
                        ]) as usize;
                        pos += 8 + chunk_size + (chunk_size % 2);
                    }

                    assert!(found_iccp, "ICCP chunk missing in {} WebP", size);
                }
                crate::gallery::image_processing::OutputFormat::Png => {
                    // For PNG, we don't yet support ICC profile preservation in output
                    // Just verify it's a valid PNG
                    let png_data = std::fs::read(&cache_path).unwrap();
                    assert!(png_data.len() >= 8);
                    assert_eq!(
                        &png_data[0..8],
                        b"\x89PNG\r\n\x1a\n",
                        "Not a valid PNG file"
                    );
                }
            }
        }
    }
}
