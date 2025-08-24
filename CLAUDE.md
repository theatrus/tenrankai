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
- **Copyright Watermarking**: Intelligent watermark placement with automatic text color selection based on background
- **Performance Optimization**: Metadata caching, image caching, and background refresh
- **Markdown Support**: Folder descriptions and image captions via markdown files
- **New Image Highlighting**: Automatic highlighting of recently modified images based on configurable threshold
- **Multiple Blog Systems**: Support for multiple independent markdown-based blog/posts systems
- **Dark Theme Code Blocks**: Optimized code block styling for readability with proper contrast

## Project Structure

### Core Modules
- `src/main.rs` - Application entry point, configuration, and server setup
- `src/api.rs` - API endpoints for health checks and authentication
- `src/templating.rs` - Liquid template engine integration
- `src/copyright.rs` - Watermarking functionality with intelligent text color selection
- `src/composite.rs` - Composite image generation for OpenGraph previews
- `src/posts/` - Posts/blog system for markdown-based content

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

### Posts Module (`src/posts/`)
A flexible markdown-based posts/blog system supporting multiple independent collections:
- `mod.rs` - Module exports
- `types.rs` - Post, PostSummary, PostsConfig structures
- `core.rs` - PostsManager for scanning, caching, and serving posts
- `handlers.rs` - HTTP handlers for posts index and detail pages
- `error.rs` - Posts-specific error types
- `tests.rs` - Comprehensive test suite

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
- **ICC Profile Preservation**: Full support for color profiles in both JPEG and WebP formats
  - JPEG: ICC profiles extracted from source and preserved in output
  - WebP: ICC profiles embedded using libwebp-sys (v0.13+) WebPMux API
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

## Recent Changes and Fixes

### Social Media Embeds
- **Template Refactoring**: Inverted header/footer usage - now included within templates with parameters
- **Open Graph Support**: Added meta tags for rich previews on Facebook, Discord, etc.
- **Twitter Cards**: Added Twitter Card meta tags for image previews
- **Dynamic Meta Tags**: Each page can specify custom title, description, and image
- **Base URL Configuration**: Added `base_url` to app config for absolute URLs in meta tags

### Multi-Gallery Architecture (December 2024)
- **Complete refactoring to support multiple gallery instances**
- Each gallery can have its own:
  - URL prefix (e.g., `/gallery`, `/portfolio`, `/family-photos`)
  - Source and cache directories
  - Image size configurations
  - Quality settings
  - Preview settings
  - Templates
- **Removed all backward compatibility** - only named galleries are supported
- **API changes**:
  - All API endpoints now include gallery name: `/api/gallery/{name}/preview`
  - Composite image API: `/api/gallery/{name}/composite/{path}`
- **Template changes**:
  - Gallery preview partial now requires `gallery_name` and `gallery_url` parameters
  - All gallery URLs are dynamically generated based on configuration

### Gallery Module Refactoring
- Split 3000-line gallery.rs into logical submodules
- Fixed "no images in directory" issue after refactoring
- Ensured gallery view uses metadata cache for performance
- Fixed missing gallery_url field in GalleryItem
- **Fixed duplicate image name in breadcrumbs** - breadcrumbs now correctly show only the directory path on image detail pages

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

### Testing URLs
- Gallery root: `http://localhost:8080/gallery` (for main gallery)
- Portfolio root: `http://localhost:8080/my-portfolio` (for portfolio gallery)
- Specific folder: `http://localhost:8080/gallery/folder-name`
- Image with size: `http://localhost:8080/gallery/image/path/to/image.jpg?size=gallery`
- Gallery preview API: `http://localhost:8080/api/gallery/main/preview?count=12`
- Composite image: `http://localhost:8080/api/gallery/main/composite/_root`

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
- **Always use `thiserror` crate for error types** - Define errors with `#[derive(Error)]` and `#[error("...")]` attributes
- **Always run `cargo fmt` before finalizing code** - Ensure consistent formatting across the codebase
- **Always run `cargo clippy` and fix warnings** - Ensure code follows Rust best practices and catches common mistakes

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

