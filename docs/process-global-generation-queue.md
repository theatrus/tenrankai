# Process-Global Generation Queue

This document tracks the generation queue feature work and the decisions that
shape it.

## Goals

- Replace FIFO/background-only cache generation with one process-global queue.
- Prioritize interactive image requests over background pre-generation.
- Deduplicate equivalent generation work across gallery pages, image detail,
  zoom tiles, uploads, admin operations, and background pregen.
- Use memory and CPU heuristics to avoid overcommitting image processing work.
- Make cold generated image URLs retry-friendly without breaking CloudFront.

## URL Behavior

Generated image artifact URLs keep the existing scheme:

```text
{gallery_prefix}/_image/{url_id}/{size}
{gallery_prefix}/image/{path}?size={size}
```

Supported artifact sizes include:

```text
thumbnail
thumbnail@2x
gallery
gallery@2x
medium
medium@2x
large
large@2x
tile_{x}_{y}
tile_{x}_{y}@2x
```

Every generated image artifact URL should support the fallback flow:

1. Check auth and permissions.
2. Return cached bytes immediately when the artifact exists.
3. On a cache miss, enqueue or upgrade the process-global generation job.
4. Return an uncached `202 Accepted` response with a retry hint.
5. Let the caller retry the same URL until it receives `200`.

The URL itself remains the artifact identity. We should not introduce a new
variant query parameter for the artifact. Any retry-only query parameter would
need to be excluded from the CloudFront cache key, so the preferred behavior is
to retry the same URL.

## CloudFront Requirements

`202 Accepted` pending responses must not be cached by browsers or CloudFront.
Use strict response headers such as:

```text
Cache-Control: no-store, max-age=0, s-maxage=0
Retry-After: 1
```

The CloudFront behavior for image artifact paths must have a minimum TTL of
zero; otherwise CloudFront can cache responses even when the origin asks it not
to.

Successful `200` artifact responses stay cacheable under the existing path-based
URLs and cache-key behavior.

## Queue Model

The queue is process-global and owned by application state. It is not per
gallery.

The primary job kinds are:

- `ResizedImage`: one generated resized image artifact.
- `TileSet`: all tiles for an image/tile size/output format. A request for one
  tile should enqueue the whole set, because current tile generation loads the
  source once and writes every tile, including retina tiles.
- `Cleanup`: cache invalidation or stale artifact cleanup.

Priorities:

- Interactive image requests are highest priority.
- Image detail, gallery visible images, and zoom tiles count as interactive.
- Upload/admin follow-up generation is medium priority.
- Background pre-generation is low priority.
- Cleanup should run promptly enough to avoid serving stale artifacts.

If a low-priority job is already pending and an interactive request needs the
same artifact, the existing job should be upgraded rather than duplicated.

## Memory Heuristics

Worker planning should use both CPU and available memory. The initial policy can
mirror the psf-guard approach:

- logical CPU count as an upper bound,
- a configurable memory budget fraction,
- a conservative peak bytes-per-pixel estimate,
- lower background concurrency while interactive jobs are pending or running.

The memory estimator should prefer cached image dimensions and fall back to a
conservative default when dimensions are unavailable.

## Frontend Fallback

React-owned image artifacts should use a retryable loader. This includes:

- gallery masonry and square-grid images,
- image detail medium image,
- previous/next preloads,
- desktop zoom tiles,
- mobile pinch-zoom tiles.

CSS `background-image` usage cannot reliably observe pending/error states, so
those render paths should move to observable image elements or a shared loader
that can retry before exposing the image.

Tile URLs must be derived from the current image URL or gallery prefix. The
frontend should not hardcode `/gallery/_image/...`, because galleries can use
custom prefixes.

## Download Exception

Download routes are not generated image artifact fallback routes:

```text
{gallery_prefix}/_download
{gallery_prefix}/_download/{path}
{gallery_prefix}/_raw/{path}
```

`_raw` serves source files. `_download` uses a zip pass and should prepare or
read the required files before streaming zip entries. It should not return a
mid-download `202`.

## Operational Notes

These notes are intentionally more detailed than the design summary above. They
capture the mental model to keep when changing this system later.

### The Two Gates

There are two gates and they are related, but they are not the same gate.

The gallery listing gate controls whether an image appears in gallery metadata.
When `pregenerate` is enabled for a gallery, `scan_directory_with_user` builds a
set of images where `ImageMetadata.preview_ready` is true. Gallery items,
subfolder counts, and subfolder preview images are filtered through that set.
If an image is not preview-ready, normal gallery pages should not put that image
or its thumbnail URL into the DOM.

The generated artifact gate controls what happens after a client requests a
specific generated image URL. This gate still checks path safety, permissions,
cache existence, and source existence. With this feature, a generated artifact
cache miss does not block the request while the image is generated. It enqueues
the work and returns an uncached `202 Accepted` response so the frontend can
retry the same URL.

The important distinction: `preview_ready` is a reveal/listing concept. It is
not currently enforced by the generated image serving path. A user with a direct
image artifact URL can still cause an allowed generated artifact to be queued.
That is acceptable if the invariant is "do not reveal unreprocessed images in
gallery listings." If the stronger invariant becomes "do not serve any generated
bytes for a pregeneration-enabled image until preview_ready," then the generated
artifact handler must also check readiness before enqueueing interactive work.

### Startup And Background Pregeneration

Startup still refreshes metadata first. That refresh validates current
`preview_ready` flags against the existing cache contents. This keeps listings
from showing images whose configured pregenerated artifacts are missing.

After metadata refresh, galleries with `pregenerate` enqueue background jobs in
the process-global manager instead of spawning the old direct pregen work from
the main startup path. Background work is intentionally lower priority than
interactive requests. If a visitor asks for an artifact that is also pending as
background work, the queue upgrades the existing job rather than duplicating it.

