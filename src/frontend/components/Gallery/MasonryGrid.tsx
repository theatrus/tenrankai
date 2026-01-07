import React, { useEffect, useRef, useState, useCallback } from 'react';

export interface GalleryImage {
  path: string; // Contains the indexed identifier (filename, sequence, or unique_id)
  name: string;
  thumbnail_url?: string;
  gallery_url?: string;
  dimensions?: [number, number];
  is_new?: boolean;
  is_directory?: boolean;
  folder_item_count?: number;
  capture_date?: string;
}

interface MasonryGridProps {
  images: GalleryImage[];
  galleryUrl: string;
}

interface DisplayDimensions {
  width: number;
  height: number;
}

export const MasonryGrid: React.FC<MasonryGridProps> = ({ images, galleryUrl }) => {
  const [columnWidth, setColumnWidth] = useState(400);
  const [numColumns, setNumColumns] = useState(2);
  const gridRef = useRef<HTMLDivElement>(null);
  const resizeTimeoutRef = useRef<number>();

  // Calculate column width based on viewport
  const calculateColumnWidth = useCallback(() => {
    const viewportWidth = window.innerWidth;
    const containerWidth = Math.min(viewportWidth, 1200);
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    const gap = 24; // 1.5rem
    
    if (viewportWidth <= 768) {
      // Mobile: single column with minimal padding
      const mobilePadding = isIOS ? 16 : 20;
      setColumnWidth(containerWidth - mobilePadding);
      setNumColumns(1);
    } else {
      // Desktop: two columns
      const desktopPadding = isIOS ? 32 : 40;
      setColumnWidth((containerWidth - desktopPadding - gap) / 2);
      setNumColumns(2);
    }
  }, []);

  // Calculate display dimensions for an image
  const calculateDisplayDimensions = (
    originalWidth: number, 
    originalHeight: number, 
    maxWidth: number
  ): DisplayDimensions => {
    if (originalWidth <= maxWidth) {
      return { width: originalWidth, height: originalHeight };
    } else {
      const ratio = maxWidth / originalWidth;
      return { 
        width: maxWidth, 
        height: Math.round(originalHeight * ratio)
      };
    }
  };

  // Setup resize handling
  useEffect(() => {
    calculateColumnWidth();
    
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    const timeout = isIOS ? 300 : 150;
    
    const handleResize = () => {
      clearTimeout(resizeTimeoutRef.current);
      resizeTimeoutRef.current = window.setTimeout(() => {
        calculateColumnWidth();
      }, timeout);
    };

    window.addEventListener('resize', handleResize);
    return () => {
      window.removeEventListener('resize', handleResize);
      clearTimeout(resizeTimeoutRef.current);
    };
  }, [calculateColumnWidth]);

  // Distribute images across columns with proper height tracking
  const distributeImages = useCallback(() => {
    const columns: Array<{ images: GalleryImage[]; height: number }> = 
      Array(numColumns).fill(null).map(() => ({ images: [], height: 0 }));
    
    images.forEach((image) => {
      // Use default dimensions if not available
      const width = image.dimensions?.[0] || 800;
      const height = image.dimensions?.[1] || 600;
      
      const displayDimensions = calculateDisplayDimensions(width, height, columnWidth);
      
      // Find shortest column
      const shortestColumnIndex = columns.reduce((minIdx, col, idx, arr) => 
        col.height < arr[minIdx].height ? idx : minIdx, 0);
      
      // Add to shortest column
      columns[shortestColumnIndex].images.push(image);
      columns[shortestColumnIndex].height += displayDimensions.height + 24; // gap
    });
    
    return columns.map(col => col.images);
  }, [images, columnWidth, numColumns]);

  // Generate clean ID from image name
  const generateCleanId = (name: string): string => {
    return name
      .replace(/\./g, '')
      .replace(/\s/g, '')
      .replace(/-/g, '')
      .replace(/_/g, '');
  };

  // Scroll to anchor if present
  useEffect(() => {
    const hash = window.location.hash;
    if (hash) {
      const targetId = hash.slice(1);
      setTimeout(() => {
        const element = document.getElementById(targetId);
        if (element) {
          element.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
      }, 100);
    }
  }, []);

  const columns = distributeImages();

  return (
    <div className="image-grid" ref={gridRef}>
      {columns.map((column, columnIndex) => (
        <div 
          key={columnIndex} 
          className="masonry-column" 
          data-column={columnIndex}
          style={{ display: columnIndex >= numColumns ? 'none' : 'flex' }}
        >
          {column.map((image) => {
            const width = image.dimensions?.[0] || 800;
            const height = image.dimensions?.[1] || 600;
            const displayDimensions = calculateDisplayDimensions(width, height, columnWidth);
            const cleanId = generateCleanId(image.name);
            
            return (
              <div 
                key={image.path} 
                className={`image-item ${image.is_new ? 'is-new' : ''}`}
                id={cleanId}
                data-id={image.path}
                style={{
                  width: `${displayDimensions.width}px`,
                  height: `${displayDimensions.height}px`
                }}
              >
                <a href={`${galleryUrl}/detail/${image.path}`} className="image-link">
                  <img 
                    src={image.gallery_url || image.thumbnail_url || ''}
                    srcSet={`${image.gallery_url} 1x, ${image.gallery_url}@2x 2x`}
                    alt={image.name}
                    width={displayDimensions.width}
                    height={displayDimensions.height}
                    style={{
                      width: '100%',
                      height: '100%',
                      objectFit: 'cover'
                    }}
                  />
                </a>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
};