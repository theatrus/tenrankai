#![cfg(feature = "avif")]

use image::imageops::FilterType;
use std::path::Path;
use tempfile::TempDir;
use tenrankai::gallery::image_processing::formats;

#[test]
fn test_avif_gain_map_preservation() {
    // Create a temp directory for output
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("resized_with_gainmap.avif");

    // Path to test AVIF with gain map
    let input_path = Path::new("photos/vacation/_A630303-HDR-gainmap.avif");

    // Skip test if file doesn't exist
    if !input_path.exists() {
        eprintln!("Test file not found, skipping test");
        return;
    }

    // Read original AVIF with gain map
    let (img, avif_info) = formats::avif::read_avif_info(input_path).unwrap();

    // Verify original has gain map
    assert!(
        avif_info.has_gain_map,
        "Original image should have gain map"
    );
    assert!(
        avif_info.gain_map_info.is_some(),
        "Gain map info should be present"
    );

    let original_gain_map = avif_info.gain_map_info.as_ref().unwrap();
    assert!(
        original_gain_map.has_image,
        "Gain map should have image data"
    );
    assert!(
        original_gain_map.gain_map_image.is_some(),
        "Gain map image should be present"
    );

    let original_gm_image = original_gain_map.gain_map_image.as_ref().unwrap();
    let original_gm_width = original_gm_image.width();
    let original_gm_height = original_gm_image.height();

    // Verify gain map dimensions match main image
    assert_eq!(
        img.width(),
        original_gm_width,
        "Gain map width should match main image"
    );
    assert_eq!(
        img.height(),
        original_gm_height,
        "Gain map height should match main image"
    );

    // Resize main image
    let target_width = 800;
    let target_height = 800;
    let resized = img.resize(
        target_width.min(img.width()),
        target_height.min(img.height()),
        FilterType::Lanczos3,
    );

    // Calculate scale factors
    let scale_x = resized.width() as f32 / img.width() as f32;
    let scale_y = resized.height() as f32 / img.height() as f32;

    // Resize gain map proportionally
    let mut resized_avif_info = avif_info.clone();
    if let Some(ref mut gm_info) = resized_avif_info.gain_map_info {
        if let Some(ref gm_image) = gm_info.gain_map_image {
            let new_gm_width = (gm_image.width() as f32 * scale_x).round() as u32;
            let new_gm_height = (gm_image.height() as f32 * scale_y).round() as u32;

            let resized_gain_map = gm_image.resize_exact(
                new_gm_width.max(1),
                new_gm_height.max(1),
                FilterType::Lanczos3,
            );

            gm_info.gain_map_image = Some(resized_gain_map);
        }
    }

    // Save with gain map
    formats::avif::save_with_info(
        &resized,
        &output_path,
        85, // quality
        6,  // speed
        Some(&resized_avif_info),
    )
    .unwrap();

    // Verify the output
    let (output_img, output_info) = formats::avif::read_avif_info(&output_path).unwrap();

    // Check that output has gain map
    assert!(
        output_info.has_gain_map,
        "Output image should have gain map"
    );
    assert!(
        output_info.gain_map_info.is_some(),
        "Output gain map info should be present"
    );

    let output_gain_map = output_info.gain_map_info.as_ref().unwrap();
    assert!(
        output_gain_map.has_image,
        "Output gain map should have image data"
    );
    assert!(
        output_gain_map.gain_map_image.is_some(),
        "Output gain map image should be present"
    );

    // Verify gain map was resized correctly
    let output_gm_image = output_gain_map.gain_map_image.as_ref().unwrap();
    assert_eq!(output_img.width(), resized.width());
    assert_eq!(output_img.height(), resized.height());
    assert_eq!(output_gm_image.width(), output_img.width());
    assert_eq!(output_gm_image.height(), output_img.height());

    // Verify metadata was preserved
    assert_eq!(output_info.color_primaries, avif_info.color_primaries);
    assert_eq!(
        output_info.transfer_characteristics,
        avif_info.transfer_characteristics
    );
    assert_eq!(output_info.is_hdr, avif_info.is_hdr);

    // Verify gain map parameters were preserved (with floating point tolerance)
    let epsilon = 0.001;
    for i in 0..3 {
        assert!(
            (output_gain_map.gamma[i] - original_gain_map.gamma[i]).abs() < epsilon,
            "Gamma[{}] mismatch: {} vs {}",
            i,
            output_gain_map.gamma[i],
            original_gain_map.gamma[i]
        );
        assert!(
            (output_gain_map.min[i] - original_gain_map.min[i]).abs() < epsilon,
            "Min[{}] mismatch: {} vs {}",
            i,
            output_gain_map.min[i],
            original_gain_map.min[i]
        );
        assert!(
            (output_gain_map.max[i] - original_gain_map.max[i]).abs() < epsilon,
            "Max[{}] mismatch: {} vs {}",
            i,
            output_gain_map.max[i],
            original_gain_map.max[i]
        );
        assert!(
            (output_gain_map.base_offset[i] - original_gain_map.base_offset[i]).abs() < epsilon,
            "Base offset[{}] mismatch: {} vs {}",
            i,
            output_gain_map.base_offset[i],
            original_gain_map.base_offset[i]
        );
        assert!(
            (output_gain_map.alternate_offset[i] - original_gain_map.alternate_offset[i]).abs()
                < epsilon,
            "Alternate offset[{}] mismatch: {} vs {}",
            i,
            output_gain_map.alternate_offset[i],
            original_gain_map.alternate_offset[i]
        );
    }
    assert!(
        (output_gain_map.base_hdr_headroom - original_gain_map.base_hdr_headroom).abs() < epsilon,
        "Base HDR headroom mismatch: {} vs {}",
        output_gain_map.base_hdr_headroom,
        original_gain_map.base_hdr_headroom
    );
    assert!(
        (output_gain_map.alternate_hdr_headroom - original_gain_map.alternate_hdr_headroom).abs()
            < epsilon,
        "Alternate HDR headroom mismatch: {} vs {}",
        output_gain_map.alternate_hdr_headroom,
        original_gain_map.alternate_hdr_headroom
    );
}

#[test]
fn test_avif_without_gain_map() {
    // Test that regular AVIF files without gain maps still work correctly
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("regular_avif.avif");

    // Create a simple test image
    let img = image::DynamicImage::new_rgb8(100, 100);

    // Save as AVIF without gain map
    formats::avif::save_with_profile(
        &img,
        &output_path,
        85,    // quality
        6,     // speed
        None,  // no ICC profile
        false, // not HDR
    )
    .unwrap();

    // Read it back
    let (read_img, read_info) = formats::avif::read_avif_info(&output_path).unwrap();

    // Verify no gain map
    assert!(
        !read_info.has_gain_map,
        "Regular AVIF should not have gain map"
    );
    assert!(
        read_info.gain_map_info.is_none(),
        "Gain map info should be None"
    );

    // Verify dimensions
    assert_eq!(read_img.width(), 100);
    assert_eq!(read_img.height(), 100);
}
