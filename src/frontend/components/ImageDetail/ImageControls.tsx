import { ImageInfo, RolePermissions } from '../../types/index.ts';

interface ImageControlsProps {
  image: ImageInfo;
  permissions: RolePermissions;
}

export function ImageControls({ image, permissions }: ImageControlsProps) {
  const fullSizeUrl = image.medium_url.replace('?size=medium', '');
  
  // Check if user can download large images
  const canDownloadLarge = permissions.can_download_large || permissions.can_download_original;

  if (canDownloadLarge) {
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
      <button 
        className="btn" 
        onClick={() => window.location.href = '/_login'}
      >
        Request Download Access
      </button>
    </div>
  );
}