import { useEffect, useRef } from 'react';
import { NavigationImage } from '../types/index.ts';

/**
 * Convert a thumbnail URL to a medium URL.
 * URL format: {url_prefix}/_image/{url_id}/thumbnail -> {url_prefix}/_image/{url_id}/medium
 */
function thumbnailToMediumUrl(thumbnailUrl: string): string {
  return thumbnailUrl.replace(/\/thumbnail$/, '/medium');
}

/**
 * Get the @2x version of a medium URL.
 */
function getMedium2xUrl(mediumUrl: string): string {
  return mediumUrl.replace(/\/medium$/, '/medium@2x');
}

/**
 * Preload images for smoother navigation.
 * Takes navigation images (most likely next first) and preloads their
 * medium-sized versions in the background, with @2x on retina displays.
 */
export function useImagePreload(images: (NavigationImage | undefined)[]) {
  // Track which images we've already preloaded to avoid duplicate requests
  const preloadedRef = useRef<Set<string>>(new Set());

  // Check if we should load @2x images
  const shouldLoad2x = typeof window !== 'undefined' && window.devicePixelRatio > 1;
  const key = images.map((img) => img?.thumbnail_url ?? '').join('|');

  useEffect(() => {
    for (const image of images) {
      if (!image?.thumbnail_url) continue;
      const mediumUrl = thumbnailToMediumUrl(image.thumbnail_url);
      const urls = shouldLoad2x ? [mediumUrl, getMedium2xUrl(mediumUrl)] : [mediumUrl];
      for (const url of urls) {
        if (preloadedRef.current.has(url)) continue;
        preloadedRef.current.add(url);
        const img = new Image();
        img.src = url;
      }
    }
  }, [key, shouldLoad2x]);
}
