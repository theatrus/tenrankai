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