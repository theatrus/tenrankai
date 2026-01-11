import { useState, useEffect, useRef, useCallback } from 'react';
import { ImageInfo, TileConfig } from '../../types/index.ts';
import { useDelayedLoading } from '../../hooks/useDelayedLoading.ts';

interface ImageDisplayProps {
  image: ImageInfo;
  canUseZoom?: boolean;
  onImageClick?: () => void;
  tileConfig?: TileConfig;
  galleryName: string;
  onZoomStateChange?: (isZoomed: boolean) => void;
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

interface PinchZoomState {
  scale: number;
  translateX: number;
  translateY: number;
  isZoomed: boolean;
  isTransitioning: boolean;
}

// Check if device supports touch
const isTouchDevice = () => {
  return 'ontouchstart' in window || navigator.maxTouchPoints > 0;
};

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

export function ImageDisplay({ image, canUseZoom = false, onImageClick, tileConfig, onZoomStateChange }: ImageDisplayProps) {
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

  // Pinch zoom state for mobile
  const [pinchZoom, setPinchZoom] = useState<PinchZoomState>({
    scale: 1,
    translateX: 0,
    translateY: 0,
    isZoomed: false,
    isTransitioning: false
  });
  const [isMobile] = useState(() => isTouchDevice());
  const lastTouchDistance = useRef<number | null>(null);
  const lastTouchCenter = useRef<{ x: number; y: number } | null>(null);
  const initialPinchScale = useRef<number>(1);
  const currentScaleRef = useRef<number>(1);
  const initialTranslateRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const isPinchingRef = useRef<boolean>(false);

  // Calculate pan limits based on image size and scale
  const calculatePanLimits = (scale: number) => {
    const imgAspect = image.dimensions[0] / image.dimensions[1];
    const viewAspect = window.innerWidth / window.innerHeight;

    // Base image display size (fits within viewport)
    let baseImgWidth: number, baseImgHeight: number;
    if (imgAspect > viewAspect) {
      baseImgWidth = window.innerWidth;
      baseImgHeight = window.innerWidth / imgAspect;
    } else {
      baseImgHeight = window.innerHeight;
      baseImgWidth = window.innerHeight * imgAspect;
    }

    // Scaled image size
    const scaledWidth = baseImgWidth * scale;
    const scaledHeight = baseImgHeight * scale;

    // Pan limits: how far can we pan before image edge leaves viewport
    // If scaled image is smaller than viewport, no panning allowed (stays centered)
    // If scaled image is larger, limit is (scaledSize - viewportSize) / 2
    const maxPanX = Math.max(0, (scaledWidth - window.innerWidth) / 2);
    const maxPanY = Math.max(0, (scaledHeight - window.innerHeight) / 2);

    return { maxPanX, maxPanY };
  };
  
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

  // Touch event handlers for pinch-to-zoom on mobile
  const getTouchDistance = (touches: React.TouchList) => {
    if (touches.length < 2) return 0;
    const dx = touches[0].clientX - touches[1].clientX;
    const dy = touches[0].clientY - touches[1].clientY;
    return Math.sqrt(dx * dx + dy * dy);
  };

  const getTouchCenter = (touches: React.TouchList) => {
    if (touches.length < 2) {
      return { x: touches[0].clientX, y: touches[0].clientY };
    }
    return {
      x: (touches[0].clientX + touches[1].clientX) / 2,
      y: (touches[0].clientY + touches[1].clientY) / 2
    };
  };

  // Open the zoom modal
  const openZoomModal = (initialScale: number = 2, centerX?: number, centerY?: number) => {
    currentScaleRef.current = initialScale;

    // Calculate initial translate to center on tap point if provided
    let translateX = 0;
    let translateY = 0;
    if (centerX !== undefined && centerY !== undefined) {
      const viewCenterX = window.innerWidth / 2;
      const viewCenterY = window.innerHeight / 2;
      translateX = (viewCenterX - centerX) * (initialScale - 1) / initialScale;
      translateY = (viewCenterY - centerY) * (initialScale - 1) / initialScale;
    }

    setPinchZoom({
      scale: initialScale,
      translateX,
      translateY,
      isZoomed: true,
      isTransitioning: true
    });

    // End transition after animation
    setTimeout(() => {
      setPinchZoom(prev => ({ ...prev, isTransitioning: false }));
    }, 300);
  };

  // Close the zoom modal
  const closeZoomModal = () => {
    setPinchZoom(prev => ({ ...prev, isTransitioning: true }));

    setTimeout(() => {
      currentScaleRef.current = 1;
      setPinchZoom({
        scale: 1,
        translateX: 0,
        translateY: 0,
        isZoomed: false,
        isTransitioning: false
      });
    }, 300);
  };

  const handleTouchStart = (e: React.TouchEvent) => {
    if (!canUseZoom || !isMobile) return;

    if (e.touches.length === 2) {
      // Starting pinch gesture
      e.preventDefault();
      isPinchingRef.current = true;
      lastTouchDistance.current = getTouchDistance(e.touches);
      lastTouchCenter.current = getTouchCenter(e.touches);
      initialPinchScale.current = currentScaleRef.current;
      initialTranslateRef.current = { x: pinchZoom.translateX, y: pinchZoom.translateY };
    } else if (e.touches.length === 1 && pinchZoom.isZoomed) {
      // Single touch on zoomed image - prepare for panning
      lastTouchCenter.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
    }
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!canUseZoom || !isMobile) return;

    if (e.touches.length === 2 && lastTouchDistance.current !== null) {
      // Pinch gesture in progress
      e.preventDefault();

      const currentDistance = getTouchDistance(e.touches);
      const currentCenter = getTouchCenter(e.touches);

      // Calculate new scale relative to initial
      const scaleRatio = currentDistance / lastTouchDistance.current;
      let newScale = initialPinchScale.current * scaleRatio;

      // Clamp scale between 1 and 5
      newScale = Math.max(1, Math.min(5, newScale));
      currentScaleRef.current = newScale;

      // Calculate pan based on center movement
      let newTranslateX = initialTranslateRef.current.x;
      let newTranslateY = initialTranslateRef.current.y;

      if (lastTouchCenter.current) {
        const deltaX = currentCenter.x - lastTouchCenter.current.x;
        const deltaY = currentCenter.y - lastTouchCenter.current.y;
        newTranslateX += deltaX;
        newTranslateY += deltaY;
      }

      // Update translate ref for next frame
      initialTranslateRef.current = { x: newTranslateX, y: newTranslateY };
      lastTouchCenter.current = currentCenter;

      // Constrain pan to keep image within viewport
      const { maxPanX, maxPanY } = calculatePanLimits(newScale);
      newTranslateX = Math.max(-maxPanX, Math.min(maxPanX, newTranslateX));
      newTranslateY = Math.max(-maxPanY, Math.min(maxPanY, newTranslateY));

      const shouldBeZoomed = newScale > 1.05;

      setPinchZoom(prev => ({
        scale: newScale,
        translateX: newTranslateX,
        translateY: newTranslateY,
        isZoomed: shouldBeZoomed,
        isTransitioning: prev.isTransitioning
      }));

      // Preload tiles when zoomed enough
      if (shouldBeZoomed && tileConfig && newScale > 1.5) {
        const imgX = (currentCenter.x / window.innerWidth) * image.dimensions[0];
        const imgY = (currentCenter.y / window.innerHeight) * image.dimensions[1];
        const scaleX = tileConfig.tiled_width / image.dimensions[0];
        const scaleY = tileConfig.tiled_height / image.dimensions[1];
        const tileX = Math.floor((imgX * scaleX) / tileConfig.tile_size);
        const tileY = Math.floor((imgY * scaleY) / tileConfig.tile_size);
        preloadSurroundingTiles(
          Math.max(0, Math.min(tileX, tileConfig.grid_width - 1)),
          Math.max(0, Math.min(tileY, tileConfig.grid_height - 1))
        );
      }
    } else if (e.touches.length === 1 && pinchZoom.isZoomed && lastTouchCenter.current) {
      // Panning while zoomed
      e.preventDefault();

      const deltaX = e.touches[0].clientX - lastTouchCenter.current.x;
      const deltaY = e.touches[0].clientY - lastTouchCenter.current.y;

      let newTranslateX = pinchZoom.translateX + deltaX;
      let newTranslateY = pinchZoom.translateY + deltaY;

      // Constrain pan to keep image within viewport
      const { maxPanX, maxPanY } = calculatePanLimits(pinchZoom.scale);
      newTranslateX = Math.max(-maxPanX, Math.min(maxPanX, newTranslateX));
      newTranslateY = Math.max(-maxPanY, Math.min(maxPanY, newTranslateY));

      lastTouchCenter.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };

      setPinchZoom(prev => ({
        ...prev,
        translateX: newTranslateX,
        translateY: newTranslateY
      }));
    }
  };

  const handleTouchEnd = (e: React.TouchEvent) => {
    if (!canUseZoom || !isMobile) return;

    if (e.touches.length < 2) {
      isPinchingRef.current = false;
      lastTouchDistance.current = null;
    }

    if (e.touches.length === 0) {
      lastTouchCenter.current = null;

      // Reset zoom if scale is close to 1
      if (currentScaleRef.current < 1.1) {
        closeZoomModal();
      } else {
        // Keep zoomed, update refs for next gesture
        initialPinchScale.current = currentScaleRef.current;
        initialTranslateRef.current = { x: pinchZoom.translateX, y: pinchZoom.translateY };
      }
    } else if (e.touches.length === 1) {
      // Transitioning from pinch to pan
      lastTouchCenter.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
      initialPinchScale.current = currentScaleRef.current;
      initialTranslateRef.current = { x: pinchZoom.translateX, y: pinchZoom.translateY };
    }
  };

  // Double-tap to zoom/reset on mobile
  const lastTapTime = useRef<number>(0);
  const lastTapPos = useRef<{ x: number; y: number } | null>(null);

  const handleDoubleTap = (e: React.TouchEvent) => {
    if (!canUseZoom || !isMobile) return;
    if (isPinchingRef.current) return; // Ignore during pinch

    const now = Date.now();
    const tapX = e.changedTouches[0].clientX;
    const tapY = e.changedTouches[0].clientY;

    // Check if double tap (same location within 300ms)
    if (now - lastTapTime.current < 300 && lastTapPos.current) {
      const dx = Math.abs(tapX - lastTapPos.current.x);
      const dy = Math.abs(tapY - lastTapPos.current.y);

      if (dx < 50 && dy < 50) {
        // Double tap detected
        e.preventDefault();

        if (pinchZoom.isZoomed) {
          closeZoomModal();
        } else {
          openZoomModal(2.5, tapX, tapY);
        }

        lastTapTime.current = 0;
        lastTapPos.current = null;
        return;
      }
    }

    lastTapTime.current = now;
    lastTapPos.current = { x: tapX, y: tapY };
  };

  // Reset pinch zoom when image changes
  useEffect(() => {
    currentScaleRef.current = 1;
    initialPinchScale.current = 1;
    initialTranslateRef.current = { x: 0, y: 0 };
    setPinchZoom({
      scale: 1,
      translateX: 0,
      translateY: 0,
      isZoomed: false,
      isTransitioning: false
    });
  }, [image.path]);

  // Notify parent of zoom state changes
  useEffect(() => {
    onZoomStateChange?.(pinchZoom.isZoomed);
  }, [pinchZoom.isZoomed, onZoomStateChange]);

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
          const tileUrl = `/gallery/_image/${imageId}/tile_${tileX}_${tileY}`;
          
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
        const url2x = url.replace(/\/tile_/, '/tile_') + '@2x';
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
    // Extract the part between /_image/ and the size
    const match = url.match(/\/_image\/(.+)\/[^/]+$/);
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
          const tileUrl = `/gallery/_image/${imageId}/tile_${tile.x}_${tile.y}`;
          const tileUrl2x = `/gallery/_image/${imageId}/tile_${tile.x}_${tile.y}@2x`;
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


  // Render tiles for mobile zoom modal
  const renderMobileZoomTiles = () => {
    if (!tileConfig || !pinchZoom.isZoomed) return null;

    const imageId = getImageIdentifierFromUrl(image.medium_url);
    const tiles: JSX.Element[] = [];

    // Calculate base image display size (before zoom scale)
    const imgAspect = image.dimensions[0] / image.dimensions[1];
    const viewAspect = window.innerWidth / window.innerHeight;
    const baseImgWidth = imgAspect > viewAspect ? window.innerWidth : window.innerHeight * imgAspect;

    // Scale from original image to display coordinates
    // This matches the scale used in the tile container
    const imgScale = baseImgWidth / image.dimensions[0];
    const tileDisplaySize = tileConfig.tile_size * imgScale;

    // Visible viewport in tiled pixel coordinates
    const visibleTiledWidth = window.innerWidth / (imgScale * pinchZoom.scale);
    const visibleTiledHeight = window.innerHeight / (imgScale * pinchZoom.scale);

    // Center of visible area in tiled coordinates (accounting for pan)
    // Pan is in screen pixels, convert to tiled coordinates
    const panInTiledX = -pinchZoom.translateX / (imgScale * pinchZoom.scale);
    const panInTiledY = -pinchZoom.translateY / (imgScale * pinchZoom.scale);

    // Center is at image center (which is at origin of tiled coordinates)
    const tiledCenterX = image.dimensions[0] / 2 + panInTiledX;
    const tiledCenterY = image.dimensions[1] / 2 + panInTiledY;

    // Calculate which tiles to render (with buffer)
    const startTileX = Math.max(0, Math.floor((tiledCenterX - visibleTiledWidth / 2) / tileConfig.tile_size) - 1);
    const endTileX = Math.min(tileConfig.grid_width - 1, Math.ceil((tiledCenterX + visibleTiledWidth / 2) / tileConfig.tile_size) + 1);
    const startTileY = Math.max(0, Math.floor((tiledCenterY - visibleTiledHeight / 2) / tileConfig.tile_size) - 1);
    const endTileY = Math.min(tileConfig.grid_height - 1, Math.ceil((tiledCenterY + visibleTiledHeight / 2) / tileConfig.tile_size) + 1);

    for (let ty = startTileY; ty <= endTileY; ty++) {
      for (let tx = startTileX; tx <= endTileX; tx++) {
        const tileUrl = `/gallery/_image/${imageId}/tile_${tx}_${ty}`;
        const tileUrl2x = `/gallery/_image/${imageId}/tile_${tx}_${ty}@2x`;

        // Position tile within container (in base display coordinates, parent scales)
        const tileX = tx * tileDisplaySize;
        const tileY = ty * tileDisplaySize;

        tiles.push(
          <div
            key={`tile_${tx}_${ty}`}
            style={{
              position: 'absolute',
              left: `${tileX}px`,
              top: `${tileY}px`,
              width: `${tileDisplaySize}px`,
              height: `${tileDisplaySize}px`,
              backgroundImage: `image-set(url("${tileUrl}") 1x, url("${tileUrl2x}") 2x)`,
              backgroundSize: '100% 100%',
              backgroundRepeat: 'no-repeat'
            }}
          />
        );

        // Preload this tile
        if (!preloadedTilesRef.current.has(tileUrl)) {
          preloadedTilesRef.current.add(tileUrl);
          const img = new Image();
          img.src = tileUrl;
          if (window.devicePixelRatio > 1) {
            const img2x = new Image();
            img2x.src = tileUrl2x;
          }
        }
      }
    }

    return tiles;
  };

  return (
    <div className="image-container-outer">
      {/* Mobile Zoom Modal */}
      {isMobile && canUseZoom && (pinchZoom.isZoomed || pinchZoom.isTransitioning) && (
        <div
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            width: '100vw',
            height: '100vh',
            backgroundColor: 'black',
            zIndex: 9999,
            opacity: pinchZoom.isTransitioning ? (pinchZoom.isZoomed ? 1 : 0) : 1,
            transition: pinchZoom.isTransitioning ? 'opacity 0.3s ease-out' : 'none',
            touchAction: 'none',
            overflow: 'hidden'
          }}
          onTouchStart={handleTouchStart}
          onTouchMove={handleTouchMove}
          onTouchEnd={(e) => { handleTouchEnd(e); handleDoubleTap(e); }}
        >
          {/* Close button */}
          <button
            onClick={closeZoomModal}
            style={{
              position: 'absolute',
              top: '16px',
              right: '16px',
              zIndex: 10001,
              width: '44px',
              height: '44px',
              borderRadius: '50%',
              backgroundColor: 'rgba(0, 0, 0, 0.6)',
              border: '2px solid rgba(255, 255, 255, 0.8)',
              color: 'white',
              fontSize: '24px',
              lineHeight: '1',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center'
            }}
          >
            ×
          </button>

          {/* Zoomed content container - handles pan */}
          <div
            style={{
              position: 'absolute',
              top: '50%',
              left: '50%',
              transform: `translate(-50%, -50%) translate(${pinchZoom.translateX}px, ${pinchZoom.translateY}px)`,
              transformOrigin: 'center center',
              transition: pinchZoom.isTransitioning ? 'transform 0.3s ease-out' : 'none'
            }}
          >
            {/* Scaling container - wraps image and tiles together */}
            {(() => {
              const imgAspect = image.dimensions[0] / image.dimensions[1];
              const viewAspect = window.innerWidth / window.innerHeight;
              const baseImgWidth = imgAspect > viewAspect ? window.innerWidth : window.innerHeight * imgAspect;
              const baseImgHeight = imgAspect > viewAspect ? window.innerWidth / imgAspect : window.innerHeight;

              return (
                <div
                  style={{
                    position: 'relative',
                    width: `${baseImgWidth}px`,
                    height: `${baseImgHeight}px`,
                    transform: `scale(${pinchZoom.scale})`,
                    transformOrigin: 'center center',
                    transition: pinchZoom.isTransitioning ? 'transform 0.3s ease-out' : 'none'
                  }}
                >
                  {/* Base image layer */}
                  <img
                    src={image.medium_url}
                    srcSet={`${image.medium_url} 1x, ${image.medium_url.replace('/medium', '/medium@2x')} 2x`}
                    alt={image.name}
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      width: '100%',
                      height: '100%',
                      objectFit: 'contain'
                    }}
                    onContextMenu={(e) => e.preventDefault()}
                    onDragStart={(e) => e.preventDefault()}
                  />

                  {/* Tile overlay - positioned exactly over base image */}
                  {tileConfig && pinchZoom.scale > 1.5 && (() => {
                    // The tiled image has the original at (0,0) with padding on right/bottom
                    // So tiles should start at (0,0) to align with the base image
                    // Scale factor: base image display size / original image size
                    const imgScale = baseImgWidth / image.dimensions[0];

                    return (
                      <div
                        style={{
                          position: 'absolute',
                          top: 0,
                          left: 0,
                          width: `${tileConfig.tiled_width * imgScale}px`,
                          height: `${tileConfig.tiled_height * imgScale}px`,
                          pointerEvents: 'none',
                          overflow: 'hidden'
                        }}
                      >
                        {renderMobileZoomTiles()}
                      </div>
                    );
                  })()}
                </div>
              );
            })()}
          </div>

          {/* Zoom level indicator */}
          <div
            style={{
              position: 'absolute',
              bottom: '16px',
              left: '50%',
              transform: 'translateX(-50%)',
              backgroundColor: 'rgba(0, 0, 0, 0.6)',
              color: 'white',
              padding: '8px 16px',
              borderRadius: '20px',
              fontSize: '14px',
              fontWeight: '500'
            }}
          >
            {Math.round(pinchZoom.scale * 100)}%
          </div>
        </div>
      )}

      {/* Main image container */}
      <div
        ref={containerRef}
        className={`image-container ${canUseZoom ? 'zoom-enabled' : ''}`}
        style={{
          width: dimensions.width > 0 ? `${dimensions.width}px` : undefined,
          height: imageLoading ? (dimensions.height > 0 ? `${dimensions.height}px` : undefined) : 'auto',
          position: 'relative',
          overflow: 'hidden',
          touchAction: canUseZoom && isMobile ? 'none' : 'pan-x pan-y'
        }}
        onMouseDown={!isMobile ? handleMouseDown : undefined}
        onMouseMove={!isMobile ? handleMouseMove : undefined}
        onMouseUp={!isMobile ? handleMouseUp : undefined}
        onMouseLeave={!isMobile ? handleMouseLeave : undefined}
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={(e) => { handleTouchEnd(e); handleDoubleTap(e); }}
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
              srcSet={`${image.medium_url} 1x, ${image.medium_url.replace('/medium', '/medium@2x')} 2x`}
              alt={image.name}
              style={{
                width: '100%',
                height: 'auto',
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
          
          {/* Zoom overlay (loupe) - desktop only */}
          {canUseZoom && zoomState.isZooming && !isMobile && (
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
                    position: 'relative',
                    width: '100%',
                    height: '100%',
                    backgroundColor: 'rgba(0, 0, 0, 0.05)', // Very subtle background
                    overflow: 'hidden',
                    borderRadius: '50%', // Ensure circular clipping
                    // Force a new stacking context to contain the blur
                    transform: 'translateZ(0)',
                    WebkitTransform: 'translateZ(0)'
                  }}
                >
                  {/* Show underlying image with blur while tiles are loading */}
                  <div
                    style={{
                      position: 'absolute',
                      // Make it larger to account for blur edge effects
                      top: '-10px',
                      left: '-10px',
                      width: 'calc(100% + 20px)',
                      height: 'calc(100% + 20px)',
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