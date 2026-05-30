import React, { useState, useEffect, useMemo } from 'react';
import { galleryManageApi, FolderInfo } from '../../api/gallery-manage';
import { SortOrderControl } from './SortOrderControl';
import type { SortOrder, SortDirection } from '../../types';

function formatFolderHierarchy(folders: FolderInfo[], excludePath?: string): { path: string; label: string; imageCount: number }[] {
  const filtered = folders.filter((f) => f.path !== excludePath);
  const sorted = [...filtered].sort((a, b) => a.path.localeCompare(b.path));

  return sorted.map((folder) => {
    if (!folder.path) {
      return { path: '', label: '(root)', imageCount: folder.image_count };
    }

    const segments = folder.path.split('/');
    const depth = segments.length - 1;
    const name = segments[segments.length - 1];
    const indent = depth > 0 ? '\u00A0\u00A0'.repeat(depth) + '└ ' : '';

    return {
      path: folder.path,
      label: `${indent}${name}`,
      imageCount: folder.image_count,
    };
  });
}

interface ManageToolbarProps {
  galleryName: string;
  galleryPath: string;
  selectedImages: Set<string>;
  onHideSuccess: (hiddenImages: string[]) => void;
  onDeleteSuccess: (deletedPaths: string[]) => void;
  onMoveSuccess: (movedCount: number) => void;
  onCopySuccess: (copiedCount: number) => void;
  onCancel: () => void;
  sortOrder?: SortOrder;
  sortDirection?: SortDirection;
  images?: { path: string; name: string; thumbnail_url?: string }[];
  onSortChanged?: () => void;
}

