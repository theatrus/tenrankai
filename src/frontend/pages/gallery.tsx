import { createRoot } from 'react-dom/client';

interface GalleryProps {
  galleryName: string;
  images: any[];
}

function GalleryApp({ galleryName, images }: GalleryProps) {
  return (
    <div className="react-gallery">
      <h2>Enhanced Gallery (React)</h2>
      <p>Gallery: {galleryName}</p>
      <p>Images: {images.length}</p>
      <p>This will be enhanced with interactive masonry grid...</p>
    </div>
  );
}

// Mount React component on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  const container = document.getElementById('react-gallery');
  if (container) {
    // Extract data from server-rendered attributes
    const galleryName = container.getAttribute('data-gallery-name') || '';
    const imagesJson = container.getAttribute('data-images') || '[]';
    
    let images;
    try {
      images = JSON.parse(imagesJson);
    } catch (e) {
      console.warn('Failed to parse images:', e);
      images = [];
    }

    const root = createRoot(container);
    root.render(
      <GalleryApp 
        galleryName={galleryName}
        images={images}
      />
    );
  }
});