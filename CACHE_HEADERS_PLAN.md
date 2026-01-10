# Cache Headers Fix Plan

## Problem Statement
Some API endpoints return dynamic data that changes frequently (like gallery preview) but may be cached by browsers or CDNs, leading to stale content being displayed to users. We need to ensure proper cache headers are set on these endpoints.

## Analysis of Current State

### API Endpoints to Review
1. `/api/gallery/{name}/preview` - Returns random selection of images
2. `/api/gallery/{name}` - Gallery listing (may change with new images)
3. `/api/gallery/{name}/refresh` - Refresh metadata endpoint
4. `/api/posts/{system}/refresh` - Refresh posts endpoint  
5. `/api/health` - Health check endpoint
6. `/api/image_detail/{name}/{path}` - Image metadata (can change with user updates)
7. Login-related endpoints - Should never be cached

### Static Content (Should be Cached)
1. `/gallery/image/{identifier}` - Processed images (already has proper cache headers)
2. `/gallery/composite/{path}` - Composite images (already has cache headers)
3. Static assets - CSS, JS, fonts (handled by axum static file serving)

## Critical Issue: Query Parameters and Caching

### The Problem
Image URLs use query parameters for size and format variations:
- `/gallery/image/IMG_1234.jpg?size=medium`
- `/gallery/image/IMG_1234.jpg?size=thumbnail`
- `/gallery/image/IMG_1234.jpg?size=tile_5_3`
- `/gallery/image/IMG_1234.jpg?size=medium@2x`

Many CDNs and proxy caches ignore query parameters by default or treat them as non-cacheable. This can result in:
1. **Cache misses** - Each size variant fetches from origin
2. **Performance degradation** - No CDN edge caching
3. **Increased server load** - Every request hits the origin server
4. **Poor user experience** - Slower image loading

### Current Implementation Problems
```
# These might not be cached by CDNs:
/gallery/image/photo.jpg?size=medium
/gallery/image/photo.jpg?size=thumbnail
/gallery/image/photo.jpg?size=tile_0_0

# CDNs often strip query params or bypass cache
```

### Proposed Solution: System-Reserved Image Serving Path

#### Recommended Approach: `/_image` System Path
```
/gallery/_image/{identifier}/medium
/gallery/_image/{identifier}/thumbnail
/gallery/_image/{identifier}/tile_5_3
/gallery/_image/{identifier}/medium@2x
/gallery/_image/{identifier}/large
```

Benefits:
- **No conflicts**: Folders named "image", "images", "img" etc. can exist
- **Clear separation**: `_image` is obviously a system endpoint
- **CDN friendly**: Path-based URLs cache perfectly
- **Clean structure**: Size variants as path segments
- **Consistent pattern**: All image serving through `/_image`

Examples:
```
# Current (problematic)
/gallery/image/vacation/photo.jpg?size=medium
/gallery/image/abc123?size=medium

# New (no conflicts)
/gallery/_image/vacation/photo.jpg/medium
/gallery/_image/vacation/photo.jpg/thumbnail
/gallery/_image/2025/sunset.jpg/tile_5_3

# With unique_id indexing (still includes folder path)
/gallery/_image/vacation/abc123/medium
/gallery/_image/2025/xyz789/thumbnail
/gallery/_image/travel/europe/def456/tile_5_3
```

#### Alternative Patterns (if needed)
```
# Double underscore for extra clarity
/gallery/__image/{path}/medium

# Prefix with dot (but some systems hide these)
/gallery/.image/{path}/medium

# API-style prefix
/gallery/_api/image/{path}/medium
```

### Implementation Changes Required

1. **Route Updates** in `src/main.rs`:
```rust
// Current routes
.route("/:prefix/image/:identifier", get(image_handler_for_named))

// New routes (keep old ones for compatibility)
.route("/:prefix/_image/*path", get(image_handler_for_named_v2))

// The handler will parse:
// /gallery/_image/vacation/photo.jpg/medium
// - prefix: gallery
// - path: vacation/photo.jpg/medium
// - Split to get identifier and size
```

2. **Handler Implementation**:
```rust
// Parse the path to extract identifier and size
let parts: Vec<&str> = path.split('/').collect();
let size = parts.last().unwrap_or("medium");
let identifier = parts[..parts.len()-1].join("/");
```

3. **Reserved Paths**:
- `/_image` - Image serving endpoint
- `/_api` - Future API endpoints
- `/_login` - Already used for auth
- Document that folders starting with `_` are reserved

2. **URL Generation Updates**:
- Frontend TypeScript code
- Liquid templates
- API responses with image URLs

