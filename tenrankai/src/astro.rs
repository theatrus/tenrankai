//! Plate solving and object overlays for astro images, powered by seiza.
//!
//! When `[astro]` data files are configured, images whose sidecar metadata
//! carries RA/Dec hints can be solved on demand: the API endpoint decodes
//! the image, detects stars, near-solves against the star catalog over a
//! ladder of pixel scales, and caches the WCS in memory. Object overlays
//! project the object catalog through the solution into pixel coordinates.

use crate::ApiResponse;
use crate::api_response::no_cache_headers;
use crate::site::ResolvedState;
use axum::extract::Path;
use axum::response::IntoResponse;
use seiza::catalog::TileCatalog;
use seiza::objects::ObjectCatalog;
use seiza::solve::{Solution, SolveHint};
use seiza::wcs::Wcs;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Pixel-scale ladder, in descending order of how often real astrophotos
/// land on each rung rather than ascending by scale: every rung a solve
/// walks past costs a full failed attempt, so the common cases go first.
/// Consecutive rungs are a factor of two apart and ±35 % tolerance makes
/// them overlap, so the set still covers roughly 0.07–7.5 arcsec/pixel
/// whatever the order.
///
/// - 2.8, 1.4: wide refractors and fast astrographs, the bulk of images
/// - 0.7, 0.35: long focal lengths and SCTs
/// - 5.6: camera lenses and wide fields
/// - 0.19, 0.11: drizzled or upscaled close-ups, well below native seeing
///   (an upscaled M51 solves at 0.16"/px)
///
/// A sidecar `pixel_scale` skips the ladder entirely — prefer setting it.
const SCALE_LADDER: &[f64] = &[2.8, 1.4, 0.7, 5.6, 0.35, 0.19, 0.11];
const SCALE_TOLERANCE: f64 = 0.35;
/// A sidecar-supplied scale is trusted, but resampling and rounding move it
/// a little, so allow a modest window around it before falling back.
const HINTED_SCALE_TOLERANCE: f64 = 0.15;
const SEARCH_RADIUS_DEG: f64 = 2.0;
/// Images are downsampled to at most this many pixels on the long side
/// before star detection (centroids are scaled back to full resolution)
const DETECT_MAX_DIM: u32 = 2800;

pub struct AstroContext {
    stars: TileCatalog,
    objects: Option<ObjectCatalog>,
    /// Identifies the loaded object catalog build (count + byte length)
    objects_version: String,
    /// Transients reload when the file changes (a cron refreshes it)
    transients: Option<ReloadingCatalog>,
    /// Whole-sky pattern index for images with no coordinate hint —
    /// built once, lazily, on the first hint-less solve
    /// Prebuilt index to memory-map; None builds the bright tiers on demand
    blind_index_path: Option<std::path::PathBuf>,
    blind_index: tokio::sync::OnceCell<Option<Arc<seiza::blind::BlindIndex>>>,
    /// Comet/asteroid elements; positions depend on the capture time
    minor_bodies: Option<ReloadingMinorBodies>,
    /// Failed solve attempts this process, keyed by "gallery/path" —
    /// successes persist into the image's metadata sidecar instead
    failed: RwLock<HashMap<String, ()>>,
}

/// Minor-body elements that reread their file when the mtime changes
/// (the nightly publish refreshes comets).
struct ReloadingMinorBodies {
    path: std::path::PathBuf,
    state: RwLock<(
        std::time::SystemTime,
        Arc<seiza::minor_bodies::MinorBodyCatalog>,
    )>,
}

impl ReloadingMinorBodies {
    fn open(path: &std::path::Path) -> Option<Self> {
        let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        let catalog = seiza::minor_bodies::MinorBodyCatalog::open(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            state: RwLock::new((mtime, Arc::new(catalog))),
        })
    }

    async fn current(&self) -> Arc<seiza::minor_bodies::MinorBodyCatalog> {
        let mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        {
            let state = self.state.read().await;
            match mtime {
                Some(mtime) if mtime != state.0 => {}
                _ => return state.1.clone(),
            }
        }
        let mut state = self.state.write().await;
        if let (Some(mtime), Ok(catalog)) = (
            mtime,
            seiza::minor_bodies::MinorBodyCatalog::open(&self.path),
        ) {
            info!(
                "astro: reloaded {} minor bodies from {}",
                catalog.len(),
                self.path.display()
            );
            *state = (mtime, Arc::new(catalog));
        }
        state.1.clone()
    }
}

