import { useState, useEffect, useRef } from 'react';
import { ImageInfo } from '../../types/index.ts';
import { useDelayedLoading } from '../../hooks/useDelayedLoading.ts';

interface ImageDisplayProps {
  image: ImageInfo;
  hasDownloadPermission: boolean;
  onImageClick?: () => void;
}

export function ImageDisplay({ image, hasDownloadPermission, onImageClick }: ImageDisplayProps) {
  const [imageLoading, setImageLoading] = useState(false);
  const [imageError, setImageError] = useState(false);
  const timeoutRef = useRef<number | null>(null);
  const loadedImageRef = useRef<string | null>(null);
  const isInitialMount = useRef(true);
  
  // Only show loading indicator after 500ms
  const showLoading = useDelayedLoading(imageLoading);

  useEffect(() => {
    if (!image.medium_url) {
      setImageError(true);
      setImageLoading(false);
      return;
    }
    
    // Only reset loading state if this is a different image
    if (loadedImageRef.current !== image.medium_url) {
      // Don't show loading on initial mount (image might already be cached)
      if (!isInitialMount.current) {
        setImageLoading(true);
      }
      setImageError(false);
      loadedImageRef.current = null; // Clear the loaded image reference
      
      // Clear any existing timeout
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      
      // Set a timeout to detect stuck loading
      timeoutRef.current = setTimeout(() => {
        setImageLoading(false);
        setImageError(true);
      }, 10000); // 10 second timeout
    }
    
    // After first mount, treat as navigation
    if (isInitialMount.current) {
      isInitialMount.current = false;
    }
    
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [image.path, image.medium_url]);

  const handleImageLoad = () => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    loadedImageRef.current = image.medium_url; // Mark this image as loaded
    setImageLoading(false);
  };

  const handleImageError = () => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    setImageLoading(false);
    setImageError(true);
  };

  const handleClick = () => {
    if (onImageClick) {
      onImageClick();
    } else {
      // Default behavior: open image in new tab
      const fullSizeUrl = hasDownloadPermission 
        ? image.medium_url.replace('?size=medium', '') 
        : image.medium_url;
      window.open(fullSizeUrl, '_blank');
    }
  };

  return (
    <div className="image-container" 
         style={{ aspectRatio: `${image.dimensions[0]} / ${image.dimensions[1]}` }}>
      {showLoading && (
        <div className="image-loading">
          <div className="loading-spinner">Loading...</div>
        </div>
      )}
      
      {imageError ? (
        <div className="image-error">
          <p>Failed to load image</p>
          <button onClick={() => window.location.reload()}>Retry</button>
        </div>
      ) : (
        <img 
          src={image.medium_url}
          srcSet={`${image.medium_url} 1x, ${image.medium_url.replace('?size=medium', '?size=medium@2x')} 2x`}
          alt={image.name}
          width={image.dimensions[0]}
          height={image.dimensions[1]}
          loading="eager"
          onLoadStart={() => {
            // Start loading when the browser actually begins loading
            if (!isInitialMount.current && loadedImageRef.current !== image.medium_url) {
              setImageLoading(true);
            }
          }}
          onLoad={handleImageLoad}
          onError={handleImageError}
          onClick={handleClick}
          style={{ 
            cursor: 'pointer',
            display: imageLoading ? 'none' : 'block'
          }}
        />
      )}
    </div>
  );
}