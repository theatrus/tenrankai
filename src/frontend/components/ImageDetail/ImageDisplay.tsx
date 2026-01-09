import { useState, useEffect, useRef } from 'react';
import { ImageInfo } from '../../types/index.ts';
import { useDelayedLoading } from '../../hooks/useDelayedLoading.ts';

interface ImageDisplayProps {
  image: ImageInfo;
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

export function ImageDisplay({ image, canUseZoom = false, onImageClick }: ImageDisplayProps) {
  const [imageLoading, setImageLoading] = useState(false);
  const [imageError, setImageError] = useState(false);
  const [zoomState, setZoomState] = useState<ZoomState>({
    isZooming: false,
    x: 0,
    y: 0,
    imageX: 0,
    imageY: 0
  });
  
  const timeoutRef = useRef<number | null>(null);
  const loadedImageRef = useRef<string | null>(null);
  const isInitialMount = useRef(true);
  const imageRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  
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

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!canUseZoom) return;
    
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
      className={`image-container ${canUseZoom ? 'zoom-enabled' : ''}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseLeave}
    >
      <div
        ref={containerRef}
        className="image-inner"
        style={{ 
          aspectRatio: `${image.dimensions[0]} / ${image.dimensions[1]}`,
          position: 'relative',
          overflow: 'hidden'
        }}
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
            <div
              ref={imageRef}
              className="image-display"
            onClick={handleClick}
            style={{ 
              position: 'relative',
              width: '100%',
              height: '100%',
              cursor: canUseZoom ? 'zoom-in' : 'default',
              userSelect: 'none',
              WebkitUserSelect: 'none',
              MozUserSelect: 'none',
              msUserSelect: 'none',
              WebkitTouchCallout: 'none'
            }}
            onContextMenu={(e) => e.preventDefault()}
            onDragStart={(e) => e.preventDefault()}
            role="img"
            aria-label={image.name}
          >
            {/* Image display with retina support */}
            {!imageLoading && !imageError && (
              <div 
                className="image-bg"
                style={{
                  width: '100%',
                  height: '100%',
                  backgroundImage: `image-set(
                    url(${image.medium_url}) 1x,
                    url(${image.medium_url.replace('?size=medium', '?size=medium@2x')}) 2x
                  )`,
                  backgroundSize: 'contain',
                  backgroundPosition: 'center',
                  backgroundRepeat: 'no-repeat'
                }}
              />
            )}
            {/* Hidden img for loading detection */}
            <img 
              src={image.medium_url}
              srcSet={`${image.medium_url} 1x, ${image.medium_url.replace('?size=medium', '?size=medium@2x')} 2x`}
              alt=""
              style={{ display: 'none' }}
              onLoad={handleImageLoad}
              onError={handleImageError}
            />
            {/* Transparent overlay to prevent right-click */}
            <div 
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: '100%',
                zIndex: 1,
                backgroundColor: 'transparent'
              }}
              onContextMenu={(e) => e.preventDefault()}
              onDragStart={(e) => e.preventDefault()}
            />
          </div>
          
          {/* Zoom overlay */}
          {canUseZoom && zoomState.isZooming && image.medium_url && (
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
              <div
                style={{
                  position: 'absolute',
                  width: '100%',
                  height: '100%',
                  backgroundImage: `url(${image.medium_url})`,
                  backgroundSize: `${containerRef.current?.clientWidth ? containerRef.current.clientWidth * 1.8 : image.dimensions[0] * 1.8}px auto`,
                  backgroundPosition: `${zoomState.imageX}% ${zoomState.imageY}%`,
                  backgroundRepeat: 'no-repeat',
                  imageRendering: '-webkit-optimize-contrast'
                }}
              />
            </div>
          )}
        </>
      )}
      </div>
    </div>
  );
}