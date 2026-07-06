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

## Testing Plan

- Unit-test priority ordering, dedupe, priority upgrades, and memory-based
  worker planning.
- Handler-test cache-miss pending responses for resized images and tiles.
- Handler-test cache-hit behavior remains `200`.
- Frontend-test retry behavior for image elements and tile URL derivation.
- Playwright-test a cold-cache gallery/detail/tile flow before and after the
  implementation, with screenshots for visual confirmation.
