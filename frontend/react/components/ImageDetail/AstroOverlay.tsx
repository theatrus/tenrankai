import { useEffect, useState } from 'react';
import {
  partitionOverlayObjects,
  suggestedDeepSkyColorForObject,
  type OverlayLayerVisibility,
  type OverlayObject,
} from '@seiza/astro-overlay';
import { AstroOverlay as OverlaySvg } from '@seiza/astro-overlay/react';

/**
 * One placed object in the overlay API response. The shape is shared with
 * `@seiza/astro-overlay` (which was modeled on this API): geometry in image
 * pixels, plus optional precise catalog outlines projected by the server.
 */
export type PlacedObject = OverlayObject;

export interface AstroSolution {
  solved: boolean;
  width: number;
  height: number;
  center?: { ra: number; dec: number };
  scale_arcsec_px?: number;
  matched_stars?: number;
  rms_arcsec?: number;
  objects?: PlacedObject[];
}

/**
 * Fetches the plate solution for an image; null until loaded, and
 * solutions with `solved: false` (or missing astro support) resolve to null.
 */
export function useAstroSolution(galleryName: string, imagePath: string): AstroSolution | null {
  const [solution, setSolution] = useState<AstroSolution | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSolution(null);
    fetch(
      `/api/gallery/${encodeURIComponent(galleryName)}/astro/${encodeURIComponent(imagePath)}`,
    )
      .then((response) => (response.ok ? response.json() : null))
      .then((data: AstroSolution | null) => {
        if (!cancelled && data && data.solved) {
          setSolution(data);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [galleryName, imagePath]);

  return solution;
}

/** Catalog group of an object, for the per-catalog display toggles. */
export function catalogGroup(o: PlacedObject): string {
  if (o.kind === 'transient') return 'transients';
  if (o.kind === 'comet' || o.kind === 'asteroid') return 'solar-system';
  const name = o.name;
  if (name.startsWith('PGC')) return 'pgc';
  if (name.startsWith('UGC')) return 'ugc';
  if (name.startsWith('LBN')) return 'lbn';
  if (name.startsWith('Ced')) return 'cederblad';
  if (name.startsWith('LDN') || /^B\d/.test(name)) return 'dark-nebulae';
  if (name.startsWith('SNR')) return 'snr';
  if (name.startsWith('WR ')) return 'wr';
  if (o.kind === 'star' || o.kind === 'double-star') return 'stars';
  if (name.startsWith('Sh2-') || name.startsWith('vdB')) return 'sharpless-vdb';
  return 'ngc-ic-messier';
}

/** Display names for catalog groups, in menu order. */
export const CATALOG_GROUPS: [string, string][] = [
  ['ngc-ic-messier', 'NGC / IC / Messier'],
  ['sharpless-vdb', 'Sharpless / vdB'],
  ['lbn', 'LBN (bright nebulae)'],
  ['cederblad', 'Cederblad'],
  ['dark-nebulae', 'Dark nebulae (B / LDN)'],
  ['snr', 'Supernova remnants'],
  ['wr', 'Wolf-Rayet stars'],
  ['stars', 'Stars (HD / named)'],
  ['ugc', 'UGC galaxies'],
  ['pgc', 'PGC galaxies'],
  ['solar-system', 'Comets / asteroids'],
];

/** Default label density (0 = only the most prominent, 1 = every object). */
export const DEFAULT_LABEL_DENSITY = 0.6;
/** Below this many rankable objects, the density control adds nothing. */
const DENSITY_FLOOR = 4;

/**
 * The package partitions by its own layer taxonomy; our per-catalog and
 * historical-transient filtering happens before objects reach it, so every
 * layer stays enabled and the grid stays off (this overlay has never
 * drawn one).
 */
const ALL_LAYERS: OverlayLayerVisibility = {
  deep_sky: true,
  named_stars: true,
  star_identifiers: true,
  field_stars: true,
  transients: true,
  minor_bodies: true,
  historical_transients: true,
  grid: false,
};

/** Objects that survive the catalog toggles and the historical-transient
 * default, i.e. everything eligible for rendering. */
function visibleObjects(
  solution: AstroSolution,
  hiddenGroups: string[],
  allTransients: boolean,
): PlacedObject[] {
  const everything = (solution.objects || []).filter(
    (o) => !hiddenGroups.includes(catalogGroup(o)),
  );
  if (allTransients) return everything;
  const distant = distantTransients(solution);
  return everything.filter((o) => !distant.includes(o));
}

/**
 * Split the field into the objects to label and the frame-filling ones shown
 * as a "Field within" caption. Prominence (from the catalog) ranks the named
 * features so a density budget can keep wide fields legible: transients and
 * minor bodies have no prominence and are always kept. Delegates to the
 * shared `@seiza/astro-overlay` partition so the button count always
 * matches what the packaged renderer draws.
 */
export function partitionObjects(
  solution: AstroSolution,
  opts: { hiddenGroups: string[]; allTransients: boolean; density: number },
): { rendered: PlacedObject[]; encompassing: PlacedObject[]; total: number } {
  const { rendered, encompassing, total } = partitionOverlayObjects(
    visibleObjects(solution, opts.hiddenGroups, opts.allTransients),
    solution.width,
    solution.height,
    { density: opts.density, minimumRankedObjects: DENSITY_FLOOR, layers: ALL_LAYERS },
  );
  return { rendered, encompassing, total };
}

/** Transients not discovered near the capture date (hidden by default). */
export function distantTransients(solution: AstroSolution): PlacedObject[] {
  return (solution.objects || []).filter(
    (o) => o.kind === 'transient' && o.near_capture === false,
  );
}

interface AstroOverlayProps {
  solution: AstroSolution;
  visible: boolean;
  allTransients: boolean;
  /** Catalog groups (see [`catalogGroup`]) currently hidden */
  hiddenGroups?: string[];
  /** Label density 0–1: fewer, most-prominent labels → every object. */
  density?: number;
  /** Draw precise catalog outlines (OpenNGC contours) instead of ellipses
   * for objects that have them. */
  preciseOutlines?: boolean;
}

/**
 * The SVG marker layer, rendered by `@seiza/astro-overlay`. Toggle buttons
 * live in [`AstroControls`]; this component only maps our per-catalog and
 * transient filtering onto the package's object list.
 */
export function AstroOverlay({
  solution,
  visible,
  allTransients,
  hiddenGroups = [],
  density = DEFAULT_LABEL_DENSITY,
  preciseOutlines = true,
}: AstroOverlayProps) {
  if (!visible) return null;

  const objects = visibleObjects(solution, hiddenGroups, allTransients).map((o) =>
    preciseOutlines || !o.outlines?.length ? o : { ...o, outlines: [] },
  );

  return (
    <OverlaySvg
      solution={{
        image_width: solution.width,
        image_height: solution.height,
        center_ra_deg: solution.center?.ra,
        center_dec_deg: solution.center?.dec,
        pixel_scale_arcsec_per_pixel: solution.scale_arcsec_px,
        matched_stars: solution.matched_stars,
        rms_arcsec: solution.rms_arcsec,
      }}
      objects={objects}
      layers={ALL_LAYERS}
      density={density}
      minimumRankedObjects={DENSITY_FLOOR}
      colorForObject={suggestedDeepSkyColorForObject}
      showCenter={false}
      aria-label="Sky object overlay"
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        width: '100%',
        height: '100%',
        zIndex: 2,
        pointerEvents: 'none',
      }}
    />
  );
}

