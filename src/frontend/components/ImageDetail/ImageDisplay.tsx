import { useState, useEffect, useRef } from 'react';
import { ImageInfo } from '../../types/index.ts';
import { useDelayedLoading } from '../../hooks/useDelayedLoading.ts';

interface ImageDisplayProps {
  image: ImageInfo;
  hasDownloadPermission: boolean;
  canUseZoom?: boolean;
  onImageClick?: () => void;
}

interface ZoomState {
  isZooming: boolean;
  x: number;
  y: number;
  imageX: number;
  imageY: number;
}

export function ImageDisplay({ image, hasDownloadPermission, canUseZoom = false, onImageClick }: ImageDisplayProps) {
  const [imageLoading, setImageLoading] = useState(false);
  const [imageError, setImageError] = useState(false);
  const [zoomState, setZoomState] = useState<ZoomState>({
    isZooming: false,
    x: 0,
    y: 0,
    imageX: 0,
    imageY: 0
  });
  const [largeImageSrc, setLargeImageSrc] = useState<string | null>(null);
  
  const timeoutRef = useRef<number | null>(null);
  const loadedImageRef = useRef<string | null>(null);
  const isInitialMount = useRef(true);
  const imageRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  
  // Only show loading indicator after 500ms
  const showLoading = useDelayedLoading(imageLoading);

  // Preload large image when zoom is available
  useEffect(() => {
    if (canUseZoom && image.large_url) {
      const img = new Image();
      img.src = image.large_url;
      img.onload = () => setLargeImageSrc(image.large_url || null);
    }
  }, [canUseZoom, image.large_url]);

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

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!canUseZoom || !largeImageSrc) return;
    
    e.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const imageX = (x / rect.width) * 100;
    const imageY = (y / rect.height) * 100;
    
    setZoomState({
      isZooming: true,
      x,
      y,
      imageX,
      imageY
    });
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!zoomState.isZooming || !canUseZoom) return;
    
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const imageX = (x / rect.width) * 100;
    const imageY = (y / rect.height) * 100;
    
    setZoomState(prev => ({
      ...prev,
      x,
      y,
      imageX,
      imageY
    }));
  };

  const handleMouseUp = () => {
    setZoomState(prev => ({
      ...prev,
      isZooming: false
    }));
  };

  const handleMouseLeave = () => {
    setZoomState(prev => ({
      ...prev,
      isZooming: false
    }));
  };

  const handleClick = () => {
    // Only handle clicks for custom actions, never for downloading
    if (onImageClick) {
      onImageClick();
    }
    // No default behavior - downloading should use the download buttons
  };

  return (
    <div 
      ref={containerRef}
      className={`image-container ${canUseZoom ? 'zoom-enabled' : ''}`}
      style={{ 
        aspectRatio: `${image.dimensions[0]} / ${image.dimensions[1]}`,
        position: 'relative',
        overflow: 'hidden'
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseLeave}
    >
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
        <>
          <img 
            ref={imageRef}
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
              cursor: canUseZoom ? 'zoom-in' : 'default',
              display: imageLoading ? 'none' : 'block',
              userSelect: 'none'
            }}
          />
          
          {/* Zoom overlay */}
          {canUseZoom && zoomState.isZooming && largeImageSrc && (
            <div 
              className="zoom-overlay"
              style={{
                position: 'absolute',
                left: `${zoomState.x}px`,
                top: `${zoomState.y}px`,
                transform: 'translate(-50%, -50%)',
                width: '300px',
                height: '300px',
                borderRadius: '50%',
                overflow: 'hidden',
                border: '2px solid rgba(255, 255, 255, 0.8)',
                boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
                pointerEvents: 'none',
                zIndex: 10
              }}
            >
              <img
                src={largeImageSrc}
                alt=""
                style={{
                  position: 'absolute',
                  width: `${image.dimensions[0] * 2}px`,
                  height: `${image.dimensions[1] * 2}px`,
                  objectFit: 'contain',
                  transform: `translate(-${zoomState.imageX}%, -${zoomState.imageY}%)`,
                  maxWidth: 'none'
                }}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}