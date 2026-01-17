use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::SystemTime;

use super::types::{ImageGroup, ImageVersion, RawFileInfo};

/// Known RAW file extensions (lowercase)
pub const RAW_EXTENSIONS: &[&str] = &[
    "dng", "arw", "crw", "cr2", "cr3", "nef", "orf", "rw2", "pef", "raf", "srw", "raw",
];

/// Displayable image extensions (lowercase)
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

/// Regex for version suffix pattern: _v followed by digits, case insensitive
static VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)_v(\d+)$").expect("Invalid version regex"));

/// Check if an extension is a RAW format
pub fn is_raw_extension(ext: &str) -> bool {
    RAW_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if an extension is a displayable image format
pub fn is_image_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if a folder name should be hidden (starts with __)
pub fn is_hidden_folder(name: &str) -> bool {
    name.starts_with("__")
}

/// Check if a path contains any hidden folder component (starts with __)
pub fn path_contains_hidden_folder(path: &str) -> bool {
    path.split('/').any(|component| component.starts_with("__"))
}

/// Extract the file extension from a filename (lowercase)
pub fn get_extension(filename: &str) -> Option<&str> {
    filename.rsplit('.').next().filter(|ext| ext.len() < filename.len())
}

/// Extract the base name without extension
pub fn get_stem(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(pos) if pos > 0 => &filename[..pos],
        _ => filename,
    }
}

/// Extract base name without version suffix
/// e.g., "IMG_0001_v2" -> "IMG_0001"
/// e.g., "IMG_0001" -> "IMG_0001"
pub fn extract_base_name(filename: &str) -> String {
    let stem = get_stem(filename);
    VERSION_REGEX.replace(stem, "").to_string()
}

/// Extract version number from filename
/// e.g., "IMG_0001_v2.jpg" -> Some(2)
/// e.g., "IMG_0001.jpg" -> None
pub fn extract_version_number(filename: &str) -> Option<u32> {
    let stem = get_stem(filename);
    VERSION_REGEX
        .captures(stem)
        .and_then(|caps: regex::Captures<'_>| {
            caps.get(1)
                .and_then(|m: regex::Match<'_>| m.as_str().parse::<u32>().ok())
        })
}

/// Compute a short hash of a path for cache key generation
pub fn compute_path_hash(path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    // Convert to base36 for a shorter string (6 chars = ~2 billion combinations)
    let full_hash = radix_fmt(hash, 36);
    // Take last 6 chars (most variable bits) and pad with zeros if needed
    let start = full_hash.len().saturating_sub(6);
    format!("{:0>6}", &full_hash[start..])
}

/// Format a number in a given radix (base)
fn radix_fmt(mut n: u64, radix: u64) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::new();
    if n == 0 {
        return "0".to_string();
    }
    while n > 0 {
        result.push(CHARS[(n % radix) as usize] as char);
        n /= radix;
    }
    result.into_iter().rev().collect()
}

/// Information about a file for grouping
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub modification_date: Option<SystemTime>,
    pub file_size: u64,
    pub is_raw: bool,
    pub is_image: bool,
    pub base_name: String,
    pub version_number: Option<u32>,
    pub in_versions_folder: bool,
}

impl FileEntry {
    pub fn from_path(path: &str, modification_date: Option<SystemTime>, file_size: u64) -> Option<Self> {
        let filename = path.rsplit('/').next().unwrap_or(path);
        let extension = get_extension(filename)?.to_lowercase();
        let is_raw = is_raw_extension(&extension);
        let is_image = is_image_extension(&extension);

        if !is_raw && !is_image {
            return None;
        }

        let base_name = extract_base_name(filename);
        let version_number = extract_version_number(filename);

        // Check if this file is in a __versions folder
        let in_versions_folder = path.contains("/__versions/");

        Some(FileEntry {
            path: path.to_string(),
            filename: filename.to_string(),
            extension,
            modification_date,
            file_size,
            is_raw,
            is_image,
            base_name,
            version_number,
            in_versions_folder,
        })
    }
}

/// Comparison key for determining primary image
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PrimaryKey {
    /// Higher version numbers come first (reversed for Ord)
    version_priority: std::cmp::Reverse<Option<u32>>,
    /// Newer modification dates come first
    mod_time: std::cmp::Reverse<Option<SystemTime>>,
}

