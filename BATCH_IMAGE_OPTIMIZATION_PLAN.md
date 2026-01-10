# Batch Image Processing Optimization Plan

## Problem Statement
Currently, when generating multiple sizes/formats of an image during batch operations (like cache pre-generation), we load the source image multiple times - once for each size variant. This is inefficient, especially for large images or when processing many variants.

## Current Behavior
1. `pregenerate_all_images_cache()` calls `process_gallery_item()` for each image
2. `process_gallery_item()` calls `get_resized_image()` for each size (thumbnail, gallery, medium, large)
3. Each `get_resized_image()` call loads the source image independently
4. For 4 sizes, we load the same image 4 times from disk

## Proposed Solution
Create a batch processing function that loads the image once and generates all requested variants in a single operation.

## Implementation Plan

### 1. Create New Batch Processing Function
```rust
// In src/gallery/image_processing/resize.rs
async fn process_image_batch(
    &self,
    original_path: &Path,
    relative_path: &str,
    variants: Vec<(String, ImageSize, bool, OutputFormat)>, // (size_str, dimensions, watermark, format)
) -> Result<Vec<PathBuf>, GalleryError> {
    // Load image once
    // Process all variants
    // Return all cache paths
}
```

### 2. Add Batch Deduplication Key
Use a single deduplication key for the entire batch:
```rust
let task_key = format!("batch:{}:{}", relative_path, hash_of_variants);
```

### 3. Update Cache Pre-generation
Modify `process_gallery_item()` to:
1. Collect all size variants needed
2. Check which ones already exist in cache
3. Call batch processing for missing variants
4. Use single deduplication key for entire batch

### 4. Extend to Format Generation
The `generate_all_missing_formats()` function should also use batch processing:
- Load image once
- Check existing formats
- Generate all missing formats (JPEG, WebP, AVIF) in one pass

### 5. Memory Considerations
- For very large images, consider memory limits
- May need to process in groups if too many variants
- Monitor memory usage during batch operations

## Benefits

### Performance Benefits
1. **Single Load**: Load source image once instead of N times
2. **I/O Reduction**: Single disk read for multiple outputs
3. **Better Caching**: OS file cache more effective
4. **Reduced Latency**: Faster pre-generation and format conversion
5. **Resource Efficiency**: Less CPU time spent on decoding

### Code Quality Benefits
1. **DRY Principle**: Single resize implementation, no duplication
2. **Maintainability**: Changes to resize logic in one place
3. **Consistency**: All resize operations use same algorithm
4. **Testability**: Test resize logic once, use everywhere
5. **Type Safety**: LoadedImage ensures metadata stays with image

## Key Design Decision: Unified Resize Function

### The Problem
Currently we have duplicate resize logic in:
1. `resize_image()` - Basic resize preserving aspect ratio
2. `process_image()` - Resize + watermark + save
3. `process_all_tiles_for_image()` - Resize + tile extraction
4. Future `process_image_batch()` - Would add another copy

### The Solution: Single Resize Pipeline
Create a unified image processing pipeline that:
1. Loads image once (with format detection, ICC profiles, AVIF info)
2. Optionally resizes to max dimensions
3. Can generate multiple outputs from the resized image

```rust
struct LoadedImage {
    image: DynamicImage,
    icc_profile: Option<Vec<u8>>,
    #[cfg(feature = "avif")]
    avif_info: Option<AvifImageInfo>,
    format: Option<ImageFormat>,
}

impl LoadedImage {
    fn resize(&mut self, dimensions: ImageSize) -> Result<(), GalleryError>;
    fn extract_tile(&self, x: u32, y: u32, size: u32) -> Result<DynamicImage, GalleryError>;
    fn apply_watermark(&mut self, config: CopyrightConfig) -> Result<(), GalleryError>;
    fn save_as(&self, path: &Path, format: OutputFormat, quality: Quality) -> Result<(), GalleryError>;
}
```

