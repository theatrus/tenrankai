# Video support

## Scope and decisions

Support mixed phone/camera originals in the same galleries as photos. The first
release targets clips up to five minutes. Preserve originals and automatically
prepare one progressive MP4 rendition with byte-range seeking. Adaptive streaming
is a later milestone.

- Discovery candidates: MP4, MOV, M4V, WebM, MKV. Probe actual streams rather than
  infer codecs or playback support from extensions.
- Playback: H.264, 8-bit 4:2:0, AAC-LC when audio exists, faststart MP4.
- Default bounds: 300 seconds, 512 MiB source, 256 MiB playback, 1920 long/1080
  short edge, 60 fps, 6 Mbit/s video ceiling, CRF 23. Preserve portrait aspect and
  source frame timing within the frame-rate cap; never upscale normal footage.
- Re-encoding policy: `auto` copies compatible streams and encodes only affected
  streams. `never` allows remuxing but fails if encoding is required. `always`
  encodes selected streams.
- Supported PQ/HLG HDR inputs get an SDR rendition and poster. Dolby Vision needs
  a supported base layer; unsupported HDR is explicit rather than guessed.
- Strip global and per-stream personal metadata and chapters from derivatives.
  Keep technical source metadata separate from playback metadata.
- Share a processor across all galleries in a server process: initially one job,
  two FFmpeg threads, 30-minute encode timeout, 1 GiB free-space reserve.
- FFmpeg/FFprobe 5.1 or later are external runtime tools, with configurable executable paths.
  Keep image-only operation available without FFmpeg installed.

## Implementation milestones

### 1. Processing foundation — complete

- [x] Add `tenrankai-video` without linking FFmpeg libraries.
- [x] Normalize source stream metadata, rotation, pixel aspect, color/HDR,
  capture-time provenance/timezone uncertainty, camera tags, and ISO 6709 GPS.
- [x] Plan stream copy, audio-only encoding, or full encoding.
- [x] Create posters and validated, sanitized playback in private scratch storage.
- [x] Bound concurrency, threads, source/output sizes, disk reserve, process
  output, and timeouts; cancel and reap child processes.
- [x] Add `video doctor`, `video probe`, and `video prepare` operator commands.
- [x] Validate with automated tests and actual production-host FFmpeg.
- [x] Prepare production scratch directories and next-restart service environment.
- [x] Include FFmpeg in Docker and recommend it for Debian installation.

### 2. Storage, gallery model, and recoverable processing — implemented

- [x] Add media kind and video-specific metadata with image defaults for old caches.
  Preserve existing image URLs and API compatibility while generalizing internals.
- [x] Add streamed range reads and file/stream writes for filesystem/S3. Stage remote
  sources once into bounded seekable files under the shared worker permit.
- [x] Add per-gallery opt-in delivery configuration; preserve fields through the
  admin UI/API and ConfigStorage conversions. Do not inject unknown TOML keys
  into an older running server.
- [x] Persist processing manifests with pending/processing/ready/failed states and probe metadata,
  bounded retries, and operator-readable failures. Reconstruct pending work on
  restart using the existing in-memory queue as a dispatch mechanism.
- [x] Derivative identity includes site/gallery/path, source ETag or size+mtime,
  delivery settings, and pipeline version. Re-check source identity before
  publication; publish completed objects before marking the manifest ready.
- [x] Detect settled synced files and completed uploads. Avoid long processing in
  startup scans and upload request handlers.
- [x] Store video artifacts under their own cache prefix. Update orphan cleanup so
  the image cache sweeper cannot delete video manifests or in-flight outputs.
- [x] Keep same-stem photos and videos separate. Use extension-qualified video
  sidecars (`clip.mov.md`, `clip.mov.toml`, `clip.mov.xmp`). Keep image version/RAW grouping from absorbing video files.
- [x] Preserve capture-date sidecar/XMP precedence. Restrict technical/location/date
  metadata in API responses using existing permissions.

### 3. Playback, uploads, and UI — implemented

- [x] Authorized GET/HEAD playback endpoints with bounded streaming, Range/206/416,
  Content-Range, ETag/Last-Modified, conditional requests, and If-Range handling.
  Authorize against the current source/folder before serving a cached artifact.
