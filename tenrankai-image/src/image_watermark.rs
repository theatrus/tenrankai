use image::{DynamicImage, Rgba, RgbaImage};
use std::error::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum WatermarkPosition {
    BottomLeft,
    #[default]
    BottomRight,
    TopLeft,
    TopRight,
    Center,
    Tiled,
}

pub struct WatermarkConfig {
    pub position: WatermarkPosition,
    pub opacity: f32,
    pub scale: f32,
    pub padding: u32,
    pub adaptive: bool,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            position: WatermarkPosition::BottomRight,
            opacity: 0.5,
            scale: 15.0,
            padding: 10,
            adaptive: true,
        }
    }
}

pub struct ImageWatermark {
    image: RgbaImage,
    original_width: u32,
    original_height: u32,
}

impl ImageWatermark {
    pub fn load(bytes: &[u8]) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let img = image::load_from_memory(bytes)?;
        let rgba = img.to_rgba8();
        let (width, height) = (rgba.width(), rgba.height());

        Ok(Self {
            image: rgba,
            original_width: width,
            original_height: height,
        })
    }

    pub fn width(&self) -> u32 {
        self.original_width
    }

    pub fn height(&self) -> u32 {
        self.original_height
    }

    pub fn apply(
        &self,
        target: &mut DynamicImage,
        config: &WatermarkConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (target_width, target_height) = (target.width(), target.height());

        let smaller_dimension = target_width.min(target_height);
        let watermark_size = (smaller_dimension as f32 * config.scale / 100.0) as u32;

        if watermark_size < 4 {
            return Ok(());
        }

        let scale_factor =
            watermark_size as f32 / self.original_width.max(self.original_height) as f32;
        let scaled_width = (self.original_width as f32 * scale_factor).round() as u32;
        let scaled_height = (self.original_height as f32 * scale_factor).round() as u32;

        if scaled_width < 2 || scaled_height < 2 {
            return Ok(());
        }

        let scaled_watermark = image::imageops::resize(
            &self.image,
            scaled_width,
            scaled_height,
            image::imageops::FilterType::Lanczos3,
        );

        let mut target_rgba = target.to_rgba8();

        match config.position {
            WatermarkPosition::Tiled => {
                self.apply_tiled(
                    &mut target_rgba,
                    &scaled_watermark,
                    config.opacity,
                    config.adaptive,
                    config.padding,
                );
            }
            _ => {
                let (x, y) = self.calculate_position(
                    target_width,
                    target_height,
                    scaled_width,
                    scaled_height,
                    config.position,
                    config.padding,
                );

                self.apply_single(
                    &mut target_rgba,
                    &scaled_watermark,
                    x,
                    y,
                    config.opacity,
                    config.adaptive,
                );
            }
        }

        *target = DynamicImage::ImageRgba8(target_rgba);
        Ok(())
    }

    fn calculate_position(
        &self,
        target_width: u32,
        target_height: u32,
        watermark_width: u32,
        watermark_height: u32,
        position: WatermarkPosition,
        padding: u32,
    ) -> (i32, i32) {
        match position {
            WatermarkPosition::TopLeft => (padding as i32, padding as i32),
            WatermarkPosition::TopRight => (
                (target_width
                    .saturating_sub(watermark_width)
                    .saturating_sub(padding)) as i32,
                padding as i32,
            ),
            WatermarkPosition::BottomLeft => (
                padding as i32,
                (target_height
                    .saturating_sub(watermark_height)
                    .saturating_sub(padding)) as i32,
            ),
            WatermarkPosition::BottomRight => (
                (target_width
                    .saturating_sub(watermark_width)
                    .saturating_sub(padding)) as i32,
                (target_height
                    .saturating_sub(watermark_height)
                    .saturating_sub(padding)) as i32,
            ),
            WatermarkPosition::Center => (
                ((target_width.saturating_sub(watermark_width)) / 2) as i32,
                ((target_height.saturating_sub(watermark_height)) / 2) as i32,
            ),
            WatermarkPosition::Tiled => (0, 0),
        }
    }

    fn apply_single(
        &self,
        target: &mut RgbaImage,
        watermark: &RgbaImage,
        x: i32,
        y: i32,
        opacity: f32,
        adaptive: bool,
    ) {
        let (wm_width, wm_height) = (watermark.width(), watermark.height());

        let should_invert = if adaptive {
            let luminance = self.sample_region_luminance(target, x, y, wm_width, wm_height);
            luminance > 0.5
        } else {
            false
        };

        self.blend_watermark(target, watermark, x, y, opacity, should_invert);
    }

    fn apply_tiled(
        &self,
        target: &mut RgbaImage,
        watermark: &RgbaImage,
        opacity: f32,
        adaptive: bool,
        spacing: u32,
    ) {
        let (target_width, target_height) = (target.width(), target.height());
        let (wm_width, wm_height) = (watermark.width(), watermark.height());

        let step_x = wm_width + spacing;
        let step_y = wm_height + spacing;

        let mut y = spacing as i32;
        while y < target_height as i32 {
            let mut x = spacing as i32;
            while x < target_width as i32 {
                let should_invert = if adaptive {
                    let luminance = self.sample_region_luminance(target, x, y, wm_width, wm_height);
                    luminance > 0.5
                } else {
                    false
                };

                self.blend_watermark(target, watermark, x, y, opacity, should_invert);
                x += step_x as i32;
            }
            y += step_y as i32;
        }
    }

    fn sample_region_luminance(
        &self,
        image: &RgbaImage,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> f32 {
        let (img_width, img_height) = (image.width(), image.height());

        let x_start = x.max(0) as u32;
        let y_start = y.max(0) as u32;
        let x_end = ((x + width as i32) as u32).min(img_width);
        let y_end = ((y + height as i32) as u32).min(img_height);

        if x_start >= x_end || y_start >= y_end {
            return 0.5;
        }

        let step = 4;
        let mut total_luminance = 0.0f64;
        let mut sample_count = 0u32;

        let mut py = y_start;
        while py < y_end {
            let mut px = x_start;
            while px < x_end {
                let pixel = image.get_pixel(px, py);
                let luminance = calculate_luminance(pixel[0], pixel[1], pixel[2]);
                total_luminance += luminance as f64;
                sample_count += 1;
                px += step;
            }
            py += step;
        }

        if sample_count == 0 {
            0.5
        } else {
            (total_luminance / sample_count as f64) as f32
        }
    }

    fn blend_watermark(
        &self,
        target: &mut RgbaImage,
        watermark: &RgbaImage,
        x: i32,
        y: i32,
        opacity: f32,
        invert: bool,
    ) {
        let (target_width, target_height) = (target.width() as i32, target.height() as i32);
        let (wm_width, wm_height) = (watermark.width() as i32, watermark.height() as i32);

        for wy in 0..wm_height {
            let target_y = y + wy;
            if target_y < 0 || target_y >= target_height {
                continue;
            }

            for wx in 0..wm_width {
                let target_x = x + wx;
                if target_x < 0 || target_x >= target_width {
                    continue;
                }

                let wm_pixel = watermark.get_pixel(wx as u32, wy as u32);
                let wm_alpha = wm_pixel[3] as f32 / 255.0 * opacity;

                if wm_alpha < 0.001 {
                    continue;
                }

                let (wm_r, wm_g, wm_b) = if invert {
                    (255 - wm_pixel[0], 255 - wm_pixel[1], 255 - wm_pixel[2])
                } else {
                    (wm_pixel[0], wm_pixel[1], wm_pixel[2])
                };

                let target_pixel = target.get_pixel_mut(target_x as u32, target_y as u32);

                let blended_r = (target_pixel[0] as f32 * (1.0 - wm_alpha) + wm_r as f32 * wm_alpha)
                    .round() as u8;
                let blended_g = (target_pixel[1] as f32 * (1.0 - wm_alpha) + wm_g as f32 * wm_alpha)
                    .round() as u8;
                let blended_b = (target_pixel[2] as f32 * (1.0 - wm_alpha) + wm_b as f32 * wm_alpha)
                    .round() as u8;

                *target_pixel = Rgba([blended_r, blended_g, blended_b, target_pixel[3]]);
            }
        }
    }
}

