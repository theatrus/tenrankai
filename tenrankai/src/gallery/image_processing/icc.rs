use tracing::debug;

/// Extract display name from ICC profile data
pub fn extract_icc_profile_name(icc_data: &[u8]) -> Option<String> {
    // ICC profile structure:
    // - Header: 128 bytes
    // - Tag table: starts at byte 128
    // - Tag data: after tag table

    if icc_data.len() < 132 {
        return None; // Too small to contain header + tag count
    }

    // Read tag count at offset 128
    let tag_count =
        u32::from_be_bytes([icc_data[128], icc_data[129], icc_data[130], icc_data[131]]) as usize;

    let tag_table_start = 132;
    let tag_entry_size = 12;

    // Look for 'desc' tag (description)
    for i in 0..tag_count {
        let tag_start = tag_table_start + (i * tag_entry_size);
        if tag_start + 12 > icc_data.len() {
            break;
        }

        let tag_signature = &icc_data[tag_start..tag_start + 4];
        if tag_signature == b"desc" {
            // Found description tag
            let offset = u32::from_be_bytes([
                icc_data[tag_start + 4],
                icc_data[tag_start + 5],
                icc_data[tag_start + 6],
                icc_data[tag_start + 7],
            ]) as usize;
            let size = u32::from_be_bytes([
                icc_data[tag_start + 8],
                icc_data[tag_start + 9],
                icc_data[tag_start + 10],
                icc_data[tag_start + 11],
            ]) as usize;

            if offset + size > icc_data.len() || size < 12 {
                continue;
            }

            // Description tag data structure:
            // - Type signature: 4 bytes (should be 'desc')
            // - Reserved: 4 bytes
            // - ASCII count: 4 bytes
            // - ASCII string: variable length

            let desc_data = &icc_data[offset..offset + size];
            if desc_data.len() < 12 || &desc_data[0..4] != b"desc" {
                continue;
            }

            let ascii_count =
                u32::from_be_bytes([desc_data[8], desc_data[9], desc_data[10], desc_data[11]])
                    as usize;

            if ascii_count > 0 && 12 + ascii_count <= desc_data.len() {
                let ascii_data = &desc_data[12..12 + ascii_count];
                // Remove null terminator if present
                let ascii_str = if ascii_data.last() == Some(&0) {
                    &ascii_data[..ascii_data.len() - 1]
                } else {
                    ascii_data
                };

                if let Ok(name) = std::str::from_utf8(ascii_str) {
                    let trimmed_name = name.trim();
                    if !trimmed_name.is_empty() {
                        debug!("Extracted ICC profile name: {}", trimmed_name);
                        return Some(trimmed_name.to_string());
                    }
                }
            }
        }
    }

    // If no desc tag found, try to identify common profiles by their characteristics
    identify_common_profile(icc_data)
}

/// Identify common ICC profiles by their size and other characteristics
fn identify_common_profile(icc_data: &[u8]) -> Option<String> {
    match icc_data.len() {
        548 => {
            // Common size for Display P3
            debug!("ICC profile size matches Display P3 (548 bytes)");
            Some("Display P3".to_string())
        }
        3144 | 3145 => {
            // Common sizes for sRGB
            debug!("ICC profile size matches sRGB ({} bytes)", icc_data.len());
            Some("sRGB".to_string())
        }
        560 => {
            // Common size for Adobe RGB
            debug!("ICC profile size matches Adobe RGB (560 bytes)");
            Some("Adobe RGB (1998)".to_string())
        }
        _ => {
            debug!("Unknown ICC profile size: {} bytes", icc_data.len());
            None
        }
    }
}