- [x] Viewing permits access to the sanitized playback bytes. Original downloads use
  `can_download_original`; metadata visibility also applies to previews/sharing.
  Protected responses must not enter shared public caches.
- [x] Mixed grids, capture-date sorting, counts, duration/play badges, type filtering,
  poster-based folder covers and previews, and visible preparation/failure states.
- [x] Native accessible player (`controls`, `playsInline`, conservative preload),
  fullscreen, seeking, poster, and keyboard handling that respects player focus.
  Suppress image-specific zoom/astro controls for videos.
- [x] Reuse captions, tags, picks, comments, navigation, hiding, sharing, and
  move/copy/delete. Update gallery/post pickers and template fallbacks.
- [x] Extend TUS allowlists and configurable size limits on client and server; enforce
  actual duration/type after probing. Upload completion must not await encoding.
  The initial 512 MiB source limit can reject large 4K originals even below five
  minutes; operators may raise it deliberately when scratch/cache budgets allow.
- [x] Replace memory-buffered folder ZIP construction before including video files.

### 4. Release and rollout

- Tests cover metadata parsing, stream decisions, tiny/silent/portrait/VFR clips,
  audio conversion, HDR tone mapping, metadata removal, limits, cancellation,
  source replacement, restart recovery, range seeking, S3, and permission checks.
- Test representative real iPhone and camera files before claiming those input
  variants supported. Synthetic HDR tests alone cannot validate Dolby Vision.
- Prerequisite check and smoke test must pass as the service account, using the
  configured tools and scratch directory, before publishing a change to `main`.
- Keep changes on a feature branch until all release gates pass. `main` publishes
  automatically; pushing or merging it is a production deployment.
- First enable one gallery with a small sample set, verify processing and seeking
  through the actual reverse proxy, then enable remaining galleries deliberately.
- Check both service logs, disk usage, processing queue, and HTTP health after the
  deployment. Roll back the binary and disable video if required; preserve sources.

## Production prerequisite audit (2026-10-10)

Host: `ec2admin.stackworks.net`, Ubuntu 24.04.4 LTS, four CPUs and about 15 GiB RAM.
FFmpeg and FFprobe 6.1.1 are already installed at `/usr/bin`. The required
`libx264` and `aac` encoders, `zscale` and `tonemap` filters are present.

Two services run as `atrus`:

| Service | Working directory | Bootstrap configuration |
| --- | --- | --- |
| `tenrankai.service` | `/home/atrus/tenrankai` | `/home/atrus/tenrankai/config.toml` |
| `tenrankai-dot-com.service` | `/home/atrus/tenrankai-docs/tenrankai-dot-com` | `config.production.toml` in its working directory |

Root has only about 4.2 GiB free; `/mnt` has about 25 GiB free. Use separate
private scratch directories on `/mnt`, not the root filesystem. Service drop-ins
now stage the three environment variables below for their next natural restart.
Neither running service was restarted.

```ini
[Service]
Environment=TENRANKAI_FFMPEG=/usr/bin/ffmpeg
Environment=TENRANKAI_FFPROBE=/usr/bin/ffprobe
Environment=TENRANKAI_VIDEO_TEMP_DIR=/mnt/tenrankai-video/main
```

The second service uses `/mnt/tenrankai-video/docs`. Directories should be owned
by `atrus:atrus` and mode 0700. Allow for a worker in each server process; the
concurrency bound is per process, not shared between the two services.

The idempotent [`prepare-video-host.sh`](../../scripts/prepare-video-host.sh)
script installed the directories and drop-ins and ran `systemctl daemon-reload`.
Both services retained their existing PIDs and September 30 start times; their
local HTTP endpoints still return 200. The standalone processing crate passed
all 13 unit and 9 real-codec integration tests
on this host. Prerequisite reports and encode/poster smoke tests passed as `atrus`
with each service's configured tool paths and scratch directory. Validation used
`/mnt/tenrankai-video/validation`, separate from deployed checkouts.