interface AstroControlsProps {
  solution: AstroSolution;
  visible: boolean;
  onVisibleChange: (visible: boolean) => void;
  allTransients: boolean;
  onAllTransientsChange: (all: boolean) => void;
  hiddenGroups: string[];
  onHiddenGroupsChange: (groups: string[]) => void;
  density: number;
  onDensityChange: (density: number) => void;
  preciseOutlines?: boolean;
  onPreciseOutlinesChange?: (outlines: boolean) => void;
}

/**
 * Overlay toggles for the image controls bar — off the image itself, so
 * they don't obscure it and stay clearly visible when active.
 */
export function AstroControls({
  solution,
  visible,
  onVisibleChange,
  allTransients,
  onAllTransientsChange,
  hiddenGroups,
  onHiddenGroupsChange,
  density,
  onDensityChange,
  preciseOutlines,
  onPreciseOutlinesChange,
}: AstroControlsProps) {
  const objects = solution.objects || [];
  const { rendered, total } = partitionObjects(solution, {
    hiddenGroups,
    allTransients,
    density,
  });
  const shown = rendered.length;
  const distant = distantTransients(solution).filter(
    (o) => !hiddenGroups.includes(catalogGroup(o)),
  );
  const availableGroups = CATALOG_GROUPS.filter(([id]) =>
    objects.some((o) => catalogGroup(o) === id),
  );
  // The density control only matters when it can hide something.
  const rankableTotal = objects.filter(
    (o) => o.prominence != null && !hiddenGroups.includes(catalogGroup(o)),
  ).length;

  return (
    <div className="control-buttons astro-controls">
      <button
        type="button"
        className={`btn ${visible ? 'btn-primary' : 'btn-secondary'}`}
        onClick={() => onVisibleChange(!visible)}
        title={`${shown} of ${total} objects — solved at ${solution.scale_arcsec_px?.toFixed(2)}″/px`}
      >
        {visible ? 'Objects ✕' : `Objects (${shown})`}
      </button>
      {visible && distant.length > 0 && (
        <button
          type="button"
          className={`btn ${allTransients ? 'btn-primary' : 'btn-secondary'}`}
          onClick={() => onAllTransientsChange(!allTransients)}
          title="Transients discovered long before or after this image was captured"
        >
          {allTransients ? 'Hide old transients' : `+${distant.length} old transients`}
        </button>
      )}
      {visible && rankableTotal > DENSITY_FLOOR && (
        <label
          className="astro-density"
          title={`Label density — showing ${shown} of ${total}`}
        >
          <span aria-hidden="true">Fewer</span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={density}
            onChange={(e) => onDensityChange(Number(e.target.value))}
            aria-label="Label density"
          />
          <span aria-hidden="true">More</span>
        </label>
      )}
      {visible && availableGroups.length > 1 && (
        <CatalogMenu
          solution={solution}
          hiddenGroups={hiddenGroups}
          onHiddenGroupsChange={onHiddenGroupsChange}
          preciseOutlines={preciseOutlines}
          onPreciseOutlinesChange={onPreciseOutlinesChange}
        />
      )}
    </div>
  );
}

