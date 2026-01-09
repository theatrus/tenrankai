import { createRoot } from 'react-dom/client';
import { MasonryGrid } from '@components/Gallery/MasonryGrid';

// Mount React masonry gallery on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  // Try to use the full gallery data first (includes metadata)
  const galleryDataElement = document.getElementById('gallery-data');
  const imagesDataElement = document.getElementById('gallery-images');
  
  let galleryData: any = null;
  let images: any[] = [];
  
  // Try to parse the full gallery data
  if (galleryDataElement) {
    try {
      const jsonText = galleryDataElement.textContent || '{}';
      galleryData = JSON.parse(jsonText);
      images = galleryData.images || [];
    } catch (e) {
      console.error('Failed to parse gallery data:', e);
    }
  }
  
  // Fall back to legacy images-only data if needed
  if (!galleryData && imagesDataElement) {
    try {
      const jsonText = imagesDataElement.textContent || '[]';
      images = JSON.parse(jsonText);
    } catch (e) {
      console.error('Failed to parse gallery images data:', e);
      return;
    }
  }
  
  if (!images.length && !galleryData) {
    console.warn('No gallery data found');
    return;
  }

  // Find the gallery URL from the page
  const galleryUrlElement = document.querySelector('[data-gallery-url]');
  const galleryUrl = galleryData?.gallery_url || galleryUrlElement?.getAttribute('data-gallery-url') || '/gallery';

  // Find the container for React - use the parent gallery-images div
  const galleryImages = document.querySelector('.gallery-images');
  if (!galleryImages) {
    console.error('Gallery images container not found');
    return;
  }

  // Clear existing content (remove the static grid)
  galleryImages.innerHTML = '';
  
  // Mount React component
  const root = createRoot(galleryImages);
  root.render(
    <MasonryGrid 
      images={images}
      galleryUrl={galleryUrl}
      permissions={galleryData?.permissions}
    />
  );
});