use std::collections::HashMap;
use std::hash::Hasher;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use thiserror::Error;
use tracing::warn;

use crate::gallery::SharedGallery;

const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

const GALLERY_PAYLOAD_WIDTH: usize = 3;
const FOLDER_PAYLOAD_WIDTH: usize = 5;
const IMAGE_PAYLOAD_WIDTH: usize = 6;

const GALLERY_MODULUS: u64 = 62 * 62 * 62; // 238,328
const FOLDER_MODULUS: u64 = 62 * 62 * 62 * 62 * 62; // 916,132,832
const IMAGE_MODULUS: u64 = 62 * 62 * 62 * 62 * 62 * 62; // 56,800,235,584

#[derive(Debug, Error)]
pub enum ShortUrlError {
    #[error("invalid shortcode format")]
    InvalidFormat,
    #[error("unknown shortcode type prefix: {0}")]
    UnknownType(char),
    #[error("shortcode not found")]
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortUrlTarget {
    Gallery {
        gallery_name: String,
    },
    Folder {
        gallery_name: String,
        folder_path: String,
    },
    Image {
        gallery_name: String,
        source_path: String,
    },
}

#[derive(Debug, Default)]
pub struct ShortcodeIndex {
    galleries: HashMap<u64, String>,
    folders: HashMap<u64, (String, String)>,
    images: HashMap<u64, (String, String)>,
    gallery_to_hash: HashMap<String, u64>,
    folder_to_hash: HashMap<(String, String), u64>,
    image_to_hash: HashMap<(String, String), u64>,
}

impl ShortcodeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(galleries: &HashMap<String, SharedGallery>) -> Self {
        let mut index = Self::new();

        for (name, gallery) in galleries {
            let g_hash = shortcode_hash(&[name.as_bytes()], GALLERY_MODULUS);
            if let Some(existing) = index.galleries.get(&g_hash) {
                warn!(
                    "Short URL collision: gallery '{}' collides with '{}' (hash {})",
                    name, existing, g_hash
                );
            }
            index.galleries.insert(g_hash, name.clone());
            index.gallery_to_hash.insert(name.clone(), g_hash);

            let folder_cache = gallery.folder_cache.read_all().await;
            for folder_path in folder_cache.keys() {
                if folder_path.is_empty() {
                    continue;
                }
                let d_hash =
                    shortcode_hash(&[name.as_bytes(), folder_path.as_bytes()], FOLDER_MODULUS);
                if let Some((existing_gallery, existing_folder)) = index.folders.get(&d_hash)
                    && (existing_gallery != name || existing_folder != folder_path)
                {
                    warn!(
                        "Short URL collision: folder '{}/{}' collides with '{}/{}' (hash {})",
                        name, folder_path, existing_gallery, existing_folder, d_hash
                    );
                }
                index
                    .folders
                    .insert(d_hash, (name.clone(), folder_path.clone()));
                index
                    .folder_to_hash
                    .insert((name.clone(), folder_path.clone()), d_hash);
            }

            let indexer = gallery.image_indexer.read().await;
            for source_path in indexer.all_source_paths() {
                let i_hash =
                    shortcode_hash(&[name.as_bytes(), source_path.as_bytes()], IMAGE_MODULUS);
                if let Some((existing_gallery, existing_path)) = index.images.get(&i_hash)
                    && (existing_gallery != name || existing_path != source_path)
                {
                    warn!(
                        "Short URL collision: image '{}/{}' collides with '{}/{}' (hash {})",
                        name, source_path, existing_gallery, existing_path, i_hash
                    );
                }
                index
                    .images
                    .insert(i_hash, (name.clone(), source_path.to_string()));
                index
                    .image_to_hash
                    .insert((name.clone(), source_path.to_string()), i_hash);
            }
        }

        index
    }

    pub fn resolve(&self, shortcode: &str) -> Result<ShortUrlTarget, ShortUrlError> {
        if shortcode.len() < 2 {
            return Err(ShortUrlError::InvalidFormat);
        }

        let type_char = shortcode.as_bytes()[0] as char;
        let payload = &shortcode[1..];

        match type_char {
            'g' if payload.len() == GALLERY_PAYLOAD_WIDTH => {
                let hash = base62_decode(payload)?;
                let gallery_name = self.galleries.get(&hash).ok_or(ShortUrlError::NotFound)?;
                Ok(ShortUrlTarget::Gallery {
                    gallery_name: gallery_name.clone(),
                })
            }
            'd' if payload.len() == FOLDER_PAYLOAD_WIDTH => {
                let hash = base62_decode(payload)?;
                let (gallery_name, folder_path) =
                    self.folders.get(&hash).ok_or(ShortUrlError::NotFound)?;
                Ok(ShortUrlTarget::Folder {
                    gallery_name: gallery_name.clone(),
                    folder_path: folder_path.clone(),
                })
            }
            'i' if payload.len() == IMAGE_PAYLOAD_WIDTH => {
                let hash = base62_decode(payload)?;
                let (gallery_name, source_path) =
                    self.images.get(&hash).ok_or(ShortUrlError::NotFound)?;
                Ok(ShortUrlTarget::Image {
                    gallery_name: gallery_name.clone(),
                    source_path: source_path.clone(),
                })
            }
            'g' | 'd' | 'i' => Err(ShortUrlError::InvalidFormat),
            _ => Err(ShortUrlError::UnknownType(type_char)),
        }
    }

