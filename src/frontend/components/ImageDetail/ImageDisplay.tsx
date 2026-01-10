import { useState, useEffect, useRef, useCallback } from 'react';
import { ImageInfo, TileConfig } from '../../types/index.ts';
import { useDelayedLoading } from '../../hooks/useDelayedLoading.ts';

interface ImageDisplayProps {
  image: ImageInfo;
  canUseZoom?: boolean;
  onImageClick?: () => void;
  tileConfig?: TileConfig;
  galleryName: string;
}

interface ZoomState {
  isZooming: boolean;
  x: number;
  y: number;
  imageX: number;
  imageY: number;
  tileX?: number;
  tileY?: number;
}

// Helper function to calculate dimensions
const calculateImageDimensions = (imageDimensions: number[], windowWidth: number, windowHeight: number) => {
  const aspectRatio = imageDimensions[0] / imageDimensions[1];
  const maxWidth = windowWidth * 0.95;
  const maxHeight = windowHeight * 0.75 - 100;
  
  let width, height;
  
  if (maxWidth / maxHeight > aspectRatio) {
    // Height constrained
    height = maxHeight;
    width = height * aspectRatio;
  } else {
    // Width constrained  
    width = maxWidth;
    height = width / aspectRatio;
  }
  
  // On mobile, adjust constraints
  if (windowWidth <= 768) {
    width = windowWidth;
    height = width / aspectRatio;
    if (height > windowHeight * 0.6) {
      height = windowHeight * 0.6;
      width = height * aspectRatio;
    }
  }
  
  return { width, height };
};