Locally, the same 22 tests pass. Server compilation without optional image codec
features and Clippy checks pass. CI now requires real codec tests on Ubuntu
24.04 and Debian Bookworm; those CI jobs must pass before merging.

Do not change current bootstrap TOML to include future video configuration: it
rejects unknown fields. Installing prerequisites and staging environment is safe
before the new binary arrives; enable video only after upgrading the binary.

## Operator commands

```sh
tenrankai video doctor --temp-directory /mnt/tenrankai-video/main --smoke-test
tenrankai video probe /path/to/clip.mov
tenrankai video prepare /path/to/clip.mov --output /path/to/new-preview-directory
```

`prepare` writes `playback.mp4`, `poster.jpg`, and `manifest.json` into a new
directory. The manifest contains private source metadata and must not be served
as a public gallery asset. These commands do not enable gallery video support.

For an independent prerequisite check without compiling the complete server:

```sh
TENRANKAI_VIDEO_TEMP_DIR=/mnt/tenrankai-video/main \
  cargo run -p tenrankai-video --example doctor -- --smoke-test
```

The dedicated CI job tests real encoding on Ubuntu 24.04 (matching production)
and Debian Bookworm (matching the Docker runtime).

## Gallery validation

All 18 video browser scenarios pass against the rebuilt server, covering
desktop Chromium, mobile Chromium, and iPhone-sized WebKit (admin settings
use the existing desktop layout). A real restart reused all eight prepared
playback files unchanged. The gallery UI has been exercised with generated
camera-style MOV (MPEG-4/PCM), portrait/silent MKV, and invalid clips. Playback,
seeking, mixed grids/type filters, permission checks, failures, and UI uploads
pass in desktop Chromium, mobile Chromium, and iPhone-sized WebKit. Admin
video enable/disable, image display/sorting, and phone image navigation checks
also pass. All 336 affected Rust library tests and 95 frontend tests pass,
as do Clippy with warnings denied, type/CSS checks, and public/admin builds.
Three library tests are ignored by default; the S3 integration test was run
separately against the emulator. All 22 processing tests pass with real FFmpeg,
and the Ubuntu 24.04 and Debian Bookworm codec CI jobs are green.
Filesystem persistence/range/ZIP tests and an S3 upload/range/server-copy test
against a local Moto emulator cover the storage changes. Real phone/camera
samples and the production reverse proxy remain rollout checks.

## Gallery operation

Enable video per gallery in admin settings or by adding `[video]` with
`reencode = "auto"` to the gallery TOML. Existing galleries remain image-only by
default. Source discovery and uploads enqueue background work. One worker permit
per process bounds staging and encoding across galleries. Persistent manifests
reuse finished derivatives after restart; source identity, settings, or pipeline
changes invalidate them. Processing failures retry up to three times with backoff.
Inspect the service log and `video doctor` for persistent failures; replacing the
source or changing preparation settings creates a new attempt generation.

Video cache objects live under `video/`, independently of image cache sweeping.
Replacement removes the prior generation; detected source deletion removes loaded
artifacts. Deletions that happened while the server was offline can leave unused
cache artifacts; comprehensive offline garbage collection is follow-up work.
Folder ZIP downloads and media copy/move operations stream or use server-side S3
copy. Full originals are never loaded into memory for video processing or playback.

The first rendition is sanitized SDR H.264/AAC, and source technical metadata is
stored privately. Video watermarking is deferred: image watermark settings do not
apply to playback or posters. Viewing grants access to the playback bytes; original
download permissions control the unsanitized source.

## Deferred work

Adaptive streaming, multiple quality levels, hardware encoders, HDR playback,
animated hover previews/scrub sprites, trimming/editing, audio transcription,
timestamped comments, automatic Live Photo grouping, and video watermarking.

## References

- [Browser codec guidance](https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Video_codecs)
- [FFprobe](https://ffmpeg.org/ffprobe.html)
- [FFmpeg metadata mapping and frame timing](https://ffmpeg.org/ffmpeg.html)
- [MP4 faststart](https://ffmpeg.org/ffmpeg-formats.html)
- [QuickTime metadata](https://exiftool.org/TagNames/QuickTime.html)
- [HTTP range semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-14)
