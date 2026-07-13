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
}

/** The SVG marker layer. Toggle buttons live in [`AstroControls`]. */
export function AstroOverlay({ solution, visible, allTransients }: AstroOverlayProps) {

  const width = solution.width;
  const height = solution.height;
  // Transients not discovered near the capture date are noise by default
  // (M31 accumulates hundreds of historical novae) but can be toggled on
  const everything = solution.objects || [];
  const distant = distantTransients(solution);
  const all = allTransients ? everything : everything.filter((o) => !distant.includes(o));
  const encompassing = all.filter((o) => encompassesFrame(o, width, height));
  const objects = all.filter((o) => !encompassing.includes(o));
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
                  // Moving bodies: a diamond plus a short trailing dash,
                  // in comet green or asteroid orange
                  <path
                    d={`M ${o.x} ${o.y - a} L ${o.x + a} ${o.y} L ${o.x} ${o.y + a} L ${o.x - a} ${o.y} Z M ${o.x + a * 1.3} ${o.y + a * 1.3} L ${o.x + a * 2.1} ${o.y + a * 2.1}`}
                    fill="none"
                    stroke={movingColor}
                    strokeWidth={stroke * 1.5}
                  />
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
}: AstroControlsProps) {
  const total = (solution.objects || []).length;
  const distant = distantTransients(solution);
  const shown = allTransients ? total : total - distant.length;

  return (
    <div className="control-buttons astro-controls">
      <button
        type="button"
        className={`btn ${visible ? 'btn-primary' : 'btn-secondary'}`}
        onClick={() => onVisibleChange(!visible)}
        title={`${shown} objects — solved at ${solution.scale_arcsec_px?.toFixed(2)}″/px`}
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
    </div>
  );
}
