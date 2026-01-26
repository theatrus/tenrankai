use axum::{
    http::{StatusCode, header, HeaderMap, HeaderValue},
    response::IntoResponse,
};
use tenrankai_config_storage::StoredThemeConfig;

use crate::site::ResolvedState;

pub async fn serve_theme_css(ResolvedState(app_state): ResolvedState) -> impl IntoResponse {
    let css = if let Some(theme) = app_state.theme() {
        generate_theme_css(theme)
    } else {
        String::new()
    };

    let etag = compute_theme_etag(&css);

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    if let Ok(etag_value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, etag_value);
    }

    (StatusCode::OK, headers, css)
}

pub fn generate_theme_css(theme: &StoredThemeConfig) -> String {
    let mut css = String::new();
    css.push_str(":root {\n");

    // Background colors
    if let Some(ref v) = theme.bg_primary {
        css.push_str(&format!("  --bg-primary: {};\n", v));
    }
    if let Some(ref v) = theme.bg_secondary {
        css.push_str(&format!("  --bg-secondary: {};\n", v));
    }
    if let Some(ref v) = theme.bg_card {
        css.push_str(&format!("  --bg-card: {};\n", v));
    }
    if let Some(ref v) = theme.bg_hover {
        css.push_str(&format!("  --bg-hover: {};\n", v));
    }
    if let Some(ref v) = theme.header_bg {
        css.push_str(&format!("  --header-bg: {};\n", v));
    }

    // Text colors
    if let Some(ref v) = theme.text_primary {
        css.push_str(&format!("  --text-primary: {};\n", v));
    }
    if let Some(ref v) = theme.text_secondary {
        css.push_str(&format!("  --text-secondary: {};\n", v));
    }
    if let Some(ref v) = theme.text_muted {
        css.push_str(&format!("  --text-muted: {};\n", v));
    }

    // Link colors
    if let Some(ref v) = theme.link_color {
        css.push_str(&format!("  --link-color: {};\n", v));
    }
    if let Some(ref v) = theme.link_hover {
        css.push_str(&format!("  --link-hover: {};\n", v));
    }

    // Border colors
    if let Some(ref v) = theme.border_color {
        css.push_str(&format!("  --border-color: {};\n", v));
    }

    // Accent/button colors
    if let Some(ref v) = theme.accent_color {
        css.push_str(&format!("  --accent-red: {};\n", v));
    }
    if let Some(ref v) = theme.btn_danger_bg {
        css.push_str(&format!("  --btn-danger-bg: {};\n", v));
    }

    // Font families
    if let Some(ref v) = theme.font_body {
        css.push_str(&format!("  --font-body: {};\n", v));
    }
    if let Some(ref v) = theme.font_heading {
        css.push_str(&format!("  --font-heading: {};\n", v));
    }
    if let Some(ref v) = theme.font_mono {
        css.push_str(&format!("  --font-mono: {};\n", v));
    }

    css.push_str("}\n");

    // If the theme is empty, return empty string
    if css == ":root {\n}\n" {
        return String::new();
    }

    css
}

pub fn compute_theme_etag(css: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(css.as_bytes());
    let hash = hasher.finalize();
    format!("\"{}\"", hex::encode(&hash[..8]))
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_theme_returns_empty_string() {
        let theme = StoredThemeConfig::default();
        let css = generate_theme_css(&theme);
        assert!(css.is_empty());
    }

    #[test]
    fn test_single_override() {
        let theme = StoredThemeConfig {
            bg_primary: Some("#ff0000".to_string()),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains("--bg-primary: #ff0000;"));
        assert!(css.starts_with(":root {"));
        assert!(css.ends_with("}\n"));
    }

    #[test]
    fn test_multiple_overrides() {
        let theme = StoredThemeConfig {
            bg_primary: Some("#111".to_string()),
            text_primary: Some("#eee".to_string()),
            link_color: Some("#00f".to_string()),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains("--bg-primary: #111;"));
        assert!(css.contains("--text-primary: #eee;"));
        assert!(css.contains("--link-color: #00f;"));
    }

    #[test]
    fn test_etag_changes_with_content() {
        let css1 = ":root { --bg-primary: #111; }";
        let css2 = ":root { --bg-primary: #222; }";
        let etag1 = compute_theme_etag(css1);
        let etag2 = compute_theme_etag(css2);
        assert_ne!(etag1, etag2);
    }

    #[test]
    fn test_etag_is_consistent() {
        let css = ":root { --bg-primary: #111; }";
        let etag1 = compute_theme_etag(css);
        let etag2 = compute_theme_etag(css);
        assert_eq!(etag1, etag2);
    }
}
