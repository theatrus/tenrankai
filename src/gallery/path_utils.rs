//! Path manipulation utilities for gallery files.
//!
//! Provides helpers for working with image paths, sidecar files, and extensions.

/// File extension extracted from a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExtension(String);

impl FileExtension {
    /// Extract the extension from a path (lowercase, without the dot).
    pub fn from_path(path: &str) -> Option<Self> {
        // Find the last dot in the path
        let dot_pos = path.rfind('.')?;

        // Get what's after the dot
        let ext = &path[dot_pos + 1..];

        // Must have content after the dot, and no slashes (not a directory separator)
        if ext.is_empty() || ext.contains('/') {
            return None;
        }

        Some(Self(ext.to_lowercase()))
    }

    /// Get the extension as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a JPEG extension.
    pub fn is_jpeg(&self) -> bool {
        matches!(self.0.as_str(), "jpg" | "jpeg")
    }

    /// Check if this is an AVIF extension.
    pub fn is_avif(&self) -> bool {
        self.0 == "avif"
    }

    /// Check if this is a PNG extension.
    pub fn is_png(&self) -> bool {
        self.0 == "png"
    }

    /// Check if this is a WebP extension.
    pub fn is_webp(&self) -> bool {
        self.0 == "webp"
    }

    /// Check if this is a supported image format.
    pub fn is_image(&self) -> bool {
        matches!(
            self.0.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "heic" | "heif"
        )
    }
}

impl AsRef<str> for FileExtension {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Paths to sidecar files for an image.
#[derive(Debug, Clone)]
pub struct SidecarPaths {
    /// XMP sidecar path (image.xmp, replacing extension).
    pub xmp: String,
    /// Markdown sidecar path format 1 (image.jpg.md, appending .md).
    pub markdown_full: String,
    /// Markdown sidecar path format 2 (image.md, replacing extension).
    pub markdown_replaced: String,
}

impl SidecarPaths {
    /// Generate sidecar paths for an image path.
    pub fn for_image(image_path: &str) -> Self {
        Self {
            xmp: replace_extension(image_path, "xmp"),
            markdown_full: format!("{}.md", image_path),
            markdown_replaced: replace_extension(image_path, "md"),
        }
    }

    /// Iterate over all markdown sidecar paths.
    pub fn markdown_paths(&self) -> impl Iterator<Item = &str> {
        [self.markdown_full.as_str(), self.markdown_replaced.as_str()].into_iter()
    }
}

/// Replace the extension of a path with a new one.
///
/// If the path has no extension, appends the new extension.
///
/// # Examples
/// ```ignore
/// assert_eq!(replace_extension("photo.jpg", "xmp"), "photo.xmp");
/// assert_eq!(replace_extension("photo", "xmp"), "photo.xmp");
/// assert_eq!(replace_extension("dir/photo.jpg", "md"), "dir/photo.md");
/// ```
pub fn replace_extension(path: &str, new_ext: &str) -> String {
    if let Some(dot_pos) = path.rfind('.') {
        // Check that the dot is in the filename, not a directory
        let after_dot = &path[dot_pos + 1..];
        if !after_dot.contains('/') {
            return format!("{}.{}", &path[..dot_pos], new_ext);
        }
    }
    format!("{}.{}", path, new_ext)
}

/// Get the filename from a path.
pub fn filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Get the parent directory of a path.
pub fn parent(path: &str) -> Option<&str> {
    path.rfind('/').map(|pos| &path[..pos])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_extension_from_path() {
        assert_eq!(
            FileExtension::from_path("photo.jpg"),
            Some(FileExtension("jpg".to_string()))
        );
        assert_eq!(
            FileExtension::from_path("PHOTO.JPG"),
            Some(FileExtension("jpg".to_string()))
        );
        assert_eq!(
            FileExtension::from_path("dir/photo.jpeg"),
            Some(FileExtension("jpeg".to_string()))
        );
        assert_eq!(FileExtension::from_path("noext"), None);
        assert_eq!(FileExtension::from_path(""), None);
    }

    #[test]
    fn test_file_extension_checks() {
        let jpg = FileExtension::from_path("test.jpg").unwrap();
        assert!(jpg.is_jpeg());
        assert!(!jpg.is_avif());
        assert!(jpg.is_image());

        let jpeg = FileExtension::from_path("test.jpeg").unwrap();
        assert!(jpeg.is_jpeg());

        let avif = FileExtension::from_path("test.avif").unwrap();
        assert!(avif.is_avif());
        assert!(!avif.is_jpeg());

        let png = FileExtension::from_path("test.png").unwrap();
        assert!(png.is_png());
    }

    #[test]
    fn test_replace_extension() {
        assert_eq!(replace_extension("photo.jpg", "xmp"), "photo.xmp");
        assert_eq!(replace_extension("photo.jpeg", "md"), "photo.md");
        assert_eq!(replace_extension("dir/photo.jpg", "xmp"), "dir/photo.xmp");
        assert_eq!(replace_extension("photo", "xmp"), "photo.xmp");
        assert_eq!(
            replace_extension("dir.name/photo.jpg", "xmp"),
            "dir.name/photo.xmp"
        );
    }

    #[test]
    fn test_sidecar_paths() {
        let sidecars = SidecarPaths::for_image("landscapes/photo.jpg");
        assert_eq!(sidecars.xmp, "landscapes/photo.xmp");
        assert_eq!(sidecars.markdown_full, "landscapes/photo.jpg.md");
        assert_eq!(sidecars.markdown_replaced, "landscapes/photo.md");
    }

    #[test]
    fn test_filename() {
        assert_eq!(filename("dir/subdir/photo.jpg"), "photo.jpg");
        assert_eq!(filename("photo.jpg"), "photo.jpg");
        assert_eq!(filename(""), "");
    }

    #[test]
    fn test_parent() {
        assert_eq!(parent("dir/subdir/photo.jpg"), Some("dir/subdir"));
        assert_eq!(parent("dir/photo.jpg"), Some("dir"));
        assert_eq!(parent("photo.jpg"), None);
    }
}
