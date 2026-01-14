# Tenrankai Project Documentation

## Project Overview
Tenrankai is a web-based photo gallery server written in Rust using the Axum web framework. It provides a dynamic, responsive gallery interface with features like image resizing, metadata extraction, watermarking, and caching. The system supports multiple independent gallery instances, each with its own configuration, URL prefix, and content directories.

## Development Commands & Best Practices

### Frontend Development (React + TypeScript)

**Build Commands:**
```bash
# Clean build (removes old build artifacts)
npm run clean && npm run build

# Development build with type checking
npm run type-check && npm run build

# Production build
npm run build:prod

# Note: Frontend is automatically built during cargo build
# To skip frontend build: TENRANKAI_SKIP_FRONTEND=1 cargo build
```

**Development Server (Frontend Only):**
```bash
# Start Vite dev server (for frontend development)
npm run dev
# Runs on http://localhost:5173/ with hot reload
# Proxies API calls to Rust server on localhost:3000
```

### Backend Development (Rust + Axum)

**Build Commands:**
```bash
# With AVIF support (default, recommended)
cargo build
cargo build --release    # Production build

# Without AVIF support (faster builds, Windows-friendly)
cargo build --no-default-features
cargo build --release --no-default-features
```

**Server Commands:**
```bash
# Standard server startup
cargo run --no-default-features -- serve
cargo run --no-default-features -- serve --port 8080 --host 0.0.0.0

# Development with auto-shutdown (for testing/CI)
cargo run --no-default-features -- serve --quit-after 5

# Check available commands
cargo run --no-default-features -- --help
cargo run --no-default-features -- serve --help
```

### Full-Stack Development Workflow

**Option 1: Separate Terminals (Recommended)**
```bash
# Terminal 1: Start Rust backend
cargo run --no-default-features -- serve

# Terminal 2: Start frontend dev server
npm run dev

# Visit http://localhost:5173/ for hot-reloading frontend
# API calls automatically proxy to Rust server on :3000
```

**Option 2: Production-like Testing**
```bash
# Build frontend for production
npm run build

# Run Rust server with built assets
cargo run --no-default-features -- serve

# Visit http://localhost:8080/ (or configured port)
```

### Testing & Quality Assurance

**Frontend Testing:**
```bash
npm run type-check     # TypeScript type checking
npm run build          # Build test (catches build errors)
```

**Backend Testing:**
```bash
# Fast tests (no AVIF, for quick iteration)
cargo test --no-default-features                    # Run all tests (~180 tests)
cargo test --no-default-features api::tests         # Run specific test module
cargo clippy --no-default-features -- -D warnings   # Lint checking
cargo fmt --check                                    # Format checking

# Full tests with AVIF support (run before merging PRs)
cargo test                                          # Run all tests (~235 tests)
cargo clippy -- -D warnings                         # Lint with all features
```

**Integration Testing:**
```bash
# Quick server startup test
cargo run --no-default-features -- serve --quit-after 3

# Test with frontend build
npm run build && cargo run --no-default-features -- serve --quit-after 5
```

**Test Coverage Notes:**
- `--no-default-features`: ~180 tests, faster builds, no AVIF dependencies
- Default features (AVIF enabled): ~235 tests, includes 19 AVIF-specific tests
- Always run full tests (`cargo test`) before merging to ensure AVIF code paths work

### Important Notes for AI Development

1. **Always use `--no-default-features`** for faster builds during development
2. **Use `--quit-after N`** for testing server startup without manual termination
3. **Frontend is automatically built** during `cargo build` (use `TENRANKAI_SKIP_FRONTEND=1` to skip)
4. **Check both TypeScript and Rust code** before committing changes
5. **Use separate terminals for frontend/backend** during active development with hot reload

### Common Troubleshooting

**Build Issues:**
```bash
# Clean everything and rebuild
npm run clean
cargo clean
npm run build
cargo build --no-default-features
```

**Port Conflicts:**
```bash
# Use different ports if needed
cargo run --no-default-features -- serve --port 3001
npm run dev  # Will automatically adjust proxy target
```

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

#### Multi-Directory Template Loading
Tenrankai supports loading templates from multiple directories with a precedence system:

```toml
[templates]
# Single directory (backward compatible)
directories = "templates"

# OR multiple directories (first match wins)
directories = ["templates-custom", "templates"]
```

This allows flexible customization:
- Override specific templates while keeping defaults
- Templates and partials are searched in directory order
- First matching file is used
- Mix templates and partials from different directories
- Perfect for themes, branding, or A/B testing

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

