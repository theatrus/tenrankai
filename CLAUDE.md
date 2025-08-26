# Tenrankai Project Documentation

## Project Overview
Tenrankai is a web-based photo gallery server written in Rust using the Axum web framework. It provides a dynamic, responsive gallery interface with features like image resizing, metadata extraction, watermarking, and caching. The system supports multiple independent gallery instances, each with its own configuration, URL prefix, and content directories.

## Testing Note for AI Development
When implementing features, use the `--quit-after` flag to test the server without it running indefinitely:
```bash
cargo run -- --quit-after 5  # Server auto-shuts down after 5 seconds
```
This is especially useful for verifying startup behavior, testing API endpoints, and checking that new features don't break server initialization.

## Key Features
- **Multiple Gallery Support**: Configure and run multiple independent gallery instances with unique URLs and settings
- **Responsive Web Gallery**: Mobile-friendly masonry layout that adapts to different screen sizes
- **Image Processing**: On-the-fly image resizing with caching for thumbnails, gallery, medium, and large sizes
- **High-DPI Support**: Automatic @2x image generation for retina displays
- **Metadata Extraction**: EXIF data parsing for camera info, GPS coordinates, and capture dates
- **Copyright Watermarking**: Per-gallery copyright holder configuration with intelligent watermark placement and automatic text color selection based on background
- **Performance Optimization**: Metadata caching, image caching, and background refresh
- **Markdown Support**: Folder descriptions and image captions via markdown files
- **New Image Highlighting**: Automatic highlighting of recently modified images based on configurable threshold
- **Multiple Blog Systems**: Support for multiple independent markdown-based blog/posts systems
- **Dark Theme Code Blocks**: Optimized code block styling for readability with proper contrast
- **Email-based Authentication**: Secure passwordless login system with email verification links
- **Email Provider Support**: Pluggable email provider system with Amazon SES support

## AVIF Feature Flag

The project includes optional AVIF (AV1 Image File Format) support that can be disabled for easier builds on platforms where AVIF dependencies are difficult to compile (particularly Windows).

### Building with AVIF Support (Default)
```bash
cargo build                    # Uses default features including AVIF
cargo build --features avif    # Explicitly enable AVIF
```

**Features with AVIF enabled:**
- Full HDR AVIF encoding/decoding with gain map support
- 10-bit encoding for HDR images with proper color space preservation
- AVIF files can be processed, served, and used in composite images
- `avif-debug` CLI command available for analyzing AVIF metadata
- ICC profile preservation through AVIF processing pipeline

### Building without AVIF Support
```bash
cargo build --no-default-features    # Disables AVIF and related dependencies
```

**Behavior without AVIF:**
- AVIF files are completely ignored by the system
- Smaller binary size and no complex AVIF build dependencies
- Faster builds, especially on Windows
- `avif-debug` CLI command not available
- System gracefully falls back for any AVIF-related operations

### Platform-Specific Defaults
- **Linux/macOS**: AVIF enabled by default (full feature set)
- **Windows CI**: AVIF disabled by default for easier builds
- **Local Development**: Use `--no-default-features` on Windows if build issues occur

## Project Structure

### Core Modules
- `src/main.rs` - Application entry point, configuration, and server setup
- `src/lib.rs` - Library components and shared types
- `src/api.rs` - API endpoints for health checks and authentication
- `src/templating.rs` - Liquid template engine integration
- `src/copyright.rs` - Watermarking functionality with intelligent text color selection
- `src/composite.rs` - Composite image generation for OpenGraph previews

### Gallery Module (`src/gallery/`)
The gallery functionality was recently refactored from a single 3000-line file into organized submodules:
- `mod.rs` - Module definitions and public exports
- `types.rs` - Core data structures (GalleryItem, ImageInfo, etc.)
- `core.rs` - Core gallery methods (directory scanning, preview generation, breadcrumbs)
- `handlers.rs` - HTTP route handlers for gallery endpoints
- `image_processing/` - Image processing submodule (recently refactored)
  - `mod.rs` - Module exports
  - `types.rs` - OutputFormat, ImageSize types
  - `formats/` - Format-specific modules
    - `avif.rs` - AVIF handling with HDR and gain map support
    - `jpeg.rs` - JPEG handling with ICC profile support
    - `png.rs` - PNG handling with ICC profile extraction
    - `webp.rs` - WebP encoding with fallback support
  - `icc.rs` - ICC profile name extraction
  - `resize.rs` - Image resizing logic
  - `serve.rs` - Image serving and response handling
  - `watermark.rs` - Copyright watermark application
