use crate::gallery::image_processing::formats;
use image::DynamicImage;
use std::path::PathBuf;

pub async fn handle_avif_debug_command(
    image_path: PathBuf,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !image_path.exists() {
        eprintln!("Error: Image file not found: {:?}", image_path);
        std::process::exit(1);
    }

    println!("=== AVIF Debug Information ===");
    println!("File: {:?}", image_path);

    // Get file size
    let metadata = std::fs::metadata(&image_path)?;
    println!(
        "Size: {} bytes ({:.2} MB)",
        metadata.len(),
        metadata.len() as f64 / 1_048_576.0
    );

    // Verify it's an AVIF file
    let extension = image_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    if extension.as_deref() != Some("avif") {
        eprintln!(
            "Error: File is not an AVIF file (extension: {})",
            extension.as_deref().unwrap_or("none")
        );
        std::process::exit(1);
    }

    // Try to get basic dimensions using image crate first
    match image::image_dimensions(&image_path) {
        Ok((width, height)) => {
            println!("Dimensions (image crate): {}x{}", width, height);
        }
        Err(e) => {
            println!("Dimensions (image crate): Failed - {}", e);
        }
    }

    println!();
    println!("=== AVIF Analysis ===");

    // Try our custom AVIF reader
    match formats::avif::read_avif_info(&image_path) {
        Ok((image, info)) => {
            println!("✓ Successfully decoded with custom AVIF reader");
            println!();

            println!("Image Properties:");
            println!("  Dimensions: {}x{}", image.width(), image.height());
            println!("  Color type: {:?}", image.color());
            println!("  In-memory format: {}", format_description(&image));
            println!();

            println!("AVIF Metadata:");
            println!("  Bit depth: {} bits", info.bit_depth);
            println!("  Has alpha: {}", info.has_alpha);
            println!("  Detected as HDR: {}", info.is_hdr);

            // Color space details
            println!();
            println!("Color Space Properties:");
            let primaries_name = color_primaries_name(info.color_primaries);
            let transfer_name = transfer_characteristics_name(info.transfer_characteristics);
            let matrix_name = matrix_coefficients_name(info.matrix_coefficients);

            println!(
                "  Color primaries: {} ({})",
                info.color_primaries, primaries_name
            );
            println!(
                "  Transfer characteristics: {} ({})",
                info.transfer_characteristics, transfer_name
            );
            println!(
                "  Matrix coefficients: {} ({})",
                info.matrix_coefficients, matrix_name
            );

            // Show the descriptive color space string
            let color_description = formats::avif::get_color_space_description(&info);
            println!();
            println!("Color Profile Description: {}", color_description);

            // HDR analysis
            println!();
            println!("HDR Analysis:");
            let is_display_p3 = info.color_primaries == 12;
            let is_bt2020 = info.color_primaries == 9;
            let has_pq_transfer = info.transfer_characteristics == 16;
            let has_hlg_transfer = info.transfer_characteristics == 18;
            let has_hdr_transfer = has_pq_transfer || has_hlg_transfer;
            let has_clli = info.max_cll > 0 || info.max_pall > 0;

            println!(
                "  Wide gamut (BT.2020 or Display P3): {}",
                is_bt2020 || is_display_p3
            );
            println!("  HDR transfer function (PQ/HLG): {}", has_hdr_transfer);
            println!("  High bit depth (>8 bits): {}", info.bit_depth > 8);
            println!("  Content Light Level Info (CLLI): {}", has_clli);
            if has_clli {
                println!("    Max Content Light Level: {} cd/m²", info.max_cll);
                println!(
                    "    Max Picture Average Light Level: {} cd/m²",
                    info.max_pall
                );
            }
            println!("  Current HDR detection result: {}", info.is_hdr);

            // Gain map information
            println!("  Gain map present: {}", info.has_gain_map);
            if info.has_gain_map {
                println!("    ✓ This AVIF contains a gain map for HDR/SDR tone mapping");
                println!("    ✓ Image should be treated as HDR content");
                if let Some(ref gain_info) = info.gain_map_info {
                    println!("    Gain map parameters:");
                    println!("      Has gain map image: {}", gain_info.has_image);
                    println!(
                        "      Gamma (R,G,B): {:.3}, {:.3}, {:.3}",
                        gain_info.gamma[0], gain_info.gamma[1], gain_info.gamma[2]
                    );
                    println!(
                        "      Min values (R,G,B): {:.3}, {:.3}, {:.3}",
                        gain_info.min[0], gain_info.min[1], gain_info.min[2]
                    );
                    println!(
                        "      Max values (R,G,B): {:.3}, {:.3}, {:.3}",
                        gain_info.max[0], gain_info.max[1], gain_info.max[2]
                    );
                    println!(
                        "      Base offset (R,G,B): {:.3}, {:.3}, {:.3}",
                        gain_info.base_offset[0],
                        gain_info.base_offset[1],
                        gain_info.base_offset[2]
                    );
                    println!(
                        "      Alternate offset (R,G,B): {:.3}, {:.3}, {:.3}",
                        gain_info.alternate_offset[0],
                        gain_info.alternate_offset[1],
                        gain_info.alternate_offset[2]
                    );
                    println!(
                        "      Base HDR headroom: {:.3}",
                        gain_info.base_hdr_headroom
                    );
                    println!(
                        "      Alternate HDR headroom: {:.3}",
                        gain_info.alternate_hdr_headroom
                    );
                    println!(
                        "      Use base color space: {}",
                        gain_info.use_base_color_space
                    );
                }
            }

            // ICC profile
            if let Some(ref icc) = info.icc_profile {
                println!("  ICC profile: {} bytes", icc.len());
                if verbose
                    && let Some(name) =
                        crate::gallery::image_processing::extract_icc_profile_name(icc)
                {
                    println!("    Profile name: {}", name);
                }
            } else {
                println!("  ICC profile: None");
            }

            // EXIF data
            if let Some(ref exif) = info.exif_data {
                println!("  EXIF data: {} bytes", exif.len());
                if verbose {
                    // Try to parse and show basic EXIF info
                    match rexif::parse_buffer(exif) {
                        Ok(parsed) => {
                            println!("    EXIF entries: {}", parsed.entries.len());
                            for entry in &parsed.entries {
                                match entry.tag {
                                    rexif::ExifTag::Make => {
                                        println!("    Camera Make: {}", entry.value_more_readable)
                                    }
                                    rexif::ExifTag::Model => {
                                        println!("    Camera Model: {}", entry.value_more_readable)
                                    }
                                    rexif::ExifTag::LensModel => {
                                        println!("    Lens Model: {}", entry.value_more_readable)
                                    }
                                    rexif::ExifTag::DateTimeOriginal => {
                                        println!("    Date Taken: {}", entry.value_more_readable)
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(e) => {
                            println!("    Failed to parse EXIF: {}", e);
                        }
                    }
                }
            } else {
                println!("  EXIF data: None");
            }

            if verbose {
                println!();
                println!("=== Technical Details ===");
                println!("HDR Detection Logic:");
                println!("  Current logic: (bit_depth > 8 AND (");
                println!("    (BT.2020 primaries AND (PQ OR HLG transfer)) OR");
                println!("    (Display P3 primaries AND bit_depth >= 10) OR");
                println!("    (bit_depth > 8 AND (PQ OR HLG transfer)) OR");
                println!("    (CLLI data present)");
                println!("  )) OR (gain map present)");
                println!();

                let traditional_hdr = is_bt2020 && has_hdr_transfer;
                let wide_gamut_hdr = is_display_p3 && info.bit_depth >= 10;
                let hdr_transfer_any = info.bit_depth > 8 && has_hdr_transfer;
                let clli_hdr = has_clli;
                let gain_map_hdr = info.has_gain_map;

                println!("  Traditional HDR (BT.2020 + PQ/HLG): {}", traditional_hdr);
                println!(
                    "  Wide gamut HDR (Display P3 + ≥10-bit): {}",
                    wide_gamut_hdr
                );
                println!("  HDR transfer + high bit depth: {}", hdr_transfer_any);
                println!("  CLLI metadata present: {}", clli_hdr);
                println!("  Gain map present: {}", gain_map_hdr);
                println!("  Final result: {}", info.is_hdr);

                // Additional technical info
                if let Some(ref gain_info) = info.gain_map_info
                    && gain_info.has_image
                    && let Some(ref gm_img) = gain_info.gain_map_image
                {
                    println!();
                    println!("Gain Map Image Details:");
                    println!("  Dimensions: {}x{}", gm_img.width(), gm_img.height());
                    println!("  Format: {:?}", gm_img.color());
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to decode with custom AVIF reader: {}", e);
        }
    }

    Ok(())
}

fn format_description(img: &DynamicImage) -> &'static str {
    match img {
        DynamicImage::ImageLuma8(_) => "8-bit Grayscale",
        DynamicImage::ImageLumaA8(_) => "8-bit Grayscale with Alpha",
        DynamicImage::ImageRgb8(_) => "8-bit RGB",
        DynamicImage::ImageRgba8(_) => "8-bit RGBA",
        DynamicImage::ImageLuma16(_) => "16-bit Grayscale",
        DynamicImage::ImageLumaA16(_) => "16-bit Grayscale with Alpha",
        DynamicImage::ImageRgb16(_) => "16-bit RGB",
        DynamicImage::ImageRgba16(_) => "16-bit RGBA",
        DynamicImage::ImageRgb32F(_) => "32-bit float RGB",
        DynamicImage::ImageRgba32F(_) => "32-bit float RGBA",
        _ => "Unknown format",
    }
}

fn color_primaries_name(value: u16) -> &'static str {
    match value {
        0 => "Reserved",
        1 => "BT.709 (sRGB/Rec.709)",
        2 => "Unspecified",
        4 => "BT.470 System M",
        5 => "BT.470 System B/G",
        6 => "BT.601 (SMPTE 170M)",
        7 => "SMPTE 240M",
        8 => "Generic film",
        9 => "BT.2020 (Rec.2020)",
        10 => "XYZ (CIE 1931)",
        11 => "SMPTE RP 431-2 (DCI P3)",
        12 => "SMPTE EG 432-1 (Display P3)",
        22 => "EBU Tech 3213-E",
        _ => "Unknown",
    }
}

fn transfer_characteristics_name(value: u16) -> &'static str {
    match value {
        0 => "Reserved",
        1 => "BT.709",
        2 => "Unspecified",
        4 => "BT.470 System M (Gamma 2.2)",
        5 => "BT.470 System B/G (Gamma 2.8)",
        6 => "BT.601 (SMPTE 170M)",
        7 => "SMPTE 240M",
        8 => "Linear",
        9 => "Logarithmic (100:1)",
        10 => "Logarithmic (316.22:1)",
        11 => "IEC 61966-2-4 (xvYCC)",
        12 => "BT.1361 extended color gamut",
        13 => "sRGB/sYCC (IEC 61966-2-1)",
        14 => "BT.2020 10-bit",
        15 => "BT.2020 12-bit",
        16 => "SMPTE-2084 (PQ) **HDR**",
        17 => "SMPTE 428-1",
        18 => "ARIB STD-B67 (HLG) **HDR**",
        _ => "Unknown",
    }
}

fn matrix_coefficients_name(value: u16) -> &'static str {
    match value {
        0 => "Identity (RGB)",
        1 => "BT.709 (Rec.709)",
        2 => "Unspecified",
        4 => "FCC 73.682",
        5 => "BT.470 System B/G",
        6 => "BT.601 (SMPTE 170M)",
        7 => "SMPTE 240M",
        8 => "YCoCg",
        9 => "BT.2020 non-constant luminance",
        10 => "BT.2020 constant luminance",
        11 => "SMPTE 2085",
        12 => "Chromaticity-derived non-constant",
        13 => "Chromaticity-derived constant",
        14 => "ICtCp",
        _ => "Unknown",
    }
}
