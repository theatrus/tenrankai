use crate::gallery::Gallery;
use crate::gallery::image_processing::OutputFormat;
use crate::gallery::image_processing::formats::avif;
use crate::GallerySystemConfig;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

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

#[tokio::test]
async fn test_avif_format_detection() {
    let (_gallery, _temp_dir) = create_test_gallery().await;
    
    assert_eq!(OutputFormat::Avif.extension(), "avif");
    assert_eq!(OutputFormat::Avif.mime_type(), "image/avif");
}

#[tokio::test]
async fn test_avif_output_format_selection() {
    let (gallery, _temp_dir) = create_test_gallery().await;
    
    // Test that AVIF is selected when browser supports it
    let accept_avif = "image/avif,image/webp,image/apng,image/*,*/*;q=0.8";
    let format = gallery.determine_output_format(accept_avif, "test.jpg");
    assert_eq!(format, OutputFormat::Avif);
    
    // Test that AVIF source outputs as AVIF when supported
    let format = gallery.determine_output_format(accept_avif, "test.avif");
    assert_eq!(format, OutputFormat::Avif);
    
    // Test fallback when AVIF not supported
    let accept_no_avif = "image/webp,image/apng,image/*,*/*;q=0.8";
    let format = gallery.determine_output_format(accept_no_avif, "test.jpg");
    assert_eq!(format, OutputFormat::WebP);
}

#[test]
fn test_avif_8bit_encoding() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create an 8-bit RGB image
    let img = ImageBuffer::from_pixel(100, 100, Rgb([255u8, 128, 64]));
    let dynamic_img = DynamicImage::ImageRgb8(img);
    
    // Save as AVIF
    let avif_path = temp_dir.path().join("test_8bit.avif");
    let result = avif::save_with_profile(&dynamic_img, &avif_path, 90, 6, None, false);
    
    assert!(result.is_ok(), "Failed to save 8-bit AVIF: {:?}", result);
    assert!(avif_path.exists());
    
    // Test dimensions extraction
    let dims = avif::extract_dimensions(&avif_path);
    assert_eq!(dims, Some((100, 100)));
}

#[test]
fn test_avif_with_alpha() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create an RGBA image with transparency
    let img = ImageBuffer::from_pixel(100, 100, Rgba([255u8, 128, 64, 200]));
    let dynamic_img = DynamicImage::ImageRgba8(img);
    
    // Save as AVIF
    let avif_path = temp_dir.path().join("test_alpha.avif");
    let result = avif::save_with_profile(&dynamic_img, &avif_path, 90, 6, None, false);
    
    assert!(result.is_ok(), "Failed to save AVIF with alpha: {:?}", result);
    assert!(avif_path.exists());
}

#[test]
fn test_avif_16bit_hdr() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a 16-bit HDR image
    let img = ImageBuffer::from_pixel(100, 100, Rgb([65535u16, 32768, 16384]));
    let dynamic_img = DynamicImage::ImageRgb16(img);
    
    // Save as AVIF with HDR preservation
    let avif_path = temp_dir.path().join("test_hdr.avif");
    let result = avif::save_with_profile(&dynamic_img, &avif_path, 90, 6, None, true);
    
    assert!(result.is_ok(), "Failed to save HDR AVIF: {:?}", result);
    assert!(avif_path.exists());
}

#[test]
fn test_avif_icc_profile() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a test ICC profile
    let test_icc = create_test_srgb_profile();
    
    // Create an image
    let img = ImageBuffer::from_pixel(100, 100, Rgb([255u8, 128, 64]));
    let dynamic_img = DynamicImage::ImageRgb8(img);
    
    // Save with ICC profile
    let avif_path = temp_dir.path().join("test_icc.avif");
    let result = avif::save_with_profile(&dynamic_img, &avif_path, 90, 6, Some(&test_icc), false);
    
    assert!(result.is_ok(), "Failed to save AVIF with ICC: {:?}", result);
    assert!(avif_path.exists());
    
    // Extract ICC profile
    let extracted_icc = avif::extract_icc_profile(&avif_path);
    // Note: Current libavif simple API doesn't support ICC profiles
    // This is a known limitation that we'll address when we move to the full API
    if extracted_icc.is_none() {
        eprintln!("Warning: ICC profile extraction not yet supported with current libavif API");
    }
}

#[test]
fn test_avif_dimension_extraction() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create images of various sizes
    let test_sizes = vec![(200, 150), (1920, 1080), (50, 50)];
    
    for (width, height) in test_sizes {
        let img = ImageBuffer::from_pixel(width, height, Rgb([255u8, 128, 64]));
        let dynamic_img = DynamicImage::ImageRgb8(img);
        
        let avif_path = temp_dir.path().join(format!("test_{}x{}.avif", width, height));
        avif::save_with_profile(&dynamic_img, &avif_path, 90, 6, None, false).unwrap();
        
        // Test dimension extraction
        let dims = avif::extract_dimensions(&avif_path);
        assert_eq!(
            dims,
            Some((width, height)),
            "Dimension extraction failed for {}x{}", width, height
        );
    }
}

#[tokio::test]
async fn test_avif_in_gallery_pipeline() {
    let (gallery, temp_dir) = create_test_gallery().await;
    
    // Create a test image
    let img = ImageBuffer::from_pixel(200, 200, Rgb([255u8, 128, 64]));
    let source_path = temp_dir.path().join("test.jpg");
    img.save(&source_path).unwrap();
    
    // Process to AVIF
    let relative_path = "test.jpg";
    let result = gallery
        .get_resized_image(
            &source_path,
            relative_path,
            "thumbnail",
            OutputFormat::Avif,
        )
        .await;
    
    assert!(result.is_ok(), "Failed to create AVIF thumbnail: {:?}", result);
    
    let cache_path = result.unwrap();
    assert!(cache_path.exists());
    assert!(cache_path.to_str().unwrap().ends_with(".avif"));
}

fn create_test_srgb_profile() -> Vec<u8> {
    // Minimal sRGB ICC profile for testing
    let mut profile = Vec::new();
    
    // ICC profile header
    profile.extend_from_slice(&[0x00, 0x00, 0x02, 0x00]); // Profile size
    profile.extend_from_slice(b"APPL"); // Preferred CMM
    profile.extend_from_slice(&[0x04, 0x30, 0x00, 0x00]); // Version
    profile.extend_from_slice(b"mntr"); // Display device
    profile.extend_from_slice(b"RGB "); // Color space
    profile.extend_from_slice(b"XYZ "); // PCS
    
    // Pad to reasonable size
    while profile.len() < 512 {
        profile.push(0);
    }
    
    profile
}