- `metadata.rs` - EXIF metadata extraction and processing
- `cache.rs` - Cache management, persistence, and pregeneration
- `error.rs` - Error type definitions

### Commands Module (`src/commands/`)
Utility commands for development and debugging:
- `avif_debug.rs` - AVIF file analysis tool for inspecting HDR properties, color spaces, and gain maps

### Posts Module (`src/posts/`)
A flexible markdown-based posts/blog system supporting multiple independent collections:
- `mod.rs` - Module exports
- `types.rs` - Post, PostSummary, PostsConfig structures
- `core.rs` - PostsManager for scanning, caching, and serving posts
- `handlers.rs` - HTTP handlers for posts index and detail pages
- `error.rs` - Posts-specific error types
- `tests.rs` - Comprehensive test suite

### Login Module (`src/login/`)
Email-based authentication system:
- `mod.rs` - Module exports
- `types.rs` - User database, login tokens, rate limiting structures
- `auth.rs` - Authentication logic and cookie handling
- `handlers.rs` - HTTP handlers for login flow
- `error.rs` - Authentication error types
- `tests.rs` - Authentication tests

### Email Module (`src/email/`)
Pluggable email provider system:
- `mod.rs` - Main module with `EmailProvider` trait
- `types.rs` - Email message types and builders
- `config.rs` - Configuration structures
- `error.rs` - Error types
- `providers/` - Provider implementations
  - `ses.rs` - Amazon SES provider

### Template Structure
Templates are organized into three directories for better maintainability:
- `templates/pages/` - Regular page templates (index, about, contact, 404)
- `templates/modules/` - Module-specific templates (gallery, image_detail, posts_index, post_detail)
- `templates/partials/` - Reusable components (_header, _footer, _gallery_preview)

All templates use the Liquid templating language. When loading templates:
- Page templates are referenced as `pages/template_name.html.liquid`
- Module templates are referenced as `modules/template_name.html.liquid`
- Partial templates are referenced as `partials/_partial_name.html.liquid`
- Partials are automatically loaded and made available to all templates

## Email Module Architecture

The email module provides a pluggable architecture for sending emails:

### Structure
- `mod.rs` - Main module with `EmailProvider` trait
- `types.rs` - Email message types and builders
- `config.rs` - Configuration structures
- `error.rs` - Error types
- `providers/` - Provider implementations
  - `ses.rs` - Amazon SES provider

### Adding New Email Providers

To add a new email provider:

1. Create a new file in `src/email/providers/` (e.g., `smtp.rs`)
2. Implement the `EmailProvider` trait:
```rust
#[async_trait]
impl EmailProvider for SmtpProvider {
    async fn send_email(&self, message: EmailMessage) -> Result<(), EmailError>;
    fn name(&self) -> &str;
}
```
3. Add the provider to the `EmailProviderConfig` enum in `config.rs`
4. Update the `create_provider` function in `mod.rs`

### Email Configuration

Email is configured in `config.toml`:
```toml
[email]
from_address = "noreply@domain.com"
from_name = "Tenrankai"
reply_to = "support@domain.com"  # optional
provider = "ses"

# Provider-specific config
region = "us-east-1"  # optional, defaults to AWS SDK default
access_key_id = "..."  # optional
secret_access_key = "..."  # optional
```

## Important Implementation Details

### Mobile Responsiveness
The gallery preview uses JavaScript to calculate appropriate column widths:
- Mobile (≤768px): Single column at 90% of available width
- Desktop: Two columns with proper spacing
- iOS-specific handling for viewport and scrolling issues

