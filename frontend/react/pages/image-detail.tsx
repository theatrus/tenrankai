import React, { useRef, useState, useCallback, useEffect } from 'react';
import { createRoot } from 'react-dom/client';
import { ImageDetailData } from '../types/index.ts';
import { useImageDetail } from '../hooks/useImageDetail.ts';
import { useKeyboardNavigation } from '../hooks/useKeyboardNavigation.ts';
import { useDelayedLoading } from '../hooks/useDelayedLoading.ts';
import { useSwipeGestures } from '../hooks/useSwipeGestures.ts';
import { useImagePreload } from '../hooks/useImagePreload.ts';
import { ImageDisplay } from '../components/ImageDetail/ImageDisplay.tsx';
import { ImageNavigation } from '../components/ImageDetail/ImageNavigation.tsx';
import { MobileNavigation } from '../components/ImageDetail/MobileNavigation.tsx';
import { VersionPicker } from '../components/ImageDetail/VersionPicker.tsx';
import { ImageMetadata, CameraMetadata, LocationMetadata, AIMetadata } from '../components/ImageDetail/ImageMetadata.tsx';
import { UserMetadata } from '../components/ImageDetail/UserMetadata.tsx';
import { ImageControls } from '../components/ImageDetail/ImageControls.tsx';
import { EditModal } from '../components/Editor/index.ts';
import { contentEditorApi } from '../api/content-editor.ts';

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

function createMountErrorFallback(): HTMLElement {
  const fallback = document.createElement('div');
  Object.assign(fallback.style, {
    padding: '2rem',
    textAlign: 'center',
    border: '2px solid #dc3545',
    background: '#f8d7da',
    color: '#721c24',
    borderRadius: '4px',
  });

  const title = document.createElement('h3');
  title.textContent = 'React Enhancement Failed';

  const message = document.createElement('p');
  message.textContent = 'The image detail page could not be loaded properly.';

  const reloadButton = document.createElement('button');
  reloadButton.type = 'button';
  reloadButton.textContent = 'Reload Page';
  reloadButton.addEventListener('click', () => window.location.reload());

  fallback.append(title, message, reloadButton);
  return fallback;
}