### Gallery Preview Partial
To include a gallery preview in any template:
```liquid
{% comment %} For the main gallery {% endcomment %}
{% assign gallery_name = "main" %}
{% assign gallery_url = "/gallery" %}
{% include "partials/_gallery_preview.html.liquid" %}

{% comment %} For a different gallery {% endcomment %}
{% assign gallery_name = "portfolio" %}
{% assign gallery_url = "/my-portfolio" %}
{% include "partials/_gallery_preview.html.liquid" %}
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

## Recent Major Changes (August 2025) - Continued

### SEO and Web Crawler Support
1. **Robots.txt Support**:
   - Added `/robots.txt` handler for proper search engine crawler guidance
   - Default permissive configuration allows all crawlers
   - Includes crawl-delay of 1 second to be respectful of resources
   - Custom robots.txt support: Place a `robots.txt` file in the static directory to override default
   
2. **Default robots.txt Content**:
   ```
   User-agent: *
   Allow: /
   Crawl-delay: 1
   ```
   
3. **Custom Override**:
   - Simply place a custom `robots.txt` file in your static directory
   - The server will automatically serve your custom file instead of the default
   - Useful for adding sitemap locations or specific crawler rules

### Cache Pre-generation
1. **New Configuration Option**: `pregenerate_cache = true`
   - When enabled, automatically generates all image sizes after metadata refresh
   - Generates thumbnail, gallery, medium sizes (plus @2x variants) for both JPEG and WebP
   - Runs after initial metadata load and scheduled refreshes
   
2. **Pre-generation Features**:
   - Concurrent processing with rate limiting (4 images at a time)
   - Progress tracking logs every 10 images processed
   - Generates up to 12 variations per image (6 sizes × 2 formats)
   - Skips already cached images for efficiency
   - Complete timing statistics on completion
   
3. **Performance Benefits**:
   - First-time visitors get instant image loading
   - No lag while images are processed on-demand
   - Background processing doesn't block server operation

### Template Reorganization (August 2025)
1. **Directory Structure**: Templates are now organized into subdirectories
   - `templates/pages/` - Contains all page templates
   - `templates/partials/` - Contains reusable partial templates
   
2. **Benefits**:
   - Better organization and maintainability
   - Clear separation between full pages and components
   - Easier to find specific templates
   
3. **Code Updates**:
   - All template loading code updated to use new paths
   - Tests updated to reflect new structure
   - No breaking changes for end users

### Posts System Implementation (August 2025)
1. **Multiple Blog Systems**: Added support for multiple independent markdown-based blog/posts systems
   - Each system has its own source directory, URL prefix, and configuration
   - Examples: /blog, /stories, /instructions, /documentation
   
2. **Post Format**:
   - Markdown files with TOML front matter (title, summary, date)
   - Full CommonMark support with extensions (tables, strikethrough, footnotes)
   - Automatic HTML generation and caching
   - **Gallery Image References**: Easy embedding of gallery images with automatic linking
   
3. **Features**:
   - Chronological sorting (newest first)
   - Pagination support
   - Subdirectory organization (URLs reflect directory structure)
   - Dynamic refresh via API (`POST /api/posts/{name}/refresh`)
   - Configurable templates for index and detail pages
   - **Gallery Integration**: Reference images from any configured gallery with smart size handling
   - **Automatic Reload on Change**: Posts are automatically reloaded when their markdown files are modified
   
4. **Implementation Details**:
   - PostsManager handles scanning, caching, and serving posts
   - Posts are cached in memory and refreshed on startup
   - Supports both simple date (YYYY-MM-DD) and RFC3339 formats
   - Comprehensive test coverage including markdown rendering tests
   - Uses chrono's built-in date formatting for human-readable dates
   - Dark theme optimized code blocks with #2d2d2d background and #f8f8f2 text
   - Inline code uses light background (#e8e8e8) with dark text (#333) for contrast
   - Gallery references are processed during markdown rendering to generate proper HTML
   - File modification times are tracked to enable automatic reloading

### Multi-Gallery Support (August 2025 - Updated December 2024)
1. **Multiple Gallery Instances**: The gallery module now supports multiple independent gallery instances
   - Each gallery has its own source directory, cache directory, and URL prefix
   - Similar architecture to the posts system for consistency
   - **BREAKING CHANGE (December 2024)**: Removed backward compatibility - only named galleries are supported

2. **Configuration Changes**:
   - Changed from single `[gallery]` section to `[[galleries]]` array format
   - Each gallery requires a unique `name` identifier
   - Custom URL prefixes allow galleries at any path (e.g., `/gallery`, `/portfolio`, `/photos/archive`)
   - Per-gallery configuration for image quality, pagination, cache settings, etc.

3. **Example Configuration**:
   ```toml
   [[galleries]]
   name = "main"
   url_prefix = "/gallery"
   source_directory = "/path/to/photos"
   cache_directory = "cache/main"
   gallery_template = "modules/gallery.html.liquid"
   image_detail_template = "modules/image_detail.html.liquid"
   images_per_page = 12
   jpeg_quality = 85
   webp_quality = 85.0
   pregenerate_cache = false
   cache_refresh_interval_minutes = 60

   [galleries.preview]
   max_depth = 3
   max_per_folder = 2
   max_images = 6

   [[galleries]]
   name = "portfolio"
   url_prefix = "/portfolio"
   source_directory = "/path/to/portfolio"
   cache_directory = "cache/portfolio"
   gallery_template = "modules/gallery.html.liquid"
   image_detail_template = "modules/image_detail.html.liquid"
   images_per_page = 20
   ```

4. **URL Structure**:
   - Gallery root: `/{url_prefix}/`
   - Gallery folders: `/{url_prefix}/folder/subfolder`
   - Image serving: `/{url_prefix}/image/path/to/image.jpg?size=gallery`
   - Image detail: `/{url_prefix}/detail/path/to/image.jpg`
   - API preview: `/api/gallery/{name}/preview`
   - API composite: `/api/gallery/{name}/composite/path`

5. **Implementation Details**:
   - AppState now contains `galleries: Arc<HashMap<String, gallery::SharedGallery>>`
   - Routes are dynamically registered for each configured gallery
   - All handlers support named galleries while maintaining backward compatibility
   - First gallery in configuration serves as default for legacy routes
   - Each gallery maintains its own metadata cache and background refresh tasks

6. **Migration from Single Gallery**:
   - Legacy single gallery configurations must be converted to the array format
   - All gallery references must include the gallery name
   - Templates must pass `gallery_name` and `gallery_url` parameters to the gallery preview partial

## Recent Bug Fixes (December 2024)

### Breadcrumb Duplication Fix
- **Issue**: Image names were appearing twice in breadcrumbs on image detail pages
- **Cause**: Breadcrumbs were built with the full image path, then the template added the name again
- **Fix**: Changed to build breadcrumbs using only the parent directory path
- **Result**: Clean navigation showing: Gallery → Folder → Subfolder → Image Name

### Test Suite Fixes
- **URL Prefix Requirements**: Fixed failing tests by ensuring all URL prefixes start with `/`
- **Configuration Updates**: Updated test configurations to use the new multi-gallery structure
- **Template Paths**: Fixed template paths in tests to match actual file locations

## New Features (December 2024)

### Automatic Post Reload on File Change
Posts now automatically reload when their markdown files are modified, making development and content editing more efficient.

#### How It Works
- When a post is requested, the system checks if the file has been modified since it was last loaded
- If the file is newer, it's automatically reloaded and reprocessed
- The updated content is immediately available without restarting the server

#### Implementation Details
- Modification times are tracked using `SystemTime` for each loaded post
- The `get_post()` method automatically handles freshness checking
- Minimal performance impact - file metadata is only checked on access
- Thread-safe implementation using RwLock for concurrent access

#### Benefits
- Live editing experience during development
- No need to restart the server when editing posts
- Content updates are immediate
- Preserves memory efficiency by only reloading changed posts

### Gallery Image References in Posts
Posts can now easily reference and embed images from any configured gallery with automatic link generation.

#### Syntax
```markdown
![gallery:gallery_name:path/to/image.jpg](size)
```

- `gallery_name`: The name of the gallery (as configured in config.toml)
- `path/to/image.jpg`: The path to the image within that gallery
- `size`: The desired image size (thumbnail, gallery, medium, or large)

#### Examples
```markdown
# My Blog Post