export const ManageToolbar: React.FC<ManageToolbarProps> = ({
  galleryName,
  galleryPath,
  selectedImages,
  onHideSuccess,
  onDeleteSuccess,
  onMoveSuccess,
  onCopySuccess,
  onCancel,
  sortOrder,
  sortDirection,
  images: imagesList,
  onSortChanged,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [isHiding, setIsHiding] = useState(false);
  const [isMoving, setIsMoving] = useState(false);
  const [isCopying, setIsCopying] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [showFolderPicker, setShowFolderPicker] = useState<'move' | 'copy' | null>(null);
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [selectedFolder, setSelectedFolder] = useState<string>('');
  const [loadingFolders, setLoadingFolders] = useState(false);

  const count = selectedImages.size;
  const paths = Array.from(selectedImages);

  const hierarchicalFolders = useMemo(
    () => formatFolderHierarchy(folders, galleryPath),
    [folders, galleryPath]
  );

  useEffect(() => {
    if (showFolderPicker && folders.length === 0) {
      setLoadingFolders(true);
      galleryManageApi.listFolders('default', galleryName)
        .then((result) => {
          setFolders(result.folders);
          setLoadingFolders(false);
        })
        .catch((err) => {
          alert(`Failed to load folders: ${err instanceof Error ? err.message : 'Unknown error'}`);
          setShowFolderPicker(null);
          setLoadingFolders(false);
        });
    }
  }, [showFolderPicker, folders.length, galleryName]);

  const handleHide = async (hide: boolean) => {
    setIsHiding(true);
    try {
      const result = await galleryManageApi.hideImages(galleryName, galleryPath, paths, hide);
      if (result.success) {
        onHideSuccess(result.hidden_images);
      }
    } catch (err) {
      alert(`Failed to ${hide ? 'hide' : 'unhide'} images: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setIsHiding(false);
    }
  };

  const handleDelete = async () => {
    setShowDeleteModal(false);
    setIsDeleting(true);
    try {
      const result = await galleryManageApi.deleteImages(galleryName, paths);
      if (result.deleted_count > 0) {
        onDeleteSuccess(paths.filter((_, i) => i < result.deleted_count));
      }
      if (result.errors && result.errors.length > 0) {
        alert('Some images could not be deleted:\n' + result.errors.join('\n'));
      }
    } catch (err) {
      alert(`Failed to delete images: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setIsDeleting(false);
    }
  };

  const handleMove = async () => {
    if (!selectedFolder && selectedFolder !== '') return;
    setIsMoving(true);
    try {
      const result = await galleryManageApi.moveImages(galleryName, galleryPath, paths, selectedFolder);
      if (result.moved_count > 0) {
        onMoveSuccess(result.moved_count);
      }
      if (result.errors.length > 0) {
        alert('Some images could not be moved:\n' + result.errors.join('\n'));
      }
      setShowFolderPicker(null);
      setSelectedFolder('');
    } catch (err) {
      alert(`Failed to move images: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setIsMoving(false);
    }
  };

  const handleCopy = async () => {
    if (!selectedFolder && selectedFolder !== '') return;
    setIsCopying(true);
    try {
      const result = await galleryManageApi.copyImages(galleryName, galleryPath, paths, selectedFolder);
      if (result.copied_count > 0) {
        onCopySuccess(result.copied_count);
      }
      if (result.errors.length > 0) {
        alert('Some images could not be copied:\n' + result.errors.join('\n'));
      }
      setShowFolderPicker(null);
      setSelectedFolder('');
    } catch (err) {
      alert(`Failed to copy images: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setIsCopying(false);
    }
  };

  return (
    <>
      {sortOrder && sortDirection && imagesList && onSortChanged && (
        <SortOrderControl
          galleryName={galleryName}
          folderPath={galleryPath}
          currentSortOrder={sortOrder}
          currentSortDirection={sortDirection}
          images={imagesList}
          onSortChanged={onSortChanged}
        />
      )}
      <div className="manage-toolbar">
        <span className="manage-count">{count} selected</span>
        <button
          className="btn btn-primary"
          disabled={count === 0 || isMoving}
          onClick={() => setShowFolderPicker('move')}
        >
          Move
        </button>
        <button
          className="btn btn-primary"
          disabled={count === 0 || isCopying}
          onClick={() => setShowFolderPicker('copy')}
        >
          Copy
        </button>
        <button
          className="btn btn-warning"
          disabled={count === 0 || isHiding}
          onClick={() => handleHide(true)}
        >
          {isHiding ? 'Hiding...' : 'Hide'}
        </button>
        <button
          className="btn btn-success"
          disabled={count === 0 || isHiding}
          onClick={() => handleHide(false)}
        >
          {isHiding ? 'Unhiding...' : 'Unhide'}
        </button>
        <button
          className="btn btn-danger"
          disabled={count === 0 || isDeleting}
          onClick={() => setShowDeleteModal(true)}
        >
          {isDeleting ? 'Deleting...' : 'Delete'}
        </button>
        <button className="btn btn-secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>

      {showDeleteModal && (
        <div className="modal-overlay" onClick={() => setShowDeleteModal(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <h3>Confirm Deletion</h3>
            <p>Are you sure you want to delete {count} image{count > 1 ? 's' : ''}?</p>
            <p className="warning-text">This action cannot be undone.</p>
            <div className="modal-actions">
              <button className="btn btn-danger" onClick={handleDelete}>
                Delete
              </button>
              <button className="btn btn-secondary" onClick={() => setShowDeleteModal(false)}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {showFolderPicker && (
        <div className="modal-overlay" onClick={() => { setShowFolderPicker(null); setSelectedFolder(''); }}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <h3>{showFolderPicker === 'move' ? 'Move' : 'Copy'} {count} image{count > 1 ? 's' : ''}</h3>
            {loadingFolders ? (
              <p>Loading folders...</p>
            ) : (
              <>
                <div className="form-group">
                  <label>Select destination folder:</label>
                  <select
                    className="form-select"
                    value={selectedFolder}
                    onChange={(e) => setSelectedFolder(e.target.value)}
                    style={{ fontFamily: 'monospace' }}
                  >
                    <option value="">Choose a folder...</option>
                    {hierarchicalFolders.map((f) => (
                      <option key={f.path} value={f.path}>
                        {f.label} ({f.imageCount} images)
                      </option>
                    ))}
                  </select>
                </div>
                <div className="modal-actions">
                  <button
                    className="btn btn-primary"
                    disabled={(selectedFolder === '' && selectedFolder !== galleryPath) || isMoving || isCopying}
                    onClick={showFolderPicker === 'move' ? handleMove : handleCopy}
                  >
                    {isMoving || isCopying ? 'Processing...' : showFolderPicker === 'move' ? 'Move Here' : 'Copy Here'}
                  </button>
                  <button className="btn btn-secondary" onClick={() => { setShowFolderPicker(null); setSelectedFolder(''); }}>
                    Cancel
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
};
