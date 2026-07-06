import { useEffect, useRef } from 'react';
import { NavigationImage } from '../types/index.ts';
import { withImageSize, withRetryFragment } from '../utils/imageUrls.ts';

function preloadImageWithRetry(url: string, attempt = 0) {
  const img = new Image();
  img.onerror = () => {
    if (attempt >= 30) {
      console.warn('[Preload] Failed to load:', url);
      return;
    }

    const delay = Math.min(750 * Math.max(1, attempt + 1), 5000);
    window.setTimeout(() => {
      preloadImageWithRetry(url, attempt + 1);
    }, delay);
  };
  img.onload = () => {
    console.log('[Preload] Loaded:', url);
  };
  img.src = withRetryFragment(url, attempt);
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
      const mediumUrl = withImageSize(nextImage.thumbnail_url, 'medium');
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
        if (shouldLoad2x) {
          imagesToPreload.push(withImageSize(mediumUrl, 'medium@2x'));
        }
      }
    }

    // Add previous image
    if (prevImage?.thumbnail_url) {
      const mediumUrl = withImageSize(prevImage.thumbnail_url, 'medium');
      if (!preloadedRef.current.has(mediumUrl)) {
        imagesToPreload.push(mediumUrl);
        if (shouldLoad2x) {
          imagesToPreload.push(withImageSize(mediumUrl, 'medium@2x'));
        }
      }
    }

    // Preload images
    if (imagesToPreload.length > 0) {
      console.log('[Preload] Loading adjacent images:', imagesToPreload);

      imagesToPreload.forEach((url) => {
        preloadImageWithRetry(url);
        preloadedRef.current.add(url);
      });
    }
  }, [prevImage?.thumbnail_url, nextImage?.thumbnail_url, shouldLoad2x]);
}
