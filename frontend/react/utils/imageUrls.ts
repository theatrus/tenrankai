export function withImageSize(url: string, size: string): string {
  const { pathAndQuery, hash } = splitUrl(url);
  const queryIndex = pathAndQuery.indexOf('?');
  if (queryIndex >= 0) {
    const path = pathAndQuery.slice(0, queryIndex);
    const query = pathAndQuery.slice(queryIndex);
    if (/[?&]size=/.test(query)) {
      return `${path}${query.replace(/([?&]size=)[^&]*/, `$1${size}`)}${hash}`;
    }
    return `${path.replace(/\/[^/]*$/, `/${size}`)}${query}${hash}`;
  }

  return `${pathAndQuery.replace(/\/[^/]*$/, `/${size}`)}${hash}`;
}

export function imageSrcSet(url: string, retinaSize: string): string {
  return `${url} 1x, ${withImageSize(url, retinaSize)} 2x`;
}

export function buildTileUrl(imageUrl: string, x: number, y: number, retina = false): string {
  return withImageSize(imageUrl, `tile_${x}_${y}${retina ? '@2x' : ''}`);
}

export function withRetryFragment(url: string, attempt: number): string {
  if (attempt <= 0 || !url) {
    return url;
  }

  return `${url.split('#')[0]}#retry-${attempt}`;
}

export function srcSetWithRetryFragment(srcSet: string | undefined, attempt: number): string | undefined {
  if (!srcSet || attempt <= 0) {
    return srcSet;
  }

  return srcSet
    .split(',')
    .map((candidate) => {
      const parts = candidate.trim().split(/\s+/);
      if (parts.length === 0 || !parts[0]) {
        return candidate;
      }
      return [withRetryFragment(parts[0], attempt), ...parts.slice(1)].join(' ');
    })
    .join(', ');
}

function splitUrl(url: string): { pathAndQuery: string; hash: string } {
  const hashIndex = url.indexOf('#');
  const withoutHash = hashIndex >= 0 ? url.slice(0, hashIndex) : url;
  const hash = hashIndex >= 0 ? url.slice(hashIndex) : '';
  return { pathAndQuery: withoutHash, hash };
}