    pub fn encode_gallery(&self, gallery_name: &str) -> Option<String> {
        self.gallery_to_hash
            .get(gallery_name)
            .map(|hash| format!("g{}", base62_encode(*hash, GALLERY_PAYLOAD_WIDTH)))
    }

    pub fn encode_folder(&self, gallery_name: &str, folder_path: &str) -> Option<String> {
        self.folder_to_hash
            .get(&(gallery_name.to_string(), folder_path.to_string()))
            .map(|hash| format!("d{}", base62_encode(*hash, FOLDER_PAYLOAD_WIDTH)))
    }

    pub fn encode_image(&self, gallery_name: &str, source_path: &str) -> Option<String> {
        self.image_to_hash
            .get(&(gallery_name.to_string(), source_path.to_string()))
            .map(|hash| format!("i{}", base62_encode(*hash, IMAGE_PAYLOAD_WIDTH)))
    }
}

fn shortcode_hash(parts: &[&[u8]], modulus: u64) -> u64 {
    let mut hasher = fnv::FnvHasher::default();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hasher.write_u8(0x00);
        }
        hasher.write(part);
    }
    hasher.finish() % modulus
}

fn base62_encode(mut value: u64, width: usize) -> String {
    let mut result = vec![b'0'; width];
    for i in (0..width).rev() {
        result[i] = BASE62_CHARS[(value % 62) as usize];
        value /= 62;
    }
    String::from_utf8(result).unwrap()
}

fn base62_decode(s: &str) -> Result<u64, ShortUrlError> {
    let mut value: u64 = 0;
    for &byte in s.as_bytes() {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u64,
            b'A'..=b'Z' => (byte - b'A' + 10) as u64,
            b'a'..=b'z' => (byte - b'a' + 36) as u64,
            _ => return Err(ShortUrlError::InvalidFormat),
        };
        value = value
            .checked_mul(62)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ShortUrlError::InvalidFormat)?;
    }
    Ok(value)
}

