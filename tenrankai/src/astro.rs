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

/// Pixel-scale ladder tried in order; ±35 % tolerance makes the rungs
/// overlap, covering roughly 0.23–7.5 arcsec/pixel.
const SCALE_LADDER: &[f64] = &[0.35, 0.7, 1.4, 2.8, 5.6];
const SCALE_TOLERANCE: f64 = 0.35;
const SEARCH_RADIUS_DEG: f64 = 2.0;
/// Images are downsampled to at most this many pixels on the long side
/// before star detection (centroids are scaled back to full resolution)
const DETECT_MAX_DIM: u32 = 2800;

pub struct AstroContext {
    stars: TileCatalog,
    objects: Option<ObjectCatalog>,
    /// Failed solve attempts this process, keyed by "gallery/path" —
    /// successes persist into the image's metadata sidecar instead
    failed: RwLock<HashMap<String, ()>>,
}

impl AstroContext {
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
        let objects = match &config.object_data {
            Some(path) => match ObjectCatalog::open(path) {
                Ok(catalog) => {
                    info!(
                        "astro: {} objects loaded from {}",
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
        Some(Arc::new(Self {
            stars,
            objects,
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
    if let Some(solution) = existing.as_ref().and_then(|m| m.astro.as_ref()) {
        return solution_response(solution);
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
            solution_response(&solution)
        }
        None => {
            astro.failed.write().await.insert(cache_key, ());
            let mut response = axum::Json(serde_json::json!({ "solved": false })).into_response();
            response.headers_mut().extend(no_cache_headers());
            response
        }
    }
}

fn solution_response(
    solution: &crate::metadata_storage::AstroSolution,
) -> axum::response::Response {
    let wcs = Wcs {
        crval: (solution.crval[0], solution.crval[1]),
        crpix: (solution.crpix[0], solution.crpix[1]),
        cd: solution.cd,
    };
    let dims = (solution.width, solution.height);
    let (center_ra, center_dec) = wcs.pixel_to_world(dims.0 as f64 / 2.0, dims.1 as f64 / 2.0);
    let footprint = wcs.footprint(dims.0, dims.1);

    let objects: Vec<serde_json::Value> = solution
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
            })
        })
        .collect();

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
async fn solve_gallery_image(
    astro: &Arc<AstroContext>,
    gallery: &crate::gallery::SharedGallery,
    path: &str,
    metadata: Option<&crate::metadata_storage::ImageUserMetadata>,
) -> Option<crate::metadata_storage::AstroSolution> {
    // The RA/Dec hint comes from the user metadata sidecar
    let metadata = metadata?;
    let ra = parse_ra(metadata.ra.as_deref()?)?;
    let dec = parse_dec(metadata.dec.as_deref()?)?;

    let bytes = match gallery.source_storage.read(path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("astro: failed to read {path}: {e}");
            return None;
        }
    };

    let astro = astro.clone();
    let path_owned = path.to_string();
    let result =
        tokio::task::spawn_blocking(move || solve_bytes(&astro, &bytes, (ra, dec), &path_owned))
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
fn placed_objects(
    astro: &AstroContext,
    wcs: &Wcs,
    dims: (u32, u32),
) -> Vec<crate::metadata_storage::AstroObject> {
    astro
        .objects
        .as_ref()
        .map(|catalog| {
            catalog
                .objects_in_footprint(wcs, dims)
                .into_iter()
                .map(|p| crate::metadata_storage::AstroObject {
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

fn solve_bytes(
    astro: &AstroContext,
    bytes: &[u8],
    hint_center: (f64, f64),
    path: &str,
) -> Option<crate::metadata_storage::AstroSolution> {
    let started = std::time::Instant::now();
    let image = match image::load_from_memory(bytes) {
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

    for &scale in SCALE_LADDER {
        let hint = SolveHint {
            center: hint_center,
            radius_deg: SEARCH_RADIUS_DEG,
            scale_arcsec_px: scale * factor,
            scale_tolerance: SCALE_TOLERANCE,
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
            let objects = placed_objects(astro, &wcs, (width, height));
            return Some(crate::metadata_storage::AstroSolution {
                solved_at: chrono::Utc::now(),
                width,
                height,
                crval: [wcs.crval.0, wcs.crval.1],
                crpix: [wcs.crpix.0, wcs.crpix.1],
                cd: wcs.cd,
                matched_stars: matched_stars as u32,
                rms_arcsec,
                objects,
            });
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
}
