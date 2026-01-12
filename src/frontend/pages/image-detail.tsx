import React, { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ImageDetailData } from '../types/index.ts';
import { useImageDetail } from '../hooks/useImageDetail.ts';
import { useKeyboardNavigation } from '../hooks/useKeyboardNavigation.ts';
import { useDelayedLoading } from '../hooks/useDelayedLoading.ts';
import { useSwipeGestures } from '../hooks/useSwipeGestures.ts';
import { ImageDisplay } from '../components/ImageDetail/ImageDisplay.tsx';
import { ImageNavigation } from '../components/ImageDetail/ImageNavigation.tsx';
import { MobileNavigation } from '../components/ImageDetail/MobileNavigation.tsx';
import { ImageMetadata, CameraMetadata, LocationMetadata, AIMetadata } from '../components/ImageDetail/ImageMetadata.tsx';
import { UserMetadata } from '../components/ImageDetail/UserMetadata.tsx';

interface ImageDetailPageProps {
  initialData: ImageDetailData;
  galleryUrl: string;
  hideMetadata?: boolean;
  isAuthenticated?: boolean;
  currentUser?: string;
}

interface Breadcrumb {
  path: string;
  display_name: string;
  is_current: boolean;
}

function Breadcrumbs({ breadcrumbs, galleryUrl, currentImageTitle }: { 
  breadcrumbs: Breadcrumb[] | any; 
  galleryUrl: string; 
  currentImageTitle: string; 
}) {
  // Handle case where breadcrumbs might not be an array
  const safeBreadcrumbs = Array.isArray(breadcrumbs) ? breadcrumbs : [];
  
  return (
    <nav className="gallery-nav">
      {safeBreadcrumbs.map((crumb, index) => (
        <React.Fragment key={index}>
          {index > 0 && <span className="nav-separator">→</span>}
          {crumb.is_current ? (
            <span className="nav-current">{crumb.display_name}</span>
          ) : (
            <a 
              href={`${galleryUrl}${crumb.path ? `/${crumb.path}` : ''}`} 
              className="nav-link"
            >
              {crumb.display_name}
            </a>
          )}
        </React.Fragment>
      ))}
      <span className="nav-separator">→</span>
      <span className="nav-current">{currentImageTitle}</span>
    </nav>
  );
}