### Refactoring Steps

1. **Create LoadedImage struct** that encapsulates:
   - The image data
   - ICC profile
   - AVIF metadata (gain maps, HDR info)
   - Original format

2. **Unify resize logic**:
   - Move `resize_image()` into `LoadedImage::resize()`
   - Handle gain map resizing in one place
   - Keep aspect ratio preservation logic

3. **Consolidate save logic**:
   - Move `save_image()` into `LoadedImage::save_as()`
   - Handle all format conversions
   - Preserve ICC profiles and AVIF metadata

4. **Update existing functions**:
   ```rust
   // Old: process_image() loads, resizes, watermarks, saves
   // New: 
   let mut img = LoadedImage::load(path)?;
   img.resize(dimensions)?;
   if watermark { img.apply_watermark(config)?; }
   img.save_as(output_path, format, quality)?;
   ```

5. **Batch processing becomes natural**:
   ```rust
   let mut img = LoadedImage::load(path)?;
   
   // Generate multiple sizes from one load
   for (size, dimensions, watermark) in variants {
       let mut variant = img.clone();
       variant.resize(dimensions)?;
       if watermark { variant.apply_watermark(config)?; }
       variant.save_as(cache_path, format, quality)?;
   }
   ```

## Implementation Steps

### Phase 1: Create Unified Image Structure
1. Create `LoadedImage` struct in `image_processing/types.rs`
2. Move image loading logic to `LoadedImage::load()`
3. Move resize logic to `LoadedImage::resize()`
4. Move save logic to `LoadedImage::save_as()`

### Phase 2: Refactor Existing Functions
1. Update `process_image()` to use `LoadedImage`
2. Update `process_all_tiles_for_image()` to use `LoadedImage`
3. Remove duplicate resize/save code
4. Ensure all tests still pass

### Phase 3: Implement Batch Processing
1. Create `process_image_batch()` using `LoadedImage`
2. Load once, generate multiple variants
3. Integrate with deduplication

### Phase 4: Update Cache Pre-generation
1. Update `process_gallery_item()` to use batch processing
2. Update `generate_missing_formats_for_image()` to use batch processing
3. Performance testing

## Code Structure

### New Files/Functions
- `LoadedImage` struct - Unified image container with metadata
- `LoadedImage::load()` - Load image with all metadata
- `LoadedImage::resize()` - Single resize implementation
- `LoadedImage::extract_tile()` - Extract tile from loaded image
- `LoadedImage::apply_watermark()` - Apply watermark in-place
- `LoadedImage::save_as()` - Save in any format with metadata
- `process_image_batch()` - Batch processing using LoadedImage

### Modified Functions
- `process_image()` - Simplified to use LoadedImage
- `process_all_tiles_for_image()` - Simplified to use LoadedImage
- `get_resized_image()` - Keep for single image requests, use LoadedImage
- `process_gallery_item()` - Use batch processing
- `generate_missing_formats_for_image()` - Use batch processing

### Functions to Remove/Consolidate
- `resize_image()` - Move into LoadedImage::resize()
- `save_image()` - Move into LoadedImage::save_as()
- `extract_image_info()` - Move into LoadedImage::load()
- `apply_copyright_watermark()` - Move into LoadedImage::apply_watermark()

### Deduplication Changes
- Add batch-aware deduplication keys
- Consider variant list in deduplication
- Ensure proper cleanup for batch operations

## Testing Strategy
1. Unit tests for batch processing logic
2. Integration tests for cache pre-generation
3. Performance benchmarks comparing old vs new
4. Memory usage monitoring
5. Concurrent batch request testing

## Rollback Plan
- Keep existing single-image processing as fallback
- Feature flag for batch processing if needed
- Monitor for any quality differences
- Easy revert if issues found

## Future Enhancements
1. Parallel variant processing within batch
2. Smart ordering of variant generation
3. Progressive generation (thumbnail first, etc.)
4. Memory-mapped file support for very large images