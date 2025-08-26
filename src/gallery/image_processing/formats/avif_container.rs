use super::avif::GainMapInfo;
use std::path::Path;
use tracing::debug;

/// Extract ICC profile from an AVIF file by parsing container structure
pub fn extract_icc_profile(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    extract_icc_profile_from_container(&data)
}

/// ICC profile extraction by parsing AVIF container
pub fn extract_icc_profile_from_container(data: &[u8]) -> Option<Vec<u8>> {
    // Parse AVIF boxes to find colr box
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let box_size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > data.len() {
            break;
        }

        let box_type = &data[pos + 4..pos + 8];

        if box_type == b"meta" && pos + 12 < data.len() {
            // Search within meta box for colr
            return find_colr_in_meta(&data[pos + 12..pos + box_size]);
        }

        pos += box_size;
    }

    None
}

/// Find colr box within meta box
fn find_colr_in_meta(meta_data: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;

    while pos + 8 <= meta_data.len() {
        let box_size = u32::from_be_bytes([
            meta_data[pos],
            meta_data[pos + 1],
            meta_data[pos + 2],
            meta_data[pos + 3],
        ]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > meta_data.len() {
            break;
        }

        let box_type = &meta_data[pos + 4..pos + 8];

        if box_type == b"colr" && box_size > 12 {
            let colr_data = &meta_data[pos + 8..pos + box_size];

            if colr_data.len() > 4 && &colr_data[0..4] == b"prof" {
                // ICC profile found
                return Some(colr_data[4..].to_vec());
            }
        }

        // Recurse into iprp (item properties) box
        if box_type == b"iprp"
            && box_size > 8
            && let Some(icc) = find_colr_in_meta(&meta_data[pos + 8..pos + box_size])
        {
            return Some(icc);
        }

        // Recurse into ipco (item property container) box
        if box_type == b"ipco"
            && box_size > 8
            && let Some(icc) = find_colr_in_meta(&meta_data[pos + 8..pos + box_size])
        {
            return Some(icc);
        }

        pos += box_size;
    }

    None
}

/// Extract dimensions from AVIF file without full decode
pub fn extract_dimensions(path: &Path) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    extract_dimensions_from_container(&data)
}

