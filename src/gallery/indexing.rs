use crate::config::ImageIndexingMode;
use std::collections::HashMap;
use std::path::Path;

/// Manages image indexing for different URL modes
#[derive(Debug)]
pub struct ImageIndexer {
    pub(crate) mode: ImageIndexingMode,
    /// Maps from image path to index (for all modes)
    path_to_index: HashMap<String, String>,
    /// Maps from index back to path (for sequence and unique_id modes)
    index_to_path: HashMap<String, String>,
}

impl ImageIndexer {
    /// Create a new indexer for a specific mode
    pub fn new(mode: ImageIndexingMode) -> Self {
        Self {
            mode,
            path_to_index: HashMap::new(),
            index_to_path: HashMap::new(),
        }
    }

    /// Build index for a set of image paths
    pub fn build_index(&mut self, paths: &[String]) {
        // Don't clear if we have entries and new paths is empty
        if !self.path_to_index.is_empty() && paths.is_empty() {
            tracing::warn!(
                "build_index called with empty paths array, but indexer has {} existing entries. Keeping existing entries.",
                self.path_to_index.len()
            );
            return;
        }

        // Clear existing mappings before building new index
        self.path_to_index.clear();
        self.index_to_path.clear();

        match self.mode {
            ImageIndexingMode::Filename => {
                // For filename mode, the index is the path itself
                for path in paths {
                    self.path_to_index.insert(path.clone(), path.clone());
                }
            }
            ImageIndexingMode::Sequence => {
                // Sort paths and assign sequential numbers
                let mut sorted_paths = paths.to_vec();
                sorted_paths.sort();

                for (index, path) in sorted_paths.iter().enumerate() {
                    let index_str = (index + 1).to_string(); // 1-based indexing
                    self.path_to_index.insert(path.clone(), index_str.clone());
                    self.index_to_path.insert(index_str, path.clone());
                }
            }
            ImageIndexingMode::UniqueId => {
                // Generate unique IDs from path hash
                for path in paths {
                    let id = generate_unique_id(path);
                    self.path_to_index.insert(path.clone(), id.clone());
                    self.index_to_path.insert(id, path.clone());
                }
            }
        }
    }

    /// Get the index/identifier for a given path
    pub fn get_index(&self, path: &str) -> Option<&str> {
        self.path_to_index.get(path).map(|s| s.as_str())
    }

    /// Get the path for a given index/identifier (for non-filename modes)
    pub fn get_path<'a>(&'a self, index: &str) -> Option<&'a str> {
        match self.mode {
            ImageIndexingMode::Filename => {
                // For filename mode, we need to return a reference from self
                // So we need to check if this index exists in our path_to_index map
                self.path_to_index
                    .iter()
                    .find(|(_path, idx)| idx.as_str() == index)
                    .map(|(path, _)| path.as_str())
            }
            _ => self.index_to_path.get(index).map(|s| s.as_str()),
        }
    }

    /// Get the display name for a path based on the indexing mode
    pub fn get_display_name(&self, path: &str) -> String {
        match self.mode {
            ImageIndexingMode::Filename => {
                // Extract just the filename from the path
                Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path)
                    .to_string()
            }
            ImageIndexingMode::Sequence | ImageIndexingMode::UniqueId => {
                // Get the index number from the path
                if let Some(index_str) = self.get_index(path) {
                    // For sequence mode, the index is already the number
                    if self.mode == ImageIndexingMode::Sequence {
                        format!("Image {}", index_str)
                    } else {
                        // For unique ID mode, find the sequence position
                        let mut sorted_paths: Vec<_> = self.path_to_index.keys().collect();
                        sorted_paths.sort();
                        if let Some(pos) = sorted_paths.iter().position(|p| p.as_str() == path) {
                            format!("Image {}", pos + 1)
                        } else {
                            format!("Image {}", index_str)
                        }
                    }
                } else {
                    // Fallback to filename if not found
                    Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path)
                        .to_string()
                }
            }
        }
    }

    /// Build index for a folder's images
    #[allow(dead_code)]
    pub fn build_folder_index(&mut self, folder_path: &str, image_paths: &[String]) {
        // For folder-specific indexing, we need to handle the folder path prefix
        let _folder_prefix = if folder_path.is_empty() {
            String::new()
        } else {
            format!("{}/", folder_path)
        };

        match self.mode {
            ImageIndexingMode::Filename => {
                // For filename mode, keep the full relative path
                for path in image_paths {
                    self.path_to_index.insert(path.clone(), path.clone());
                }
            }
            ImageIndexingMode::Sequence => {
                // Group by folder and assign sequences within each folder
                let mut folder_images: HashMap<String, Vec<String>> = HashMap::new();

                for path in image_paths {
                    let folder = Path::new(path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    folder_images
                        .entry(folder.to_string())
                        .or_default()
                        .push(path.clone());
                }

                // Sort and index each folder's images separately
                for (folder, mut images) in folder_images {
                    images.sort();
                    for (index, path) in images.iter().enumerate() {
                        let index_key = if folder.is_empty() {
                            (index + 1).to_string()
                        } else {
                            format!("{}/{}", folder, index + 1)
                        };
                        self.path_to_index.insert(path.clone(), index_key.clone());
                        self.index_to_path.insert(index_key, path.clone());
                    }
                }
            }
            ImageIndexingMode::UniqueId => {
                // Generate unique IDs maintaining folder structure
                for path in image_paths {
                    let id = generate_unique_id(path);
                    // For unique IDs within folders, we only store the ID part
                    // The handler will combine folder + ID when needed
                    self.path_to_index.insert(path.clone(), id.clone());
                    self.index_to_path.insert(id.clone(), path.clone());
                }
            }
        }
    }
}

