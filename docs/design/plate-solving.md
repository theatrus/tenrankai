# Plate Solving and Object Overlays for Astro Images

Status: **planned** — decisions confirmed 2026-07-13. Prior art in the
codebase: astro sidecar metadata (`ra`/`dec`/`telescope`/…), the Sky
Position card (all-sky chart from RA/Dec), and per-embed hover details.

## Goal

For astrophotography images, compute a real WCS (world coordinate system)
solution — where the frame is on the sky, at what scale and rotation — and
use it to render object overlays (galaxies, nebulae, named stars, optional
coordinate grid) on the image detail page, plus a true field-of-view
rectangle on the Sky Position chart.

## Decisions

- **Native Rust near-solver**, no external solver binaries. Solving is
  seeded by the sidecar RA/Dec hint (±few degrees) and a pixel-scale guess
  from metadata (focal length + sensor) when available. Blind solving is
  explicitly out of scope for v1: images without hints remain unsolved.
  A tetra3-style quad index for wide fields can be added later without
  changing the WCS/overlay machinery.
- **Star catalog: Gaia DR3 subset**, default tier G ≤ 15 (~25M stars,
  ~300–500 MB), packed at build time with proper motion applied to a fixed
  contemporary epoch. A `lite` tier (G ≤ 12, ~30 MB) is also published for
  wide-field-only installs; a `deep` tier (G ≤ 17) can follow if needed.
- **Solving runs in both places**: the server solves unsolved images during
  gallery refresh (opt-in per gallery) and persists the WCS in the metadata
  cache; the CLI can pre-solve and write the WCS as sidecar data so offline
  / synced-content workflows need no server compute.

## Components

### `tenrankai-astro` crate (workspace member)

1. **Star detection** — operates on the already-decoded image from the
   gallery pipeline (downsampled grayscale): grid background estimation
   (sigma-clipped), thresholding, connected components, flux-weighted
   centroids. Output: star list (x, y, flux), brightest N kept.
2. **WCS model** — TAN projection with a CD matrix (center, scale,
   rotation, parity). SIP distortion terms are a later refinement.
3. **Near-solver** — load catalog stars from HEALPix tiles around the
   hinted center (padded by the hint uncertainty), match detected stars to
   catalog stars via triangle/quad geometric hashing + RANSAC, then
   least-squares refine. Emit quality metrics: matched-star count, RMS
   residual (arcsec), and a pass/fail threshold before anything persists.
4. **Catalog access** — memory-mapped HEALPix-binned tile files under a
   configurable `astro_data_directory`; ~8–10 bytes/star (quantized
   RA/Dec, magnitude).
5. **Data builder + downloader** — `tenrankai astro build-data` assembles
   bundles from primary sources; `tenrankai astro download-data` fetches
   prebuilt versioned bundles from GitHub release assets.

### Datasets (all rebuilt from primary sources, no NINA redistribution)

| Bundle | Source | Contents | License |
|--------|--------|----------|---------|
| stars-lite / stars-standard | Gaia DR3 (TAP) | positions + G mag, PM-corrected | Gaia DPAC (free, attribution) |
| objects | OpenNGC | NGC/IC, sizes, PA, types, Messier/Caldwell names | CC-BY-SA-4.0 |
| objects | VizieR: Sharpless Sh2, Barnard | emission nebulae, dark nebulae | free |
| star names | IAU WGSN + Yale BSC | labels for bright/named stars | free |

OpenNGC major/minor axes + position angles let overlays render true
ellipses rather than markers.

### Server integration

- Gallery config: `plate_solving = true` (opt-in), bootstrap config:
  `astro_data_directory`.
- On refresh, images with RA/Dec hints and no stored WCS get solved; the
  WCS (center, CD matrix, match count, RMS) persists in the metadata cache
  alongside dimensions. CLI-written sidecar WCS is honored and skips the
  server solve.
- `GET /api/gallery/{name}/astro/{*path}` — returns the WCS plus overlay
  objects **projected to pixel coordinates** server-side:
  `{name, common_name, type, x, y, a, b, pa, mag}` for DSOs in the
  footprint, plus named-star label positions. Gated like technical
  metadata (`can_see_technical_details`); cacheable.

### Frontend

- Image detail page: an "Overlay" toggle for solved images rendering an
  SVG layer sized over the displayed image — DSO ellipses with labels,
  named-star markers, optional RA/Dec grid; per-group toggles. v1 targets
  the standard view; the tiled deep-zoom viewer integration is a follow-up.
- Sky Position card: solved images show the actual FOV rectangle (from the
  WCS footprint) instead of only the crosshair.

## Validation

- Synthetic round-trip tests: render fake star fields from the catalog
  with a known WCS (plus noise, missing stars, false detections), solve,
  assert recovered parameters within tolerance.
- Regression suite over the real gallery: ~30 astro images with known
  targets; assert solve success, plausible scale/rotation, and that the
  expected catalog object (e.g. M31, Sh2-101) lands inside the frame.

## Phases (each independently shippable)

1. Data builder + bundle download CLI (star tiles + object DB).
2. Solver core: detection + near-solve, `tenrankai astro solve` CLI with
   annotated debug output.
3. Server solve-on-refresh + WCS persistence + astro overlay API.
4. Overlay UI + sky-map FOV rectangle.

Later: blind solving via quad index (wide fields first), SIP distortion,
overlays inside the tiled zoom viewer, PGC/HyperLEDA faint-galaxy layer.