### Image URL Indexing
The gallery supports three modes for generating image URLs:
- **Filename Mode** (default): Uses actual filenames in URLs (e.g., `/gallery/image/IMG_1234.jpg`)
  - Pros: Direct file access, easy debugging, predictable URLs
  - Cons: Exposes internal filenames, may contain personal information
- **Sequence Mode**: Uses sequential numbers (e.g., `/gallery/image/1`, `/gallery/image/2`)
  - Pros: Clean URLs, no filename exposure
  - Cons: URLs change when images are added/removed, not stable across deployments
- **Unique ID Mode**: Uses 6-character base36 hash IDs (e.g., `/gallery/image/a8k3m9`)
  - Pros: Stable URLs, no filename exposure, clean and short
  - Cons: Not human-readable, requires reverse lookup
- Configure per gallery with `image_indexing = "filename"` (or "sequence" or "unique_id")

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
# Use permissions.roles.<role>.permissions.can_see_exact_dates instead of deprecated approximate_dates_for_public
copyright_holder = "Your Name"            # Copyright holder for watermarking medium images
image_indexing = "filename"               # Image URL mode: "filename", "sequence", or "unique_id"
                                         # - filename: Use actual filename in URLs (default)
                                         # - sequence: Use sequential numbers (1, 2, 3...)
                                         # - unique_id: Use base36 hash IDs (a8k3m9, b2n7x4...)

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
image_indexing = "unique_id"              # Use hash IDs for cleaner portfolio URLs
# No new_threshold_days - this gallery won't highlight new images
```

### Environment Variables
- `RUST_LOG` - Controls logging verbosity (trace, debug, info, warn, error)

## React + Rust Development Guide

### Modern Frontend Architecture
Tenrankai uses React with TypeScript for enhanced user interfaces, built with Vite and served alongside the Rust backend. The frontend uses:

- **Progressive Enhancement**: Server-side rendered pages work without JavaScript
- **React SPA Features**: Enhanced navigation, real-time updates, improved UX
- **ESM Modules**: Modern JavaScript module system (no CJS deprecation warnings)
- **TypeScript**: Full type safety across frontend and API integration
- **Embedded JSON**: Fast initial page loads with server-rendered data

### Quick Start Commands

**Development (Recommended - Hot Reload):**
```bash
# Terminal 1: Backend server
cargo run --no-default-features -- serve

# Terminal 2: Frontend dev server with hot reload
npm run dev

# Visit http://localhost:5173/ for development
# Frontend automatically proxies API calls to :3000
```

**Production Testing:**
```bash
# Build frontend assets
npm run build

# Run integrated server
cargo run --no-default-features -- serve --port 8080

# Visit http://localhost:8080/ for production-like testing
```

**Quick Testing (Auto-shutdown):**
```bash
# Test server startup and shutdown
cargo run --no-default-features -- serve --quit-after 5

# Test with frontend built
npm run build && cargo run --no-default-features -- serve --quit-after 3
```

### Docker Support

The project includes production-ready Dockerfiles with multi-stage builds:

#### Available Dockerfiles
- **`Dockerfile`** - Full build with AVIF support
  - Includes all features including HDR AVIF with gain maps
  - Uses Rust 1.89 with release builds
  - Final image: ~168 MB (release mode)
  
- **`Dockerfile.no-avif`** - Build without AVIF support  
  - Faster builds, smaller image
  - No complex AVIF dependencies
  - Final image: ~130 MB (release mode)

#### Building Docker Images
```bash
# Build with full AVIF support (recommended)
docker build -t tenrankai:latest .

# Build without AVIF support (faster builds)
docker build -f Dockerfile.no-avif -t tenrankai:no-avif .

# Using podman
podman build -t tenrankai:latest .
```

#### Docker Features
- **Production-optimized**: Uses `cargo build --release` for optimal performance
- **Multi-stage builds**: Minimal final image with only runtime dependencies
- **Security**: Runs as non-root user (appuser, UID 1001)
- **Full AVIF support**: Includes HDR and gain map preservation
- **GitHub Actions**: Automated builds and publishing to ghcr.io
- **Multi-architecture**: Supports both linux/amd64 and linux/arm64
- **Size comparison**:
  - Debug builds: ~700 MB image, 585 MB binary
  - Release builds: ~168 MB image, 45 MB binary
  - 93% smaller binary, ~75% smaller image with release mode

### Essential Development Commands

**Frontend Commands:**
```bash
npm run type-check              # TypeScript type checking
npm run build                    # Build React components
npm run build:prod              # Production build with optimizations
npm run clean                    # Clean build artifacts
npm run dev                      # Development server with hot reload
```

**Backend Commands:**
```bash
# Server operations
cargo run --no-default-features -- serve                    # Standard server
cargo run --no-default-features -- serve --quit-after 5     # Auto-shutdown testing
cargo run --no-default-features -- serve --port 8080        # Custom port

