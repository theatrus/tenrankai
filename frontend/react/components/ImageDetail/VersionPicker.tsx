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

  // Need at least 2 versions to show the picker (current + at least one other)
  if (versions.length < 2) {
    return null;
  }

  const handleVersionClick = (version: ImageVersion) => {
    // Don't navigate if clicking the current version
    if (version.path === currentPath) {
      return;
    }
    if (onVersionSelect) {
      onVersionSelect(version);
    } else {
      window.location.href = `${galleryUrl}/detail/${version.url_id}`;
    }
  };

  const formatVersionLabel = (version: ImageVersion): string => {
    // Check for null, undefined, or missing version_number
    if (version.version_number != null) {
      return `v${version.version_number}`;
    }
    // No version number means it's the original/base file
    return 'Original';
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

  // Sort versions: original first, then by version number
  const sortedVersions = [...versions].sort((a, b) => {
    const aNum = a.version_number ?? -1;
    const bNum = b.version_number ?? -1;
    return aNum - bNum;
  });

  return (
    <div className="version-picker">
      <span className="version-picker-label">Versions:</span>
      <div className="version-picker-strip">
        {sortedVersions.map((version) => {
          const thumbnailUrl = version.thumbnail_url;
          const thumbnail2xUrl = thumbnailUrl.replace(/\/thumbnail$/, '/thumbnail@2x');
          const versionLabel = formatVersionLabel(version);
          const versionDate = formatVersionDate(version);
          const isCurrent = version.path === currentPath;

          return (
            <button
              key={version.path}
              className={`nav-strip-thumb${isCurrent ? ' nav-strip-thumb-current' : ''}`}
              onClick={() => handleVersionClick(version)}
              title={versionDate ? `${versionLabel} - ${versionDate}` : versionLabel}
              aria-label={isCurrent ? `Current version: ${versionLabel}` : `View ${versionLabel}${versionDate ? ` from ${versionDate}` : ''}`}
              aria-current={isCurrent ? 'true' : undefined}
            >
              <img
                src={thumbnailUrl}
                srcSet={`${thumbnailUrl} 1x, ${thumbnail2xUrl} 2x`}
                alt={versionLabel}
                loading="lazy"
              />
            </button>
          );
        })}
      </div>
    </div>
  );
}