/// An object catalog that rereads its file when the mtime changes.
struct ReloadingCatalog {
    path: std::path::PathBuf,
    state: RwLock<(std::time::SystemTime, Arc<ObjectCatalog>)>,
}

impl ReloadingCatalog {
    fn open(path: &std::path::Path) -> Option<Self> {
        let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        let catalog = ObjectCatalog::open(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            state: RwLock::new((mtime, Arc::new(catalog))),
        })
    }

    async fn current(&self) -> Arc<ObjectCatalog> {
        let mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        {
            let state = self.state.read().await;
            match mtime {
                Some(mtime) if mtime != state.0 => {}
                _ => return state.1.clone(),
            }
        }
        let mut state = self.state.write().await;
        if let (Some(mtime), Ok(catalog)) = (mtime, ObjectCatalog::open(&self.path)) {
            info!(
                "astro: reloaded {} transients from {}",
                catalog.len(),
                self.path.display()
            );
            *state = (mtime, Arc::new(catalog));
        }
        state.1.clone()
    }
}

impl AstroContext {
    pub(crate) fn objects_version(&self) -> &str {
        &self.objects_version
    }

    /// Load catalogs from the configured data files. Returns None (with a
    /// warning) when loading fails so a bad path cannot take the site down.
    pub fn load(config: &crate::config::AstroConfig) -> Option<Arc<Self>> {
        let stars = match TileCatalog::open(&config.star_data) {
            Ok(catalog) => {
                info!(
                    "astro: {} stars loaded from {} (epoch {})",
                    catalog.star_count(),
                    config.star_data.display(),
                    catalog.epoch()
                );
                catalog
            }
            Err(e) => {
                warn!(
                    "astro: failed to open star data {}: {e}; plate solving disabled",
                    config.star_data.display()
                );
                return None;
            }
        };
        let mut objects_version = String::new();
        let objects = match &config.object_data {
            Some(path) => match ObjectCatalog::open(path) {
                Ok(catalog) => {
                    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                    // The `p2` schema tag bumps whenever the persisted overlay
                    // shape changes (here: added prominence), forcing a
                    // one-time reprojection even if the catalog file is
                    // unchanged.
                    objects_version = format!("p2:{}:{}", catalog.len(), bytes);
                    info!(
                        "astro: {} objects loaded from {} (version {objects_version})",
                        catalog.len(),
                        path.display()
                    );
                    Some(catalog)
                }
                Err(e) => {
                    warn!("astro: failed to open object data {}: {e}", path.display());
                    None
                }
            },
            None => None,
        };
        let transients = config.transient_data.as_deref().and_then(|path| {
            let catalog = ReloadingCatalog::open(path);
            if catalog.is_none() {
                warn!("astro: failed to open transient data {}", path.display());
            }
            catalog
        });
        let minor_bodies = config.minor_body_data.as_deref().and_then(|path| {
            let catalog = ReloadingMinorBodies::open(path);
            match &catalog {
                Some(c) => info!(
                    "astro: {} minor bodies loaded from {}",
                    // Length via a blocking read is fine during startup
                    c.state.try_read().map(|s| s.1.len()).unwrap_or(0),
                    path.display()
                ),
                None => warn!("astro: failed to open minor-body data {}", path.display()),
            }
            catalog
        });
        Some(Arc::new(Self {
            stars,
            objects,
            objects_version,
            transients,
            blind_index_path: config.blind_index.clone(),
            blind_index: tokio::sync::OnceCell::new(),
            minor_bodies,
            failed: RwLock::new(HashMap::new()),
        }))
    }
}

/// Parse a right ascension string to degrees in [0, 360). Hour-based forms
/// ("00h 42m 44s", "5:34:32") convert at 15°/h; others are degrees.
pub fn parse_ra(value: &str) -> Option<f64> {
    let numbers = extract_numbers(value);
    let (first, rest) = numbers.split_first()?;
    if *first < 0.0 {
        return None;
    }
    let minutes = rest.first().copied().unwrap_or(0.0);
    let seconds = rest.get(1).copied().unwrap_or(0.0);
    let value_lower = value.to_lowercase();
    let hourly = value_lower.contains('h') || value.contains(':');
    let mut degrees = if hourly {
        (first + minutes / 60.0 + seconds / 3600.0) * 15.0
    } else {
        first + minutes / 60.0 + seconds / 3600.0
    };
    degrees %= 360.0;
    if degrees < 0.0 {
        degrees += 360.0;
    }
    Some(degrees)
}

