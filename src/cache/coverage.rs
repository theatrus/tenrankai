use crate::gallery::image_processing::OutputFormat;

/// Tracks which formats are available for a specific image and size
#[derive(Debug, Clone, Default)]
pub struct FormatCoverage {
    pub has_jpeg: bool,
    pub has_webp: bool,
    pub has_png: bool,
    #[cfg(feature = "avif")]
    pub has_avif: bool,
}

impl FormatCoverage {
    /// Get all expected formats for a given source image path
    fn expected_formats(source_path: &str) -> Vec<OutputFormat> {
        let mut formats = vec![OutputFormat::Jpeg];

        // Skip WebP for PNG sources to preserve transparency
        if !source_path.to_lowercase().ends_with(".png") {
            formats.push(OutputFormat::WebP);
        }

        #[cfg(feature = "avif")]
        formats.push(OutputFormat::Avif);

        formats
    }

    /// Check if this coverage has all expected formats for the source
    pub fn is_complete(&self, source_path: &str) -> bool {
        let expected = Self::expected_formats(source_path);

        expected.iter().all(|format| match format {
            OutputFormat::Jpeg => self.has_jpeg,
            OutputFormat::WebP => self.has_webp,
            OutputFormat::Png => self.has_png,
            #[cfg(feature = "avif")]
            OutputFormat::Avif => self.has_avif,
        })
    }

    /// Get missing formats for this coverage
    pub fn missing_formats(&self, source_path: &str) -> Vec<OutputFormat> {
        let expected = Self::expected_formats(source_path);

        expected
            .into_iter()
            .filter(|format| match format {
                OutputFormat::Jpeg => !self.has_jpeg,
                OutputFormat::WebP => !self.has_webp,
                OutputFormat::Png => !self.has_png,
                #[cfg(feature = "avif")]
                OutputFormat::Avif => !self.has_avif,
            })
            .collect()
    }
}