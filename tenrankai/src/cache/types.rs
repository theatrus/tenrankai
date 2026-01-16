use crate::gallery;

/// Cache type system for consistent cache operations and filenames
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheType {
    /// Image metadata cache (metadata_cache.json)
    ImageMetadata,
    /// Cache metadata with version tracking (cache_metadata.json)
    CacheMetadata,
    /// Processed image cache with format and watermark variants
    ProcessedImage {
        format: gallery::image_processing::OutputFormat,
        watermarked: bool,
    },
    /// Composite image cache for folder previews  
    Composite {
        gallery_name: String,
        path_key: String,
    },
}

impl CacheType {
    /// Get the filename for this cache type
    pub fn filename(&self, key: Option<&str>) -> String {
        match self {
            CacheType::ImageMetadata => "metadata_cache.json".to_string(),
            CacheType::CacheMetadata => "cache_metadata.json".to_string(),
            CacheType::ProcessedImage {
                format,
                watermarked,
            } => {
                let key = key.expect("ProcessedImage cache requires a key");
                let suffix = if *watermarked { "_watermarked" } else { "" };
                format!("{}{}.{}", key, suffix, format.extension())
            }
            CacheType::Composite {
                gallery_name,
                path_key,
            } => {
                let key = key.unwrap_or("default");
                format!("composite_{}_{}_{}.jpg", gallery_name, path_key, key)
            }
        }
    }

    /// Get the maximum age for this cache type
    pub fn max_age(&self) -> std::time::Duration {
        match self {
            CacheType::ImageMetadata => std::time::Duration::from_secs(24 * 60 * 60), // 24 hours
            CacheType::CacheMetadata => std::time::Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            CacheType::ProcessedImage { .. } => std::time::Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            CacheType::Composite { .. } => std::time::Duration::from_secs(24 * 60 * 60), // 24 hours
        }
    }

    /// Check if this cache type should be persisted to disk
    pub fn is_persistent(&self) -> bool {
        match self {
            CacheType::ImageMetadata | CacheType::CacheMetadata => true,
            CacheType::ProcessedImage { .. } | CacheType::Composite { .. } => true,
        }
    }

    /// Check if this cache type is automatically cleaned up
    pub fn is_auto_cleanup(&self) -> bool {
        match self {
            CacheType::ImageMetadata | CacheType::CacheMetadata => false,
            CacheType::ProcessedImage { .. } | CacheType::Composite { .. } => true,
        }
    }

    /// Get the cache priority (higher numbers = more important)
    pub fn priority(&self) -> u8 {
        match self {
            CacheType::CacheMetadata => 10,        // Highest priority
            CacheType::ImageMetadata => 9,         // Second highest
            CacheType::ProcessedImage { .. } => 5, // Medium priority
            CacheType::Composite { .. } => 3,      // Lower priority
        }
    }

    /// Check if this cache type requires a key
    pub fn requires_key(&self) -> bool {
        matches!(self, CacheType::ProcessedImage { .. })
    }

    /// Get all cache types that are JSON-based
    pub const JSON_TYPES: &'static [CacheType] =
        &[CacheType::ImageMetadata, CacheType::CacheMetadata];

    /// Get all cache types that are binary/image-based
    pub fn binary_types() -> Vec<CacheType> {
        vec![
            CacheType::ProcessedImage {
                format: gallery::image_processing::OutputFormat::Jpeg,
                watermarked: false,
            },
            CacheType::Composite {
                gallery_name: "example".to_string(),
                path_key: "example".to_string(),
            },
        ]
    }

    /// Create a ProcessedImage cache type
    pub fn processed_image(
        format: gallery::image_processing::OutputFormat,
        watermarked: bool,
    ) -> Self {
        CacheType::ProcessedImage {
            format,
            watermarked,
        }
    }

    /// Create a Composite cache type
    pub fn composite(gallery_name: String, path_key: String) -> Self {
        CacheType::Composite {
            gallery_name,
            path_key,
        }
    }

    /// Parse a composite cache key and create CacheType if valid
    pub fn from_composite_cache_key(cache_key: &str) -> Option<Self> {
        let parts: Vec<&str> = cache_key.split('_').collect();
        if parts.len() >= 3 && parts[0] == "composite" {
            let gallery_name = parts[1].to_string();
            let path_key = parts[2..].join("_"); // Join remaining parts in case of underscores in path
            Some(CacheType::Composite {
                gallery_name,
                path_key,
            })
        } else {
            None
        }
    }

    /// Get the MIME type for this cache type
    pub fn mime_type(&self) -> &'static str {
        match self {
            CacheType::ImageMetadata | CacheType::CacheMetadata => "application/json",
            CacheType::ProcessedImage { format, .. } => format.mime_type(),
            CacheType::Composite { .. } => "image/jpeg",
        }
    }
}