/// Parse a declination string to degrees in [-90, 90].
pub fn parse_dec(value: &str) -> Option<f64> {
    let numbers = extract_numbers(value);
    let (first, rest) = numbers.split_first()?;
    let negative = value.trim_start().starts_with(['-', '−']);
    let minutes = rest.first().copied().unwrap_or(0.0);
    let seconds = rest.get(1).copied().unwrap_or(0.0);
    let degrees = (first.abs() + minutes.abs() / 60.0 + seconds.abs() / 3600.0)
        * if negative { -1.0 } else { 1.0 };
    (-90.0..=90.0).contains(&degrees).then_some(degrees)
}

fn extract_numbers(value: &str) -> Vec<f64> {
    let normalized = value.replace('−', "-");
    let mut numbers = Vec::new();
    let mut current = String::new();
    for c in normalized.chars() {
        if c.is_ascii_digit() || c == '.' || (c == '-' && current.is_empty()) {
            current.push(c);
        } else if !current.is_empty() {
            if let Ok(n) = current.parse() {
                numbers.push(n);
            }
            current.clear();
        }
    }
    if let Ok(n) = current.parse() {
        numbers.push(n);
    }
    numbers
}

/// GET /api/gallery/{name}/astro/{*path} — the WCS solution and object
/// overlay for an astro image, solving on first request.
pub async fn astro_handler(
    ResolvedState(app_state): ResolvedState,
    Path((gallery_name, image_path)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> axum::response::Response {
    let Some(astro) = app_state.astro.clone() else {
        return ApiResponse::NotFound.into_response();
    };
    let Some(gallery) = app_state.galleries().get(&gallery_name).cloned() else {
        return ApiResponse::GalleryNotFound.into_response();
    };

    // Resolve index identifiers to the source path
    let resolved_path = {
        let indexer = gallery.image_indexer.read().await;
        indexer
            .get_path(&image_path)
            .map(str::to_string)
            .unwrap_or_else(|| image_path.clone())
    };

    // Gated like the rest of the technical metadata
    let parent = resolved_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    match crate::permissions::resolve_permissions_for_path(
        &app_state,
        &gallery_name,
        parent,
        auth.username(),
    )
    .await
    {
        Ok(perms) if perms.permissions.can_see_technical_details => {}
        Ok(_) => return ApiResponse::Forbidden.into_response(),
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    }

    // Serve a previously persisted solution from the metadata sidecar
    let existing = gallery
        .user_metadata_storage
        .load(&resolved_path)
        .await
        .ok()
        .flatten();

    // The capture date scopes transients and positions minor bodies:
    // EXIF first, else the sidecar's capture_date frontmatter
    let capture_date = match gallery
        .get_image_metadata_cached(&resolved_path)
        .await
        .ok()
        .and_then(|m| m.capture_date)
        .map(chrono::DateTime::<chrono::Utc>::from)
    {
        Some(date) => Some(date),
        None => existing
            .as_ref()
            .and_then(|m| m.capture_date.as_deref())
            .and_then(parse_capture_date),
    };
    if let Some(solution) = existing.as_ref().and_then(|m| m.astro.as_ref()) {
        // Catalog upgrades reproject the overlay through the stored WCS —
        // no re-solve needed
        if astro.objects.is_some() && solution.objects_version != astro.objects_version {
            let wcs = Wcs {
                crval: (solution.crval[0], solution.crval[1]),
                crpix: (solution.crpix[0], solution.crpix[1]),
                cd: solution.cd,
            };
            let mut updated = solution.clone();
            updated.objects = placed_objects(&astro, &wcs, (solution.width, solution.height));
            updated.objects_version = astro.objects_version.clone();
            info!(
                "astro: refreshed overlay objects for {resolved_path} ({} objects)",
                updated.objects.len()
            );
            if let Err(e) = gallery
                .user_metadata_storage
                .save_astro(&resolved_path, Some(&updated))
                .await
            {
                warn!("astro: failed to persist refreshed objects for {resolved_path}: {e}");
            }
            return solution_response(&updated, &astro, capture_date).await;
        }
        return solution_response(solution, &astro, capture_date).await;
    }

    let cache_key = format!("{gallery_name}/{resolved_path}");
    if astro.failed.read().await.contains_key(&cache_key) {
        let mut response = axum::Json(serde_json::json!({ "solved": false })).into_response();
        response.headers_mut().extend(no_cache_headers());
        return response;
    }

    let solution = solve_gallery_image(&astro, &gallery, &resolved_path, existing.as_ref()).await;
    match solution {
        Some(solution) => {
            // Persist as overlay coordinates in the app-managed sidecar so
            // it survives restarts and syncs with the content
            if let Err(e) = gallery
                .user_metadata_storage
                .save_astro(&resolved_path, Some(&solution))
                .await
            {
                warn!("astro: failed to persist solution for {resolved_path}: {e}");
            }
            solution_response(&solution, &astro, capture_date).await
        }
        None => {
            astro.failed.write().await.insert(cache_key, ());
            let mut response = axum::Json(serde_json::json!({ "solved": false })).into_response();
            response.headers_mut().extend(no_cache_headers());
            response
        }
    }
}

async fn solution_response(
    solution: &crate::metadata_storage::AstroSolution,
    astro: &Arc<AstroContext>,
    capture_date: Option<chrono::DateTime<chrono::Utc>>,
) -> axum::response::Response {
    let wcs = Wcs {
        crval: (solution.crval[0], solution.crval[1]),
        crpix: (solution.crpix[0], solution.crpix[1]),
        cd: solution.cd,
    };
    let dims = (solution.width, solution.height);
    let (center_ra, center_dec) = wcs.pixel_to_world(dims.0 as f64 / 2.0, dims.1 as f64 / 2.0);
    let footprint = wcs.footprint(dims.0, dims.1);

    let mut objects: Vec<serde_json::Value> = solution
        .objects
        .iter()
        .map(|o| {
            serde_json::json!({
                "name": o.name,
                "common_name": o.common_name,
                "kind": o.kind,
                "mag": o.mag,
                "x": o.x,
                "y": o.y,
                "semi_major_px": o.semi_major_px,
                "semi_minor_px": o.semi_minor_px,
                "angle_deg": o.angle_deg,
                "prominence": o.prominence,
            })
        })
        .collect();

    // Transients are projected live (never persisted): discoveries change.
    // Each carries its discovery date and whether that falls near the
    // image's capture date, so the UI can hide long-gone events by
    // default (M31 alone accumulates hundreds of historical novae).
    if let Some(transients) = &astro.transients {
        let catalog = transients.current().await;
        let placed = catalog
            .objects_in_footprint(&wcs, dims)
            .unwrap_or_else(|e| {
                warn!("astro: transient footprint query failed: {e}");
                Vec::new()
            });
        for p in placed {
            let discovered = transient_discovery_date(&p.object.common_name);
            let near_capture = match (&discovered, capture_date) {
                (Some(date), Some(capture)) => chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map(|date| {
                        let capture = capture.date_naive();
                        date <= capture + chrono::Duration::days(30)
                            && date >= capture - chrono::Duration::days(365)
                    })
                    .unwrap_or(true),
                // Without both dates there is nothing to scope by
                _ => true,
            };
            objects.push(serde_json::json!({
                "name": p.object.name,
                "common_name": p.object.common_name,
                "kind": "transient",
                "mag": p.object.mag,
                "x": p.x,
                "y": p.y,
                "semi_major_px": p.semi_major_px,
                "semi_minor_px": p.semi_minor_px,
                "angle_deg": p.angle_deg,
                "discovered": discovered,
                "near_capture": near_capture,
            }));
        }
    }

    // Minor bodies (comets/asteroids) move: only meaningful with a
    // capture time, propagated to that instant, never persisted
    if let (Some(minor_bodies), Some(capture)) = (&astro.minor_bodies, capture_date) {
        let jd = 2_440_587.5 + capture.timestamp() as f64 / 86_400.0;
        let catalog = minor_bodies.current().await;
        for m in catalog.objects_in_footprint(&wcs, dims, jd, 18.0) {
            let kind = match m.body.kind {
                seiza::minor_bodies::MinorBodyKind::Comet => "comet",
                seiza::minor_bodies::MinorBodyKind::Asteroid => "asteroid",
            };
            // Sky position angle (comet tail / asteroid trail) becomes a
            // pixel-space angle by projecting a small step along it
            let angle_deg = m.direction_pa_deg.and_then(|pa| {
                let step = 0.05;
                let ra2 = m.ra + step * pa.to_radians().sin() / m.dec.to_radians().cos().max(0.05);
                let dec2 = (m.dec + step * pa.to_radians().cos()).clamp(-89.9, 89.9);
                let (x2, y2) = wcs.world_to_pixel(ra2, dec2)?;
                Some((y2 - m.y).atan2(x2 - m.x).to_degrees())
            });
            objects.push(serde_json::json!({
                "name": m.body.name,
                "common_name": format!("V~{:.1}, {:.2} AU", m.mag, m.delta_au),
                "kind": kind,
                "mag": m.mag,
                "x": m.x,
                "y": m.y,
                "semi_major_px": 0.0,
                "semi_minor_px": 0.0,
                "angle_deg": angle_deg.unwrap_or(0.0),
                "near_capture": true,
            }));
        }
    }

    let mut response = axum::Json(serde_json::json!({
        "solved": true,
        "width": solution.width,
        "height": solution.height,
        "center": { "ra": center_ra, "dec": center_dec },
        "scale_arcsec_px": wcs.scale_arcsec_per_px(),
        "matched_stars": solution.matched_stars,
        "rms_arcsec": solution.rms_arcsec,
        "solved_at": solution.solved_at.to_rfc3339(),
        "footprint": footprint.iter().map(|(r, d)| vec![*r, *d]).collect::<Vec<_>>(),
        "wcs": {
            "crval": solution.crval,
            "crpix": solution.crpix,
            "cd": solution.cd,
        },
        "objects": objects,
    }))
    .into_response();
    response.headers_mut().extend(no_cache_headers());
    response
}

/// Decode, detect, and solve one gallery image. Returns None when the image
/// has no RA/Dec hint or no solution is found.
/// Blind-solve parameters for the gallery path: the scale range spans the
/// hinted ladder's coverage. Without a prebuilt index the magnitude limit
/// keeps an on-demand build to a couple of hundred MB instead of multiple
/// GB on deep catalogs — at the cost of the fine-scale tiers.
fn blind_build_params() -> seiza::blind::BlindParams {
    seiza::blind::BlindParams {
        min_scale_arcsec_px: SCALE_LADDER[0] * (1.0 - SCALE_TOLERANCE),
        max_scale_arcsec_px: SCALE_LADDER[SCALE_LADDER.len() - 1] * (1.0 + SCALE_TOLERANCE),
        index_mag_limit: 11.8,
        ..Default::default()
    }
}

/// Solving must use the depth and pattern extent the index was built with,
/// whichever way it was obtained.
fn blind_params(index: &seiza::blind::BlindIndex) -> seiza::blind::BlindParams {
    seiza::blind::BlindParams {
        index_mag_limit: index.index_mag_limit(),
        max_pattern_deg: index.max_pattern_deg(),
        ..blind_build_params()
    }
}

impl AstroContext {
    /// The blind pattern index: memory-mapped from the configured file, or
    /// built from the star catalog on first use (a few seconds).
    async fn blind_index(self: &Arc<Self>) -> Option<Arc<seiza::blind::BlindIndex>> {
        self.blind_index
            .get_or_init(|| async {
                let astro = self.clone();
                let built = tokio::task::spawn_blocking(move || {
                    let started = std::time::Instant::now();
                    if let Some(path) = &astro.blind_index_path {
                        match seiza::blind::BlindIndex::open(path) {
                            Ok(index) => {
                                let built_from = index.source_star_count();
                                let stars = astro.stars.star_count();
                                if built_from > 0
                                    && built_from.max(stars) > 2 * built_from.min(stars)
                                {
                                    warn!(
                                        "astro: blind index {} was built from {built_from} stars \
                                         but star_data has {stars}; blind solves may fail",
                                        path.display()
                                    );
                                }
                                info!(
                                    "astro: blind index mapped from {}: {} patterns (G<={:.1}) in {:.2}s",
                                    path.display(),
                                    index.pattern_count(),
                                    index.index_mag_limit(),
                                    started.elapsed().as_secs_f64()
                                );
                                return Some(Arc::new(index));
                            }
                            Err(e) => {
                                warn!(
                                    "astro: failed to open blind index {}: {e}; building the \
                                     bright tiers from star_data instead",
                                    path.display()
                                );
                            }
                        }
                    }
                    let index = seiza::blind::BlindIndex::build(&astro.stars, &blind_build_params());
                    info!(
                        "astro: blind index built: {} patterns in {:.1}s",
                        index.pattern_count(),
                        started.elapsed().as_secs_f64()
                    );
                    Some(Arc::new(index))
                })
                .await;
                match built {
                    Ok(index) => index,
                    Err(e) => {
                        warn!("astro: blind index load panicked: {e}");
                        None
                    }
                }
            })
            .await
            .clone()
    }
}

/// True when the image's folder (or any ancestor) sets `astro = true`
/// in its `_folder.md` config.
async fn folder_is_astro(gallery: &crate::gallery::SharedGallery, path: &str) -> bool {
    let mut folder = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    loop {
        if let Some(metadata) = gallery.read_folder_metadata_full(folder).await
            && metadata.config.astro
        {
            return true;
        }
        if folder.is_empty() {
            return false;
        }
        folder = folder.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    }
}

async fn solve_gallery_image(
    astro: &Arc<AstroContext>,
    gallery: &crate::gallery::SharedGallery,
    path: &str,
    metadata: Option<&crate::metadata_storage::ImageUserMetadata>,
) -> Option<crate::metadata_storage::AstroSolution> {
    // The RA/Dec hint comes from the user metadata sidecar; without one
    // (comets and other targets with no fixed coordinates) fall back to
    // blind solving — but only for images marked as astro (a telescope in
    // the metadata), never arbitrary photos
    let hint =
        metadata.and_then(|m| Some((parse_ra(m.ra.as_deref()?)?, parse_dec(m.dec.as_deref()?)?)));
    if hint.is_none()
        && metadata.is_none_or(|m| m.telescope.is_none())
        && !folder_is_astro(gallery, path).await
    {
        return None;
    }

    let bytes = match gallery.source_storage.read(path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("astro: failed to read {path}: {e}");
            return None;
        }
    };

    // A sidecar pixel scale turns the ladder walk into a single solve
    let hint_scale = metadata.and_then(|m| m.pixel_scale).filter(|s| *s > 0.0);
    let blind_index = match hint {
        Some(_) => None,
        None => Some(astro.blind_index().await?),
    };
    let astro = astro.clone();
    let path_owned = path.to_string();
    let result = tokio::task::spawn_blocking(move || match (hint, blind_index) {
        (Some(center), _) => solve_bytes(&astro, &bytes, center, hint_scale, &path_owned),
        (None, Some(index)) => solve_bytes_blind(&astro, &bytes, &index, &path_owned),
        (None, None) => None,
    })
    .await;
    match result {
        Ok(solution) => solution,
        Err(e) => {
            warn!("astro: solve task panicked for {path}: {e}");
            None
        }
    }
}

/// Project the object catalog through a solution into overlay coordinates.
/// Parse a sidecar capture_date string: ISO datetime, plain date, or the
/// app's own "July 12, 2026 at 04:15:00" rendering. Plain dates resolve
/// to local midnight-ish UTC — fine for objects moving arcmin/day.
fn parse_capture_date(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let text = text.trim();
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(datetime.into());
    }
    if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") {
        return Some(datetime.and_utc());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(date.and_hms_opt(6, 0, 0)?.and_utc());
    }
    if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(text, "%B %d, %Y at %H:%M:%S") {
        return Some(datetime.and_utc());
    }
    None
}