/// Generate a unique base36 identifier from a path
fn generate_unique_id(path: &str) -> String {
    use std::hash::{Hash, Hasher};

    // Use a stable hash function (FNV-1a)
    let mut hasher = fnv::FnvHasher::default();
    path.hash(&mut hasher);
    let hash = hasher.finish();

    // Convert to base36 and take first 6 characters for a short ID
    // This gives us 36^6 = ~2.2 billion unique IDs
    let base36 = base36_encode(hash);
    base36.chars().take(6).collect()
}

/// Convert a u64 to base36 string
fn base36_encode(mut num: u64) -> String {
    const BASE36_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    if num == 0 {
        return "0".to_string();
    }

    let mut result = Vec::new();
    while num > 0 {
        result.push(BASE36_CHARS[(num % 36) as usize]);
        num /= 36;
    }

    result.reverse();
    String::from_utf8(result).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_indexing() {
        let mut indexer = ImageIndexer::new(ImageIndexingMode::Filename);
        let paths = vec![
            "folder1/image1.jpg".to_string(),
            "folder1/image2.jpg".to_string(),
            "image3.jpg".to_string(),
        ];

        indexer.build_index(&paths);

        assert_eq!(
            indexer.get_index("folder1/image1.jpg"),
            Some("folder1/image1.jpg")
        );
        assert_eq!(
            indexer.get_path("folder1/image1.jpg"),
            Some("folder1/image1.jpg")
        );
    }

    #[test]
    fn test_sequence_indexing() {
        let mut indexer = ImageIndexer::new(ImageIndexingMode::Sequence);
        let paths = vec![
            "b.jpg".to_string(),
            "a.jpg".to_string(),
            "c.jpg".to_string(),
        ];

        indexer.build_index(&paths);

        assert_eq!(indexer.get_index("a.jpg"), Some("1"));
        assert_eq!(indexer.get_index("b.jpg"), Some("2"));
        assert_eq!(indexer.get_index("c.jpg"), Some("3"));

        assert_eq!(indexer.get_path("1"), Some("a.jpg"));
        assert_eq!(indexer.get_path("2"), Some("b.jpg"));
    }

    #[test]
    fn test_unique_id_indexing() {
        let mut indexer = ImageIndexer::new(ImageIndexingMode::UniqueId);
        let paths = vec!["test.jpg".to_string()];

        indexer.build_index(&paths);

        let id = indexer.get_index("test.jpg").unwrap();
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));

        assert_eq!(indexer.get_path(id), Some("test.jpg"));
    }

    #[test]
    fn test_base36_encoding() {
        assert_eq!(base36_encode(0), "0");
        assert_eq!(base36_encode(35), "z");
        assert_eq!(base36_encode(36), "10");
        assert_eq!(base36_encode(1296), "100");
    }

    #[test]
    fn test_display_names() {
        // Test filename mode
        let mut indexer = ImageIndexer::new(ImageIndexingMode::Filename);
        let paths = vec!["folder/image.jpg".to_string(), "photo.png".to_string()];
        indexer.build_index(&paths);

        assert_eq!(indexer.get_display_name("folder/image.jpg"), "image.jpg");
        assert_eq!(indexer.get_display_name("photo.png"), "photo.png");

        // Test sequence mode
        let mut indexer = ImageIndexer::new(ImageIndexingMode::Sequence);
        indexer.build_index(&paths);

        assert_eq!(indexer.get_display_name("folder/image.jpg"), "Image 1");
        assert_eq!(indexer.get_display_name("photo.png"), "Image 2");

        // Test unique ID mode
        let mut indexer = ImageIndexer::new(ImageIndexingMode::UniqueId);
        indexer.build_index(&paths);

        assert_eq!(indexer.get_display_name("folder/image.jpg"), "Image 1");
        assert_eq!(indexer.get_display_name("photo.png"), "Image 2");
    }
}