/// Extract dimensions by parsing AVIF container directly
pub fn extract_dimensions_from_container(data: &[u8]) -> Option<(u32, u32)> {
    // Look for ispe (image spatial extents) box
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let box_size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > data.len() {
            break;
        }

        let box_type = &data[pos + 4..pos + 8];

        if box_type == b"meta" && pos + 12 < data.len() {
            // Search within meta box for ispe
            if let Some((w, h)) = find_ispe_in_meta(&data[pos + 12..pos + box_size]) {
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
        let box_size = u32::from_be_bytes([
            meta_data[pos],
            meta_data[pos + 1],
            meta_data[pos + 2],
            meta_data[pos + 3],
        ]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > meta_data.len() {
            break;
        }

        let box_type = &meta_data[pos + 4..pos + 8];

        // ispe box contains image dimensions
        if box_type == b"ispe" && pos + 20 <= meta_data.len() {
            let width = u32::from_be_bytes([
                meta_data[pos + 12],
                meta_data[pos + 13],
                meta_data[pos + 14],
                meta_data[pos + 15],
            ]);
            let height = u32::from_be_bytes([
                meta_data[pos + 16],
                meta_data[pos + 17],
                meta_data[pos + 18],
                meta_data[pos + 19],
            ]);
            return Some((width, height));
        }

        // Recurse into iprp (item properties) box
        if box_type == b"iprp"
            && box_size > 8
            && let Some(dims) = find_ispe_in_meta(&meta_data[pos + 8..pos + box_size])
        {
            return Some(dims);
        }

        // Recurse into ipco (item property container) box
        if box_type == b"ipco"
            && box_size > 8
            && let Some(dims) = find_ispe_in_meta(&meta_data[pos + 8..pos + box_size])
        {
            return Some(dims);
        }

        pos += box_size;
    }

    None
}

/// Detect gain map presence in AVIF container by parsing auxiliary items
/// This is a container-level detection since libavif 1.0.4 doesn't have gain map API
pub fn detect_gain_map_in_container(data: &[u8]) -> (bool, Option<GainMapInfo>) {
    debug!(
        "detect_gain_map_in_container called with {} bytes",
        data.len()
    );

    // Quick search for 'tmap' signature anywhere in the file as a first test
    let tmap_signature = b"tmap";
    let has_tmap_anywhere = data.windows(4).any(|window| window == tmap_signature);
    debug!("Raw tmap search result: {}", has_tmap_anywhere);

    if has_tmap_anywhere {
        debug!("Found tmap signature in file - returning true immediately");
        let gain_map_info = Some(GainMapInfo {
            has_image: false,       // Can't decode actual image without full API
            gamma: [1.0, 1.0, 1.0], // Default values
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            base_offset: [0.0, 0.0, 0.0],
            alternate_offset: [0.0, 0.0, 0.0],
            base_hdr_headroom: 1.0,
            alternate_hdr_headroom: 1.0,
            use_base_color_space: true,
            gain_map_image: None,   // Can't extract without libavif API
        });
        return (true, gain_map_info);
    }

    // Look for auxiliary items that might be gain maps
    // Gain maps are typically stored as auxiliary items with specific URNs
    let mut pos = 0;
    let mut has_gain_map = false;

    while pos + 8 <= data.len() {
        let box_size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > data.len() {
            break;
        }

        let box_type = &data[pos + 4..pos + 8];
        debug!(
            "Found box type: {:?} at position {}, size {}",
            std::str::from_utf8(box_type).unwrap_or("invalid"),
            pos,
            box_size
        );

        // Look for meta box which contains item references
        if box_type == b"meta" && pos + 12 < data.len() {
            debug!("Processing meta box at position {}", pos);
            if let Some(gain_map_detected) =
                detect_gain_map_in_meta(&data[pos + 12..pos + box_size])
            {
                has_gain_map = gain_map_detected;
                break;
            }
        }

        // Also check for tmap directly at top level (in case it's not in meta)
        if box_type == b"tmap" {
            debug!("Detected tone mapping (tmap) box at top level - gain map present");
            has_gain_map = true;
            break;
        }

        pos += box_size;
    }

    // If we detected a gain map, create basic gain map info
    // This is a container-level detection - actual values come from libavif API when available
    let gain_map_info = if has_gain_map {
        Some(GainMapInfo {
            has_image: false,       // Can't decode actual image without full API
            gamma: [1.0, 1.0, 1.0], // Default values
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            base_offset: [0.0, 0.0, 0.0],
            alternate_offset: [0.0, 0.0, 0.0],
            base_hdr_headroom: 1.0,
            alternate_hdr_headroom: 1.0,
            use_base_color_space: true,
            gain_map_image: None,   // Can't extract without libavif API
        })
    } else {
        None
    };

    debug!("Gain map detection: has_gain_map={}", has_gain_map);
    (has_gain_map, gain_map_info)
}

/// Search for gain map indicators within meta box structure
fn detect_gain_map_in_meta(meta_data: &[u8]) -> Option<bool> {
    let mut pos = 0;

    while pos + 8 <= meta_data.len() {
        let box_size = u32::from_be_bytes([
            meta_data[pos],
            meta_data[pos + 1],
            meta_data[pos + 2],
            meta_data[pos + 3],
        ]) as usize;

        if box_size == 0 || box_size == 1 || pos + box_size > meta_data.len() {
            break;
        }

        let box_type = &meta_data[pos + 4..pos + 8];
        debug!(
            "Meta box - Found box type: {:?} at position {}, size {}",
            std::str::from_utf8(box_type).unwrap_or("invalid"),
            pos,
            box_size
        );

        // Look for auxiliary type properties that might indicate gain maps
        if box_type == b"auxC" && box_size > 8 {
            // auxC box contains auxiliary type URN
            if let Some(aux_type) = extract_aux_type_from_auxc(&meta_data[pos + 8..pos + box_size])
            {
                // Check for gain map URNs (these are hypothetical as spec isn't finalized)
                if aux_type.contains("gainmap")
                    || aux_type.contains("tonemap")
                    || aux_type.contains("hdr_reconstruction")
                {
                    debug!("Detected potential gain map auxiliary type: {}", aux_type);
                    return Some(true);
                }
            }
        }

        // Look for tone mapping (tmap) boxes
        if box_type == b"tmap" {
            debug!("Detected tone mapping (tmap) box - likely gain map present");
            return Some(true);
        }

        // Recursively search in property containers
        if (box_type == b"iprp" || box_type == b"ipco" || box_type == b"iref")
            && box_size > 8
            && let Some(result) = detect_gain_map_in_meta(&meta_data[pos + 8..pos + box_size])
        {
            return Some(result);
        }

        pos += box_size;
    }

    None
}

/// Extract auxiliary type URN from auxC box
fn extract_aux_type_from_auxc(auxc_data: &[u8]) -> Option<String> {
    if auxc_data.is_empty() {
        return None;
    }

    // auxC box format: [version(1)] [flags(3)] [aux_type(null-terminated string)]
    let version_and_flags = &auxc_data[0..4];
    if version_and_flags[0] != 0 {
        // Only version 0 is supported
        return None;
    }

    let aux_type_start = 4;
    if aux_type_start >= auxc_data.len() {
        return None;
    }

    // Find null terminator
    let aux_type_data = &auxc_data[aux_type_start..];
    if let Some(null_pos) = aux_type_data.iter().position(|&b| b == 0)
        && let Ok(aux_type) = std::str::from_utf8(&aux_type_data[..null_pos])
    {
        return Some(aux_type.to_string());
    }

    None
}