import { ImageInfo, RolePermissions } from '../../types/index.ts';
import { parseRa, parseDec, formatDegrees } from '../../utils/astro-coords.ts';
import { BRIGHT_STARS } from './bright-stars.ts';

// Hammer projection of the full sky, centered on RA 12h / Dec 0°, with
// right ascension increasing to the left (the usual sky-chart convention).
const SQRT2 = Math.SQRT2;

function project(raDeg: number, decDeg: number): { x: number; y: number } {
  let lambda = (180 - raDeg) % 360;
  if (lambda > 180) lambda -= 360;
  if (lambda < -180) lambda += 360;

  const halfLambda = (lambda * Math.PI) / 360;
  const phi = (decDeg * Math.PI) / 180;
  const z = Math.sqrt(1 + Math.cos(phi) * Math.cos(halfLambda));
  return {
    x: (2 * SQRT2 * Math.cos(phi) * Math.sin(halfLambda)) / z,
    y: (SQRT2 * Math.sin(phi)) / z,
  };
}

const WIDTH = 640;
const HEIGHT = 340;
const CX = WIDTH / 2;
const CY = HEIGHT / 2;
const SCALE = (WIDTH / 2 - 16) / (2 * SQRT2);

function toSvg(raDeg: number, decDeg: number): { x: number; y: number } {
  const p = project(raDeg, decDeg);
  return { x: CX + p.x * SCALE, y: CY - p.y * SCALE };
}

function polyline(points: { x: number; y: number }[]): string {
  return points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
}

function meridian(raDeg: number): string {
  const points = [];
  for (let dec = -90; dec <= 90; dec += 5) {
    points.push(toSvg(raDeg, dec));
  }
  return polyline(points);
}

function parallel(decDeg: number): string {
  // Sample in projection longitude so the line doesn't jump across the seam
  const points = [];
  for (let lambda = -180; lambda <= 180; lambda += 5) {
    points.push(toSvg(180 - lambda, decDeg));
  }
  return polyline(points);
}

const MERIDIANS = [0, 45, 90, 135, 180, 225, 270, 315];
const PARALLELS = [-60, -30, 30, 60];

export function AstroSkyMap({
  image,
  permissions,
}: {
  image: ImageInfo;
  permissions: RolePermissions;
}) {
  const camera = image.camera_info;
  if (!camera?.ra || !camera?.dec || !permissions.can_see_technical_details) {
    return null;
  }

  const ra = parseRa(camera.ra);
  const dec = parseDec(camera.dec);
  if (ra === null || dec === null) {
    return null;
  }

  const target = toSvg(ra, dec);
  const coords = `${formatDegrees(ra)} ${dec >= 0 ? '+' : ''}${formatDegrees(dec)}`;
  const aladinUrl = `https://aladin.cds.unistra.fr/AladinLite/?target=${encodeURIComponent(coords)}&fov=3`;
  const simbadUrl = `https://simbad.cds.unistra.fr/simbad/sim-coo?Coord=${encodeURIComponent(coords)}&Radius=30&Radius.unit=arcmin`;

  return (
    <div className="sky-map card">
      <h3>Sky Position</h3>
      <div className="sky-map-coordinates">
        <span className="coord-label">RA / Dec:</span>
        <span className="coordinates-text">
          {camera.ra} / {camera.dec}
        </span>
      </div>

      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        className="sky-map-chart"
        role="img"
        aria-label={`Sky map marking ${camera.ra} ${camera.dec}`}
      >
        <ellipse
          className="sky-map-sphere"
          cx={CX}
          cy={CY}
          rx={2 * SQRT2 * SCALE}
          ry={SQRT2 * SCALE}
        />
        {MERIDIANS.map((m) => (
          <polyline key={`m${m}`} className="sky-map-grid" points={meridian(m)} />
        ))}
        {PARALLELS.map((p) => (
          <polyline key={`p${p}`} className="sky-map-grid" points={parallel(p)} />
        ))}
        <polyline className="sky-map-grid sky-map-equator" points={parallel(0)} />

        {[0, 6, 12, 18].map((hour) => {
          const pos = toSvg(hour * 15, 0);
          return (
            <text key={`h${hour}`} className="sky-map-label" x={pos.x + 3} y={pos.y - 3}>
              {hour}h
            </text>
          );
        })}

        {BRIGHT_STARS.map(([starRa, starDec, mag], index) => {
          const pos = toSvg(starRa, starDec);
          return (
            <circle
              key={index}
              className="sky-map-star"
              cx={pos.x}
              cy={pos.y}
              r={Math.max(0.5, 2.4 - 0.5 * mag)}
            />
          );
        })}

        <circle className="sky-map-target-glow" cx={target.x} cy={target.y} r={11} />
        <circle className="sky-map-target" cx={target.x} cy={target.y} r={6} />
        <line
          className="sky-map-target"
          x1={target.x - 12}
          y1={target.y}
          x2={target.x + 12}
          y2={target.y}
        />
        <line
          className="sky-map-target"
          x1={target.x}
          y1={target.y - 12}
          x2={target.x}
          y2={target.y + 12}
        />
      </svg>

      <div className="map-links">
        <a href={aladinUrl} target="_blank" rel="noopener noreferrer" className="map-link">
          Aladin Lite
        </a>
        <a href={simbadUrl} target="_blank" rel="noopener noreferrer" className="map-link">
          SIMBAD
        </a>
      </div>
    </div>
  );
}
