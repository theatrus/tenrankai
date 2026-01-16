import { useState, useRef, useCallback } from 'react';
import { ImageArea } from '../../types/index.ts';

interface AreaSelectorProps {
  imageUrl: string;
  dimensions: [number, number];
  onAreaSelected: (area: ImageArea | null) => void;
  existingArea?: ImageArea | null;
}

export function AreaSelector({ imageUrl, dimensions, onAreaSelected, existingArea }: AreaSelectorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isSelecting, setIsSelecting] = useState(false);
  const [startPoint, setStartPoint] = useState({ x: 0, y: 0 });
  const [endPoint, setEndPoint] = useState({ x: 0, y: 0 });
  const [selectedArea, setSelectedArea] = useState<ImageArea | null>(existingArea || null);
  
  // Calculate displayed image dimensions to maintain aspect ratio
  const aspectRatio = dimensions[0] / dimensions[1];
  const maxWidth = window.innerWidth * 0.8;
  const maxHeight = window.innerHeight * 0.6;
  
  let displayWidth, displayHeight;
  
  if (maxWidth / maxHeight > aspectRatio) {
    displayHeight = maxHeight;
    displayWidth = displayHeight * aspectRatio;
  } else {
    displayWidth = maxWidth;
    displayHeight = displayWidth / aspectRatio;
  }

  const getPercentageCoordinates = useCallback((clientX: number, clientY: number) => {
    if (!containerRef.current) return { x: 0, y: 0 };
    
    const rect = containerRef.current.getBoundingClientRect();
    const x = ((clientX - rect.left) / rect.width) * 100;
    const y = ((clientY - rect.top) / rect.height) * 100;
    
    // Clamp values between 0 and 100
    return {
      x: Math.max(0, Math.min(100, x)),
      y: Math.max(0, Math.min(100, y))
    };
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    const coords = getPercentageCoordinates(e.clientX, e.clientY);
    setStartPoint(coords);
    setEndPoint(coords);
    setIsSelecting(true);
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isSelecting) return;
    
    const coords = getPercentageCoordinates(e.clientX, e.clientY);
    setEndPoint(coords);
  };

  const handleMouseUp = () => {
    if (!isSelecting) return;
    
    setIsSelecting(false);
    
    // Calculate the selected area
    const minX = Math.min(startPoint.x, endPoint.x);
    const minY = Math.min(startPoint.y, endPoint.y);
    const width = Math.abs(endPoint.x - startPoint.x);
    const height = Math.abs(endPoint.y - startPoint.y);
    
    // Only save area if it has meaningful size (at least 2% in both dimensions)
    if (width > 2 && height > 2) {
      const area: ImageArea = {
        x: minX,
        y: minY,
        width,
        height
      };
      setSelectedArea(area);
      onAreaSelected(area);
    }
  };

  const clearSelection = () => {
    setSelectedArea(null);
    onAreaSelected(null);
  };

  // Touch event handlers for mobile
  const handleTouchStart = (e: React.TouchEvent) => {
    if (e.touches.length === 1) {
      const touch = e.touches[0];
      const coords = getPercentageCoordinates(touch.clientX, touch.clientY);
      setStartPoint(coords);
      setEndPoint(coords);
      setIsSelecting(true);
    }
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!isSelecting || e.touches.length !== 1) return;
    
    const touch = e.touches[0];
    const coords = getPercentageCoordinates(touch.clientX, touch.clientY);
    setEndPoint(coords);
  };

  const handleTouchEnd = () => {
    handleMouseUp();
  };

  // Get current selection box coordinates
  const getSelectionBox = () => {
    if (!isSelecting && !selectedArea) return null;
    
    if (selectedArea && !isSelecting) {
      return {
        left: selectedArea.x,
        top: selectedArea.y,
        width: selectedArea.width,
        height: selectedArea.height
      };
    }
    
    const minX = Math.min(startPoint.x, endPoint.x);
    const minY = Math.min(startPoint.y, endPoint.y);
    const width = Math.abs(endPoint.x - startPoint.x);
    const height = Math.abs(endPoint.y - startPoint.y);
    
    return {
      left: minX,
      top: minY,
      width,
      height
    };
  };

  const selectionBox = getSelectionBox();

  return (
    <div className="area-selector-container">
      <div 
        className="area-selector-instructions"
      >
        {selectedArea ? (
          <>
            <span>Area selected</span>
            <button 
              className="area-clear-btn"
              onClick={clearSelection}
            >
              Clear selection
            </button>
          </>
        ) : (
          <span>Click and drag on the image to select an area</span>
        )}
      </div>
      
      <div
        ref={containerRef}
        className="area-selector-image"
        style={{
          width: `${displayWidth}px`,
          height: `${displayHeight}px`,
          backgroundImage: `url(${imageUrl})`,
          backgroundSize: 'contain',
          backgroundPosition: 'center',
          backgroundRepeat: 'no-repeat',
          position: 'relative',
          cursor: 'crosshair',
          userSelect: 'none',
          WebkitUserSelect: 'none',
          MozUserSelect: 'none',
          msUserSelect: 'none'
        }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={handleTouchEnd}
      >
        {selectionBox && (
          <div
            className="area-selection-box"
            style={{
              position: 'absolute',
              left: `${selectionBox.left}%`,
              top: `${selectionBox.top}%`,
              width: `${selectionBox.width}%`,
              height: `${selectionBox.height}%`,
              border: '2px solid rgba(59, 130, 246, 0.8)',
              backgroundColor: 'rgba(59, 130, 246, 0.1)',
              pointerEvents: 'none'
            }}
          >
            {selectedArea && (
              <div className="area-selection-label">
                Selected area
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}