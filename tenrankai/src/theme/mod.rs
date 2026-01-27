use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use tenrankai_config_storage::{StoredThemeConfig, ThemeColorSet};

use crate::site::ResolvedState;

pub async fn serve_theme_css(ResolvedState(app_state): ResolvedState) -> impl IntoResponse {
    let css = if let Some(theme) = app_state.theme() {
        generate_theme_css(theme)
    } else {
        String::new()
    };

    let etag = compute_theme_etag(&css);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    if let Ok(etag_value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, etag_value);
    }

    (StatusCode::OK, headers, css)
}

macro_rules! push_css_var {
    ($css:expr, $colors:expr, $field:ident, $var_name:expr) => {
        if let Some(ref v) = $colors.$field {
            $css.push_str(&format!("  {}: {};\n", $var_name, v));
        }
    };
}

fn generate_color_set_vars(colors: &ThemeColorSet) -> String {
    let mut css = String::new();
    push_css_var!(css, colors, bg_primary, "--bg-primary");
    push_css_var!(css, colors, bg_secondary, "--bg-secondary");
    push_css_var!(css, colors, bg_card, "--bg-card");
    push_css_var!(css, colors, bg_hover, "--bg-hover");
    push_css_var!(css, colors, header_bg, "--header-bg");
    push_css_var!(css, colors, text_primary, "--text-primary");
    push_css_var!(css, colors, text_secondary, "--text-secondary");
    push_css_var!(css, colors, text_muted, "--text-muted");
    push_css_var!(css, colors, link_color, "--link-color");
    push_css_var!(css, colors, link_hover, "--link-hover");
    push_css_var!(css, colors, border_color, "--border-color");
    push_css_var!(css, colors, accent_color, "--accent-red");
    push_css_var!(css, colors, btn_danger_bg, "--btn-danger-bg");
    css
}

fn generate_font_vars(theme: &StoredThemeConfig) -> String {
    let mut css = String::new();
    push_css_var!(css, theme, font_body, "--font-body");
    push_css_var!(css, theme, font_heading, "--font-heading");
    push_css_var!(css, theme, font_mono, "--font-mono");
    css
}

pub fn generate_theme_css(theme: &StoredThemeConfig) -> String {
    let mut css = String::new();

    let font_vars = generate_font_vars(theme);
    let dark_vars = theme
        .dark
        .as_ref()
        .map(generate_color_set_vars)
        .unwrap_or_default();
    let light_vars = theme
        .light
        .as_ref()
        .map(generate_color_set_vars)
        .unwrap_or_default();

    let has_fonts = !font_vars.is_empty();
    let has_dark = !dark_vars.is_empty();
    let has_light = !light_vars.is_empty();
    let has_force = theme.force_color_scheme.is_some();

    if !has_fonts && !has_dark && !has_light && !has_force {
        return String::new();
    }

    if has_fonts {
        // Use grouped selector to match specificity of style.css
        css.push_str(":root,\n:root[data-theme=\"dark\"],\n:root[data-theme=\"light\"] {\n");
        css.push_str(&font_vars);
        css.push_str("}\n\n");
    }

    match theme.force_color_scheme.as_deref() {
        Some("dark") => {
            if has_dark {
                // Use data-theme selector to match specificity of style.css
                css.push_str(":root[data-theme=\"dark\"] {\n");
                css.push_str(&dark_vars);
                css.push_str("}\n");
            }
        }
        Some("light") => {
            if has_light {
                // Use data-theme selector to match specificity of style.css
                css.push_str(":root[data-theme=\"light\"] {\n");
                css.push_str(&light_vars);
                css.push_str("}\n");
            }
        }
        _ => {
            if has_dark {
                // Use :root[data-theme] to match specificity of style.css
                css.push_str(":root,\n:root[data-theme=\"dark\"] {\n");
                css.push_str(&dark_vars);
                css.push_str("}\n\n");
            }

            if has_light {
                css.push_str(":root[data-theme=\"light\"] {\n");
                css.push_str(&light_vars);
                css.push_str("}\n");
            }
        }
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
    fn test_dark_mode_override() {
        let theme = StoredThemeConfig {
            dark: Some(ThemeColorSet {
                bg_primary: Some("#1a1a1a".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root[data-theme=\"dark\"]"));
        assert!(css.contains("--bg-primary: #1a1a1a;"));
    }

    #[test]
    fn test_light_mode_override() {
        let theme = StoredThemeConfig {
            light: Some(ThemeColorSet {
                bg_primary: Some("#ffffff".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(css.contains("--bg-primary: #ffffff;"));
    }

    #[test]
    fn test_force_dark_mode() {
        let theme = StoredThemeConfig {
            force_color_scheme: Some("dark".to_string()),
            dark: Some(ThemeColorSet {
                bg_primary: Some("#1a1a1a".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root[data-theme=\"dark\"]"));
        assert!(css.contains("--bg-primary: #1a1a1a;"));
        assert!(!css.contains("@media"));
    }

    #[test]
    fn test_force_light_mode() {
        let theme = StoredThemeConfig {
            force_color_scheme: Some("light".to_string()),
            light: Some(ThemeColorSet {
                bg_primary: Some("#ffffff".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(css.contains("--bg-primary: #ffffff;"));
        assert!(!css.contains("@media"));
    }

    #[test]
    fn test_both_modes() {
        let theme = StoredThemeConfig {
            dark: Some(ThemeColorSet {
                bg_primary: Some("#1a1a1a".to_string()),
                ..Default::default()
            }),
            light: Some(ThemeColorSet {
                bg_primary: Some("#ffffff".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root[data-theme=\"dark\"]"));
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(css.contains("--bg-primary: #1a1a1a;"));
        assert!(css.contains("--bg-primary: #ffffff;"));
    }

    #[test]
    fn test_fonts_in_root() {
        let theme = StoredThemeConfig {
            font_body: Some("'Poppins', sans-serif".to_string()),
            ..Default::default()
        };
        let css = generate_theme_css(&theme);
        assert!(css.contains(":root,"));
        assert!(css.contains(":root[data-theme=\"dark\"]"));
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(css.contains("--font-body: 'Poppins', sans-serif;"));
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
