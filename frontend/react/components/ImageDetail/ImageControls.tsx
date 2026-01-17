import { ImageInfo, RolePermissions } from '../../types/index.ts';

interface ImageControlsProps {
  image: ImageInfo;
  permissions: RolePermissions;
  onEditClick?: () => void;
}

export function ImageControls({ image, permissions, onEditClick }: ImageControlsProps) {
  // Build image URLs from medium_url base
  // medium_url is like /gallery/_image/path/medium - replace size suffix for different sizes
  const baseUrl = image.medium_url.replace(/\/medium$/, '');
  const largeUrl = `${baseUrl}/large`;

  const canDownloadLarge = permissions.can_download_large;
  const canDownloadMedium = permissions.can_download_medium || canDownloadLarge;

  // Determine best available download option
  const downloadUrl = canDownloadLarge ? largeUrl : `${baseUrl}/medium`;
  const downloadLabel = canDownloadLarge ? 'Download Large' : 'Download';

  // Show edit button when user can edit and there's no title/description
  const showEditButton = permissions.can_edit_content && !image.title && !image.description && onEditClick;

  if (canDownloadMedium) {
    return (
      <div className="control-buttons">
        <a href={downloadUrl} download={image.name} className="btn btn-primary">
          {downloadLabel}
        </a>
        {showEditButton && (
          <button className="btn btn-secondary" onClick={onEditClick}>
            Add Title/Description
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="control-buttons">
      {showEditButton ? (
        <button className="btn btn-secondary" onClick={onEditClick}>
          Add Title/Description
        </button>
      ) : (
        <button
          className="btn"
          onClick={() => window.location.href = '/_login'}
        >
          Request Download Access
        </button>
      )}
    </div>
  );
}