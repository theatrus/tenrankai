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
 * Takes navigation images and preloads their medium-sized versions in the background.
 * Automatically handles @2x versions for retina displays.
 */
export function useImagePreload(
  prevImage?: NavigationImage,
  nextImage?: NavigationImage
) {
  // Track which images we've already preloaded to avoid duplicate requests
  const preloadedRef = useRef<Set<string>>(new Set());

  // Check if we should load @2x images
  const shouldLoad2x = typeof window !== 'undefined' && window.devicePixelRatio > 1;

  useEffect(() => {
    const imagesToPreload: string[] = [];

    // Add next image first (more likely to be navigated to)
    if (nextImage?.thumbnail_url) {
      const mediumUrl = thumbnailToMediumUrl(nextImage.thumbnail_url);
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
        if (shouldLoad2x) {
          imagesToPreload.push(getMedium2xUrl(mediumUrl));
        }
      }
    }

    // Add previous image
    if (prevImage?.thumbnail_url) {
      const mediumUrl = thumbnailToMediumUrl(prevImage.thumbnail_url);
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
        if (shouldLoad2x) {
          imagesToPreload.push(getMedium2xUrl(mediumUrl));
        }
      }
    }

    // Preload images
    if (imagesToPreload.length > 0) {
      console.log('[Preload] Loading adjacent images:', imagesToPreload);

      imagesToPreload.forEach(url => {
        const img = new Image();
        img.onload = () => {
          console.log('[Preload] Loaded:', url);
        };
        img.onerror = () => {
          console.warn('[Preload] Failed to load:', url);
        };
        img.src = url;
        preloadedRef.current.add(url);
      });
    }
  }, [prevImage?.thumbnail_url, nextImage?.thumbnail_url, shouldLoad2x]);
}
