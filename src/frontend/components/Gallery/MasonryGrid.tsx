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
  user_metadata?: {
    comments: Array<any>;
    highlighted: boolean;
    pick_status?: 'pick' | 'no_pick' | 'undecided';
    tags: string[];
  };
}

interface MasonryGridProps {
  images: GalleryImage[];
  galleryUrl: string;
  permissions?: any;
}

interface DisplayDimensions {
  width: number;
  height: number;
}

export const MasonryGrid: React.FC<MasonryGridProps> = ({ images, galleryUrl, permissions }) => {
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
  }, [filteredImages, columnWidth, numColumns]);

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

  // Function to render metadata badges
  const renderBadges = (image: GalleryImage) => {
    const badges = [];
    
    // Only show metadata if permissions allow
    if (!permissions?.can_read_metadata) {
      return null;
    }
    
    const metadata = image.user_metadata;
    if (!metadata) {
      return null;
    }
    
    // Comment count badge
    if (metadata.comments && metadata.comments.length > 0) {
      badges.push(
        <span key="comments" className="image-badge badge-comments" title={`${metadata.comments.length} comment${metadata.comments.length > 1 ? 's' : ''}`}>
          💬 {metadata.comments.length}
        </span>
      );
    }
    
    // Highlighted/starred badge
    if (metadata.highlighted) {
      badges.push(
        <span key="highlighted" className="image-badge badge-highlighted" title="Highlighted">
          ⭐
        </span>
      );
    }
    
    // Pick status badge
    if (metadata.pick_status) {
      if (metadata.pick_status === 'pick') {
        badges.push(
          <span key="pick" className="image-badge badge-pick" title="Pick">
            ✓
          </span>
        );
      } else if (metadata.pick_status === 'no_pick') {
        badges.push(
          <span key="reject" className="image-badge badge-reject" title="Rejected">
            ✗
          </span>
        );
      } else if (metadata.pick_status === 'undecided') {
        badges.push(
          <span key="undecided" className="image-badge badge-undecided" title="Undecided">
            ?
          </span>
        );
      }
    }
    
    // Tags badge
    if (metadata.tags && metadata.tags.length > 0) {
      badges.push(
        <span key="tags" className="image-badge badge-tags" title={`Tags: ${metadata.tags.join(', ')}`}>
          🏷️ {metadata.tags.length}
        </span>
      );
    }
    
    return badges.length > 0 ? (
      <div className="image-badges">
        {badges}
      </div>
    ) : null;
  };

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
                <a 
                  href={`${galleryUrl}/detail/${image.path}`} 
                  className="image-link"
                  onContextMenu={(e) => e.preventDefault()}
                  onDragStart={(e) => e.preventDefault()}
                >
                  <div 
                    className="gallery-image-container"
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      width: '100%',
                      height: '100%',
                      backgroundImage: `image-set(
                        url(${image.gallery_url || image.thumbnail_url || ''}) 1x,
                        url(${(image.gallery_url || image.thumbnail_url || '').replace('?size=gallery', '?size=gallery@2x')}) 2x
                      )`,
                      backgroundSize: 'cover',
                      backgroundPosition: 'center',
                      backgroundRepeat: 'no-repeat'
                    }}
                    role="img"
                    aria-label={image.name}
                  />
                  {renderBadges(image)}
                </a>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
};