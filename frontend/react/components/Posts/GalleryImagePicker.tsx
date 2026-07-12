import React, { useCallback, useEffect, useState } from 'react';

export interface GalleryImageSelection {
  /** gallery:name:path reference for frontmatter / markdown */
  reference: string;
  gallery: string;
  path: string;
  size: string;
  details: boolean;
}

interface PickerItem {
  name: string;
  display_name?: string | null;
  path: string;
  is_directory: boolean;
  thumbnail_url?: string | null;
}

interface PickerData {
  directories: PickerItem[];
  images: PickerItem[];
  page: number;
  total_pages: number;
}

interface GalleryImagePickerProps {
  isOpen: boolean;
  galleries: string[];
  /** Show size + details options (for content embeds); hero picking omits them */
  withOptions?: boolean;
  onClose: () => void;
  onSelect: (selection: GalleryImageSelection) => void;
}

export const GalleryImagePicker: React.FC<GalleryImagePickerProps> = ({
  isOpen,
  galleries,
  withOptions = false,
  onClose,
  onSelect,
}) => {
  const [gallery, setGallery] = useState(galleries[0] || '');
  const [folder, setFolder] = useState('');
  const [page, setPage] = useState(0);
  const [data, setData] = useState<PickerData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [size, setSize] = useState('medium');
  const [details, setDetails] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setGallery(galleries[0] || '');
    setFolder('');
    setPage(0);
    setError(null);
  }, [isOpen, galleries]);

  useEffect(() => {
    if (!isOpen || !gallery) return;
    const url = `/api/gallery/${encodeURIComponent(gallery)}/data${
      folder ? `/${folder}` : ''
    }?page=${page}`;
    setData(null);
    setError(null);
    fetch(url)
      .then((response) => {
        if (!response.ok) throw new Error(`Failed to browse gallery: ${response.status}`);
        return response.json();
      })
      .then((json: PickerData) => setData(json))
      .catch((err) => setError(err instanceof Error ? err.message : 'Failed to browse gallery'));
  }, [isOpen, gallery, folder, page]);

  const navigateTo = useCallback((path: string) => {
    setFolder(path);
    setPage(0);
  }, []);

  const parentFolder = folder.includes('/') ? folder.slice(0, folder.lastIndexOf('/')) : '';

  if (!isOpen) return null;

  return (
    <div className="edit-modal-overlay gallery-picker-overlay" onClick={onClose}>
      <div className="edit-modal gallery-picker" onClick={(e) => e.stopPropagation()}>
        <div className="edit-modal-header">
          <h2>Choose a gallery image</h2>
          <button type="button" className="edit-modal-close" onClick={onClose} aria-label="Close">
            &times;
          </button>
        </div>

        <div className="edit-modal-body">
          <div className="gallery-picker-nav">
            {galleries.length > 1 && (
              <select
                value={gallery}
                onChange={(e) => {
                  setGallery(e.target.value);
                  navigateTo('');
                }}
                className="edit-modal-input gallery-picker-select"
                aria-label="Gallery"
              >
                {galleries.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            )}
            {folder && (
              <button
                type="button"
                className="edit-modal-btn gallery-picker-up"
                onClick={() => navigateTo(parentFolder)}
              >
                ← Up
              </button>
            )}
            <span className="gallery-picker-path">/{folder}</span>
          </div>

          {error && <div className="edit-modal-error">{error}</div>}
          {!data && !error && <div className="gallery-picker-loading">Loading…</div>}

          {data && (
            <div className="gallery-picker-grid">
              {data.directories.map((dir) => (
                <button
                  key={dir.path}
                  type="button"
                  className="gallery-picker-item gallery-picker-folder"
                  onClick={() => navigateTo(dir.path)}
                >
                  <span className="gallery-picker-folder-icon" aria-hidden="true">
                    📁
                  </span>
                  <span className="gallery-picker-name">{dir.display_name || dir.name}</span>
                </button>
              ))}
              {data.images.map((image) => (
                <button
                  key={image.path}
                  type="button"
                  className="gallery-picker-item gallery-picker-image"
                  title={image.name}
                  onClick={() =>
                    onSelect({
                      reference: `gallery:${gallery}:${image.path}`,
                      gallery,
                      path: image.path,
                      size,
                      details,
                    })
                  }
                >
                  {image.thumbnail_url ? (
                    <img src={image.thumbnail_url} alt={image.name} loading="lazy" />
                  ) : (
                    <span className="gallery-picker-name">{image.name}</span>
                  )}
                </button>
              ))}
              {data.directories.length === 0 && data.images.length === 0 && (
                <p className="gallery-picker-empty">This folder is empty.</p>
              )}
            </div>
          )}

          {data && data.total_pages > 1 && (
            <div className="gallery-picker-pages">
              <button
                type="button"
                className="edit-modal-btn"
                disabled={page === 0}
                onClick={() => setPage(page - 1)}
              >
                ← Prev
              </button>
              <span>
                Page {data.page + 1} of {data.total_pages}
              </span>
              <button
                type="button"
                className="edit-modal-btn"
                disabled={page + 1 >= data.total_pages}
                onClick={() => setPage(page + 1)}
              >
                Next →
              </button>
            </div>
          )}
        </div>

        {withOptions && (
          <div className="edit-modal-footer gallery-picker-options">
            <label className="gallery-picker-option">
              Size
              <select
                value={size}
                onChange={(e) => setSize(e.target.value)}
                className="edit-modal-input gallery-picker-select"
              >
                <option value="thumbnail">Thumbnail</option>
                <option value="gallery">Gallery</option>
                <option value="medium">Medium</option>
                <option value="large">Large</option>
              </select>
            </label>
            <label className="gallery-picker-option">
              <input
                type="checkbox"
                checked={details}
                onChange={(e) => setDetails(e.target.checked)}
              />
              Show technical details on hover
            </label>
          </div>
        )}
      </div>
    </div>
  );
};

export default GalleryImagePicker;
