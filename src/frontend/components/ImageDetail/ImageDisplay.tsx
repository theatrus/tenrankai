import { useState, useEffect, useRef } from 'react';
import { ImageInfo } from '../../types/index.ts';

interface ImageDisplayProps {
  image: ImageInfo;
  hasDownloadPermission: boolean;
  onImageClick?: () => void;
}

export function ImageDisplay({ image, hasDownloadPermission, onImageClick }: ImageDisplayProps) {
  const [imageLoading, setImageLoading] = useState(true);
  const [imageError, setImageError] = useState(false);
  const timeoutRef = useRef<number | null>(null);
  const loadedImageRef = useRef<string | null>(null);

  useEffect(() => {
    console.log('ImageDisplay: Image path changed, checking if need to reload', {
      path: image.path,
      medium_url: image.medium_url,
      previouslyLoaded: loadedImageRef.current,
      needsReload: loadedImageRef.current !== image.medium_url
    });
    
    if (!image.medium_url) {
      console.error('ImageDisplay: No medium_url provided for image');
      setImageError(true);
      setImageLoading(false);
      return;
    }
    
    // Only reset loading state if this is a different image
    if (loadedImageRef.current !== image.medium_url) {
      console.log('ImageDisplay: Different image, resetting loading state');
      setImageLoading(true);
      setImageError(false);
      loadedImageRef.current = null; // Clear the loaded image reference
      
      // Clear any existing timeout
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      
      // Set a timeout to detect stuck loading
      timeoutRef.current = setTimeout(() => {
        console.warn('ImageDisplay: Image taking too long to load, checking if still loading...');
        setImageLoading(false);
        setImageError(true);
      }, 10000); // 10 second timeout
    } else {
      console.log('ImageDisplay: Same image already loaded, skipping reset');
    }
    
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [image.path, image.medium_url]);

  const handleImageLoad = () => {
    console.log('ImageDisplay: Image loaded successfully, setting imageLoading to false');
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    loadedImageRef.current = image.medium_url; // Mark this image as loaded
    setImageLoading(false);
  };

  const handleImageError = (e) => {
    console.log('ImageDisplay: Image failed to load', e);
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

  console.log('ImageDisplay: Rendering with state', { imageLoading, imageError });

  return (
    <div className="image-container" 
         style={{ aspectRatio: `${image.dimensions[0]} / ${image.dimensions[1]}` }}>
      {imageLoading && (
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