3. **Backward Compatibility**:
- Keep old routes temporarily
- Redirect old URLs to new format
- Set Deprecation headers on old endpoints

## Implementation Strategy

### 1. Create Cache Control Helper Functions
```rust
// In src/api_response.rs or new cache module
pub fn no_cache_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, "no-cache, no-store, must-revalidate".parse().unwrap());
    headers.insert(PRAGMA, "no-cache".parse().unwrap());
    headers.insert(EXPIRES, "0".parse().unwrap());
    headers
}

pub fn short_cache_headers(seconds: u32) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, format!("public, max-age={}", seconds).parse().unwrap());
    headers
}
```

### 2. Apply Headers to Dynamic Endpoints

#### Gallery Preview API
```rust
// In src/api.rs - gallery_preview_api
// This returns random images, should not be cached
response.headers_mut().extend(no_cache_headers());
```

#### Gallery Listing API
```rust
// In src/api.rs - gallery_api
// Could use short cache (e.g., 60 seconds) or no-cache
response.headers_mut().extend(short_cache_headers(60));
```

#### Image Detail API
```rust
// In src/api.rs - image_detail_api
// User metadata can change, use short cache
response.headers_mut().extend(short_cache_headers(300)); // 5 minutes
```

#### Refresh Endpoints
```rust
// Already return appropriate responses, but ensure no caching
response.headers_mut().extend(no_cache_headers());
```

### 3. Review Existing Cache Headers

#### Image Serving
- Already has proper long-term caching with ETags
- No changes needed

#### Composite Images
- Already has proper cache headers
- No changes needed

### 4. Add Tests
1. Test that gallery preview API returns no-cache headers
2. Test that image serving returns proper cache headers
3. Test that refresh endpoints return no-cache headers
4. Test that login endpoints are never cached

## Headers to Set

### No-Cache Headers (Dynamic Content)
```
Cache-Control: no-cache, no-store, must-revalidate
Pragma: no-cache
Expires: 0
```

### Short Cache Headers (Semi-Dynamic Content)
```
Cache-Control: public, max-age=60
```

### Long Cache Headers (Static Content - Already Implemented)
```
Cache-Control: public, max-age=31536000
ETag: "hash-of-content"
```

## Affected Files

### For Cache Headers
1. `src/api.rs` - Add headers to API endpoints
2. `src/api_response.rs` - Add cache header helper functions
3. `src/login/handlers.rs` - Ensure login endpoints aren't cached
4. Tests to add/update in respective test files

### For URL Structure Changes
1. `src/main.rs` - Add new `/_image` routes
2. `src/gallery/handlers.rs` - Add new handler function
3. `src/gallery/types.rs` - Update URL generation methods
4. `src/frontend/types/index.ts` - Update TypeScript interfaces
5. `src/frontend/components/ImageDetail/ImageDisplay.tsx` - Update tile URL generation
6. `templates/modules/gallery.html.liquid` - Update image URLs
7. `templates/modules/image_detail.html.liquid` - Update image URLs
8. `src/api.rs` - Update API responses with new URL format
9. `src/gallery/core.rs` - Update any URL generation in gallery logic

## Testing Plan
1. Use curl or browser dev tools to verify headers
2. Test that gallery preview returns different results on reload
3. Verify that cached images still load quickly
4. Check that metadata updates are visible after short cache period
5. Ensure CDNs respect the cache headers
6. **CDN Testing**: Test with actual CDN to verify query parameter handling
7. **URL Migration**: Test both old and new URL formats work during transition

## Implementation Priority

### Phase 1: Critical Cache Headers (Do First)
- Add no-cache headers to gallery preview API
- Add no-cache headers to refresh endpoints
- Add short cache to gallery listing API
- This fixes immediate caching issues

### Phase 2: URL Structure Migration (Do Second)
- Implement new URL routes
- Update URL generation
- Add redirects from old to new
- Update frontend code
- This improves CDN caching effectiveness

## CDN Configuration Notes

### CloudFlare
- By default ignores query strings for caching
- Need "Cache Level: Cache Everything" + Page Rules
- Or use path-based URLs (recommended)

### AWS CloudFront
- Requires explicit configuration to include query strings
- Better to use path parameters
- Can whitelist specific query parameters if needed

### Fastly
- More flexible with query parameters
- Still better performance with path-based URLs

## Rollback Plan
- Headers are additive and won't break existing functionality
- Can easily remove or adjust cache durations if needed
- URL changes can coexist with old format using redirects
- Monitor for any performance impact from reduced caching