export function ImageDisplay({ image, canUseZoom = false, onImageClick, tileConfig }: ImageDisplayProps) {
  const [imageLoading, setImageLoading] = useState(true);
  const [imageError, setImageError] = useState(false);
  
  // Calculate initial dimensions immediately to prevent flicker
  const [dimensions, setDimensions] = useState(() => 
    calculateImageDimensions(image.dimensions, window.innerWidth, window.innerHeight)
  );
  
  const [zoomState, setZoomState] = useState<ZoomState>({
    isZooming: false,
    x: 0,
    y: 0,
    imageX: 0,
    imageY: 0,
    tileX: 0,
    tileY: 0
  });
  
  const timeoutRef = useRef<number | null>(null);
  const loadedImageRef = useRef<string | null>(null);
  const isInitialMount = useRef(true);
  const imageRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const preloadedTilesRef = useRef<Set<string>>(new Set());
  const [loadingTiles, setLoadingTiles] = useState<Set<string>>(new Set());
  
  // Only show loading indicator after 500ms
  const showLoading = useDelayedLoading(imageLoading);

  // Calculate dimensions based on viewport and image aspect ratio
  const calculateDimensions = () => {
    const newDimensions = calculateImageDimensions(
      image.dimensions, 
      window.innerWidth, 
      window.innerHeight
    );
    setDimensions(newDimensions);
  };

  // Recalculate on resize
  useEffect(() => {
    calculateDimensions();
    
    const handleResize = () => {
      calculateDimensions();
    };
    
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [image.dimensions]);

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
    
    // Calculate initial tile if tiles are configured
    let tileX: number | undefined;
    let tileY: number | undefined;
    
    if (tileConfig) {
      // Debug: log all the values
      console.log('Mouse down debug:', {
        imagePercent: `${imageX}%, ${imageY}%`,
        imageDims: `${image.dimensions[0]}x${image.dimensions[1]}`,
        tiledDims: `${tileConfig.tiled_width}x${tileConfig.tiled_height}`,
        tileSize: tileConfig.tile_size,
        gridSize: `${tileConfig.grid_width}x${tileConfig.grid_height}`
      });
      
      // The percentage represents position in the DISPLAYED image
      // We need to map this to the tiled coordinate system
      
      // First, get position in original image coordinates
      const imgX = (imageX / 100) * image.dimensions[0];
      const imgY = (imageY / 100) * image.dimensions[1];
      
      // Then scale down to tiled coordinates
      const scaleX = tileConfig.tiled_width / image.dimensions[0];
      const scaleY = tileConfig.tiled_height / image.dimensions[1];
      
      const tiledX = imgX * scaleX;
      const tiledY = imgY * scaleY;
      
      // Calculate which tile we're in
      tileX = Math.floor(tiledX / tileConfig.tile_size);
      tileY = Math.floor(tiledY / tileConfig.tile_size);
      
      console.log('Tile calculation:', {
        imgPos: `${imgX}, ${imgY}`,
        scale: `${scaleX}, ${scaleY}`,
        tiledPos: `${tiledX}, ${tiledY}`,
        tile: `${tileX}, ${tileY}`
      });
      
      // Clamp to valid tile range
      tileX = Math.max(0, Math.min(tileX, tileConfig.grid_width - 1));
      tileY = Math.max(0, Math.min(tileY, tileConfig.grid_height - 1));
      
      // Preload surrounding tiles when starting zoom
      preloadSurroundingTiles(tileX, tileY);
    }
    
    setZoomState({
      isZooming: true,
      x,
      y,
      imageX,
      imageY,
      tileX,
      tileY
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
    
    // Calculate which tile we're over if tiles are configured
    let tileX: number | undefined;
    let tileY: number | undefined;
    
    if (tileConfig) {
      // The percentage represents position in the DISPLAYED image
      // We need to map this to the tiled coordinate system
      
      // First, get position in original image coordinates
      const imgX = (imageX / 100) * image.dimensions[0];
      const imgY = (imageY / 100) * image.dimensions[1];
      
      // Then scale down to tiled coordinates
      const scaleX = tileConfig.tiled_width / image.dimensions[0];
      const scaleY = tileConfig.tiled_height / image.dimensions[1];
      
      const tiledX = imgX * scaleX;
      const tiledY = imgY * scaleY;
      
      // Calculate which tile we're in
      tileX = Math.floor(tiledX / tileConfig.tile_size);
      tileY = Math.floor(tiledY / tileConfig.tile_size);
      
      // Clamp to valid tile range
      tileX = Math.max(0, Math.min(tileX, tileConfig.grid_width - 1));
      tileY = Math.max(0, Math.min(tileY, tileConfig.grid_height - 1));
      
      // Preload tiles as we move
      if ((tileX !== zoomState.tileX || tileY !== zoomState.tileY)) {
        preloadSurroundingTiles(tileX, tileY);
      }
    }
    
    setZoomState(prev => ({
      ...prev,
      x,
      y,
      imageX,
      imageY,
      tileX,
      tileY
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
    // Only handle clicks for custom actions
    if (onImageClick) {
      onImageClick();
    }
    // No default behavior - downloading should use the download buttons
  };
  
  // Clear preloaded tiles cache when image changes
  useEffect(() => {
    preloadedTilesRef.current.clear();
    setLoadingTiles(new Set());
  }, [image.path]);

  // Preload surrounding tiles
  const preloadSurroundingTiles = useCallback((centerX: number, centerY: number) => {
    if (!tileConfig) return;
    
    const imageId = getImageIdentifierFromUrl(image.medium_url);
    const tilesToPreload: string[] = [];
    
    // Preload a 3x3 grid around the current tile
    for (let dy = -1; dy <= 1; dy++) {
      for (let dx = -1; dx <= 1; dx++) {
        const tileX = centerX + dx;
        const tileY = centerY + dy;
        
        if (tileX >= 0 && tileX < tileConfig.grid_width && 
            tileY >= 0 && tileY < tileConfig.grid_height) {
          const tileUrl = `/gallery/image/${imageId}?size=tile_${tileX}_${tileY}`;
          
          // Only preload if we haven't already
          if (!preloadedTilesRef.current.has(tileUrl)) {
            tilesToPreload.push(tileUrl);
            preloadedTilesRef.current.add(tileUrl);
          }
        }
      }
    }
    
    // Preload tiles in the background (both 1x and 2x for retina)
    tilesToPreload.forEach(url => {
      // Mark tile as loading
      setLoadingTiles(prev => new Set(prev).add(url));
      
      const img = new Image();
      img.onload = () => {
        setLoadingTiles(prev => {
          const newSet = new Set(prev);
          newSet.delete(url);
          return newSet;
        });
      };
      img.onerror = () => {
        setLoadingTiles(prev => {
          const newSet = new Set(prev);
          newSet.delete(url);
          return newSet;
        });
      };
      img.src = url;
      
      // Also preload 2x version for retina displays
      if (window.devicePixelRatio > 1) {
        const url2x = url.replace(/\?size=tile_/, '?size=tile_') + '@2x';
        setLoadingTiles(prev => new Set(prev).add(url2x));
        
        const img2x = new Image();
        img2x.onload = () => {
          setLoadingTiles(prev => {
            const newSet = new Set(prev);
            newSet.delete(url2x);
            return newSet;
          });
        };
        img2x.onerror = () => {
          setLoadingTiles(prev => {
            const newSet = new Set(prev);
            newSet.delete(url2x);
            return newSet;
          });
        };
        img2x.src = url2x;
      }
    });
  }, [tileConfig, image.medium_url]);
  
  // Extract the image identifier from a URL
  const getImageIdentifierFromUrl = (url: string): string => {
    // Extract the part between /image/ and ?size=
    const match = url.match(/\/image\/([^?]+)\?size=/);
    return match ? match[1] : image.path;
  };
  
  // Render multiple tiles for the zoom overlay
  const renderZoomTiles = () => {
    if (!tileConfig || zoomState.tileX === undefined || zoomState.tileY === undefined) {
      return null;
    }
    
    const zoomScale = 1.0; // No additional zoom since tiles are already high-res
    
    // Map from display percentage to tiled coordinates
    const imgX = (zoomState.imageX / 100) * image.dimensions[0];
    const imgY = (zoomState.imageY / 100) * image.dimensions[1];
    
    const scaleX = tileConfig.tiled_width / image.dimensions[0];
    const scaleY = tileConfig.tiled_height / image.dimensions[1];
    
    const tiledX = imgX * scaleX;
    const tiledY = imgY * scaleY;
    
    // Calculate which tiles we need to render (could be up to 4 at corners)
    const tilesToRender: Array<{x: number, y: number}> = [];
    
    // Current tile position
    const currentTileX = Math.floor(tiledX / tileConfig.tile_size);
    const currentTileY = Math.floor(tiledY / tileConfig.tile_size);
    
    // Position within current tile
    const tileLocalX = tiledX - (currentTileX * tileConfig.tile_size);
    const tileLocalY = tiledY - (currentTileY * tileConfig.tile_size);
    
    // Determine if we need adjacent tiles based on zoom radius (150px)
    const zoomRadius = 150 / zoomScale; // Effective radius in tile pixels
    
    // Always add current tile
    tilesToRender.push({x: currentTileX, y: currentTileY});
    
    // Check if we need tile to the right
    if (tileLocalX + zoomRadius > tileConfig.tile_size && currentTileX + 1 < tileConfig.grid_width) {
      tilesToRender.push({x: currentTileX + 1, y: currentTileY});
    }
    
    // Check if we need tile to the left
    if (tileLocalX - zoomRadius < 0 && currentTileX > 0) {
      tilesToRender.push({x: currentTileX - 1, y: currentTileY});
    }
    
    // Check if we need tile below
    if (tileLocalY + zoomRadius > tileConfig.tile_size && currentTileY + 1 < tileConfig.grid_height) {
      tilesToRender.push({x: currentTileX, y: currentTileY + 1});
    }
    
    // Check if we need tile above
    if (tileLocalY - zoomRadius < 0 && currentTileY > 0) {
      tilesToRender.push({x: currentTileX, y: currentTileY - 1});
    }
    
    // Check corners
    if (tileLocalX + zoomRadius > tileConfig.tile_size && tileLocalY + zoomRadius > tileConfig.tile_size &&
        currentTileX + 1 < tileConfig.grid_width && currentTileY + 1 < tileConfig.grid_height) {
      tilesToRender.push({x: currentTileX + 1, y: currentTileY + 1});
    }
    
    if (tileLocalX - zoomRadius < 0 && tileLocalY + zoomRadius > tileConfig.tile_size &&
        currentTileX > 0 && currentTileY + 1 < tileConfig.grid_height) {
      tilesToRender.push({x: currentTileX - 1, y: currentTileY + 1});
    }
    
    if (tileLocalX + zoomRadius > tileConfig.tile_size && tileLocalY - zoomRadius < 0 &&
        currentTileX + 1 < tileConfig.grid_width && currentTileY > 0) {
      tilesToRender.push({x: currentTileX + 1, y: currentTileY - 1});
    }
    
    if (tileLocalX - zoomRadius < 0 && tileLocalY - zoomRadius < 0 &&
        currentTileX > 0 && currentTileY > 0) {
      tilesToRender.push({x: currentTileX - 1, y: currentTileY - 1});
    }
    
    const imageId = getImageIdentifierFromUrl(image.medium_url);
    
    // Calculate offset for the entire tile grid
    const offsetX = -(tiledX * zoomScale - 150);
    const offsetY = -(tiledY * zoomScale - 150);
    
    return (
      <>
        {tilesToRender.map(tile => {
          // Create image-set for retina support
          const tileUrl = `/gallery/image/${imageId}?size=tile_${tile.x}_${tile.y}`;
          const tileUrl2x = `/gallery/image/${imageId}?size=tile_${tile.x}_${tile.y}@2x`;
          const imageSetValue = `image-set(url("${tileUrl}") 1x, url("${tileUrl2x}") 2x)`;
          
          const tileStyle: React.CSSProperties = {
            position: 'absolute',
            left: `${offsetX + tile.x * tileConfig.tile_size * zoomScale}px`,
            top: `${offsetY + tile.y * tileConfig.tile_size * zoomScale}px`,
            width: `${tileConfig.tile_size * zoomScale}px`,
            height: `${tileConfig.tile_size * zoomScale}px`,
            backgroundSize: '100% 100%',
            backgroundPosition: 'center',
            backgroundRepeat: 'no-repeat',
            imageRendering: 'auto'
          };
          
          // Set both standard and WebKit prefixed versions
          tileStyle.backgroundImage = imageSetValue;
          (tileStyle as any).WebkitBackgroundImage = imageSetValue;
          
          const tileKey = `tile_${tile.x}_${tile.y}`;
          
          return (
            <div
              key={tileKey}
              style={tileStyle}
            />
          );
        })}
      </>
    );
  };


  return (
    <div className="image-container-outer">
      <div 
        ref={containerRef}
        className={`image-container ${canUseZoom ? 'zoom-enabled' : ''}`}
        style={{ 
          width: dimensions.width > 0 ? `${dimensions.width}px` : undefined,
          height: dimensions.height > 0 ? `${dimensions.height}px` : undefined,
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
            {/* Image display with retina support - using img element for HDR compatibility */}
            <img 
              src={image.medium_url}
              srcSet={`${image.medium_url} 1x, ${image.medium_url.replace('?size=medium', '?size=medium@2x')} 2x`}
              alt={image.name}
              style={{ 
                width: '100%',
                height: '100%',
                objectFit: 'contain',
                display: imageLoading || imageError ? 'none' : 'block'
              }}
              onLoad={handleImageLoad}
              onError={handleImageError}
              key={image.path} // Force re-render on image change
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
          {canUseZoom && zoomState.isZooming && (
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
              {tileConfig ? (
                <div
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    height: '100%',
                    backgroundColor: 'rgba(0, 0, 0, 0.05)', // Very subtle background
                    overflow: 'hidden'
                  }}
                >
                  {/* Show underlying image with blur while tiles are loading */}
                  <div
                    style={{
                      position: 'absolute',
                      width: '100%',
                      height: '100%',
                      backgroundImage: `url(${image.medium_url})`,
                      backgroundSize: `${containerRef.current?.clientWidth ? containerRef.current.clientWidth * 1.8 : image.dimensions[0] * 1.8}px auto`,
                      backgroundPosition: `${zoomState.imageX}% ${zoomState.imageY}%`,
                      backgroundRepeat: 'no-repeat',
                      imageRendering: 'auto',
                      filter: loadingTiles.size > 0 ? 'blur(4px)' : 'none',
                      transition: 'filter 0.3s ease-out'
                    }}
                  />
                  
                  {/* Loading spinner overlay */}
                  {loadingTiles.size > 0 && (
                    <div
                      style={{
                        position: 'absolute',
                        top: '50%',
                        left: '50%',
                        transform: 'translate(-50%, -50%)',
                        zIndex: 20,
                        width: '40px',
                        height: '40px'
                      }}
                    >
                      <div
                        style={{
                          width: '40px',
                          height: '40px',
                          border: '3px solid rgba(255, 255, 255, 0.3)',
                          borderTop: '3px solid rgba(255, 255, 255, 0.9)',
                          borderRadius: '50%',
                          animation: 'spinOnly 0.8s linear infinite',
                          position: 'absolute',
                          top: 0,
                          left: 0
                        }}
                      />
                    </div>
                  )}
                  
                  {/* Render tiles on top */}
                  <div style={{ position: 'relative', zIndex: 10 }}>
                    {renderZoomTiles()}
                  </div>
                </div>
              ) : (
                <div
                  style={{
                    position: 'absolute',
                    width: '100%',
                    height: '100%',
                    backgroundImage: `url(${image.medium_url})`,
                    backgroundSize: `${containerRef.current?.clientWidth ? containerRef.current.clientWidth * 1.8 : image.dimensions[0] * 1.8}px auto`,
                    backgroundPosition: `${zoomState.imageX}% ${zoomState.imageY}%`,
                    backgroundRepeat: 'no-repeat',
                    imageRendering: 'auto'
                  }}
                />
              )}
            </div>
          )}
        </>
      )}
      </div>
    </div>
  );
}