impl PrimaryKey {
    fn from_entry(entry: &FileEntry) -> Self {
        PrimaryKey {
            version_priority: std::cmp::Reverse(entry.version_number),
            mod_time: std::cmp::Reverse(entry.modification_date),
        }
    }
}

/// Group files by base name and create ImageGroup structures
///
/// # Arguments
/// * `entries` - Iterator of (path, modification_date, file_size) tuples
/// * `url_builder` - Function to build URL identifiers from paths
/// * `thumbnail_builder` - Function to build thumbnail URLs from URL identifiers
pub(crate) fn group_files<'a, I, F, T>(
    entries: I,
    url_builder: F,
    thumbnail_builder: T,
) -> Vec<ImageGroup>
where
    I: Iterator<Item = (&'a str, Option<SystemTime>, u64)>,
    F: Fn(&str) -> String,
    T: Fn(&str) -> String,
{
    // Parse all entries into FileEntry structs
    let file_entries: Vec<FileEntry> = entries
        .filter_map(|(path, mod_date, size)| FileEntry::from_path(path, mod_date, size))
        .collect();

    // Group by base name (within the same parent folder context)
    let mut groups: HashMap<String, Vec<FileEntry>> = HashMap::new();
    for entry in file_entries {
        // For files in __versions folder, extract parent folder's base context
        let group_key = if entry.in_versions_folder {
            // Group with parent folder files
            entry.base_name.clone()
        } else {
            entry.base_name.clone()
        };
        groups.entry(group_key).or_default().push(entry);
    }

    // Build ImageGroup for each base name
    let mut result = Vec::new();
    for (base_name, entries) in groups {
        // Separate images and RAW files
        let (images, raws): (Vec<_>, Vec<_>) =
            entries.iter().cloned().partition(|e| e.is_image);

        if images.is_empty() {
            // No displayable images, skip this group
            continue;
        }

        // Determine primary image (highest version, or newest by mod time)
        let mut images = images;
        images.sort_by_key(PrimaryKey::from_entry);
        let primary = images.remove(0);

        // Build version list from remaining images (sorted oldest-first)
        images.reverse(); // Now oldest first
        let versions: Vec<ImageVersion> = images
            .iter()
            .map(|img| {
                let url_id = url_builder(&img.path);
                ImageVersion {
                    path: img.path.clone(),
                    version_number: img.version_number,
                    modification_date: img.modification_date,
                    url_id: url_id.clone(),
                    thumbnail_url: thumbnail_builder(&url_id),
                }
            })
            .collect();

        // Build RAW file list
        let raw_files: Vec<RawFileInfo> = raws
            .iter()
            .map(|raw| RawFileInfo {
                path: raw.path.clone(),
                format: raw.extension.clone(),
                file_size: raw.file_size,
            })
            .collect();

        // Collect all image paths for filtering
        let mut all_image_paths: Vec<String> = vec![primary.path.clone()];
        all_image_paths.extend(images.iter().map(|i| i.path.clone()));

        // Find the latest modification time across all files
        let group_modified = entries
            .iter()
            .filter_map(|e| e.modification_date)
            .max();

        result.push(ImageGroup {
            primary_path: primary.path.clone(),
            all_image_paths,
            raw_files,
            versions,
            base_name,
            primary_hash: compute_path_hash(&primary.path),
            group_modified,
        });
    }

    // Sort groups by primary path for consistent ordering
    result.sort_by(|a, b| a.primary_path.cmp(&b.primary_path));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_base_name_simple() {
        assert_eq!(extract_base_name("IMG_0001.jpg"), "IMG_0001");
        assert_eq!(extract_base_name("photo.png"), "photo");
    }

    #[test]
    fn test_extract_base_name_with_version() {
        assert_eq!(extract_base_name("IMG_0001_v1.jpg"), "IMG_0001");
        assert_eq!(extract_base_name("IMG_0001_v2.jpg"), "IMG_0001");
        assert_eq!(extract_base_name("IMG_0001_V10.jpg"), "IMG_0001");
    }

    #[test]
    fn test_extract_version_number() {
        assert_eq!(extract_version_number("IMG_0001.jpg"), None);
        assert_eq!(extract_version_number("IMG_0001_v1.jpg"), Some(1));
        assert_eq!(extract_version_number("IMG_0001_v2.jpg"), Some(2));
        assert_eq!(extract_version_number("IMG_0001_V10.jpg"), Some(10));
        assert_eq!(extract_version_number("photo_v99.png"), Some(99));
    }

    #[test]
    fn test_is_raw_extension() {
        assert!(is_raw_extension("dng"));
        assert!(is_raw_extension("DNG"));
        assert!(is_raw_extension("arw"));
        assert!(is_raw_extension("CR2"));
        assert!(!is_raw_extension("jpg"));
        assert!(!is_raw_extension("png"));
    }

    #[test]
    fn test_is_hidden_folder() {
        assert!(is_hidden_folder("__versions"));
        assert!(is_hidden_folder("__hidden"));
        assert!(!is_hidden_folder("_folder.md"));
        assert!(!is_hidden_folder("normal"));
    }

    #[test]
    fn test_path_contains_hidden_folder() {
        assert!(path_contains_hidden_folder("photos/__versions/img.jpg"));
        assert!(path_contains_hidden_folder("__hidden/secret.jpg"));
        assert!(!path_contains_hidden_folder("photos/vacation/img.jpg"));
        assert!(!path_contains_hidden_folder("_folder.md"));
    }

    #[test]
    fn test_compute_path_hash() {
        let hash1 = compute_path_hash("photos/IMG_0001.jpg");
        let hash2 = compute_path_hash("photos/IMG_0002.jpg");
        assert_ne!(hash1, hash2);
        assert_eq!(hash1.len(), 6);
    }

    #[test]
    fn test_group_files_simple() {
        let entries = vec![
            ("photos/IMG_0001.jpg", None, 1000u64),
            ("photos/IMG_0001.dng", None, 5000u64),
        ];

        let groups = group_files(
            entries.iter().map(|(p, m, s)| (*p, *m, *s)),
            |p| p.to_string(),
            |u| format!("{}/thumbnail", u),
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].primary_path, "photos/IMG_0001.jpg");
        assert_eq!(groups[0].raw_files.len(), 1);
        assert_eq!(groups[0].raw_files[0].format, "dng");
        assert!(groups[0].versions.is_empty());
    }

    #[test]
    fn test_group_files_with_versions() {
        let entries = vec![
            ("photos/IMG_0001.jpg", None, 1000u64),
            ("photos/IMG_0001_v1.jpg", None, 1100u64),
            ("photos/IMG_0001_v2.jpg", None, 1200u64),
        ];

        let groups = group_files(
            entries.iter().map(|(p, m, s)| (*p, *m, *s)),
            |p| p.to_string(),
            |u| format!("{}/thumbnail", u),
        );

        assert_eq!(groups.len(), 1);
        // v2 should be primary (highest version)
        assert_eq!(groups[0].primary_path, "photos/IMG_0001_v2.jpg");
        // versions should be sorted oldest-first (base, then v1)
        assert_eq!(groups[0].versions.len(), 2);
        assert_eq!(groups[0].versions[0].path, "photos/IMG_0001.jpg");
        assert_eq!(groups[0].versions[1].path, "photos/IMG_0001_v1.jpg");
    }

    #[test]
    fn test_group_files_versions_folder() {
        let now = SystemTime::now();
        let older = now - std::time::Duration::from_secs(3600);

        let entries = vec![
            ("photos/IMG_0001.jpg", Some(older), 1000u64),
            ("photos/__versions/IMG_0001.jpg", Some(now), 1100u64),
        ];

        let groups = group_files(
            entries.iter().map(|(p, m, s)| (*p, *m, *s)),
            |p| p.to_string(),
            |u| format!("{}/thumbnail", u),
        );

        assert_eq!(groups.len(), 1);
        // __versions/IMG_0001.jpg should be primary (newer mod time)
        assert_eq!(groups[0].primary_path, "photos/__versions/IMG_0001.jpg");
        assert_eq!(groups[0].versions.len(), 1);
        assert_eq!(groups[0].versions[0].path, "photos/IMG_0001.jpg");
    }
}