interface CatalogMenuProps {
  solution: AstroSolution;
  hiddenGroups: string[];
  onHiddenGroupsChange: (groups: string[]) => void;
  preciseOutlines?: boolean;
  onPreciseOutlinesChange?: (outlines: boolean) => void;
  /** Small pill styling for tight spots (post embeds) */
  compact?: boolean;
}

/** The per-catalog visibility dropdown, shared by the detail-page
 * controls and post embeds. */
export function CatalogMenu({
  solution,
  hiddenGroups,
  onHiddenGroupsChange,
  preciseOutlines,
  onPreciseOutlinesChange,
  compact = false,
}: CatalogMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const groupCounts = new Map<string, number>();
  for (const o of solution.objects || []) {
    const group = catalogGroup(o);
    groupCounts.set(group, (groupCounts.get(group) || 0) + 1);
  }
  const availableGroups = CATALOG_GROUPS.filter(([id]) => groupCounts.has(id));
  if (availableGroups.length < 2) return null;
  // The outline toggle only appears when the catalog actually carries
  // precise outlines for something in this field.
  const outlined = (solution.objects || []).filter((o) => o.outlines?.length).length;

  const toggleGroup = (id: string) => {
    onHiddenGroupsChange(
      hiddenGroups.includes(id)
        ? hiddenGroups.filter((g) => g !== id)
        : [...hiddenGroups, id],
    );
  };
  const stop = (e: React.SyntheticEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <span
      style={{ position: 'relative', display: 'inline-block', pointerEvents: 'auto' }}
      onClick={compact ? stop : undefined}
      onTouchEnd={compact ? (e) => e.stopPropagation() : undefined}
    >
      <button
        type="button"
        className={compact ? 'post-astro-toggle-pill' : `btn ${hiddenGroups.length > 0 ? 'btn-primary' : 'btn-secondary'}`}
        aria-expanded={menuOpen}
        onClick={(e) => {
          if (compact) stop(e);
          setMenuOpen(!menuOpen);
        }}
        title="Choose which catalogs to label"
        style={
          compact
            ? {
                touchAction: 'manipulation',
                padding: '0.1rem 0.6rem',
                borderRadius: '999px',
                border: '1px solid rgba(255,255,255,0.4)',
                background:
                  hiddenGroups.length > 0 ? 'rgba(80,180,255,0.4)' : 'rgba(0,0,0,0.55)',
                color: '#fff',
                fontSize: '0.72rem',
                cursor: 'pointer',
              }
            : undefined
        }
      >
        Catalogs {menuOpen ? '▴' : '▾'}
      </button>
      {menuOpen && (
        <span className="astro-catalog-menu" role="menu">
          {availableGroups.map(([id, label]) => (
            <label key={id} className="astro-catalog-item">
              <input
                type="checkbox"
                checked={!hiddenGroups.includes(id)}
                onChange={() => toggleGroup(id)}
              />
              <span>
                {label} ({groupCounts.get(id)})
              </span>
            </label>
          ))}
          {onPreciseOutlinesChange && outlined > 0 && (
            <label
              className="astro-catalog-item astro-outline-item"
              title="Draw catalog brightness contours instead of ellipses where available"
            >
              <input
                type="checkbox"
                checked={preciseOutlines !== false}
                onChange={() => onPreciseOutlinesChange(preciseOutlines === false)}
              />
              <span>Precise outlines ({outlined})</span>
            </label>
          )}
        </span>
      )}
    </span>
  );
}