# Development builds
cargo build --no-default-features                           # Fast dev build
cargo build --release --no-default-features                # Production build

# Quality assurance
cargo test --no-default-features                           # Run tests
cargo clippy --no-default-features -- -D warnings          # Lint checking
cargo fmt --check                                           # Format checking

# Advanced commands
cargo run --no-default-features -- avif-debug file.avif    # AVIF analysis
cargo run --no-default-features -- user --help             # User management
```

**Quality Checks (Run Before Commit):**
```bash
# Frontend checks
npm run type-check && npm run build

# Backend checks  
cargo clippy --no-default-features -- -D warnings && cargo fmt --check

# Full integration test
npm run build && cargo run --no-default-features -- serve --quit-after 3
```

### Development URLs

**Frontend Development (Hot Reload):**
- Frontend dev server: `http://localhost:5173/`
- Gallery pages: `http://localhost:5173/gallery/`
- Image details: `http://localhost:5173/gallery/detail/image.jpg`
- API calls automatically proxy to Rust server

**Backend Server (Production-like):**
- Main server: `http://localhost:8080/` (or configured port)
- Gallery root: `http://localhost:8080/gallery`
- Image detail: `http://localhost:8080/gallery/detail/image.jpg`
- JSON API: `http://localhost:8080/api/gallery/main/preview?count=12`
- Health check: `http://localhost:8080/api/health`

**React Enhancement Testing:**
- Check browser console for "✅ Enhanced with React" messages
- Verify SPA navigation works (no full page reloads)
- Test keyboard shortcuts (←/→ arrows in image detail)
- Confirm embedded JSON data loads instantly

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

## AppState and Multi-Site Architecture

### Handler Pattern
All HTTP handlers must use `ResolvedState` instead of `State<AppState>`:
```rust
// CORRECT - uses ResolvedState which handles multi-site resolution
pub async fn my_handler(
    ResolvedState(app_state): ResolvedState,
) -> impl IntoResponse {
    // ...
}

// WRONG - bypasses site resolution middleware
pub async fn my_handler(
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    // ...
}
```

### Accessing Site-Specific Values
Always use accessor methods on `AppState` - never access `config` fields directly:

```rust
// CORRECT - use accessor methods (per-site values)
let secret = app_state.cookie_secret();
let url = app_state.base_url();
let name = app_state.app_name();
let has_auth = app_state.user_database_manager().is_some();

// WRONG - accessing config directly bypasses per-site values
let secret = app_state.config.app.cookie_secret;  // Don't do this
let has_auth = app_state.config.app.user_database.is_some();  // Don't do this
```

### Why This Matters
In multi-site mode, each site can have different:
- `cookie_secret` - for signing authentication cookies
- `base_url` - for generating absolute URLs
- `app_name` - displayed name
- `user_database` - authentication configuration
- `galleries`, `posts`, `templates`, `static_files`

The `ResolvedState` extractor and accessor methods ensure the correct per-site values are used.

### Available Accessor Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `cookie_secret()` | `&str` | Cookie signing secret (per-site) |
| `base_url()` | `Option<&str>` | Base URL for links (per-site) |
| `app_name()` | `&str` | Application name (per-site) |
| `user_database_manager()` | `&Option<UserDatabaseManager>` | Auth config (per-site) |
| `template_engine()` | `&Arc<TemplateEngine>` | Templates (per-site) |
| `static_handler()` | `&StaticFileHandler` | Static files (per-site) |
| `galleries()` | `&Arc<HashMap<...>>` | Galleries (per-site) |
| `posts_managers()` | `&Arc<HashMap<...>>` | Posts (per-site) |
| `login_state()` | `&Arc<RwLock<LoginState>>` | Login state (per-site) |
| `email_config()` | `Option<&SiteEmailConfig>` | Email config (per-site) |

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
- **Always use `ResolvedState` in handlers** - Never use `State<AppState>` directly (see AppState section above)

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

### Pluggable Storage Abstraction (January 2026)
1. **Storage Trait Architecture** (`src/storage/`):
   - Unified `Storage` trait with async operations (read, write, list, delete, metadata)
   - `DynStorage` type alias for `Arc<dyn Storage>` enables runtime polymorphism
   - Streaming support with `read_stream()` and range reads via `read_range()`
   - Signed URL generation for S3 redirect support

2. **Storage Backends**:
   - **Filesystem** (`filesystem.rs`): Default backend using tokio async filesystem
   - **S3** (`s3.rs`): Full AWS S3 support with presigned URLs, streaming, and metadata
   - URL-based configuration: `s3://bucket/prefix?region=us-west-2`