impl std::fmt::Display for CacheType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheType::ImageMetadata => write!(f, "ImageMetadata"),
            CacheType::CacheMetadata => write!(f, "CacheMetadata"),
            CacheType::ProcessedImage {
                format,
                watermarked,
            } => {
                write!(
                    f,
                    "ProcessedImage({}{})",
                    format.extension(),
                    if *watermarked { ",watermarked" } else { "" }
                )
            }
            CacheType::Composite {
                gallery_name,
                path_key,
            } => {
                write!(f, "Composite({}:{})", gallery_name, path_key)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::image_processing::OutputFormat;

    #[test]
    fn test_cache_type_enum_functionality() {
        // Test JSON cache types
        let metadata_cache = CacheType::ImageMetadata;
        assert_eq!(metadata_cache.filename(None), "metadata_cache.json");
        assert!(metadata_cache.is_persistent());
        assert!(!metadata_cache.is_auto_cleanup());
        assert_eq!(metadata_cache.priority(), 9);
        assert!(!metadata_cache.requires_key());
        assert_eq!(metadata_cache.mime_type(), "application/json");

        let cache_metadata = CacheType::CacheMetadata;
        assert_eq!(cache_metadata.filename(None), "cache_metadata.json");
        assert_eq!(cache_metadata.priority(), 10); // Highest priority
        assert_eq!(cache_metadata.mime_type(), "application/json");

        // Test processed image cache types
        let jpeg_cache = CacheType::processed_image(OutputFormat::Jpeg, false);
        assert_eq!(jpeg_cache.filename(Some("abcd1234")), "abcd1234.jpg");
        assert!(jpeg_cache.requires_key());
        assert_eq!(jpeg_cache.mime_type(), "image/jpeg");

        let watermarked_cache = CacheType::processed_image(OutputFormat::WebP, true);
        assert_eq!(
            watermarked_cache.filename(Some("efgh5678")),
            "efgh5678_watermarked.webp"
        );
        assert_eq!(watermarked_cache.mime_type(), "image/webp");

        // Test composite cache
        let composite_cache = CacheType::composite("main".to_string(), "root".to_string());
        assert_eq!(
            composite_cache.filename(None),
            "composite_main_root_default.jpg"
        );
        assert_eq!(
            composite_cache.filename(Some("abc123")),
            "composite_main_root_abc123.jpg"
        );

        let composite_cache_path =
            CacheType::composite("main".to_string(), "vacation_2024".to_string());
        assert_eq!(
            composite_cache_path.filename(Some("def456")),
            "composite_main_vacation_2024_def456.jpg"
        );
        assert_eq!(composite_cache.mime_type(), "image/jpeg");
        assert_eq!(composite_cache.priority(), 3);

        // Test cache behavior methods
        assert!(CacheType::ImageMetadata.is_persistent());
        assert!(
            CacheType::ProcessedImage {
                format: OutputFormat::Png,
                watermarked: false
            }
            .is_auto_cleanup()
        );

        // Test max age
        assert!(CacheType::ImageMetadata.max_age().as_secs() > 0);
        assert!(CacheType::CacheMetadata.max_age() > CacheType::ImageMetadata.max_age());

        // Test display
        assert_eq!(format!("{}", CacheType::ImageMetadata), "ImageMetadata");
        assert_eq!(format!("{}", jpeg_cache), "ProcessedImage(jpg)");
        assert_eq!(
            format!("{}", watermarked_cache),
            "ProcessedImage(webp,watermarked)"
        );

        // Test constants
        assert!(CacheType::JSON_TYPES.contains(&CacheType::ImageMetadata));
        assert!(CacheType::JSON_TYPES.contains(&CacheType::CacheMetadata));
        assert_eq!(CacheType::JSON_TYPES.len(), 2);

        let binary_types = CacheType::binary_types();
        assert!(!binary_types.is_empty());

        // Test display formatting
        assert_eq!(format!("{}", composite_cache), "Composite(main:root)");

        // Test composite cache key parsing
        let parsed = CacheType::from_composite_cache_key("composite_main_vacation_2024");
        assert!(parsed.is_some());
        if let Some(CacheType::Composite {
            gallery_name,
            path_key,
        }) = parsed
        {
            assert_eq!(gallery_name, "main");
            assert_eq!(path_key, "vacation_2024");
        }

        // Test parsing with underscores in path
        let parsed_complex =
            CacheType::from_composite_cache_key("composite_gallery_folder_subfolder_item");
        assert!(parsed_complex.is_some());
        if let Some(CacheType::Composite {
            gallery_name,
            path_key,
        }) = parsed_complex
        {
            assert_eq!(gallery_name, "gallery");
            assert_eq!(path_key, "folder_subfolder_item");
        }

        // Test invalid cache key
        assert!(CacheType::from_composite_cache_key("not_a_composite").is_none());
        assert!(CacheType::from_composite_cache_key("composite_only").is_none());
        assert!(CacheType::from_composite_cache_key("").is_none());
    }
}