fn calculate_luminance(r: u8, g: u8, b: u8) -> f32 {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let r_linear = if r <= 0.03928 {
        r / 12.92
    } else {
        ((r + 0.055) / 1.055).powf(2.4)
    };
    let g_linear = if g <= 0.03928 {
        g / 12.92
    } else {
        ((g + 0.055) / 1.055).powf(2.4)
    };
    let b_linear = if b <= 0.03928 {
        b / 12.92
    } else {
        ((b + 0.055) / 1.055).powf(2.4)
    };

    0.2126 * r_linear + 0.7152 * g_linear + 0.0722 * b_linear
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watermark_config_default() {
        let config = WatermarkConfig::default();
        assert_eq!(config.position, WatermarkPosition::BottomRight);
        assert_eq!(config.opacity, 0.5);
        assert_eq!(config.scale, 15.0);
        assert_eq!(config.padding, 10);
        assert!(config.adaptive);
    }

    #[test]
    fn test_calculate_luminance_black() {
        let lum = calculate_luminance(0, 0, 0);
        assert!(lum < 0.01, "Black should have near-zero luminance");
    }

    #[test]
    fn test_calculate_luminance_white() {
        let lum = calculate_luminance(255, 255, 255);
        assert!(lum > 0.99, "White should have near-one luminance");
    }

    #[test]
    fn test_calculate_luminance_gray() {
        let lum = calculate_luminance(128, 128, 128);
        assert!(
            lum > 0.2 && lum < 0.3,
            "Mid-gray luminance should be around 0.21"
        );
    }

    #[test]
    fn test_position_calculation() {
        let watermark = ImageWatermark {
            image: RgbaImage::new(100, 50),
            original_width: 100,
            original_height: 50,
        };

        let (x, y) =
            watermark.calculate_position(1000, 800, 100, 50, WatermarkPosition::TopLeft, 10);
        assert_eq!((x, y), (10, 10));

        let (x, y) =
            watermark.calculate_position(1000, 800, 100, 50, WatermarkPosition::TopRight, 10);
        assert_eq!((x, y), (890, 10));

        let (x, y) =
            watermark.calculate_position(1000, 800, 100, 50, WatermarkPosition::BottomLeft, 10);
        assert_eq!((x, y), (10, 740));

        let (x, y) =
            watermark.calculate_position(1000, 800, 100, 50, WatermarkPosition::BottomRight, 10);
        assert_eq!((x, y), (890, 740));

        let (x, y) =
            watermark.calculate_position(1000, 800, 100, 50, WatermarkPosition::Center, 10);
        assert_eq!((x, y), (450, 375));
    }

    #[test]
    fn test_sample_luminance_uniform_black() {
        let image = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        let watermark = ImageWatermark {
            image: RgbaImage::new(10, 10),
            original_width: 10,
            original_height: 10,
        };

        let lum = watermark.sample_region_luminance(&image, 10, 10, 50, 50);
        assert!(lum < 0.01, "Black image should have near-zero luminance");
    }

    #[test]
    fn test_sample_luminance_uniform_white() {
        let image = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let watermark = ImageWatermark {
            image: RgbaImage::new(10, 10),
            original_width: 10,
            original_height: 10,
        };

        let lum = watermark.sample_region_luminance(&image, 10, 10, 50, 50);
        assert!(lum > 0.99, "White image should have near-one luminance");
    }

    #[test]
    fn test_load_watermark_invalid_data() {
        let result = ImageWatermark::load(&[0, 1, 2, 3]);
        assert!(result.is_err());
    }
}
