import { ImageInfo, RolePermissions } from '../../types/index.ts';
import { ShareButton } from '../ShareModal.tsx';

interface ImageControlsProps {
  image: ImageInfo;
  permissions: RolePermissions;
  onEditClick?: () => void;
  shareUrl?: string;
  baseUrl?: string;
}

export function ImageControls({ image, permissions, onEditClick, shareUrl, baseUrl }: ImageControlsProps) {
  // Build image URLs from medium_url base
  // medium_url is like /gallery/_image/path/medium - replace size suffix for different sizes
  const imageBaseUrl = image.medium_url.replace(/\/medium$/, '');
  const largeUrl = `${imageBaseUrl}/large`;

  const canDownloadOriginal = permissions.can_download_original;
  const canDownloadLarge = permissions.can_download_large || canDownloadOriginal;
  const canDownloadMedium = permissions.can_download_medium || canDownloadLarge;

  // Determine best available download option
  let downloadUrl: string;
  let downloadLabel: string;
  if (canDownloadOriginal) {
    downloadUrl = imageBaseUrl;
    downloadLabel = 'Download Original';
  } else if (canDownloadLarge) {
    downloadUrl = largeUrl;
    downloadLabel = 'Download Large';
  } else {
    downloadUrl = `${imageBaseUrl}/medium`;
    downloadLabel = 'Download';
  }

  // Show edit button when user can edit and there's no title/description
  const showEditButton = permissions.can_edit_content && !image.title && !image.description && onEditClick;

  const fullShareUrl = shareUrl ? `${baseUrl || window.location.origin}${shareUrl}` : undefined;

  if (canDownloadMedium) {
    return (
      <div className="control-buttons">
        <a href={downloadUrl} download={image.name} className="btn btn-primary">
          {downloadLabel}
        </a>
        {fullShareUrl && <ShareButton shareUrl={fullShareUrl} />}
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
      {fullShareUrl && <ShareButton shareUrl={fullShareUrl} />}
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
