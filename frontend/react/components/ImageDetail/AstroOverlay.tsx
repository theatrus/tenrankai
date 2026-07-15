import { useEffect, useState } from 'react';

interface PlacedObject {
  name: string;
  common_name: string;
  kind: string;
  mag?: number | null;
  x: number;
  y: number;
  semi_major_px: number;
  semi_minor_px: number;
  angle_deg: number;
  /** Catalog prominence 0–1; absent for transients and minor bodies. */
  prominence?: number | null;
  /** Transients only: ISO discovery date, when known */
  discovered?: string | null;
  /** Transients only: discovered near this image's capture date */
  near_capture?: boolean;
}

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

/**
 * Renders inside the image display container: a toggle button and, when
 * enabled, an SVG layer drawing the solved objects over the image.
 */
/** True when the object's ellipse contains the entire image frame. */
function encompassesFrame(o: PlacedObject, width: number, height: number): boolean {
  if (o.semi_major_px <= 0) return false;
  const rad = (o.angle_deg * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  return [
    [0, 0],
    [width, 0],
    [width, height],
    [0, height],
  ].every(([cx, cy]) => {
    const dx = cx - o.x;
    const dy = cy - o.y;
    const u = (dx * cos + dy * sin) / o.semi_major_px;
    const v = (-dx * sin + dy * cos) / Math.max(o.semi_minor_px, 1);
    return u * u + v * v <= 1;
  });
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
 * Split the field into the objects to label and the frame-filling ones shown
 * as a "Field within" caption. Prominence (from the catalog) ranks the named
 * features so a density budget can keep wide fields legible: transients and
 * minor bodies have no prominence and are always kept. Shared by the overlay
 * and the controls so the button count matches what is drawn.
 */
export function partitionObjects(
  solution: AstroSolution,
  opts: { hiddenGroups: string[]; allTransients: boolean; density: number },
): { rendered: PlacedObject[]; encompassing: PlacedObject[]; total: number } {
  const { width, height } = solution;
  const everything = (solution.objects || []).filter(
    (o) => !opts.hiddenGroups.includes(catalogGroup(o)),
  );
  const distant = distantTransients(solution).filter((o) => everything.includes(o));
  const all = opts.allTransients ? everything : everything.filter((o) => !distant.includes(o));
  const encompassing = all.filter((o) => encompassesFrame(o, width, height));
  const inFrame = all.filter((o) => !encompassing.includes(o));

  const rankable = inFrame
    .filter((o) => o.prominence != null)
    .sort((a, b) => (b.prominence as number) - (a.prominence as number));
  const unrankable = inFrame.filter((o) => o.prominence == null);

  const floor = Math.min(rankable.length, DENSITY_FLOOR);
  const budget = Math.round(floor + (rankable.length - floor) * opts.density);
  // Unrankable (transients / comets) first so their labels win collisions,
  // then the most prominent named features up to the density budget.
  const rendered = [...unrankable, ...rankable.slice(0, Math.max(floor, budget))];
  return { rendered, encompassing, total: inFrame.length };
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
}

/** The SVG marker layer. Toggle buttons live in [`AstroControls`]. */
export function AstroOverlay({
  solution,
  visible,
  allTransients,
  hiddenGroups = [],
  density = DEFAULT_LABEL_DENSITY,
}: AstroOverlayProps) {

  const height = solution.height;
  // Prominence-ranked, density-budgeted labels; frame-filling objects become
  // a caption instead. Transients far from the capture date stay hidden
  // unless allTransients is set (M31 accumulates hundreds of historical novae).
  const { rendered: objects, encompassing } = partitionObjects(solution, {
    hiddenGroups,
    allTransients,
    density,
  });
  const stroke = Math.max(solution.width / 1200, 1.5);
  const fontSize = Math.max(solution.width / 70, 14);

  // Nudge colliding labels apart: labels default above their object; when
  // two anchors are close, later ones stack further up (or flip below).
  const labelText = (o: PlacedObject) =>
    o.common_name && o.common_name !== o.name
      ? `${o.name} · ${o.common_name}`
      : o.common_name || o.name;
  const placedLabels: { x: number; y: number; halfWidth: number }[] = [];
  const labelY = (o: PlacedObject): number => {
    const halfWidth = (labelText(o).length * fontSize * 0.55) / 2;
    const b = Math.max(o.semi_minor_px, fontSize);
    let y = o.y - b - fontSize * 0.5;
    let collided = true;
    let attempts = 0;
    while (collided && attempts < 6) {
      collided = placedLabels.some(
        (l) =>
          Math.abs(l.y - y) < fontSize * 1.3 && Math.abs(l.x - o.x) < l.halfWidth + halfWidth,
      );
      if (collided) {
        y -= fontSize * 1.4;
        attempts += 1;
      }
    }
    placedLabels.push({ x: o.x, y, halfWidth });
    return y;
  };

  return (
    <>
      {visible && (
        <svg
          viewBox={`0 0 ${solution.width} ${solution.height}`}
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            width: '100%',
            height: '100%',
            zIndex: 2,
            pointerEvents: 'none',
          }}
          aria-label="Sky object overlay"
        >
          {encompassing.length > 0 && (
            <text
              x={fontSize}
              y={height - fontSize}
              fontSize={fontSize}
              fill="#aee8ff"
              stroke="rgba(0,0,0,0.8)"
              strokeWidth={fontSize / 10}
              paintOrder="stroke"
            >
              {`Field within: ${encompassing.map(labelText).join(' · ')}`}
            </text>
          )}
          {objects.map((o) => {
            const isStar = o.kind === 'star';
            const isTransient = o.kind === 'transient';
            const isComet = o.kind === 'comet';
            const isAsteroid = o.kind === 'asteroid';
            const movingColor = isComet ? '#7bffd0' : '#ffb36b';
            const label = labelText(o);
            const a = Math.max(o.semi_major_px, fontSize);
            const b = Math.max(o.semi_minor_px, fontSize);
            const y = labelY(o);
            return (
              <g key={`${o.name}-${o.x}-${o.y}`}>
                {isComet || isAsteroid ? (
                  // Moving bodies: a diamond plus a directional dash — the
                  // comet's anti-solar tail or the asteroid's motion trail
                  (() => {
                    const rad = ((o.angle_deg || 45) * Math.PI) / 180;
                    const [dx, dy] = [Math.cos(rad), Math.sin(rad)];
                    return (
                      <path
                        d={`M ${o.x} ${o.y - a} L ${o.x + a} ${o.y} L ${o.x} ${o.y + a} L ${o.x - a} ${o.y} Z M ${o.x + a * 1.3 * dx} ${o.y + a * 1.3 * dy} L ${o.x + a * 3.2 * dx} ${o.y + a * 3.2 * dy}`}
                        fill="none"
                        stroke={movingColor}
                        strokeWidth={stroke * 1.5}
                      />
                    );
                  })()
                ) : isTransient ? (
                  <path
                    d={`M ${o.x} ${o.y - a} L ${o.x + a} ${o.y} L ${o.x} ${o.y + a} L ${o.x - a} ${o.y} Z`}
                    fill="none"
                    stroke="#ff7be0"
                    strokeWidth={stroke * 1.5}
                  />
                ) : isStar ? (
                  <>
                    <line
                      x1={o.x - a}
                      y1={o.y}
                      x2={o.x - a / 3}
                      y2={o.y}
                      stroke="#ffd479"
                      strokeWidth={stroke}
                    />
                    <line
                      x1={o.x + a / 3}
                      y1={o.y}
                      x2={o.x + a}
                      y2={o.y}
                      stroke="#ffd479"
                      strokeWidth={stroke}
                    />
                  </>
                ) : (
                  <ellipse
                    cx={0}
                    cy={0}
                    rx={a}
                    ry={b}
                    transform={`translate(${o.x} ${o.y}) rotate(${o.angle_deg})`}
                    fill="none"
                    stroke="#5fd3ff"
                    strokeWidth={stroke}
                    opacity={0.85}
                  />
                )}
                <text
                  x={o.x}
                  y={y}
                  textAnchor="middle"
                  fontSize={fontSize}
                  fill={
                    isComet || isAsteroid
                      ? movingColor
                      : isTransient
                        ? '#ff7be0'
                        : isStar
                          ? '#ffd479'
                          : '#aee8ff'
                  }
                  stroke="rgba(0,0,0,0.8)"
                  strokeWidth={fontSize / 10}
                  paintOrder="stroke"
                >
                  {label}
                </text>
              </g>
            );
          })}
        </svg>
      )}
    </>
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
        />
      )}
    </div>
  );
}

interface CatalogMenuProps {
  solution: AstroSolution;
  hiddenGroups: string[];
  onHiddenGroupsChange: (groups: string[]) => void;
  /** Small pill styling for tight spots (post embeds) */
  compact?: boolean;
}

/** The per-catalog visibility dropdown, shared by the detail-page
 * controls and post embeds. */
export function CatalogMenu({
  solution,
  hiddenGroups,
  onHiddenGroupsChange,
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
        </span>
      )}
    </span>
  );
}
