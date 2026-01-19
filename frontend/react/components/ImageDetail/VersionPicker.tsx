import { ImageVersion } from '../../types/index.ts';

interface VersionPickerProps {
  versions: ImageVersion[];
  currentPath: string;
  galleryUrl: string;
  onVersionSelect?: (version: ImageVersion) => void;
}

export function VersionPicker({
  versions,
  currentPath,
  galleryUrl,
  onVersionSelect
}: VersionPickerProps) {
  if (!versions || versions.length === 0) {
    return null;
  }

  const handleVersionClick = (version: ImageVersion) => {
    if (onVersionSelect) {
      onVersionSelect(version);
    } else {
      window.location.href = `${galleryUrl}/detail/${version.url_id}`;
    }
  };

  const formatVersionLabel = (version: ImageVersion, index: number): string => {
    if (version.version_number !== undefined) {
      return `v${version.version_number}`;
    }
    // Fallback to index-based label
    return `v${index + 1}`;
  };

  const formatVersionDate = (version: ImageVersion): string | null => {
    if (!version.modification_date) return null;
    try {
      const date = new Date(version.modification_date);
      return date.toLocaleDateString(undefined, {
        year: 'numeric',
        month: 'short',
        day: 'numeric'
      });
    } catch {
      return null;
    }
  };

  return (
    <div className="version-picker">
      <div className="version-picker-label">Previous versions:</div>
      <div className="version-picker-strip">
        {versions.map((version, index) => {
          const isCurrentVersion = version.path === currentPath;
          const thumbnailUrl = version.thumbnail_url;
          const thumbnail2xUrl = thumbnailUrl.replace(/\/thumbnail$/, '/thumbnail@2x');
          const versionLabel = formatVersionLabel(version, index);
          const versionDate = formatVersionDate(version);

          return (
            <button
              key={version.path}
              className={`version-thumb ${isCurrentVersion ? 'version-thumb-current' : ''}`}
              onClick={() => handleVersionClick(version)}
              title={versionDate ? `${versionLabel} - ${versionDate}` : versionLabel}
              aria-label={`View ${versionLabel}${versionDate ? ` from ${versionDate}` : ''}`}
              disabled={isCurrentVersion}
            >
              <img
                src={thumbnailUrl}
                srcSet={`${thumbnailUrl} 1x, ${thumbnail2xUrl} 2x`}
                alt={versionLabel}
                loading="lazy"
              />
              <span className="version-label">{versionLabel}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
