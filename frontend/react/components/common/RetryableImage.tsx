import React, { useEffect, useRef, useState } from 'react';
import { srcSetWithRetryFragment, withRetryFragment } from '../../utils/imageUrls.ts';

interface RetryableImageProps extends React.ImgHTMLAttributes<HTMLImageElement> {
  retryDelayMs?: number;
  maxRetries?: number;
}

export function RetryableImage({
  src,
  srcSet,
  retryDelayMs = 750,
  maxRetries = 30,
  onError,
  onLoad,
  ...props
}: RetryableImageProps) {
  const [attempt, setAttempt] = useState(0);
  const timerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    setAttempt(0);
    if (timerRef.current !== undefined) {
      window.clearTimeout(timerRef.current);
      timerRef.current = undefined;
    }
  }, [src, srcSet]);

  useEffect(() => {
    return () => {
      if (timerRef.current !== undefined) {
        window.clearTimeout(timerRef.current);
      }
    };
  }, []);

  const retrySrc = src ? withRetryFragment(src, attempt) : src;
  const retrySrcSet = srcSetWithRetryFragment(srcSet, attempt);

  return (
    <img
      {...props}
      src={retrySrc}
      srcSet={retrySrcSet}
      data-retry-attempt={attempt}
      onLoad={(event) => {
        if (timerRef.current !== undefined) {
          window.clearTimeout(timerRef.current);
          timerRef.current = undefined;
        }
        onLoad?.(event);
      }}
      onError={(event) => {
        if (attempt < maxRetries) {
          const delay = Math.min(retryDelayMs * Math.max(1, attempt + 1), 5000);
          timerRef.current = window.setTimeout(() => {
            setAttempt((current) => current + 1);
          }, delay);
          return;
        }

        onError?.(event);
      }}
    />
  );
}
