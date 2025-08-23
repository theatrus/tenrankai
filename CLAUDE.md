# DynServer Project Documentation

## Project Overview
DynServer is a web-based photo gallery server written in Rust using the Axum web framework. It provides a dynamic, responsive gallery interface with features like image resizing, metadata extraction, watermarking, and caching.

## Key Features
- **Responsive Web Gallery**: Mobile-friendly masonry layout that adapts to different screen sizes
- **Image Processing**: On-the-fly image resizing with caching for thumbnails, gallery, medium, and large sizes
- **High-DPI Support**: Automatic @2x image generation for retina displays
- **Metadata Extraction**: EXIF data parsing for camera info, GPS coordinates, and capture dates
- **Copyright Watermarking**: Intelligent watermark placement with automatic text color selection based on background
- **Performance Optimization**: Metadata caching, image caching, and background refresh
- **Markdown Support**: Folder descriptions and image captions via markdown files

## Project Structure

### Core Modules
- `src/main.rs` - Application entry point, configuration, and server setup
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
- `image_processing.rs` - Image resizing and serving
- `metadata.rs` - EXIF metadata extraction and processing
- `cache.rs` - Cache management and persistence
- `error.rs` - Error type definitions

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
- **Quality settings**: Configurable quality for both JPEG (default: 85) and WebP (default: 85.0)
- **Cache separation**: Different cache files for JPEG and WebP versions
- **Content negotiation**: Automatic format selection based on browser capabilities

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
- Requires DejaVuSans.ttf font in static directory

## Configuration

### Key Configuration Files
- `config.toml` - Main application configuration
- `cache/metadata_cache.json` - Persisted image metadata
- `cache/cache_metadata.json` - Cache version tracking

### Configuration Options
```toml
[gallery]
# Image quality settings
jpeg_quality = 85        # JPEG quality (1-100)
webp_quality = 85.0      # WebP quality (0.0-100.0)
```

### Environment Variables
- `RUST_LOG` - Controls logging verbosity (trace, debug, info, warn, error)

## Recent Changes and Fixes

### Social Media Embeds
- **Template Refactoring**: Inverted header/footer usage - now included within templates with parameters
- **Open Graph Support**: Added meta tags for rich previews on Facebook, Discord, etc.
- **Twitter Cards**: Added Twitter Card meta tags for image previews
- **Dynamic Meta Tags**: Each page can specify custom title, description, and image
- **Base URL Configuration**: Added `base_url` to app config for absolute URLs in meta tags

### Gallery Module Refactoring
- Split 3000-line gallery.rs into logical submodules
- Fixed "no images in directory" issue after refactoring
- Ensured gallery view uses metadata cache for performance
- Fixed missing gallery_url field in GalleryItem

### Mobile and Display Fixes
1. Gallery preview width adjustments for mobile
2. Fixed uneven margins (left/right)
3. Resolved iOS scrolling issues
4. Added @2x image support for high-DPI displays
5. Fixed image width calculations in masonry layout

### Copyright Watermarking
- Created new copyright module with intelligent color selection
- Integrated watermarking for medium-sized images only
- Fixed RGBA/JPEG compatibility issues

### Gallery Preview Improvements
- Changed from newest-first to random selection
- Fixed thumbnail vs gallery size issue
- Added dimension support for proper layout
- Added @2x support in preview

## Testing and Development

### Building and Running
```bash
cargo build
cargo run -- --host 0.0.0.0 --port 8080
```

### Common Commands
- Check warnings: `cargo build 2>&1 | grep warning`
- Run with debug logging: `RUST_LOG=debug cargo run`

### Testing URLs
- Gallery root: `http://localhost:8080/gallery`
- Specific folder: `http://localhost:8080/gallery/folder-name`
- Image with size: `http://localhost:8080/gallery/image/path/to/image.jpg?size=gallery`

## Known Issues and Considerations

1. **Font Loading**: Copyright watermarking requires DejaVuSans.ttf in the static directory
2. **Authentication**: Large image downloads require proper authentication cookie
3. **Performance**: Initial metadata extraction can be slow for large galleries
4. **Memory Usage**: Metadata cache grows with gallery size

## Code Style Guidelines
- No comments unless explicitly requested
- Follow existing patterns and conventions
- Use existing libraries (check Cargo.toml first)
- Prefer editing existing files over creating new ones
- Always handle errors appropriately
- Use proper Rust idioms (match, if let, etc.)

## Useful Patterns

### Template Meta Tags
To add social media meta tags to a template:
```liquid
{% assign page_title = "Your Page Title" %}
{% assign meta_description = "Page description for SEO" %}
{% assign og_title = "Open Graph Title" %}
{% assign og_description = "Open Graph description" %}
{% assign og_image = base_url | append: "/path/to/image.jpg" %}
{% assign og_image_width = "1200" %}
{% assign og_image_height = "630" %}
{% assign twitter_card_type = "summary_large_image" %}
{% include "_header.html.liquid" %}
```

### Adding New Image Sizes
1. Add size to the match statement in `get_resized_image`
2. Update size validation in `image_handler`
3. Add corresponding configuration in GalleryConfig

### Debugging Gallery Issues
1. Check `gallery_url` vs `thumbnail_url` usage
2. Verify metadata cache is being populated
3. Ensure dimensions are available for layout calculations
4. Check browser console for JavaScript errors

