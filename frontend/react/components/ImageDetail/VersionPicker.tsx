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

  // Filter out the currently viewed version
  const otherVersions = versions.filter(v => v.path !== currentPath);

  if (otherVersions.length === 0) {
    return null;
  }

  const handleVersionClick = (version: ImageVersion) => {
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
  const sortedVersions = [...otherVersions].sort((a, b) => {
    const aNum = a.version_number ?? -1;
    const bNum = b.version_number ?? -1;
    return aNum - bNum;
  });

  return (
    <div className="version-picker">
      <span className="version-picker-label">Other versions:</span>
      <div className="version-picker-strip">
        {sortedVersions.map((version) => {
          const thumbnailUrl = version.thumbnail_url;
          const thumbnail2xUrl = thumbnailUrl.replace(/\/thumbnail$/, '/thumbnail@2x');
          const versionLabel = formatVersionLabel(version);
          const versionDate = formatVersionDate(version);

          return (
            <button
              key={version.path}
              className="nav-strip-thumb"
              onClick={() => handleVersionClick(version)}
              title={versionDate ? `${versionLabel} - ${versionDate}` : versionLabel}
              aria-label={`View ${versionLabel}${versionDate ? ` from ${versionDate}` : ''}`}
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
