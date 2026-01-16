use std::path::Path;
use crate::gallery::image_processing::formats::avif;

#[test]
fn test_real_avif_files() {
        let test_files = [
            ("photos/vacation/_A639941.avif", "Non-HDR AVIF"),
            ("photos/vacation/_A630303-HDR.avif", "HDR AVIF"),
        ];
        
        for (path_str, description) in &test_files {
            let path = Path::new(path_str);
            if !path.exists() {
                eprintln!("Skipping {} - file not found", path_str);
                continue;
            }
            
            println!("\nTesting {}: {}", description, path_str);
            
            // Test dimension extraction
            match avif::extract_dimensions(path) {
                Some((w, h)) => println!("  Dimensions: {}x{}", w, h),
                None => println!("  Failed to extract dimensions"),
            }
            
            // Test reading the file
            match avif::read_avif_info(path) {
                Ok((img, info)) => {
                    let (width, height) = img.dimensions();
                    println!("  Successfully read AVIF:");
                    println!("    - Size: {}x{}", width, height);
                    println!("    - Bit depth: {}", info.bit_depth);
                    println!("    - Has alpha: {}", info.has_alpha);
                    println!("    - Is HDR: {}", info.is_hdr);
                    println!("    - Has ICC profile: {}", info.icc_profile.is_some());
                    
                    // Verify dimensions match
                    if let Some((w, h)) = avif::extract_dimensions(path) {
                        assert_eq!(w, width, "Width mismatch for {}", path_str);
                        assert_eq!(h, height, "Height mismatch for {}", path_str);
                    }
                }
                Err(e) => println!("  Failed to read AVIF: {:?}", e),
        }
    }
}