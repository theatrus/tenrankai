/// Calculate relative luminance using the WCAG formula
/// Returns a value between 0.0 (black) and 1.0 (white)
///
/// See: https://www.w3.org/WAI/GL/wiki/Relative_luminance
#[inline]
pub fn calculate_luminance(r: u8, g: u8, b: u8) -> f32 {
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
    fn test_luminance_black() {
        let lum = calculate_luminance(0, 0, 0);
        assert!(lum < 0.01, "Black should have near-zero luminance");
    }

    #[test]
    fn test_luminance_white() {
        let lum = calculate_luminance(255, 255, 255);
        assert!(lum > 0.99, "White should have near-one luminance");
    }

    #[test]
    fn test_luminance_gray() {
        let lum = calculate_luminance(128, 128, 128);
        assert!(
            lum > 0.2 && lum < 0.3,
            "Mid-gray luminance should be around 0.21, got {}",
            lum
        );
    }

    #[test]
    fn test_luminance_red() {
        let lum = calculate_luminance(255, 0, 0);
        assert!(
            lum > 0.2 && lum < 0.22,
            "Pure red luminance should be around 0.2126, got {}",
            lum
        );
    }

    #[test]
    fn test_luminance_green() {
        let lum = calculate_luminance(0, 255, 0);
        assert!(
            lum > 0.71 && lum < 0.72,
            "Pure green luminance should be around 0.7152, got {}",
            lum
        );
    }

    #[test]
    fn test_luminance_blue() {
        let lum = calculate_luminance(0, 0, 255);
        assert!(
            lum > 0.07 && lum < 0.08,
            "Pure blue luminance should be around 0.0722, got {}",
            lum
        );
    }
}