### Image Sizing
- **Thumbnail**: Small preview images
- **Gallery**: Standard viewing size (used in gallery grid)
- **Medium**: Larger size with optional copyright watermark
- **Large**: Full quality (requires authentication)
- All sizes support @2x variants for high-DPI displays

### Image Format Support
- **Automatic WebP delivery**: Serves WebP format to browsers that support it (based on Accept header)
- **JPEG fallback**: Falls back to JPEG for browsers without WebP support
- **PNG support**: PNG images are always served as PNG to preserve transparency
- **AVIF support**: Full HDR AVIF encoding/decoding with gain map preservation
  - Uses libavif-rs with AOM codec for high quality AVIF support
  - HDR preservation with 10-bit encoding for HDR images
  - Gain map support for HDR/SDR tone mapping:
    - Detects gain maps using libavif 1.2.1+ experimental APIs
    - Preserves gain map parameters (gamma, min/max, offsets, HDR headroom)
    - Resizes gain maps proportionally with main image
    - Attaches gain maps to output AVIF files
  - Automatic HDR detection based on:
    - Color primaries (BT.2020, Display P3)
    - Transfer characteristics (PQ/HLG)
    - Bit depth (>8 bits)
    - CLLI metadata presence
    - Gain map presence
  - Container-level parsing for gain map detection when libavif decoding fails
- **Quality settings**: Configurable quality for JPEG (default: 85), WebP (default: 85.0), and AVIF
- **Cache separation**: Different cache files for JPEG, WebP, PNG, and AVIF versions
- **Content negotiation**: Automatic format selection based on browser capabilities and source format
- **ICC Profile Preservation**: Full support for color profiles in JPEG, PNG, WebP, and AVIF formats
  - JPEG: ICC profiles extracted from source and preserved in output
  - WebP: ICC profiles embedded using libwebp-sys (v0.13+) WebPMux API
  - AVIF: ICC profiles preserved through libavif with full HDR metadata support
  - Display P3 and other wide gamut color spaces fully supported
  - Profiles preserved through entire processing pipeline including watermarking

### Metadata Caching

#### Cache Storage
- **In-memory cache**: HashMap storing image metadata (dimensions, EXIF, GPS, camera info)
- **Persistent storage**: JSON files in cache directory
  - `metadata_cache.json` - Image metadata
  - `cache_metadata.json` - Cache version and last refresh timestamp

#### Cache Refresh Mechanisms
1. **Version-based refresh**: Automatic full refresh when app version changes
2. **Background refresh**: Configurable interval (default 60 minutes)
3. **Incremental updates**: 
   - `refresh_single_image_metadata()` - Update single image
   - `refresh_directory_metadata()` - Update all images in directory
   - `refresh_all_metadata()` - Full gallery refresh

#### Cache Persistence
- **Automatic saves**:
  - Every 5 minutes if cache is dirty
  - After every 100 metadata updates
  - After each full refresh
  - On graceful shutdown (SIGTERM/SIGINT)
- **Dirty tracking**: AtomicBool flag tracks unsaved changes
- **Update counting**: Tracks updates since last save

#### Performance Features
- Lazy loading: Metadata extracted on first access if not cached
- Batch saves: Reduces disk I/O by grouping updates
- Lock optimization: Releases write locks before disk operations

### Gallery Preview
- Shows random selection of images from across the gallery
- Respects max_depth and max_per_folder configuration
- Updates on each page load for variety

### Watermarking
- Applied only to medium-sized images
- Uses WCAG luminance calculation to determine text color (black/white)
- Automatically converts RGBA to RGB for JPEG compatibility
- Preserves ICC color profiles from source images through watermark processing
- Requires DejaVuSans.ttf font in static directory

### Authentication Flow

1. User enters username/email at `/_login`
2. System generates a secure token with 10-minute expiry
3. Email is sent with login link containing token
4. User clicks link, token is verified
5. Session cookie is created (7-day expiry)

## Configuration

### Key Configuration Files
- `config.toml` - Main application configuration
- `cache/metadata_cache.json` - Persisted image metadata
- `cache/cache_metadata.json` - Cache version tracking

### Configuration Options