### Performance Optimization
1. Use metadata cache for dimensions instead of loading images
2. Batch operations where possible
3. Use background tasks for expensive operations
4. Leverage browser caching with proper headers

### Cache Management API
```rust
// Refresh single image metadata
gallery.refresh_single_image_metadata("path/to/image.jpg").await

// Refresh all images in a directory
gallery.refresh_directory_metadata("vacation-2024").await

// Manual cache save
gallery.save_metadata_cache().await

// Check cache dirty status
if gallery.metadata_cache_dirty.load(Ordering::Relaxed) {
    // Cache has unsaved changes
}
```

## Recent Major Changes (August 2025)

### OpenGraph Composite Image Preview
1. **New API Endpoint**: `/api/gallery/composite/{path}`
   - Generates a 2x2 grid composite image for OpenGraph previews
   - Creates a 1210x1210px image with 4 gallery images
   - Use `_root` as path for the root gallery
   - Returns JPEG with 1-hour cache header

2. **Composite Module** (`src/composite.rs`):
   - `create_composite_preview()` - Creates 2x2 grid from gallery images
   - `add_border()` - Adds colored border around images
   - Includes comprehensive unit tests with tempfile for testing
   - Handles missing images gracefully

3. **Composite Caching**:
   - Composite images are cached in the gallery cache system
   - Cache key format: `composite_{path}_composite_jpg` (slashes replaced with underscores)
   - Automatically creates cache directories if they don't exist
   - Served from cache if available, generated on demand if not
   - Stored as JPEG with configurable quality
   - Integrated with the existing image cache infrastructure
   - Includes comprehensive tests for cache storage and retrieval

3. **Gallery OpenGraph Integration**:
   - Gallery pages now use composite preview when 2+ images available
   - Single image galleries use the single image as preview
   - Proper dimensions included for optimal social media display
   - Automatic fallback to single image or no image

4. **Static File Serving Fix**:
   - Added dedicated `/static/*` route
   - Fixed path handling in template fallback
   - Properly strips `/static/` prefix when checking files

4. **Access Logging**:
   - Re-added HTTP access logging using tower-http TraceLayer
   - Logs method, path, query, user agent, referer
   - Logs response status, size, and latency
   - Uses `access_log` target for easy filtering

5. **Breadcrumb Improvements**:
   - All breadcrumb links now clickable on both gallery and image detail pages
   - Added `build_breadcrumbs_with_mode` method with `all_clickable` parameter
   - Consistent navigation experience across all pages

## Recent Major Changes (August 2025)

### Library/Binary Architecture Refactoring
- **Created lib.rs**: Separated library components from binary
- **Moved Types**: All config types (Config, ServerConfig, etc.) now in lib.rs
- **Public API**: Exposed modules and types for external use
- **Cleaner main.rs**: Binary now just handles CLI and server startup
- **Benefits**: Better code organization, reusable components, testability

### Gallery Preview API & Dynamic Updates
1. **New API Endpoint**: `/api/gallery/preview`
   - Accepts `count` parameter (default 6, max 20)
   - Returns JSON with gallery preview images
   - Example: `/api/gallery/preview?count=12`

2. **Client-Side Gallery Preview**:
   - Removed server-side rendering of gallery preview data
   - Gallery preview now fetches data via API
   - Added dynamic image replacement every 10-15 seconds
   - Smooth fade transitions when swapping images

3. **Random Image Selection Fix**:
   - Fixed issue where images were shuffled but then sorted by date
   - Now truly random selection on each API call
   - Multiple shuffle passes for better randomness

4. **DOM Replacement Improvements**:
   - Added `data-image-path` attributes for reliable element identification
   - Robust fallback logic for finding elements to replace
   - Proper error handling during DOM manipulation
   - Force reflow for smooth animations

### Template System Updates
1. **Gallery Preview as Partial**:
   - Converted gallery preview to a Liquid partial like header/footer
   - Removed dependency on server-side variables
   - Simplified template rendering logic

2. **Fixed Route Mismatches**:
   - Changed `/gallery/info/` to `/gallery/detail/` to match templates
   - Changed `/api/download/verify` to `/api/verify` to match client code

3. **Wildcard Route Syntax**:
   - Updated from `/*path` to `/{*path}` for Axum 0.8 compatibility

### Code Quality Improvements
- Fixed all compiler warnings (unused variables, imports)
- Updated test dependencies (axum-test v17)
- Cleaned up debug logging (can be enabled with `?debug_replacement`)
- Improved error messages and handling

## Debugging Tips

### Gallery Preview Issues
1. **Images not replacing**: Check browser console for errors
2. **Debug mode**: Add `?debug_replacement` to URL for detailed logs
3. **API testing**: Visit `/api/gallery/preview` directly to see JSON response
4. **DOM inspection**: Check for `data-image-path` attributes on preview links

### Template Rendering Issues
1. **Missing includes**: Ensure partial exists in templates directory
2. **Variable errors**: Check template doesn't reference removed variables
3. **Test rendering**: Use integration tests to verify templates work

### Performance Monitoring
- Gallery preview API calls happen every 10-15 seconds
- 30% chance of fetching fresh images on each cycle
- Images pool can grow up to 20 for variety
- Smooth 0.5s fade out → replace → 0.5s fade in

## Future Improvements
1. Consider adding image preloading for smoother transitions
2. Add configuration for replacement interval
3. Consider WebSocket for real-time updates
4. Add analytics for popular images