export function ImageDetailPage({ 
  initialData, 
  galleryUrl, 
  hideMetadata = false,
  isAuthenticated = false,
  currentUser
}: ImageDetailPageProps) {
  const { data: imageData, loading, error, loadImage, updateMetadata } = useImageDetail({
    galleryName: initialData.gallery_name,
    initialData
  });
  
  // Use initialData immediately if no other data is available
  const currentData = imageData || initialData;
  
  // Only show loading after 500ms delay
  const showLoading = useDelayedLoading(loading && !currentData);
  
  // Ref for swipe gestures
  const imageContainerRef = useRef<HTMLDivElement>(null);

  // Track zoom state to disable swipe navigation when zoomed
  const [isImageZoomed, setIsImageZoomed] = useState(false);

  // Navigate to a specific image by its navigation data
  const handleNavigateToImage = async (image: { path: string; name: string }) => {
    try {
      // Load new image data
      await loadImage(image.path);

      // Update URL without page reload
      const newUrl = `${galleryUrl}/detail/${image.path}`;
      window.history.pushState({}, '', newUrl);

      // Update document title
      document.title = `${image.name} - theatr.us`;

    } catch (err) {
      console.error(`Failed to navigate to image:`, err);
    }
  };

  // Enhanced navigation with SPA-style URL updates
  const handleNavigation = async (direction: 'prev' | 'next') => {
    if (!currentData) return;

    const targetImage = direction === 'prev' ? currentData.prev_image : currentData.next_image;
    if (!targetImage) return;

    await handleNavigateToImage(targetImage);
  };

  useKeyboardNavigation({
    prevImage: currentData?.prev_image,
    nextImage: currentData?.next_image,
    galleryUrl,
    imagePath: currentData?.image.path || '',
    onNavigate: (direction) => {
      if (direction === 'back') {
        // Navigate back to gallery - extract folder from image path
        const pathParts = currentData?.image.path.split('/') || [];
        const folderPath = pathParts.length > 1 ? pathParts.slice(0, -1).join('/') : '';
        window.location.href = folderPath ? `${galleryUrl}/${folderPath}` : galleryUrl;
      } else {
        handleNavigation(direction);
      }
    }
  });

  // Add swipe gesture support (disabled when image is zoomed)
  useSwipeGestures(imageContainerRef, {
    onSwipeLeft: () => {
      if (currentData?.next_image) {
        handleNavigation('next');
      }
    },
    onSwipeRight: () => {
      if (currentData?.prev_image) {
        handleNavigation('prev');
      }
    }
  }, { disabled: isImageZoomed });

  if (error) {
    return (
      <div style={{ padding: '2rem', textAlign: 'center' }}>
        <h2>Error Loading Image</h2>
        <p>{error}</p>
        <button onClick={() => window.location.reload()}>Try Again</button>
      </div>
    );
  }

  if (showLoading) {
    return (
      <div style={{ padding: '2rem', textAlign: 'center' }}>
        <p>Loading image...</p>
      </div>
    );
  }

  return (
    <>
      <Breadcrumbs 
        breadcrumbs={currentData.breadcrumbs} 
        galleryUrl={galleryUrl}
        currentImageTitle={currentData.image.title || currentData.image.name}
      />
      
      <div className="image-detail-content">
        {/* Image viewer section - full viewport */}
        <div className="image-viewer-section">
          <MobileNavigation
            prevImage={currentData.prev_image}
            nextImage={currentData.next_image}
            onNavigate={handleNavigation}
          />
          
          {/* Swipe hint for mobile */}
          <div className="mobile-swipe-hint">
            <span>
              {isImageZoomed
                ? 'Pinch out to exit zoom'
                : currentData.permissions.can_use_zoom
                  ? 'Pinch to zoom • Swipe to navigate'
                  : 'Swipe to navigate'}
            </span>
          </div>
          
          <div className="image-container-wrapper">
            <div ref={imageContainerRef} className="swipeable-image-area">
              <ImageDisplay
                image={currentData.image}
                canUseZoom={currentData.permissions.can_use_zoom}
                tileConfig={currentData.tile_config}
                galleryName={currentData.gallery_name}
                onZoomStateChange={setIsImageZoomed}
              />
            </div>
          </div>
          
          {/* Thumbnail navigation */}
          <ImageNavigation
            prevImage={currentData.prev_image}
            nextImage={currentData.next_image}
            prevImages={currentData.prev_images || []}
            nextImages={currentData.next_images || []}
            galleryUrl={galleryUrl}
            onNavigate={handleNavigation}
            onNavigateToImage={handleNavigateToImage}
            title={currentData.image.title}
            description={currentData.image.description}
          />
          
          
          {(currentData.prev_image || currentData.next_image) && (
            <div className="nav-hint">
              {currentData.prev_image && currentData.next_image ? (
                <>Use ← → keys to navigate between images</>
              ) : currentData.prev_image ? (
                <>Use ← key to go to previous image</>
              ) : (
                <>Use → key to go to next image</>
              )}
            </div>
          )}
        </div>
        
        {/* Info section - below the image viewer */}
        <div className="image-info-section">
          {/* Image title and description - mobile only */}
          <div className="image-header-mobile">
            {currentData.image.title && (
              <h1 className="image-title">{currentData.image.title}</h1>
            )}
            {currentData.image.description && (
              <div 
                className="image-description"
                dangerouslySetInnerHTML={{ __html: currentData.image.description }}
              />
            )}
          </div>
          
          {!hideMetadata && (
            <div className="metadata-grid">
              <ImageMetadata image={currentData.image} hideMetadata={hideMetadata} permissions={currentData.permissions} />
              <CameraMetadata image={currentData.image} permissions={currentData.permissions} />
              <LocationMetadata image={currentData.image} permissions={currentData.permissions} />
            </div>
          )}

          <AIMetadata image={currentData.image} permissions={currentData.permissions} />
          
          {currentData.permissions.can_read_metadata && (
            <UserMetadata
              metadata={currentData.image.user_metadata}
              imagePath={currentData.image.path}
              galleryName={currentData.gallery_name}
              isAuthenticated={isAuthenticated}
              currentUser={currentUser}
              permissions={currentData.permissions}
              onUpdate={(updatedMetadata) => updateMetadata(updatedMetadata)}
              image={{
                medium_url: currentData.image.medium_url,
                dimensions: currentData.image.dimensions
              }}
            />
          )}
        </div>
      </div>
      
    </>
  );
}

// Mount React component on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  const container = document.getElementById('react-image-detail');
  if (!container) return;

  try {
    // Extract embedded JSON data from script tag
    const dataScript = document.getElementById('image-detail-data');
    if (!dataScript?.textContent) {
      throw new Error('No image detail data found');
    }

    const initialData: ImageDetailData = JSON.parse(dataScript.textContent);
    const galleryUrl = container.getAttribute('data-gallery-url') || '/gallery';
    const hideMetadata = container.getAttribute('data-hide-metadata') === 'true';
    const isAuthenticated = container.getAttribute('data-authenticated') === 'true';
    const currentUser = container.getAttribute('data-current-user') || undefined;
    
    console.log('React taking over image detail page with data:', {
      imagePath: initialData.image.path,
      imageUrl: initialData.image.medium_url,
      breadcrumbs: initialData.breadcrumbs,
      galleryUrl,
      hideMetadata,
      isAuthenticated,
      permissions: initialData.permissions
    });

    const root = createRoot(container);
    root.render(
      <ImageDetailPage 
        initialData={initialData}
        galleryUrl={galleryUrl}
        hideMetadata={hideMetadata}
        isAuthenticated={isAuthenticated}
        currentUser={currentUser}
      />
    );
  } catch (error) {
    console.error('Failed to mount React image detail component:', error);
    
    // Show fallback message
    container.innerHTML = `
      <div style="padding: 2rem; text-align: center; border: 2px solid #dc3545; background: #f8d7da; color: #721c24; border-radius: 4px;">
        <h3>React Enhancement Failed</h3>
        <p>The image detail page could not be loaded properly.</p>
        <button onclick="window.location.reload()">Reload Page</button>
      </div>
    `;
  }
});