#### Multiple Gallery Configuration
```toml
# Define multiple galleries, each with its own configuration
[[galleries]]
name = "main"                              # Unique identifier for this gallery
url_prefix = "/gallery"                    # URL prefix (must start with /)
source_directory = "photos"                # Directory containing photos
cache_directory = "cache/main"             # Cache directory for this gallery
gallery_template = "modules/gallery.html.liquid"
image_detail_template = "modules/image_detail.html.liquid"
images_per_page = 50
jpeg_quality = 85                         # JPEG quality (1-100)
webp_quality = 85.0                       # WebP quality (0.0-100.0)
new_threshold_days = 7                    # Mark images modified within 7 days as "NEW"
pregenerate_cache = false                 # Pre-generate all image sizes on startup
approximate_dates_for_public = false      # Show only month/year to non-authenticated users
copyright_holder = "Your Name"            # Copyright holder for watermarking medium images

[galleries.thumbnail]
width = 300
height = 300

[galleries.gallery_size]
width = 800
height = 800

[galleries.medium]
width = 1200
height = 1200

[galleries.large]
width = 1600
height = 1600

[galleries.preview]
max_images = 6
max_depth = 3
max_per_folder = 3

# Add a second gallery with different settings
[[galleries]]
name = "portfolio"
url_prefix = "/my-portfolio"
source_directory = "portfolio"
cache_directory = "cache/portfolio"
gallery_template = "modules/gallery.html.liquid"
image_detail_template = "modules/image_detail.html.liquid"
images_per_page = 20
jpeg_quality = 90
webp_quality = 90.0
# No new_threshold_days - this gallery won't highlight new images
```

### Environment Variables
- `RUST_LOG` - Controls logging verbosity (trace, debug, info, warn, error)

## Testing and Development

### Building and Running
```bash
# Default build (includes AVIF support)
cargo build

# Build without AVIF (for Windows or when AVIF dependencies are problematic)
cargo build --no-default-features

# Run the server
cargo run -- --host 0.0.0.0 --port 8080

# Run without AVIF support
cargo run --no-default-features -- --host 0.0.0.0 --port 8080
```

### Testing with Auto-Shutdown
The `--quit-after` flag allows the server to automatically shut down after a specified number of seconds, which is useful for testing startup behavior and running automated tests:

```bash
# Run server for 5 seconds then auto-shutdown
cargo run -- --quit-after 5

# Test startup checks with auto-shutdown
cargo run -- --quit-after 3 --log-level debug

# Verify server starts and serves requests
cargo run -- --quit-after 10 &
sleep 2
curl http://localhost:3000/api/health
```

This feature is particularly helpful when implementing new features to verify the server starts correctly without needing to manually terminate the process.

### Common Commands
- Check warnings: `cargo build 2>&1 | grep warning`
- Run with debug logging: `RUST_LOG=debug cargo run`
- Test startup and shutdown: `cargo run -- --quit-after 5`
- Run linters: `cargo clippy -- -D warnings`
- Run linters (no AVIF): `cargo clippy --no-default-features -- -D warnings`
- Check formatting: `cargo fmt --check`
- Build for production: `cargo build --release`
- Build for production (no AVIF): `cargo build --release --no-default-features`
- Check dependencies: `cargo outdated` and `cargo audit`
- Test all features: `cargo test --all-features`
- Test without AVIF: `cargo test --no-default-features`

### Testing URLs
- Gallery root: `http://localhost:8080/gallery` (for main gallery)
- Portfolio root: `http://localhost:8080/my-portfolio` (for portfolio gallery)
- Specific folder: `http://localhost:8080/gallery/folder-name`
- Image with size: `http://localhost:8080/gallery/image/path/to/image.jpg?size=gallery`
- Gallery preview API: `http://localhost:8080/api/gallery/main/preview?count=12`
- Composite image: `http://localhost:8080/api/gallery/main/composite/_root`

### AVIF Debug Command
Test AVIF HDR and gain map support:
```bash
# Analyze an AVIF file
cargo run -- avif-debug photos/vacation/_A630303-HDR.avif

# Get detailed technical information
cargo run -- avif-debug photos/vacation/_A630303-HDR.avif --verbose
```

