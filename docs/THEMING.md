# Theming Guide

Tenrankai supports custom themes through CSS variable overrides. This guide explains how to create and apply custom themes.

## Quick Start

1. Create a custom static directory (e.g., `static-custom/`)
2. Copy `static/theme-override.css` to your custom directory
3. Configure the static file directories in `config.toml`:
   ```toml
   [static_files]
   directories = ["static-custom", "static"]
   ```
4. Edit your `theme-override.css` to customize colors, fonts, and styles
5. Restart the server

## How It Works

The theme system uses CSS custom properties (variables) that can be overridden. The loading order is:

1. `style.css` - Base styles with default variables
2. `theme-override.css` - Your customizations (loaded from highest-priority static directory)
3. Page-specific CSS - Module-specific styles

With `directories = ["static-custom", "static"]`, files in `static-custom/` take precedence over `static/`.

## CSS Variables Reference

### Colors

#### Background Colors
| Variable | Description | Dark Default | Light Default |
|----------|-------------|--------------|---------------|
| `--bg-primary` | Main background | `#1a1a1a` | `#ffffff` |
| `--bg-secondary` | Secondary background (containers) | `#2a2a2a` | `#f8f9fa` |
| `--bg-card` | Card/panel backgrounds | `#606060` | `#ffffff` |
| `--bg-hover` | Hover state backgrounds | `#505050` | `#e9ecef` |
| `--header-bg` | Header background | `#606060` | `#f0f4f8` |

#### Text Colors
| Variable | Description | Dark Default | Light Default |
|----------|-------------|--------------|---------------|
| `--text-primary` | Main text | `#e0e0e0` | `#212529` |
| `--text-secondary` | Secondary/muted text | `#b8b8b8` | `#6c757d` |
| `--text-muted` | Muted text | `#ccc` | `#6c757d` |

#### Link Colors
| Variable | Description | Dark Default | Light Default |
|----------|-------------|--------------|---------------|
| `--link-color` | Link color | `#66b3ff` | `#0d6efd` |
| `--link-hover` | Link hover color | `#99ccff` | `#0a58ca` |

#### Border Colors
| Variable | Description | Dark Default | Light Default |
|----------|-------------|--------------|---------------|
| `--border-color` | Default border | `#555` | `#dee2e6` |
| `--border-hover` | Border hover state | `#777` | `#adb5bd` |

#### Accent Colors
| Variable | Description | Dark Default | Light Default |
|----------|-------------|--------------|---------------|
| `--accent-red` | Red accent (danger, alerts) | `#ff6b6b` | `#dc3545` |

#### Message Colors
| Variable | Description |
|----------|-------------|
| `--message-info-bg` | Info message background |
| `--message-info-color` | Info message text |
| `--message-success-bg` | Success message background |
| `--message-success-color` | Success message text |
| `--message-error-bg` | Error message background |
| `--message-error-color` | Error message text |
| `--message-warning-bg` | Warning message background |
| `--message-warning-color` | Warning message text |

#### Code Block Colors
| Variable | Description |
|----------|-------------|
| `--code-bg` | Code block background |
| `--code-text` | Code block text |
| `--code-border` | Code block border |
| `--code-inline-bg` | Inline code background |
| `--code-inline-text` | Inline code text |

### Fonts

| Variable | Description | Default |
|----------|-------------|---------|
| `--font-body` | Body text font stack | `'Poppins', -apple-system, ...` |
| `--font-heading` | Heading font stack | `var(--font-body)` |
| `--font-mono` | Monospace font stack | `'Consolas', 'Monaco', ...` |

### Spacing

| Variable | Default |
|----------|---------|
| `--spacing-xs` | `0.25rem` |
| `--spacing-sm` | `0.5rem` |
| `--spacing-md` | `1rem` |
| `--spacing-lg` | `1.5rem` |
| `--spacing-xl` | `2rem` |
| `--spacing-xxl` | `3rem` |

### Shadows

| Variable | Description |
|----------|-------------|
| `--shadow-light` | Light shadow |
| `--shadow-medium` | Medium shadow |
| `--header-shadow` | Header drop shadow |

