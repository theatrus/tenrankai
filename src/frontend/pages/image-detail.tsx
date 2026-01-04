import { createRoot } from 'react-dom/client';

interface ImageDetailProps {
  imagePath: string;
  galleryName: string;
  metadata: any;
}

function ImageDetailApp({ imagePath, galleryName, metadata }: ImageDetailProps) {
  console.log('Metadata:', metadata); // Use metadata to avoid TS error for now
  
  return (
    <div className="react-image-detail">
      <h2>Enhanced Image Detail (React)</h2>
      <p>Image: {imagePath}</p>
      <p>Gallery: {galleryName}</p>
      <p>This will be enhanced with rich interactivity...</p>
    </div>
  );
}

// Mount React component on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  const container = document.getElementById('react-image-detail');
  if (container) {
    // Extract data from server-rendered attributes
    const imagePath = container.getAttribute('data-image-path') || '';
    const galleryName = container.getAttribute('data-gallery-name') || '';
    const metadataJson = container.getAttribute('data-metadata') || '{}';
    
    let metadata;
    try {
      metadata = JSON.parse(metadataJson);
    } catch (e) {
      console.warn('Failed to parse metadata:', e);
      metadata = {};
    }

    const root = createRoot(container);
    root.render(
      <ImageDetailApp 
        imagePath={imagePath}
        galleryName={galleryName}
        metadata={metadata}
      />
    );
  }
});