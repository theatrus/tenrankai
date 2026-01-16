import { useState, useEffect } from 'react';

/**
 * Hook that delays showing loading state for a specified duration
 * @param isLoading - The actual loading state
 * @param delay - Delay in milliseconds before showing loading (default 500ms)
 * @returns Whether to show the loading UI
 */
export function useDelayedLoading(isLoading: boolean, delay: number = 500): boolean {
  const [showLoading, setShowLoading] = useState(false);

  useEffect(() => {
    let timer: number | undefined;

    if (isLoading) {
      // Start a timer to show loading after delay
      timer = setTimeout(() => {
        setShowLoading(true);
      }, delay);
    } else {
      // Clear loading state immediately when done
      setShowLoading(false);
    }

    // Cleanup timer on unmount or when loading changes
    return () => {
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [isLoading, delay]);

  return showLoading;
}