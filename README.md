# Tenrankai

[![CI](https://github.com/yourusername/tenrankai/actions/workflows/ci.yml/badge.svg)](https://github.com/yourusername/tenrankai/actions/workflows/ci.yml)
[![Security Audit](https://github.com/yourusername/tenrankai/actions/workflows/security.yml/badge.svg)](https://github.com/yourusername/tenrankai/actions/workflows/security.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

A high-performance web-based photo gallery server written in Rust using the Axum web framework. Tenrankai provides a responsive gallery interface with automatic image resizing, metadata extraction, and intelligent caching.

The name "Tenrankai" (展覧会) is Japanese for "exhibition" or "gallery show", reflecting the project's purpose as a platform for displaying photographic collections.

## Features

- **Responsive Web Gallery**: Mobile-friendly masonry layout that adapts to different screen sizes
- **Automatic Image Processing**: On-the-fly image resizing with caching for multiple sizes
- **High-DPI Support**: Automatic @2x image generation for retina displays
- **Metadata Extraction**: EXIF data parsing including camera info, GPS coordinates, and capture dates
- **Smart Caching**: Persistent metadata caching and image cache with background refresh
- **Multiple Format Support**: Automatic WebP delivery for supported browsers with JPEG fallback
- **Color Profile Preservation**: Full ICC profile support for both JPEG and WebP, including Display P3
- **Copyright Watermarking**: Intelligent watermark placement with automatic text color selection
- **Markdown Support**: Folder descriptions and image captions via markdown files
- **Hidden Folders**: Hide folders from listings while keeping them accessible via direct URL
- **New Image Highlighting**: Configurable highlighting of recently modified images
- **Multiple Blog Systems**: Support for multiple independent blog/posts systems with markdown
- **Dark Theme Code Blocks**: Optimized code block styling for readability in dark theme

## Installation

### Prerequisites

- Rust 1.89.0 or later (automatically managed by rust-toolchain.toml)
- DejaVuSans.ttf font file (required for watermarking)

### Building from Source

```bash
git clone https://github.com/yourusername/tenrankai.git
cd tenrankai
cargo build --release
```

The project includes a `rust-toolchain.toml` file that will automatically download and use Rust 1.89.0 when you run cargo commands. This ensures consistent builds across all development environments.

## Configuration

Create a `config.toml` file in the project root. See `config.example.toml` for a complete example:

```toml
[server]
host = "127.0.0.1"
port = 3000

[app]
name = "My Gallery"
download_secret = "change-me-in-production"
download_password = "secure-password"
copyright_holder = "Your Name"
base_url = "https://yourdomain.com"

# Gallery configuration (multiple galleries supported)
[[galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "photos/main"
cache_directory = "cache/main"
images_per_page = 50
jpeg_quality = 85
webp_quality = 85.0

[[galleries]]
name = "portfolio"
url_prefix = "/portfolio"
source_directory = "photos/portfolio"
cache_directory = "cache/portfolio"
images_per_page = 20
jpeg_quality = 90
webp_quality = 90.0
```

### Key Configuration Options

**Gallery Configuration:**
- `name`: Unique identifier for the gallery (required for multiple galleries)
- `url_prefix`: URL path where the gallery will be accessible (e.g., `/gallery`, `/portfolio`)
- `source_directory`: Path to your photo directory
- `cache_directory`: Where processed images and metadata are cached
- `images_per_page`: Number of images to display per page
- `new_threshold_days`: Days to consider an image "new" (remove to disable)
- `pregenerate_cache`: Pre-generate all image sizes on startup/refresh
- `jpeg_quality`: JPEG compression quality (1-100)
- `webp_quality`: WebP compression quality (0.0-100.0)
- `gallery_template`: Custom template for gallery pages (default: "modules/gallery.html.liquid")
- `image_detail_template`: Custom template for image detail pages (default: "modules/image_detail.html.liquid")

## Usage

### Running the Server

```bash
# Using default config.toml
cargo run --release

# With custom configuration
cargo run --release -- --config /path/to/config.toml

# Specify host and port
cargo run --release -- --host 0.0.0.0 --port 8080

# Enable debug logging
cargo run --release -- --log-level debug
```

### Command Line Options

- `--config <path>`: Path to configuration file (default: config.toml)
- `--host <address>`: Override configured host address
- `--port <number>`: Override configured port
- `--log-level <level>`: Set logging level (trace, debug, info, warn, error)
- `--quit-after <seconds>`: Auto-shutdown after specified seconds (useful for testing)

## Gallery Features

### Multiple Galleries

Tenrankai supports multiple independent gallery instances, each with its own:
- Source directory for photos
- URL prefix for web access
- Cache directory and settings
- Templates (customizable per gallery)
- Image quality and pagination settings

Example URLs for different galleries:
- Main gallery: `http://localhost:8080/gallery/`
- Portfolio: `http://localhost:8080/portfolio/`
- Archive: `http://localhost:8080/photos/archive/`

### Gallery Organization

#### Directory Structure

```
photos/
├── vacation-2024/
│   ├── _folder.md          # Folder description (markdown)
│   ├── IMG_001.jpg
│   ├── IMG_001.md          # Image caption (markdown)
│   └── IMG_002.jpg
└── landscapes/
    ├── _folder.md
    └── sunset.jpg
```

### Markdown Support

- `_folder.md`: Place in any directory to add a description that appears at the top of the gallery page
- `<imagename>.md`: Create alongside any image to add a caption (e.g., `sunset.jpg` → `sunset.md`)

#### Advanced Folder Configuration

Folders can use TOML front matter in `_folder.md` files for advanced configuration:

```markdown
+++
hidden = true
title = "Private Collection"
+++

# Optional Markdown Content

This folder is hidden from gallery listings but remains accessible via direct URL.
```

**Configuration Options:**
- `hidden = true`: Hides the folder from gallery listings, previews, and counts (but allows direct access)
- `title = "Custom Name"`: Override the folder display name

**Hidden Folders:**
- Do not appear in gallery navigation or listings
- Are excluded from gallery preview images and image counts
- Remain fully accessible if you know the direct URL
- Perfect for private collections or work-in-progress galleries

## Posts System

Tenrankai includes a flexible posts/blog system that supports multiple independent collections:

### Post Format

Posts are markdown files with TOML front matter:

```markdown
+++
title = "My Post Title"
summary = "A brief summary of the post"
date = "2024-08-24"
+++

# Post Content

Your markdown content here...
```

### Multiple Post Systems

Configure multiple independent post systems in your `config.toml`:

```toml
[[posts]]
name = "blog"
source_directory = "posts/blog"
url_prefix = "/blog"
posts_per_page = 20
refresh_interval_minutes = 30  # Auto-refresh posts every 30 minutes

[[posts]]
name = "stories"
source_directory = "posts/stories"
url_prefix = "/stories"
posts_per_page = 10
```

Each system has its own:
- Source directory for markdown files
- URL prefix for web access
- Templates (customizable)
- Posts per page setting
- Optional automatic refresh interval for detecting new/changed posts

### Features

- Full CommonMark support with extensions (tables, strikethrough, footnotes)
- Automatic HTML generation from markdown
- Chronological sorting (newest first)
- Pagination support
- Subdirectory organization (URL reflects directory structure)
- Dynamic refresh via API
- Automatic periodic refresh (configurable interval)
- Individual post reloading when files change
- Dark theme optimized code blocks with syntax highlighting
- Responsive post layout for mobile and desktop

## Image Sizes

Tenrankai automatically generates multiple sizes for each image:

- **Thumbnail**: Small preview images for gallery grid
- **Gallery**: Standard viewing size used in the gallery layout
- **Medium**: Larger size with optional copyright watermark
- **Large**: Full quality (requires authentication)

All sizes support @2x variants for high-DPI displays.

### Color Profile Support

Tenrankai preserves ICC color profiles throughout the image processing pipeline:

- **JPEG**: Extracts and preserves ICC profiles from source images
- **WebP**: Embeds ICC profiles using libwebp-sys WebPMux API
- **Wide Gamut**: Full support for Display P3, Adobe RGB, and other color spaces
- **Watermarking**: Color profiles maintained even when adding copyright notices

This ensures accurate color reproduction across all devices and browsers that support color management.

## Authentication

Large image downloads require authentication. Users can authenticate by:

1. Visiting `/api/auth` and entering the configured password
2. Using the download links which include authentication tokens

## API Endpoints

### Gallery Endpoints
- `GET /gallery` - Gallery root
- `GET /gallery/{path}` - Browse specific folder
- `GET /gallery/image/{path}?size={size}` - Get resized image
- `GET /gallery/detail/{path}` - View image details page
- `POST /api/auth` - Authenticate for downloads
- `GET /api/gallery/preview` - Get random gallery preview images

### Posts Endpoints (configurable prefix)
- `GET /{prefix}` - List posts with pagination
- `GET /{prefix}/{slug}` - View individual post
- `POST /api/posts/{name}/refresh` - Refresh posts cache

## Performance

Tenrankai includes several performance optimizations:

- Persistent metadata caching reduces file system access
- Background cache refresh keeps data fresh without blocking requests
- Concurrent image processing with rate limiting
- Automatic cache pre-generation option for instant loading
- Browser-based caching headers for processed images

## Template Structure

Templates are organized into three directories:

```
templates/
├── pages/              # Regular page templates
│   ├── index.html.liquid
│   ├── about.html.liquid
│   ├── contact.html.liquid
│   └── 404.html.liquid
├── modules/            # Module-specific templates
│   ├── gallery.html.liquid
│   ├── image_detail.html.liquid
│   ├── posts_index.html.liquid
│   └── post_detail.html.liquid
└── partials/           # Reusable template components
    ├── _header.html.liquid
    ├── _footer.html.liquid
    └── _gallery_preview.html.liquid
```

All templates use the Liquid templating language and support includes for reusable components.

## Static Files

Place the following in the `static` directory:

- `DejaVuSans.ttf` - Required for copyright watermarking
- `robots.txt` - Custom robots file (optional, defaults provided)
- Any other static assets referenced in templates

## Logging

Control logging verbosity with the `RUST_LOG` environment variable or `--log-level` flag:

```bash
# Examples
RUST_LOG=debug cargo run
cargo run -- --log-level trace
```

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.