// Parsers for the free-form RA/Dec strings stored in image metadata.

function extractNumbers(value: string): number[] {
  const matches = value.replace(/−/g, '-').match(/-?\d+(?:\.\d+)?/g);
  return matches ? matches.map(Number) : [];
}

/**
 * Parse a right ascension string to degrees in [0, 360).
 *
 * Hour-based forms ("00h 42m 44s", "0h42m", "5:34:32") convert at 15°/h;
 * anything else ("83.82", "83° 49′") is read as degrees.
 */
export function parseRa(value: string): number | null {
  const v = value.trim();
  if (!v) return null;

  const parts = extractNumbers(v);
  if (parts.length === 0 || parts.some((n) => !Number.isFinite(n)) || parts[0] < 0) {
    return null;
  }

  const [first, minutes = 0, seconds = 0] = parts;
  const hourly = /h/i.test(v) || v.includes(':');
  let degrees = hourly
    ? (first + minutes / 60 + seconds / 3600) * 15
    : first + minutes / 60 + seconds / 3600;

  degrees %= 360;
  if (degrees < 0) degrees += 360;
  return degrees;
}

/**
 * Parse a declination string to degrees in [-90, 90].
 *
 * Accepts sexagesimal forms ("+41° 16′ 09″", "-05d 23m 28s", "-5:23:28")
 * and plain degrees ("41.269").
 */
export function parseDec(value: string): number | null {
  const v = value.trim();
  if (!v) return null;

  const parts = extractNumbers(v);
  if (parts.length === 0 || parts.some((n) => !Number.isFinite(n))) {
    return null;
  }

  const negative = /^[-−]/.test(v);
  const [d, m = 0, s = 0] = parts.map(Math.abs);
  const degrees = (negative ? -1 : 1) * (d + m / 60 + s / 3600);
  return degrees >= -90 && degrees <= 90 ? degrees : null;
}

/** Format decimal degrees for display and external viewer links. */
export function formatDegrees(value: number, digits = 4): string {
  return value.toFixed(digits).replace(/\.?0+$/, '');
}
