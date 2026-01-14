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
 * Preload images for smoother navigation.
 * Takes navigation images and preloads their medium-sized versions in the background.
 */
export function useImagePreload(
  prevImage?: NavigationImage,
  nextImage?: NavigationImage
) {
  // Track which images we've already preloaded to avoid duplicate requests
  const preloadedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const imagesToPreload: string[] = [];

    // Add next image first (more likely to be navigated to)
    if (nextImage?.thumbnail_url) {
      const mediumUrl = thumbnailToMediumUrl(nextImage.thumbnail_url);
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
      }
    }

    // Add previous image
    if (prevImage?.thumbnail_url) {
      const mediumUrl = thumbnailToMediumUrl(prevImage.thumbnail_url);
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
      }
    }

    // Preload images
    imagesToPreload.forEach(url => {
      const img = new Image();
      img.src = url;
      preloadedRef.current.add(url);
    });
  }, [prevImage?.thumbnail_url, nextImage?.thumbnail_url]);
}
