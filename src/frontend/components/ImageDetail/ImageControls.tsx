import { ImageInfo, RolePermissions } from '../../types/index.ts';

interface ImageControlsProps {
  image: ImageInfo;
  permissions: RolePermissions;
}

export function ImageControls({ image, permissions }: ImageControlsProps) {
  // Build image URLs from medium_url base
  // medium_url is like /gallery/_image/path/medium - replace size suffix for different sizes
  const baseUrl = image.medium_url.replace(/\/medium$/, '');
  const largeUrl = `${baseUrl}/large`;
  const originalUrl = baseUrl; // No size suffix = original

  const canDownloadOriginal = permissions.can_download_original;
  const canDownloadLarge = permissions.can_download_large || canDownloadOriginal;
  const canDownloadMedium = permissions.can_download_medium || canDownloadLarge;

  // Determine best available download option
  const downloadUrl = canDownloadOriginal ? originalUrl : canDownloadLarge ? largeUrl : `${baseUrl}/medium`;
  const downloadLabel = canDownloadOriginal ? 'Download Original' : canDownloadLarge ? 'Download Large' : 'Download';

  if (canDownloadMedium) {
    return (
      <div className="control-buttons">
        {canDownloadLarge && (
          <a href={largeUrl} target="_blank" rel="noopener noreferrer" className="btn">
            View Full Size
          </a>
        )}
        <a href={downloadUrl} download={image.name} className="btn btn-primary">
          {downloadLabel}
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