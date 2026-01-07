import { useState, useEffect } from 'react';
import { ImageInfo } from '../../types/index.ts';
import { ImageDetailApiClient } from '../../api/image-detail.ts';

interface ImageControlsProps {
  image: ImageInfo;
}

export function ImageControls({ image }: ImageControlsProps) {
  const [hasDownloadPermission, setHasDownloadPermission] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const checkPermission = async () => {
      const apiClient = new ImageDetailApiClient();
      try {
        const hasPermission = await apiClient.checkDownloadPermission();
        setHasDownloadPermission(hasPermission);
      } catch (error) {
        console.warn('Failed to check download permission:', error);
        setHasDownloadPermission(false);
      } finally {
        setLoading(false);
      }
    };

    checkPermission();
  }, []);

  if (loading) {
    return (
      <div className="control-buttons">
        <div className="loading-text">Checking permissions...</div>
      </div>
    );
  }

  if (hasDownloadPermission === null) {
    return null;
  }

  const fullSizeUrl = image.medium_url.replace('?size=medium', '');

  if (hasDownloadPermission) {
    return (
      <div className="control-buttons">
        <a href={fullSizeUrl} target="_blank" rel="noopener noreferrer" className="btn">
          View Full Size
        </a>
        <a href={fullSizeUrl} download={image.name} className="btn">
          Download
        </a>
      </div>
    );
  }

  return (
    <div className="control-buttons">
      <a href={image.medium_url} target="_blank" rel="noopener noreferrer" className="btn">
        View Medium Size
      </a>
      <button 
        className="btn" 
        onClick={() => window.location.href = '/_login'}
      >
        Request Download Access
      </button>
    </div>
  );
}