## Example Themes

### Warm Sepia Theme

```css
@import url('https://fonts.googleapis.com/css2?family=Merriweather:wght@400;700&family=Open+Sans:wght@400;600&display=swap');

:root[data-theme="light"],
:root:not([data-theme]) {
    --bg-primary: #faf8f5;
    --bg-secondary: #f5f0e8;
    --bg-card: #ffffff;
    --text-primary: #3d3d3d;
    --text-secondary: #6b6b6b;
    --link-color: #8b5a2b;
    --link-hover: #6b4423;
    --border-color: #e0d6c8;

    --font-body: 'Open Sans', sans-serif;
    --font-heading: 'Merriweather', Georgia, serif;
}

:root[data-theme="dark"] {
    --bg-primary: #1f1a15;
    --bg-secondary: #2a241d;
    --bg-card: #332b22;
    --text-primary: #e8e0d5;
    --text-secondary: #a89f94;
    --link-color: #d4a574;
    --link-hover: #e8c4a0;
    --border-color: #4a4035;
}
```

### High Contrast Theme

```css
:root[data-theme="light"],
:root:not([data-theme]) {
    --bg-primary: #ffffff;
    --bg-secondary: #f0f0f0;
    --bg-card: #ffffff;
    --text-primary: #000000;
    --text-secondary: #333333;
    --link-color: #0000cc;
    --link-hover: #000099;
    --border-color: #000000;
}

:root[data-theme="dark"] {
    --bg-primary: #000000;
    --bg-secondary: #1a1a1a;
    --bg-card: #0a0a0a;
    --text-primary: #ffffff;
    --text-secondary: #cccccc;
    --link-color: #66b3ff;
    --link-hover: #99ccff;
    --border-color: #ffffff;
}
```

### Minimal Monospace Theme

```css
@import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&display=swap');

:root[data-theme="light"],
:root:not([data-theme]),
:root[data-theme="dark"] {
    --font-body: 'JetBrains Mono', monospace;
    --font-heading: 'JetBrains Mono', monospace;
    --font-mono: 'JetBrains Mono', monospace;
}
```

## Safe Component Classes

These classes are intended for theme customization and are unlikely to change between versions:

| Class | Description |
|-------|-------------|
| `.gallery-grid` | Gallery image grid container |
| `.gallery-item` | Individual gallery item |
| `.card` | Card/panel component |
| `.navbar` | Navigation bar |
| `.container` | Main content container |
| `.image-detail-content` | Image detail page content |

## Using Custom Fonts

### Google Fonts

Add an `@import` at the top of your `theme-override.css`:

```css
@import url('https://fonts.googleapis.com/css2?family=Playfair+Display:wght@400;700&display=swap');

:root,
:root[data-theme="dark"],
:root[data-theme="light"] {
    --font-heading: 'Playfair Display', Georgia, serif;
}
```

### Local Fonts

1. Place font files in your custom static directory (e.g., `static-custom/fonts/`)
2. Define `@font-face` rules:

```css
@font-face {
    font-family: 'CustomFont';
    src: url('/static/fonts/CustomFont-Regular.woff2') format('woff2');
    font-weight: 400;
    font-style: normal;
    font-display: swap;
}

@font-face {
    font-family: 'CustomFont';
    src: url('/static/fonts/CustomFont-Bold.woff2') format('woff2');
    font-weight: 700;
    font-style: normal;
    font-display: swap;
}

:root,
:root[data-theme="dark"],
:root[data-theme="light"] {
    --font-body: 'CustomFont', sans-serif;
}
```

## Tips

1. **Dark/Light Toggle**: Always define both `:root[data-theme="light"]` and `:root[data-theme="dark"]` to ensure your theme works with the built-in theme toggle.

2. **Auto Theme**: Include `:root:not([data-theme])` in your light theme selector to support automatic OS preference detection.

3. **Test Both Modes**: After making changes, test both light and dark modes using the theme toggle in the navigation.

4. **Font Loading**: Use `font-display: swap` for custom fonts to prevent invisible text during loading.

5. **Specificity**: If your overrides aren't applying, check that your selectors have sufficient specificity.