3. **Sync/Async Bridge** (`sync_adapter.rs`):
   - `SyncStorageAdapter` for `spawn_blocking` contexts (image processing)
   - `SyncStorageReader` implements `Read + Seek` with range-based seeking
   - Chunked caching for efficient random access on remote storage
   - Optional prefetch mode for full-file downloads

4. **Components with S3 Support**:
   | Component | S3 Support | Notes |
   |-----------|------------|-------|
   | Static files | ✅ | With signed URL redirects |
   | Templates | ✅ | Multi-directory with precedence |
   | Posts | ✅ | Markdown from S3 |
   | Gallery cache | ✅ | Metadata + processed images |
   | Gallery source | ✅ | Images from S3 |

5. **Configuration Examples**:
   ```toml
   # S3 cache with local source
   [[galleries]]
   source_directory = "photos"
   cache_directory = "s3://bucket/cache?region=us-west-2"

   # Static files from S3 with redirects
   [static_files]
   directories = ["s3://bucket/static?region=us-west-2"]
   use_redirects = true

   # Templates with S3 fallback
   [templates]
   directories = ["templates-local", "s3://bucket/templates"]
   ```

6. **MetadataStorage System** (`src/metadata_storage/`):
   - Pluggable metadata storage for image sidecar files
   - LRU cache with configurable size and TTL
   - Supports `.md` (markdown with TOML frontmatter) and `.toml` formats
   - Automatic cache invalidation based on file modification time

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
1. **Folder-Level Access Control** (Updated to role-based permissions):
   - Use `permissions.public_role = "none"` to require authentication
   - Define custom roles with specific permissions under `permissions.roles`
   - Assign users to roles via `permissions.user_roles`
   - Hierarchical access control (parent folder restrictions apply to children)
   - Fine-grained permissions for viewing, downloading, metadata access, etc.

### Image URL Indexing (January 2026)
1. **Flexible Image URL Generation**:
   - Three indexing modes: filename (default), sequence, and unique_id
   - Per-gallery configuration with `image_indexing` setting
   - Filename mode: Direct file access, easy debugging
   - Sequence mode: Clean numbered URLs (1, 2, 3...)
   - Unique ID mode: 6-character base36 hash IDs for privacy
   - Stable IDs based on FNV hash of file path
   - Reverse lookup system for ID/sequence to filename mapping

### Multi-Directory Template System (December 2025)
1. **Template Override Support**:
   - Multiple template directories with precedence-based loading
   - First matching template/partial wins
   - Allows partial overrides while keeping defaults
   - Perfect for themes, custom branding, A/B testing
   - Docker-friendly for mounting custom templates

2. **Configuration**:
   ```toml
   [templates]
   # Single directory
   directories = "templates"
   
   # Multiple directories (first match wins)
   directories = ["themes/dark", "templates"]
   ```

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

## Modern Development Stack (2026)

### Frontend Technologies
- **React 18**: Modern React with hooks and concurrent features
- **TypeScript**: Full type safety with strict checking enabled
- **Vite 5**: Lightning-fast builds and hot module replacement
- **ESM**: Modern module system (no CommonJS deprecation warnings)
- **Progressive Enhancement**: Pages work without JavaScript, enhanced with React

### Key Architectural Decisions

1. **Embedded JSON Strategy**:
   - Server renders initial data as JSON in HTML
   - React hydrates instantly without API calls
   - Subsequent navigation uses API for SPA experience
   - Perfect Lighthouse scores for initial page load

2. **Authentication Integration**:
   - Server-side privacy filtering preserved in JSON embedding
   - Client-side permission checks for UX
   - API respects all security policies
   - No sensitive data exposure to unauthorized users

3. **Build System**:
   - Development: `npm run dev` + `cargo run serve` (separate processes)
   - Production: `npm run build` → static assets served by Rust
   - TypeScript compilation and type checking integrated
   - No build tool warnings or deprecations

### Development Best Practices

**Always Use These Commands:**
```bash
# Backend development (faster builds)
cargo run --no-default-features -- serve
cargo build --no-default-features
cargo test --no-default-features

# Auto-shutdown for testing
cargo run --no-default-features -- serve --quit-after N

# Frontend development
npm run dev              # Hot reload development
npm run type-check       # TypeScript validation
npm run build           # Production build test
```

**Code Quality Checklist:**
- [ ] TypeScript types are correct (`npm run type-check`)
- [ ] Rust code compiles without warnings (`cargo clippy --no-default-features`)
- [ ] Frontend builds successfully (`npm run build`)
- [ ] Server starts without errors (test with `--quit-after`)
- [ ] React enhancement works (check browser console)
- [ ] Full test suite passes before merging (`cargo test && cargo clippy -- -D warnings`)

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