The command shows:
- Image dimensions and file size
- Color space properties (primaries, transfer, matrix)
- HDR detection (based on bit depth, color space, CLLI, gain maps)
- Gain map presence and parameters
- ICC profile information
- Detailed HDR detection logic (with --verbose)

### AWS SES Testing
- Use SES sandbox for development (verify sender/recipient emails)
- Monitor AWS CloudWatch for delivery metrics
- Check SES suppression list if emails aren't delivered

## Code Style Guidelines
- No comments unless explicitly requested
- Follow existing patterns and conventions
- Use existing libraries (check Cargo.toml first)
- Prefer editing existing files over creating new ones
- Always handle errors appropriately
- Use proper Rust idioms (match, if let, etc.)
- **Always use `thiserror` crate for error types** - Define errors with `#[derive(Error)]` and `#[error("...")]` attributes
- **Always run `cargo fmt` before finalizing code** - Ensure consistent formatting across the codebase
- **Always run `cargo clippy` and fix warnings** - Ensure code follows Rust best practices and catches common mistakes

## Key Design Decisions

### Email Provider Architecture
- Trait-based design allows easy addition of new providers
- Async/await throughout for non-blocking I/O
- AWS SDK v2 for modern async support
- Support for text, HTML, or both email formats

### Security Considerations
- Login tokens are cryptographically random (32 bytes)
- Tokens expire after 10 minutes
- Rate limiting prevents brute force (5 attempts per 5 minutes)
- Session cookies are HTTPOnly and use signed values

## Troubleshooting

### Email Not Sending
1. Check email configuration in `config.toml`
2. Verify AWS credentials (if using SES)
3. Check server logs for detailed errors
4. Ensure sender email is verified in SES

### Login Links Not Working
1. Verify `base_url` in config matches actual URL
2. Check token expiry (10 minutes)
3. Ensure cookies are enabled in browser
4. Check for clock skew between server and client

### Gallery Issues
1. Check `gallery_url` vs `thumbnail_url` usage
2. Verify metadata cache is being populated
3. Ensure dimensions are available for layout calculations
4. Check browser console for JavaScript errors

### Performance Issues
1. Use metadata cache for dimensions instead of loading images
2. Batch operations where possible
3. Use background tasks for expensive operations
4. Leverage browser caching with proper headers

## Major Features

### Pluggable Email Provider System
1. **Email Module Architecture**:
   - Trait-based architecture for easy provider addition
   - Amazon SES provider implementation
   - Null provider for development/testing
   - Support for text, HTML, or both email formats
   - Configurable from address, name, and reply-to

2. **Login Integration**:
   - Login system sends actual emails instead of logging URLs
   - Falls back to logging if no email provider configured
   - Professional HTML and text email templates

### Library/Binary Architecture
- **lib.rs**: Separated library components from binary
- **Moved Types**: All config types (Config, ServerConfig, etc.) now in lib.rs
- **Public API**: Exposed modules and types for external use
- **Cleaner main.rs**: Binary now just handles CLI and server startup
- **Benefits**: Better code organization, reusable components, testability

### Multi-Gallery Support
1. **Multiple Gallery Instances**: The gallery module supports multiple independent gallery instances
   - Each gallery has its own source directory, cache directory, and URL prefix
   - Named galleries only (no backward compatibility mode)

### Posts System
1. **Multiple Blog Systems**: Support for multiple independent markdown-based blog/posts systems
   - Each system has its own source directory, URL prefix, and configuration
   - **Gallery Image References**: Easy embedding of gallery images with automatic linking
   - **Automatic Reload on Change**: Posts are automatically reloaded when their markdown files are modified

### Email-based Authentication System
1. **Login Module** (`src/login/`):
   - User database stored in TOML file (`users.toml`)
   - No self-registration - admin manages users via CLI tool
   - Email-based passwordless authentication
   - Rate limiting per IP address (5 attempts per 5 minutes)
   - Secure token generation with 10-minute expiration
   - Periodic cleanup of expired tokens and rate limits