One compatibility wrinkle: older direct pregen code paths still exist in some
site reload/manager flows. The main server path uses the new manager, and the
interactive artifact path uses the new manager. If later work fully removes the
legacy cache queue and direct pregen paths, audit `refresh_metadata_and_pregenerate_cache`
callers at the same time.

### Gallery Page Flow

The intended gallery-page flow is:

1. Metadata refresh discovers source images and stores metadata.
2. If pregeneration is enabled, readiness is computed from configured sizes and
   formats. Tiles are not part of `preview_ready`; readiness checks only
   non-tile generated sizes.
3. Gallery list APIs/pages include only ready images when pregeneration is
   enabled.
4. The frontend receives thumbnail/gallery URLs only for listed images.
5. Thumbnail `<img>` elements request those URLs.
6. A cache hit returns the generated bytes with the normal cacheable response.
7. A rare cache miss after listing, for example stale readiness or an evicted
   cache object, returns uncached `202`, queues interactive work, and retries.

This means the queue is not supposed to make unrevealed gallery images suddenly
appear in a gallery listing. It is a recovery and prioritization layer for
requested artifacts.

### Detail And Zoom Flow

The detail page uses the same artifact rules as gallery thumbnails. The medium
image is rendered through the retryable image component. Adjacent-image preloads
also retry because they may race generation or cache eviction.

Zoom tiles are special because one tile request generates the full tile set for
that image/tile size/output format. The queue job is `TileSet` rather than a
single-tile job. This matches the existing tile generator, which loads the
source once and writes every tile, including the `@2x` tile files.

Tile URLs must be derived from the current image URL. Do not hardcode
`/gallery/_image/...`; galleries can use custom prefixes, and legacy query
routes still exist. The helper should preserve the active URL scheme:

```text
/portfolio/_image/abc/medium -> /portfolio/_image/abc/tile_1_2
/gallery/image/a.jpg?size=medium -> /gallery/image/a.jpg?size=tile_1_2
```

### Cache Miss Responses

Generated artifact cache misses return `202 Accepted` only when the source image
exists. Missing source images return `404`, not `202`. This matters because a
retrying client would otherwise poll forever for an artifact that can never be
generated.

The pending response must remain uncacheable:

```text
Cache-Control: no-store, max-age=0, s-maxage=0
Retry-After: 1
```

Do not add a retry query string to artifact URLs. The frontend currently changes
only the URL fragment (`#retry-N`), which is never sent to the server and should
not affect CloudFront cache keys. The fragment is only a browser-side nudge to
make an `<img>` retry after an error response.

### Queue Semantics

The queue is process-global. It is shared across sites and galleries in the
same process. Job keys include the site/gallery key, image path, and job kind,
so deduplication does not collapse work across different galleries.

Priority order is:

1. Interactive image artifacts.
2. Cleanup.
3. Normal upload/admin follow-up pregeneration.
4. Background gallery pregeneration.

If a job is already running, enqueueing the same job again is intentionally a
no-op. If the job is pending at lower priority, enqueueing it at higher priority
upgrades the pending entry. The heap can contain stale entries after upgrades;
workers skip stale entries when popping.

Background jobs yield while interactive jobs are active. Worker planning uses
logical CPU count plus a memory cap based on cached dimensions and a conservative
peak bytes-per-pixel estimate. If metadata dimensions are missing, the queue
falls back to CPU-based planning rather than blocking enqueue.

### Readiness Updates

Successful resized and pregenerate jobs update preview readiness after writing
artifacts. Cleanup jobs delete generated cache files but do not by themselves
force an immediate listing rebuild. If a cache mutation should affect listing
visibility immediately, make sure the relevant readiness flags are recomputed
and persisted.

Pregeneration readiness is based on the configured pregenerate sizes and
formats. It intentionally excludes tiles. A gallery can be visible before zoom
tiles are generated; the tile fallback handles cold tile artifacts on demand.

### CDN Friendliness

The CDN-friendly shape is:

- Stable artifact URLs identify generated outputs.
- Successful `200` responses can be cached normally.
- Pending `202` responses must not be cached.
- Retry state is kept out of the request URL seen by CloudFront.
- Direct downloads and raw source routes do not use the `202` artifact fallback.

If CloudFront has a nonzero minimum TTL on artifact paths, it may cache `202`
despite the origin headers. The origin behavior is correct only if the
CloudFront behavior allows origin no-cache/no-store directives to win.

### Future Change Checklist

When touching this system, check these invariants:

- Gallery listings with `pregenerate` still filter by `preview_ready`.
- Generated artifact cache hits still return bytes without enqueueing work.
- Generated artifact cache misses still check source existence before returning
  `202`.
- Pending responses remain `no-store` and include `Retry-After`.
- Frontend retries do not add server-visible cache-key variants.
- Tile requests enqueue one tile-set job, not one job per tile.
- Legacy query URLs and path-based URLs continue to produce exact size and tile
  variants.
- Upload/admin follow-up work uses the process-global manager, not a separate
  per-gallery queue.
- Any stronger "no bytes before preview_ready" policy is implemented in the
  artifact handler, not only in gallery listing code.

## Testing Plan

- Unit-test priority ordering, dedupe, priority upgrades, and memory-based
  worker planning.
- Handler-test cache-miss pending responses for resized images and tiles.
- Handler-test cache-hit behavior remains `200`.
- Frontend-test retry behavior for image elements and tile URL derivation.
- Playwright-test a cold-cache gallery/detail/tile flow before and after the
  implementation, with screenshots for visual confirmation.
