import React, { useState } from 'react';
import { galleryManageApi } from '../../api/gallery-manage';

interface ManageToolbarProps {
  galleryName: string;
  galleryPath: string;
  selectedImages: Set<string>;
  onHideSuccess: (hiddenImages: string[]) => void;
  onDeleteSuccess: (deletedPaths: string[]) => void;
  onCancel: () => void;
}

export const ManageToolbar: React.FC<ManageToolbarProps> = ({
  galleryName,
  galleryPath,
  selectedImages,
  onHideSuccess,
  onDeleteSuccess,
  onCancel,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [isHiding, setIsHiding] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);

  const count = selectedImages.size;
  const paths = Array.from(selectedImages);

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

  return (
    <>
      <div className="manage-toolbar">
        <span className="manage-count">{count} selected</span>
        <button
          className="btn btn-warning"
          disabled={count === 0 || isHiding}
          onClick={() => handleHide(true)}
        >
          {isHiding ? 'Hiding...' : 'Hide Selected'}
        </button>
        <button
          className="btn btn-success"
          disabled={count === 0 || isHiding}
          onClick={() => handleHide(false)}
        >
          {isHiding ? 'Unhiding...' : 'Unhide Selected'}
        </button>
        <button
          className="btn btn-danger"
          disabled={count === 0 || isDeleting}
          onClick={() => setShowDeleteModal(true)}
        >
          {isDeleting ? 'Deleting...' : 'Delete Selected'}
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
    </>
  );
};