function Breadcrumbs({ breadcrumbs, galleryUrl, currentImageTitle, imagePath }: {
  breadcrumbs: Breadcrumb[] | any;
  galleryUrl: string;
  currentImageTitle: string;
  imagePath: string;
}) {
  // Handle case where breadcrumbs might not be an array
  const safeBreadcrumbs = Array.isArray(breadcrumbs) ? breadcrumbs : [];

  return (
    <nav className="gallery-nav">
      {safeBreadcrumbs.map((crumb, index) => {
        const isLast = index === safeBreadcrumbs.length - 1;
        // Add anchor to the last breadcrumb (current folder) to scroll to this image
        const anchor = isLast ? `#${imagePath}` : '';
        const href = `${galleryUrl}${crumb.path ? `/${crumb.path}` : ''}${anchor}`;

        return (
          <React.Fragment key={index}>
            {index > 0 && <span className="nav-separator">→</span>}
            <a href={href} className="nav-link">{crumb.display_name}</a>
          </React.Fragment>
        );
      })}
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

  // Modal state for editing image info
  const [isEditModalOpen, setIsEditModalOpen] = useState(false);

  // Handler for saving image title and description
  const handleSaveImageInfo = useCallback(async (title: string, markdown: string) => {
    await contentEditorApi.updateImageDescription(
      currentData.gallery_name,
      currentData.image.path,
      markdown,
      title || undefined
    );
    // Reload image data to get updated info
    await loadImage(currentData.image.path);
  }, [currentData?.gallery_name, currentData?.image.path, loadImage]);

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
        // Navigate back to gallery with anchor to scroll to this image
        const imagePath = currentData?.image.path || '';
        const pathParts = imagePath.split('/');
        const folderPath = pathParts.length > 1 ? pathParts.slice(0, -1).join('/') : '';
        const anchor = `#${imagePath}`;
        window.location.href = folderPath ? `${galleryUrl}/${folderPath}${anchor}` : `${galleryUrl}${anchor}`;
      } else {
        handleNavigation(direction);
      }
    }
  });

  // Handle browser back/forward navigation
  useEffect(() => {
    const handlePopState = () => {
      const detailPrefix = `${galleryUrl}/detail/`;
      const path = window.location.pathname;
      if (path.startsWith(detailPrefix)) {
        const imagePath = decodeURIComponent(path.slice(detailPrefix.length));
        if (imagePath && imagePath !== currentData?.image.path) {
          loadImage(imagePath);
        }
      }
    };

    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, [galleryUrl, loadImage, currentData?.image.path]);

  // Preload previous and next images for faster navigation
  useImagePreload(currentData?.prev_image, currentData?.next_image);

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
        imagePath={currentData.image.path}
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
                canSeeAiAltText={currentData.permissions.can_see_ai_alt_text}
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
            canEditContent={currentData.permissions.can_edit_content}
            onEditClick={() => setIsEditModalOpen(true)}
          />

          {/* Version picker - shows previous versions if available */}
          {currentData.image.versions && currentData.image.versions.length > 0 && (
            <VersionPicker
              versions={currentData.image.versions}
              currentPath={currentData.image.path}
              galleryUrl={galleryUrl}
            />
          )}

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
        
        {/* Image description - shown when description exists */}
        {currentData.image.description && (
          <div className="image-description-section hide-mobile">
            <div
              className="image-description-content"
              dangerouslySetInnerHTML={{ __html: currentData.image.description }}
            />
          </div>
        )}

        {/* Info section - below the image viewer */}
        <div className="image-info-section">
          {/* Hidden image indicator */}
          {currentData.is_hidden && (
            <div className="image-hidden-badge" title="This image is hidden from users without permission">
              <span className="hidden-icon">HIDDEN</span>
            </div>
          )}

          {/* Image title and description */}
          <div className="image-header-mobile">
            {/* Title with edit icon */}
            {(currentData.image.title || currentData.permissions.can_edit_content) && (
              <div className="image-title-row">
                {currentData.image.title ? (
                  <h1 className="image-title">{currentData.image.title}</h1>
                ) : currentData.permissions.can_edit_content ? (
                  <span className="image-title-placeholder">Untitled image</span>
                ) : null}
                {currentData.permissions.can_edit_content && (
                  <button
                    type="button"
                    className="image-edit-icon"
                    onClick={() => setIsEditModalOpen(true)}
                    title="Edit image info"
                    aria-label="Edit image info"
                  >
                    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/>
                    </svg>
                  </button>
                )}
              </div>
            )}
            {/* Description content */}
            {currentData.image.description && (
              <div
                className="image-description"
                dangerouslySetInnerHTML={{ __html: currentData.image.description }}
              />
            )}
          </div>

          {/* Edit modal */}
          <EditModal
            isOpen={isEditModalOpen}
            modalTitle="Edit Image"
            title={currentData.image.title || ''}
            markdownContent={currentData.image.user_metadata?.description || ''}
            descriptionPlaceholder="Add image description..."
            onSave={handleSaveImageInfo}
            onClose={() => setIsEditModalOpen(false)}
          />
          
          {!hideMetadata && (
            <div className="metadata-grid">
              <ImageMetadata image={currentData.image} hideMetadata={hideMetadata} permissions={currentData.permissions} />
              <CameraMetadata image={currentData.image} permissions={currentData.permissions} />
              <LocationMetadata image={currentData.image} permissions={currentData.permissions} />
            </div>
          )}

          <AIMetadata image={currentData.image} permissions={currentData.permissions} />

          <ImageControls image={currentData.image} permissions={currentData.permissions} onEditClick={() => setIsEditModalOpen(true)} shareUrl={currentData.share_url} baseUrl={currentData.base_url} />

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
    container.replaceChildren(createMountErrorFallback());
  }
});