pub async fn short_url_handler(
    crate::site::ResolvedState(app_state): crate::site::ResolvedState,
    Path(shortcode): Path<String>,
) -> Response {
    let shortcode_index = app_state.site.shortcode_index();

    let index = shortcode_index.read().await;
    let target = match index.resolve(&shortcode) {
        Ok(t) => t,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Short URL not found").into_response();
        }
    };

    let galleries = app_state.galleries();
    let redirect_url = match &target {
        ShortUrlTarget::Gallery { gallery_name } => {
            let gallery = match galleries.get(gallery_name) {
                Some(g) => g,
                None => {
                    return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
                }
            };
            format!("{}/", gallery.get_config().url_prefix)
        }
        ShortUrlTarget::Folder {
            gallery_name,
            folder_path,
        } => {
            let gallery = match galleries.get(gallery_name) {
                Some(g) => g,
                None => {
                    return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
                }
            };
            format!("{}/{}/", gallery.get_config().url_prefix, folder_path)
        }
        ShortUrlTarget::Image {
            gallery_name,
            source_path,
        } => {
            let gallery = match galleries.get(gallery_name) {
                Some(g) => g,
                None => {
                    return (StatusCode::NOT_FOUND, "Gallery not found").into_response();
                }
            };
            let url_id = gallery.build_url_identifier(source_path).await;
            format!("{}/detail/{}", gallery.get_config().url_prefix, url_id)
        }
    };

    Redirect::temporary(&redirect_url).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base62_encode_decode_roundtrip() {
        for value in [0, 1, 61, 62, 100, 238_327, 916_132_831, 1_000_000] {
            let encoded = base62_encode(value, 6);
            let decoded = base62_decode(&encoded).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for {}", value);
        }
    }

    #[test]
    fn test_base62_encode_width() {
        assert_eq!(base62_encode(0, 3).len(), 3);
        assert_eq!(base62_encode(0, 5).len(), 5);
        assert_eq!(base62_encode(0, 6).len(), 6);
        assert_eq!(base62_encode(238_327, 3).len(), 3);
    }

    #[test]
    fn test_base62_encode_values() {
        assert_eq!(base62_encode(0, 1), "0");
        assert_eq!(base62_encode(9, 1), "9");
        assert_eq!(base62_encode(10, 1), "A");
        assert_eq!(base62_encode(35, 1), "Z");
        assert_eq!(base62_encode(36, 1), "a");
        assert_eq!(base62_encode(61, 1), "z");
        assert_eq!(base62_encode(62, 2), "10");
    }

    #[test]
    fn test_base62_decode_invalid() {
        assert!(base62_decode("!").is_err());
        assert!(base62_decode("@").is_err());
        assert!(base62_decode(" ").is_err());
    }

    #[test]
    fn test_shortcode_hash_deterministic() {
        let h1 = shortcode_hash(&[b"main"], GALLERY_MODULUS);
        let h2 = shortcode_hash(&[b"main"], GALLERY_MODULUS);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_shortcode_hash_different_inputs() {
        let h1 = shortcode_hash(&[b"main"], GALLERY_MODULUS);
        let h2 = shortcode_hash(&[b"wedding"], GALLERY_MODULUS);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_shortcode_hash_separator_prevents_ambiguity() {
        let h1 = shortcode_hash(&[b"main", b"photos"], IMAGE_MODULUS);
        let h2 = shortcode_hash(&[b"mainp", b"hotos"], IMAGE_MODULUS);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_shortcode_hash_within_modulus() {
        let h = shortcode_hash(&[b"test"], GALLERY_MODULUS);
        assert!(h < GALLERY_MODULUS);

        let h = shortcode_hash(&[b"test", b"path"], FOLDER_MODULUS);
        assert!(h < FOLDER_MODULUS);

        let h = shortcode_hash(&[b"test", b"image.jpg"], IMAGE_MODULUS);
        assert!(h < IMAGE_MODULUS);
    }

    #[test]
    fn test_resolve_gallery_format() {
        let mut index = ShortcodeIndex::new();
        let hash = shortcode_hash(&[b"main"], GALLERY_MODULUS);
        index.galleries.insert(hash, "main".to_string());
        index.gallery_to_hash.insert("main".to_string(), hash);

        let code = index.encode_gallery("main").unwrap();
        assert_eq!(code.len(), 4);
        assert!(code.starts_with('g'));

        let target = index.resolve(&code).unwrap();
        assert_eq!(
            target,
            ShortUrlTarget::Gallery {
                gallery_name: "main".to_string()
            }
        );
    }

    #[test]
    fn test_resolve_folder_format() {
        let mut index = ShortcodeIndex::new();
        let hash = shortcode_hash(&[b"main", b"vacation/2024"], FOLDER_MODULUS);
        index
            .folders
            .insert(hash, ("main".to_string(), "vacation/2024".to_string()));
        index
            .folder_to_hash
            .insert(("main".to_string(), "vacation/2024".to_string()), hash);

        let code = index.encode_folder("main", "vacation/2024").unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.starts_with('d'));

        let target = index.resolve(&code).unwrap();
        assert_eq!(
            target,
            ShortUrlTarget::Folder {
                gallery_name: "main".to_string(),
                folder_path: "vacation/2024".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_image_format() {
        let mut index = ShortcodeIndex::new();
        let hash = shortcode_hash(&[b"main", b"vacation/IMG_1234.jpg"], IMAGE_MODULUS);
        index.images.insert(
            hash,
            ("main".to_string(), "vacation/IMG_1234.jpg".to_string()),
        );
        index.image_to_hash.insert(
            ("main".to_string(), "vacation/IMG_1234.jpg".to_string()),
            hash,
        );

        let code = index.encode_image("main", "vacation/IMG_1234.jpg").unwrap();
        assert_eq!(code.len(), 7);
        assert!(code.starts_with('i'));

        let target = index.resolve(&code).unwrap();
        assert_eq!(
            target,
            ShortUrlTarget::Image {
                gallery_name: "main".to_string(),
                source_path: "vacation/IMG_1234.jpg".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_invalid_format() {
        let index = ShortcodeIndex::new();
        assert!(index.resolve("").is_err());
        assert!(index.resolve("x").is_err());
        assert!(index.resolve("g").is_err());
        assert!(index.resolve("gAB").is_err());
        assert!(index.resolve("gABCD").is_err());
    }

    #[test]
    fn test_resolve_unknown_type() {
        let index = ShortcodeIndex::new();
        let result = index.resolve("xABC");
        assert!(matches!(result, Err(ShortUrlError::UnknownType('x'))));
    }

    #[test]
    fn test_resolve_not_found() {
        let index = ShortcodeIndex::new();
        let result = index.resolve("g000");
        assert!(matches!(result, Err(ShortUrlError::NotFound)));
    }

    #[test]
    fn test_encode_nonexistent() {
        let index = ShortcodeIndex::new();
        assert!(index.encode_gallery("nonexistent").is_none());
        assert!(index.encode_folder("main", "missing").is_none());
        assert!(index.encode_image("main", "missing.jpg").is_none());
    }
}