Here's a thumbnail that links to the full gallery view:
![gallery:main:vacation/beach.jpg](thumbnail)

A larger gallery-sized image:
![gallery:main:vacation/sunset.jpg](gallery)

An image from my portfolio gallery:
![gallery:portfolio:projects/app-screenshot.png](medium)
```

#### Generated HTML
The markdown processor automatically converts gallery references into proper HTML:
```html
<a href="/gallery/detail/vacation%2Fbeach.jpg" class="gallery-image-link">
    <img src="/gallery/image/vacation%2Fbeach.jpg?size=thumbnail" 
         alt="beach.jpg" loading="lazy" 
         class="gallery-image gallery-image-thumbnail" />
</a>
```

#### Features
- Automatic URL encoding for proper path handling
- Links wrap images to navigate to the gallery detail view
- Lazy loading for better performance
- CSS classes for easy styling
- Falls back gracefully if gallery or image doesn't exist

## Integration Testing

The project includes comprehensive integration tests for all major features:

### Gallery Integration Tests (`tests/gallery_integration_tests.rs`)
- Multiple gallery instances with different configurations
- Pagination functionality
- OpenGraph metadata generation (composite and single images)
- Gallery preview API
- Breadcrumb navigation
- Folder metadata display
- 404 handling for non-existent galleries

### Posts Integration Tests (`tests/posts_integration.rs`)
- Multiple posts systems
- Pagination
- Post rendering with markdown
- API refresh functionality
- Subdirectory support
- 404 handling

### Template Integration Tests (`tests/template_integration.rs`)
- Template rendering with partials
- Gallery preview inclusion
- Error handling for missing templates
- Meta tag generation

## Recent Changes (December 2025)

### Hidden Gallery Folders
1. **TOML Front Matter Support in _folder.md**:
   - Gallery folders can now use TOML front matter similar to posts
   - Allows configuration of folder behavior and metadata
   - Backward compatible - folders without TOML continue to work as before
   
2. **Hidden Folder Feature**:
   - Folders can be marked as `hidden = true` in TOML config
   - Hidden folders are excluded from:
     - Gallery listings (scan_directory)
     - Gallery preview images
     - Image counts
     - Recursive directory traversal
   - Hidden folders remain accessible if you know the direct URL
   
3. **_folder.md Format**:
   ```markdown
   +++
   hidden = true
   title = "Private Photos"
   +++
   
   # Optional Markdown Title
   
   Description content in markdown...
   ```
   
4. **Configuration Options**:
   - `hidden`: Boolean flag to hide folder from listings (default: false)
   - `title`: Override folder display name (optional)
   
5. **Implementation Details**:
   - New types: `FolderConfig` and `FolderMetadata` 
   - `read_folder_metadata_full()` returns full metadata including config
   - `read_folder_metadata()` maintains backward compatibility
   - Title priority: TOML title > Markdown # heading > folder name

### Posts Periodic Refresh
1. **Added Configurable Posts Refresh**:
   - Posts can now be automatically refreshed at a configurable interval
   - New configuration option: `refresh_interval_minutes` in posts config
   - Similar to gallery metadata refresh functionality
   
2. **Configuration Example**:
   ```toml
   [[posts]]
   name = "blog"
   source_directory = "content/blog"
   url_prefix = "/blog"
   posts_per_page = 10
   refresh_interval_minutes = 30  # Refresh every 30 minutes
   ```
   
3. **Features**:
   - Background task spawned for each posts system with refresh enabled
   - Non-blocking refresh - server continues serving while refreshing
   - Automatic detection of new, modified, or deleted posts
   - Individual post reloading on access if file has changed
   
4. **Implementation Details**:
   - `PostsManager::start_background_refresh()` spawns tokio task
   - Initial refresh happens on startup
   - Logs refresh start and completion/errors
   - Skips first immediate tick to avoid duplicate initial refresh

### Gallery OpenGraph Metadata Fix
1. **Fixed Generic OpenGraph Tags**:
   - Gallery pages now use folder display name and description for OpenGraph metadata
   - Previously showed hardcoded "Photo Gallery" and "Browse our collection of photos"
   - Now properly displays custom folder titles from `_folder.md` files
   
2. **Smart Description Processing**:
   - HTML tags are stripped from folder descriptions for social media previews
   - Descriptions are truncated to 160 characters for optimal display
   - Falls back to generic description if no folder description is available
   
3. **Improved Page Titles**:
   - Root gallery shows "Photo Gallery"
   - Subfolders show "[Folder Display Name] - Photo Gallery"
   - Uses folder display names when available, otherwise uses folder names

### ICC Color Profile Implementation  
1. **JPEG ICC Profile Extraction**:
   - Manually parses JPEG file structure to find APP2 segments containing ICC_PROFILE markers
   - Extracts ICC profile data from JPEG APP2 segments (standard location for color profiles)
   - Successfully extracts Display P3, Adobe RGB, and other wide color gamut profiles
   - Handles multi-segment ICC profiles correctly

2. **JPEG ICC Profile Preservation**:
   - Embeds extracted ICC profiles directly into JPEG output using `JpegEncoder::set_icc_profile()`
   - Maintains complete color accuracy for JPEG images including watermarked versions
   - Graceful fallback to standard JPEG if profile embedding fails
   - Preserves photographer's color grading and camera-specific color profiles

3. **WebP ICC Profile Support** (Updated August 2025):
   - **libwebp-sys Integration**: Created wrapper module using libwebp-sys v0.13+ for proper ICC support
   - **WebPMux API**: Uses WebPMux to add ICC profile chunks to WebP files after encoding
   - **Full Color Support**: Display P3, Adobe RGB, and other wide gamut profiles now preserved
   - **Fallback Strategy**: Gracefully falls back to basic WebP encoding if ICC embedding fails
   - **Validated Format**: Produces properly formatted VP8X extended WebP files with ICCP chunks

4. **Color Profile Workflow**:
   ```
   JPEG Source → Extract ICC Profile → Resize → Apply Watermark → Encode with ICC → Cache
   ```

5. **Watermark Compatibility**:
   - ICC profiles preserved even when copyright watermarks are applied to medium-sized images
   - Color information flows through entire processing pipeline without degradation
   - Both watermarked and non-watermarked JPEG images retain original color profiles

6. **Benefits**:
   - **Complete JPEG color accuracy**: Display P3, Adobe RGB, and custom profiles preserved perfectly
   - **Professional photography support**: Camera color profiles and custom grading maintained
   - **Watermark color preservation**: Copyright watermarks don't affect color profile integrity  
   - **Format-appropriate handling**: JPEG for critical color accuracy, WebP for efficient web delivery
   - **Graceful degradation**: Fallback mechanisms ensure all images process successfully

## Future Improvements
1. Consider adding image preloading for smoother transitions
2. Add configuration for replacement interval
3. Consider WebSocket for real-time updates
4. Add analytics for popular images
5. Support ICC profile preservation for other source formats (PNG, TIFF)
5. Add support for video files in galleries
6. Implement tag-based filtering for galleries
7. Add gallery image browser/picker UI for posts editor
8. Support for image captions in gallery references
9. Batch gallery reference processing for better performance