import React from 'react';
import { createRoot } from 'react-dom/client';
import { ImageDetailData } from '../types/index.ts';
import { useImageDetail } from '../hooks/useImageDetail.ts';
import { useKeyboardNavigation } from '../hooks/useKeyboardNavigation.ts';
import { useDelayedLoading } from '../hooks/useDelayedLoading.ts';
import { ImageDisplay } from '../components/ImageDetail/ImageDisplay.tsx';
import { ImageNavigation } from '../components/ImageDetail/ImageNavigation.tsx';
import { ImageMetadata, CameraMetadata, LocationMetadata } from '../components/ImageDetail/ImageMetadata.tsx';
import { ImageControls } from '../components/ImageDetail/ImageControls.tsx';

interface ImageDetailPageProps {
  initialData: ImageDetailData;
  galleryUrl: string;
  hideMetadata?: boolean;
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

export function ImageDetailPage({ initialData, galleryUrl, hideMetadata = false }: ImageDetailPageProps) {
  const { data: imageData, loading, error, loadImage } = useImageDetail({
    galleryName: initialData.gallery_name,
    initialData
  });
  
  // Use initialData immediately if no other data is available
  const currentData = imageData || initialData;
  
  // Only show loading after 500ms delay
  const showLoading = useDelayedLoading(loading && !currentData);

  // Enhanced navigation with SPA-style URL updates
  const handleNavigation = async (direction: 'prev' | 'next') => {
    if (!currentData) return;

    try {
      const targetImage = direction === 'prev' ? currentData.prev_image : currentData.next_image;
      if (!targetImage) return;

      // Load new image data
      await loadImage(targetImage.path);
      
      // Update URL without page reload
      const newUrl = `${galleryUrl}/detail/${targetImage.path}`;
      window.history.pushState({}, '', newUrl);
      
      // Update document title
      document.title = `${targetImage.name} - theatr.us`;
      
    } catch (err) {
      console.error(`Failed to navigate to ${direction} image:`, err);
    }
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
        <div className="image-main">
          <ImageDisplay 
            image={currentData.image} 
            hasDownloadPermission={false} 
          />
          
          <ImageNavigation
            prevImage={currentData.prev_image}
            nextImage={currentData.next_image}
            galleryUrl={galleryUrl}
            onNavigate={handleNavigation}
          />
          
          <div className="image-controls">
            <ImageControls image={currentData.image} />
            
            {(currentData.prev_image || currentData.next_image) && (
              <div className="nav-hint-container">
                {currentData.prev_image && currentData.next_image ? (
                  <span className="nav-hint">Use ← → keys to navigate between images</span>
                ) : currentData.prev_image ? (
                  <span className="nav-hint">Use ← key to go to previous image</span>
                ) : (
                  <span className="nav-hint">Use → key to go to next image</span>
                )}
              </div>
            )}
          </div>
        </div>
        
        <div className="image-info">
          {currentData.image.title && (
            <h2 className="image-title">{currentData.image.title}</h2>
          )}
          
          {currentData.image.description && (
            <div className="image-description">
              {currentData.image.description}
            </div>
          )}
          
          {!hideMetadata && (
            <>
              <ImageMetadata image={currentData.image} hideMetadata={hideMetadata} />
              <CameraMetadata image={currentData.image} />
              <LocationMetadata image={currentData.image} />
            </>
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
    
    console.log('React taking over image detail page with data:', {
      imagePath: initialData.image.path,
      imageUrl: initialData.image.medium_url,
      breadcrumbs: initialData.breadcrumbs,
      galleryUrl,
      hideMetadata
    });

    const root = createRoot(container);
    root.render(
      <ImageDetailPage 
        initialData={initialData}
        galleryUrl={galleryUrl}
        hideMetadata={hideMetadata}
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