import { createRoot } from 'react-dom/client';
import { MasonryGrid } from '@components/Gallery/MasonryGrid';

// Mount React masonry gallery on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  const imagesDataElement = document.getElementById('gallery-images');
  
  if (!imagesDataElement) {
    console.warn('No gallery images data element found');
    return;
  }
  
  let images;
  try {
    const jsonText = imagesDataElement.textContent || '[]';
    images = JSON.parse(jsonText);
  } catch (e) {
    console.error('Failed to parse gallery images data:', e);
    return;
  }

  // Find the gallery URL from the page
  const galleryUrlElement = document.querySelector('[data-gallery-url]');
  const galleryUrl = galleryUrlElement?.getAttribute('data-gallery-url') || '/gallery';

  // Find the container for React
  const container = document.getElementById('gallery-grid');
  if (!container) {
    console.error('Gallery grid container not found');
    return;
  }

  // Clear existing content
  container.innerHTML = '';
  
  // Mount React component
  const root = createRoot(container);
  root.render(
    <MasonryGrid 
      images={images}
      galleryUrl={galleryUrl}
    />
  );
});