### Hidden Gallery Folders
1. **TOML Front Matter Support in _folder.md**:
   - Gallery folders can use TOML front matter similar to posts
   - Folders can be marked as `hidden = true` in TOML config
   - Hidden folders are excluded from listings but remain accessible via direct URL

### WebAuthn/Passkey Support
1. **Modern Authentication**: WebAuthn/Passkey authentication
   - Biometric authentication (fingerprint, face recognition)
   - Hardware security key support
   - Cross-device passkey sync via platform providers
   - Multiple passkeys per user account
   - Seamless fallback to email authentication

2. **User Experience Improvements**:
   - Passkey enrollment flow after email login
   - Profile page at `/_login/profile` for managing passkeys
   - Improved login page UI with better contrast
   - Template reorganization into modules/ directory

### Gallery Access Control
1. **Folder-Level Access Control**:
   - `require_auth = true` in _folder.md TOML to require authentication
   - `allowed_users = ["user1", "user2"]` to restrict access to specific users
   - Hierarchical access control (parent folder restrictions apply to children)
   - Access control applies to gallery views, previews, and image serving

### Build System Improvements (December 2025)
1. **Cross-Platform Build Support**:
   - **Ubuntu/macOS**: Full builds with AVIF support and complete dependencies
   - **Windows CI**: Builds with `--no-default-features` (no AVIF) for easier compilation
   - Windows uses simplified vcpkg setup (only OpenSSL, no AVIF build tools)
   - Improved build reliability across all platforms

2. **AVIF Feature Flag System**:
   - Optional AVIF compilation controlled by cargo features
   - Conditional compilation guards throughout codebase
   - Graceful fallbacks when AVIF support is disabled
   - Platform-specific CI configurations for optimal builds

### AVIF HDR Support with Gain Maps
1. **Advanced AVIF Support**:
   - Full HDR AVIF encoding/decoding using libavif-rs with AOM codec
   - Gain map detection and preservation for HDR/SDR tone mapping
   - Preserves HDR metadata including color primaries, transfer functions, and CLLI
   - Container-level gain map detection fallback when libavif decoding fails
   - 10-bit encoding for HDR images with proper color space preservation
   - Browser fallback: AVIF sources served as WebP/JPEG for non-AVIF browsers (resized images only)

2. **Gain Map Implementation**:
   - Detects gain maps using libavif 1.2.1+ experimental APIs
   - Extracts gain map image data by setting `imageContentToDecode = AVIF_IMAGE_CONTENT_ALL`
   - Preserves gain map parameters:
     - Gamma values for R,G,B channels
     - Min/max values for tone mapping
     - Base and alternate offsets
     - HDR headroom values
   - Resizes gain maps proportionally with main image during processing
   - Attaches gain maps to output AVIF files maintaining HDR/SDR compatibility

3. **HDR Detection Logic**:
   - Detects HDR content based on multiple criteria:
     - BT.2020 color primaries with PQ/HLG transfer
     - Display P3 primaries with ≥10-bit depth
     - Any >8-bit image with PQ/HLG transfer
     - Presence of CLLI (Content Light Level Info)
     - Presence of gain map
   - Preserves exact color space properties without unwanted modifications

4. **Testing and Debug Tools**:
   - `avif-debug` command for analyzing AVIF files
   - Integration tests for gain map preservation
   - Epsilon comparisons for floating-point metadata

## Future Improvements

1. **Additional Email Providers**
   - SMTP provider for generic email servers
   - SendGrid provider
   - Mailgun provider

2. **Email Features**
   - Email templates with Liquid
   - Multi-language support
   - HTML email preview in development

3. **Authentication Enhancements**
   - Remember me option
   - Account recovery flow
   - Two-factor authentication

4. **Gallery Enhancements**
   - Support ICC profile preservation for other source formats (PNG, TIFF)
   - Add support for video files in galleries
   - Implement tag-based filtering for galleries
   - Add gallery image browser/picker UI for posts editor

5. **General Improvements**
   - Consider adding image preloading for smoother transitions
   - Add configuration for replacement interval
   - Consider WebSocket for real-time updates
   - Add analytics for popular images