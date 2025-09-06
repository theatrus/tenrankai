use image::ImageFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Jpeg,
    WebP,
    Png,
    #[cfg(feature = "avif")]
    Avif,
}

impl OutputFormat {
    /// All supported output formats
    pub const ALL: &'static [OutputFormat] = &[
        OutputFormat::Jpeg,
        OutputFormat::WebP,
        OutputFormat::Png,
        #[cfg(feature = "avif")]
        OutputFormat::Avif,
    ];

    /// Formats that support transparency
    pub const TRANSPARENCY_FORMATS: &'static [OutputFormat] = &[
        OutputFormat::Png,
        OutputFormat::WebP,
        #[cfg(feature = "avif")]
        OutputFormat::Avif,
    ];

    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::WebP => "webp",
            OutputFormat::Png => "png",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "avif",
        }
    }

    #[allow(dead_code)]
    pub fn image_format(&self) -> ImageFormat {
        match self {
            OutputFormat::Jpeg => ImageFormat::Jpeg,
            OutputFormat::WebP => ImageFormat::WebP,
            OutputFormat::Png => ImageFormat::Png,
            #[cfg(feature = "avif")]
            OutputFormat::Avif => ImageFormat::Avif,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::WebP => "image/webp",
            OutputFormat::Png => "image/png",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "image/avif",
        }
    }

    /// Determine output format from Accept header
    /// Used for content negotiation with browser support detection
    pub fn from_accept_header(accept_header: &str) -> OutputFormat {
        // Check browser support in priority order (best quality first)
        #[cfg(feature = "avif")]
        if accept_header.contains("image/avif") {
            return OutputFormat::Avif;
        }

        if accept_header.contains("image/webp") {
            OutputFormat::WebP
        } else {
            OutputFormat::Jpeg
        }
    }

    /// Determine output format from file extension
    pub fn from_file_extension(path: &str) -> Option<OutputFormat> {
        let extension = path.rsplit('.').next()?.to_lowercase();
        match extension.as_str() {
            "jpg" | "jpeg" => Some(OutputFormat::Jpeg),
            "webp" => Some(OutputFormat::WebP),
            "png" => Some(OutputFormat::Png),
            #[cfg(feature = "avif")]
            "avif" => Some(OutputFormat::Avif),
            _ => None,
        }
    }

    /// Check if this format supports transparency (alpha channel)
    pub fn supports_transparency(&self) -> bool {
        Self::TRANSPARENCY_FORMATS.contains(self)
    }

    /// Get the quality range for this format (min, max)
    pub fn quality_range(&self) -> (f32, f32) {
        match self {
            OutputFormat::Jpeg => (1.0, 100.0),
            OutputFormat::WebP => (0.0, 100.0),
            OutputFormat::Png => (0.0, 9.0), // PNG compression level
            #[cfg(feature = "avif")]
            OutputFormat::Avif => (0.0, 63.0), // AVIF quality parameter range
        }
    }

    /// Get the default quality for this format
    pub fn default_quality(&self) -> f32 {
        match self {
            OutputFormat::Jpeg => 85.0,
            OutputFormat::WebP => 85.0,
            OutputFormat::Png => 6.0, // PNG compression level
            #[cfg(feature = "avif")]
            OutputFormat::Avif => 50.0,
        }
    }

    /// Check if quality parameter applies to this format
    pub fn uses_quality(&self) -> bool {
        match self {
            OutputFormat::Jpeg | OutputFormat::WebP => true,
            OutputFormat::Png => false, // PNG uses compression level, not quality
            #[cfg(feature = "avif")]
            OutputFormat::Avif => true,
        }
    }

    /// Get display name for this format (for UI/logging)
    pub fn display_name(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "JPEG",
            OutputFormat::WebP => "WebP",
            OutputFormat::Png => "PNG",
            #[cfg(feature = "avif")]
            OutputFormat::Avif => "AVIF",
        }
    }

    /// Check if this format should be used based on source format
    /// PNG sources should stay PNG to preserve transparency
    pub fn should_preserve_source_format(source_path: &str) -> bool {
        source_path.to_lowercase().ends_with(".png")
    }
}

#[derive(Debug, Clone)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

impl ImageSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn with_multiplier(&self, multiplier: u32) -> Self {
        Self {
            width: self.width * multiplier,
            height: self.height * multiplier,
        }
    }
}
