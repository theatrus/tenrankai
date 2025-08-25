use image::ImageFormat;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Jpeg,
    WebP,
    Png,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::WebP => "webp",
            OutputFormat::Png => "png",
        }
    }

    #[allow(dead_code)]
    pub fn image_format(&self) -> ImageFormat {
        match self {
            OutputFormat::Jpeg => ImageFormat::Jpeg,
            OutputFormat::WebP => ImageFormat::WebP,
            OutputFormat::Png => ImageFormat::Png,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::WebP => "image/webp",
            OutputFormat::Png => "image/png",
        }
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
