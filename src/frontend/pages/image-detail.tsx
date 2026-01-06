import { createRoot } from 'react-dom/client';

interface ImageDetailProps {
  imagePath: string;
  galleryName: string;
  imageName: string;
  imageTitle: string;
}

function ImageDetailApp({ imagePath, galleryName, imageName, imageTitle }: ImageDetailProps) {
  return (
    <div className="react-image-detail">
      <h2>Enhanced Image Detail (React)</h2>
      <p><strong>Image:</strong> {imagePath}</p>
      <p><strong>Gallery:</strong> {galleryName}</p>
      <p><strong>Name:</strong> {imageName}</p>
      <p><strong>Title:</strong> {imageTitle}</p>
      <p style={{ color: '#28a745', fontWeight: 'bold' }}>
        ✅ React component successfully mounted!
      </p>
      <p>This will be enhanced with rich interactivity like zoom, pan, and navigation...</p>
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
    const imageName = container.getAttribute('data-image-name') || '';
    const imageTitle = container.getAttribute('data-image-title') || '';
    
    console.log('React component mounting with data:', {
      imagePath,
      galleryName, 
      imageName,
      imageTitle
    });

    const root = createRoot(container);
    root.render(
      <ImageDetailApp 
        imagePath={imagePath}
        galleryName={galleryName}
        imageName={imageName}
        imageTitle={imageTitle}
      />
    );
  }
});