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

export function AstroOverlay({ solution }: { solution: AstroSolution }) {
  const [visible, setVisible] = useState(false);

  const width = solution.width;
  const height = solution.height;
  const all = solution.objects || [];
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
      <button
        type="button"
        className="astro-overlay-toggle"
        onClick={(e) => {
          e.stopPropagation();
          setVisible(!visible);
        }}
        style={{
          position: 'absolute',
          top: '0.75rem',
          right: '0.75rem',
          zIndex: 3,
          pointerEvents: 'auto',
          padding: '0.35rem 0.8rem',
          borderRadius: '999px',
          border: '1px solid rgba(255,255,255,0.4)',
          background: visible ? 'rgba(80,180,255,0.35)' : 'rgba(0,0,0,0.45)',
          color: '#fff',
          fontSize: '0.85rem',
          cursor: 'pointer',
        }}
        title={`${all.length} objects — solved at ${solution.scale_arcsec_px?.toFixed(2)}″/px`}
      >
        {visible ? 'Objects ✕' : `Objects (${all.length})`}
      </button>

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
            const label = labelText(o);
            const a = Math.max(o.semi_major_px, fontSize);
            const b = Math.max(o.semi_minor_px, fontSize);
            const y = labelY(o);
            return (
              <g key={`${o.name}-${o.x}-${o.y}`}>
                {isStar ? (
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
                  fill={isStar ? '#ffd479' : '#aee8ff'}
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