/// Extract the discovery date from a transient's detail text
/// ("type II, disc. 2026/07/08, in NGC 3310") as an ISO date.
fn transient_discovery_date(details: &str) -> Option<String> {
    let raw = details
        .split(", ")
        .find_map(|part| part.strip_prefix("disc. "))?;
    let mut parts = raw.split('/');
    let year: i32 = parts.next()?.trim().parse().ok()?;
    let month: u32 = parts.next()?.trim().parse().ok()?;
    let day: u32 = parts.next()?.trim().parse().ok()?;
    ((1900..3000).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day))
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

pub(crate) fn placed_objects(
    astro: &AstroContext,
    wcs: &Wcs,
    dims: (u32, u32),
) -> Vec<crate::metadata_storage::AstroObject> {
    astro
        .objects
        .as_ref()
        .map(|catalog| {
            let placed = catalog.objects_in_footprint(wcs, dims).unwrap_or_else(|e| {
                warn!("astro: object footprint query failed: {e}");
                Vec::new()
            });
            // objects_in_footprint gives placement but drops the catalog
            // prominence; a second query over the same footprint recovers it,
            // joined by name (unique within the catalog).
            let prominence = object_prominence(catalog, wcs, dims);
            placed
                .into_iter()
                .map(|p| crate::metadata_storage::AstroObject {
                    prominence: prominence.get(p.object.name.as_str()).copied(),
                    name: p.object.name,
                    common_name: p.object.common_name,
                    kind: p.object.kind.as_str().to_string(),
                    mag: p.object.mag,
                    x: p.x,
                    y: p.y,
                    semi_major_px: p.semi_major_px,
                    semi_minor_px: p.semi_minor_px,
                    angle_deg: p.angle_deg,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Catalog prominence (0–1) keyed by object name for the image footprint.
/// `objects_in_footprint` computes this internally but returns only placement,
/// so we re-run the region query to keep the ranking without duplicating the
/// projection math.
fn object_prominence(
    catalog: &ObjectCatalog,
    wcs: &Wcs,
    dims: (u32, u32),
) -> std::collections::HashMap<String, f32> {
    let region = seiza::objects::SkyRegion::Polygon {
        vertices: wcs.footprint(dims.0, dims.1).to_vec(),
    };
    catalog
        .query_region(&region, &seiza::objects::ObjectQuery::default())
        .map(|hits| {
            hits.into_iter()
                .map(|h| (h.object.name, h.predicted_prominence as f32))
                .collect()
        })
        .unwrap_or_default()
}

/// Decode (including AVIF), downsample for detection, and return
/// full-resolution star centroids plus the original dimensions.
fn detect_in_bytes(bytes: &[u8], path: &str) -> Option<(Vec<seiza::DetectedStar>, (u32, u32))> {
    // AVIF needs the custom reader (the image crate cannot decode it)
    #[cfg(feature = "avif")]
    let decoded = if bytes.len() > 12 && &bytes[4..12] == b"ftypavif" {
        crate::gallery::image_processing::formats::avif::read_avif_info_from_bytes(bytes)
            .map(|(image, _)| image)
            .map_err(|e| format!("{e}"))
    } else {
        image::load_from_memory(bytes).map_err(|e| format!("{e}"))
    };
    #[cfg(not(feature = "avif"))]
    let decoded = image::load_from_memory(bytes).map_err(|e| format!("{e}"));
    let image = match decoded {
        Ok(image) => image,
        Err(e) => {
            warn!("astro: failed to decode {path}: {e}");
            return None;
        }
    };
    let (width, height) = (image.width(), image.height());

    // Downsample for detection speed; scale centroids back afterwards
    let factor = (width.max(height) as f64 / DETECT_MAX_DIM as f64).max(1.0);
    let detected_image = if factor > 1.0 {
        image.thumbnail(
            (width as f64 / factor) as u32,
            (height as f64 / factor) as u32,
        )
    } else {
        image
    };

    let config = seiza::DetectConfig {
        // Captions and frames live at the edges
        ignore_border: (detected_image.height() as f32 * 0.02) as u32,
        max_stars: 200,
        ..Default::default()
    };
    let mut stars = seiza::detect_stars(&detected_image, &config);
    for star in &mut stars {
        star.x *= factor;
        star.y *= factor;
    }
    Some((stars, (width, height)))
}

/// Persistable solution from a solved WCS.
fn solution_from(
    astro: &AstroContext,
    wcs: &Wcs,
    matched_stars: usize,
    rms_arcsec: f64,
    dims: (u32, u32),
) -> crate::metadata_storage::AstroSolution {
    let objects = placed_objects(astro, wcs, dims);
    crate::metadata_storage::AstroSolution {
        objects_version: astro.objects_version.clone(),
        solved_at: chrono::Utc::now(),
        width: dims.0,
        height: dims.1,
        crval: [wcs.crval.0, wcs.crval.1],
        crpix: [wcs.crpix.0, wcs.crpix.1],
        cd: wcs.cd,
        matched_stars: matched_stars as u32,
        rms_arcsec,
        objects,
    }
}

/// Blind-solve an image with no coordinate hint.
fn solve_bytes_blind(
    astro: &AstroContext,
    bytes: &[u8],
    index: &seiza::blind::BlindIndex,
    path: &str,
) -> Option<crate::metadata_storage::AstroSolution> {
    let started = std::time::Instant::now();
    let (stars, dims) = detect_in_bytes(bytes, path)?;
    match seiza::blind::solve_blind(&stars, &astro.stars, index, &blind_params(index), dims) {
        Ok(Solution {
            wcs,
            matched_stars,
            rms_arcsec,
        }) => {
            info!(
                "astro: blind-solved {path} at {:.3}\"/px, {} stars, RMS {:.2}\" in {:.2}s",
                wcs.scale_arcsec_per_px(),
                matched_stars,
                rms_arcsec,
                started.elapsed().as_secs_f64()
            );
            Some(solution_from(astro, &wcs, matched_stars, rms_arcsec, dims))
        }
        Err(e) => {
            info!("astro: blind solve failed for {path}: {e}");
            None
        }
    }
}

fn solve_bytes(
    astro: &AstroContext,
    bytes: &[u8],
    hint_center: (f64, f64),
    hint_scale: Option<f64>,
    path: &str,
) -> Option<crate::metadata_storage::AstroSolution> {
    let started = std::time::Instant::now();
    let (stars, (width, height)) = detect_in_bytes(bytes, path)?;

    // The sidecar scale (when given) is one solve; the ladder is a fallback
    // for a wrong or absent one, and each rung it walks is a failed solve
    let attempts = hint_scale
        .map(|scale| (scale, HINTED_SCALE_TOLERANCE))
        .into_iter()
        .chain(SCALE_LADDER.iter().map(|&s| (s, SCALE_TOLERANCE)));

    for (scale, scale_tolerance) in attempts {
        let hint = SolveHint {
            center: hint_center,
            radius_deg: SEARCH_RADIUS_DEG,
            // Detections were rescaled to full-resolution pixels above, so
            // the ladder scale applies as-is regardless of downsampling
            scale_arcsec_px: scale,
            scale_tolerance,
        };
        // Solve in full-resolution pixel space
        if let Ok(Solution {
            wcs,
            matched_stars,
            rms_arcsec,
        }) = seiza::solve::solve(&stars, &astro.stars, &hint, (width, height))
        {
            info!(
                "astro: solved {path} at {:.3}\"/px, {} stars, RMS {:.2}\" in {:.2}s",
                wcs.scale_arcsec_per_px(),
                matched_stars,
                rms_arcsec,
                started.elapsed().as_secs_f64()
            );
            return Some(solution_from(
                astro,
                &wcs,
                matched_stars,
                rms_arcsec,
                (width, height),
            ));
        }
    }
    info!(
        "astro: no solution for {path} after {:.2}s",
        started.elapsed().as_secs_f64()
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ra_forms() {
        assert!((parse_ra("00h 42m 44s").unwrap() - 10.6833).abs() < 0.001);
        assert!((parse_ra("5:34:32").unwrap() - 83.6333).abs() < 0.001);
        assert!((parse_ra("83.82").unwrap() - 83.82).abs() < 1e-9);
        assert!(parse_ra("").is_none());
        assert!(parse_ra("north").is_none());
    }

    #[test]
    fn parses_dec_forms() {
        assert!((parse_dec("+41° 16′ 09″").unwrap() - 41.2692).abs() < 0.001);
        assert!((parse_dec("-05d 23m 28s").unwrap() - -5.3911).abs() < 0.001);
        assert!((parse_dec("−22° 00′").unwrap() - -22.0).abs() < 1e-6);
        assert!(parse_dec("95").is_none());
        assert!(parse_dec("").is_none());
    }

    /// Reordering the ladder is only safe if the rungs still overlap: a
    /// scale that falls in no rung's window can never solve, whatever the
    /// order. Sorted, each rung's upper reach must meet the next's lower.
    #[test]
    fn scale_ladder_rungs_overlap() {
        let mut sorted = SCALE_LADDER.to_vec();
        sorted.sort_by(f64::total_cmp);
        for pair in sorted.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            assert!(
                low * (1.0 + SCALE_TOLERANCE) >= high * (1.0 - SCALE_TOLERANCE),
                "gap between {low} and {high}: scales in between cannot solve"
            );
        }
    }

    /// The common rungs come first: an image that solves at a typical
    /// astrophoto scale must not pay for failed fine-scale attempts.
    #[test]
    fn scale_ladder_tries_common_scales_first() {
        assert_eq!(SCALE_LADDER[0], 2.8);
        assert_eq!(SCALE_LADDER[1], 1.4);
        let fine = SCALE_LADDER.iter().position(|&s| s <= 0.2).unwrap();
        let common = SCALE_LADDER.iter().position(|&s| s == 1.4).unwrap();
        assert!(common < fine, "fine scales must be tried after common ones");
    }
}
