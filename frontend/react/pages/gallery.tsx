import React, { useState, useCallback, useMemo, useEffect } from 'react';
import { createRoot } from 'react-dom/client';
import { GalleryWithFilter } from '@components/Gallery/GalleryWithFilter';
import { EditModal } from '../components/Editor/index.ts';
import { UploadModal, NewDropdown } from '../components/Upload/index.ts';
import { contentEditorApi } from '../api/content-editor.ts';
import { galleryManageApi } from '../api/gallery-manage.ts';
import type { GalleryData, GalleryItem } from '../types/index.ts';

interface GalleryPageProps {
  galleryData: GalleryData;
  images: GalleryItem[];
  galleryUrl: string;
  filterMount: HTMLElement | null;
  toolbarMount: HTMLElement | null;
  manageButton: HTMLElement | null;
}

const GalleryPage: React.FC<GalleryPageProps> = ({
  galleryData,
  images: initialImages,
  galleryUrl,
  filterMount,
  toolbarMount,
  manageButton,
}) => {
  const [images, setImages] = useState(initialImages);
  const [hiddenImages, setHiddenImages] = useState<string[]>(galleryData.hidden_images || []);
  const [isManageMode, setIsManageMode] = useState(false);
  const [selectedImages, setSelectedImages] = useState<Set<string>>(new Set());

  const canManageImages = galleryData.permissions?.can_manage_images;

  // Wire up the manage button click
  useEffect(() => {
    if (!manageButton || !canManageImages) return;

    const handleClick = () => {
      if (isManageMode) {
        setIsManageMode(false);
        setSelectedImages(new Set());
        manageButton.textContent = 'Edit';
        manageButton.classList.remove('active');
      } else {
        setIsManageMode(true);
        manageButton.textContent = 'Cancel';
        manageButton.classList.add('active');
      }
    };

    manageButton.addEventListener('click', handleClick);
    return () => manageButton.removeEventListener('click', handleClick);
  }, [manageButton, canManageImages, isManageMode]);

  const handleToggleSelect = useCallback((path: string) => {
    setSelectedImages((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const handleHideSuccess = useCallback((newHiddenImages: string[]) => {
    setHiddenImages(newHiddenImages);
    setSelectedImages(new Set());
    setIsManageMode(false);
    if (manageButton) {
      manageButton.textContent = 'Edit';
      manageButton.classList.remove('active');
    }
  }, [manageButton]);

  const handleDeleteSuccess = useCallback((deletedPaths: string[]) => {
    setImages((prev) => prev.filter((img) => !deletedPaths.includes(img.path)));
    setSelectedImages(new Set());
    setIsManageMode(false);
    if (manageButton) {
      manageButton.textContent = 'Edit';
      manageButton.classList.remove('active');
    }
  }, [manageButton]);

  const handleCancelManage = useCallback(() => {
    setIsManageMode(false);
    setSelectedImages(new Set());
    if (manageButton) {
      manageButton.textContent = 'Edit';
      manageButton.classList.remove('active');
    }
  }, [manageButton]);

  const handleMoveSuccess = useCallback((movedCount: number) => {
    // Remove moved images from the current view
    setImages((prev) => prev.filter((img) => !selectedImages.has(img.path)));
    setSelectedImages(new Set());
    setIsManageMode(false);
    if (manageButton) {
      manageButton.textContent = 'Edit';
      manageButton.classList.remove('active');
    }
    alert(`Successfully moved ${movedCount} image(s)`);
  }, [manageButton, selectedImages]);

  const handleCopySuccess = useCallback((copiedCount: number) => {
    // Images stay in current view after copy
    setSelectedImages(new Set());
    setIsManageMode(false);
    if (manageButton) {
      manageButton.textContent = 'Edit';
      manageButton.classList.remove('active');
    }
    alert(`Successfully copied ${copiedCount} image(s)`);
  }, [manageButton]);

  return (
    <GalleryWithFilter
      images={images}
      galleryUrl={galleryUrl}
      permissions={galleryData.permissions}
      filterMount={filterMount}
      galleryName={galleryData.gallery_name}
      galleryPath={galleryData.gallery_path}
      hiddenImages={hiddenImages}
      isManageMode={isManageMode}
      selectedImages={selectedImages}
      onToggleSelect={handleToggleSelect}
      onHideSuccess={handleHideSuccess}
      onDeleteSuccess={handleDeleteSuccess}
      onMoveSuccess={handleMoveSuccess}
      onCopySuccess={handleCopySuccess}
      onCancelManage={handleCancelManage}
      toolbarMount={toolbarMount}
      gridMode={galleryData.grid_mode === 'square' ? 'square' : 'masonry'}
      maxColumns={galleryData.max_columns}
    />
  );
};

// Mount React masonry gallery on server-rendered page
document.addEventListener('DOMContentLoaded', () => {
  // Try to use the full gallery data first (includes metadata)
  const galleryDataElement = document.getElementById('gallery-data');
  const imagesDataElement = document.getElementById('gallery-images');

  let galleryData: GalleryData | null = null;
  let images: GalleryItem[] = [];

  // Try to parse the full gallery data
  if (galleryDataElement) {
    try {
      const jsonText = galleryDataElement.textContent || '{}';
      galleryData = JSON.parse(jsonText);
      images = galleryData?.images || [];
    } catch (e) {
      console.error('Failed to parse gallery data:', e);
    }
  }

  // Fall back to legacy images-only data if needed
  if (!galleryData && imagesDataElement) {
    try {
      const jsonText = imagesDataElement.textContent || '[]';
      images = JSON.parse(jsonText);
    } catch (e) {
      console.error('Failed to parse gallery images data:', e);
    }
  }

  // Always mount folder description editor (works even without images)
  mountFolderDescriptionEditor(galleryData);

  // Always set up the "+ New" dropdown (replaces old "+ Folder" button)
  mountNewDropdown(galleryData);

  // Always set up delete folder button (only shows for empty folders)
  mountDeleteFolderButton();

  // Only mount gallery grid if there are images
  if (!images.length) {
    return;
  }

  // Find the gallery URL from the page
  const galleryUrlElement = document.querySelector('[data-gallery-url]');
  const galleryUrl = galleryData?.gallery_path
    ? galleryUrlElement?.getAttribute('data-gallery-url') || '/gallery'
    : '/gallery';

  // Find the container for React - use the parent gallery-images div
  const galleryImages = document.querySelector('.gallery-images');
  if (!galleryImages) {
    console.error('Gallery images container not found');
    return;
  }

  // Find mount points
  const filterMount = document.getElementById('gallery-filter-mount');
  const manageButton = document.getElementById('manage-images-btn');

  // Create toolbar mount point if owner access
  let toolbarMount: HTMLElement | null = null;
  if (galleryData?.permissions?.owner_access) {
    toolbarMount = document.getElementById('manage-toolbar-mount');
    if (!toolbarMount) {
      toolbarMount = document.createElement('div');
      toolbarMount.id = 'manage-toolbar-mount';
      document.body.appendChild(toolbarMount);
    }
  }

  // Clear existing content (remove the static grid)
  galleryImages.innerHTML = '';

  // Mount React component
  const root = createRoot(galleryImages);
  root.render(
    <GalleryPage
      galleryData={galleryData!}
      images={images}
      galleryUrl={galleryUrl}
      filterMount={filterMount}
      toolbarMount={toolbarMount}
      manageButton={manageButton}
    />
  );
});

interface CreateFolderModalProps {
  galleryName: string;
  parentFolder: string;
  onSuccess: () => void;
  onClose: () => void;
}

const CreateFolderModal: React.FC<CreateFolderModalProps> = ({
  galleryName,
  parentFolder,
  onSuccess,
  onClose,
}) => {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setIsCreating(true);
    setError(null);
    try {
      await galleryManageApi.createFolder(galleryName, parentFolder, {
        name: name.trim(),
        description: description.trim() || undefined,
      });
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create folder');
      setIsCreating(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ textAlign: 'left', maxWidth: '400px' }}>
        <h3 style={{ marginBottom: '1rem' }}>Create Folder</h3>
        <form onSubmit={handleSubmit}>
          <div className="form-group" style={{ marginBottom: '1rem' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 'bold' }}>
              Parent Folder
            </label>
            <input
              type="text"
              value={parentFolder || '(root)'}
              disabled
              style={{
                width: '100%',
                padding: '8px',
                border: '1px solid #ddd',
                borderRadius: '4px',
                background: '#f5f5f5',
                boxSizing: 'border-box'
              }}
            />
          </div>
          <div className="form-group" style={{ marginBottom: '1rem' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 'bold' }}>
              Folder Name *
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="new-folder"
              pattern="[a-zA-Z0-9_\- ]+"
              title="Only letters, numbers, hyphens, underscores, and spaces"
              required
              autoFocus
              style={{
                width: '100%',
                padding: '8px',
                border: '1px solid #ddd',
                borderRadius: '4px',
                boxSizing: 'border-box'
              }}
            />
            <small style={{ color: '#666', fontSize: '12px' }}>
              Letters, numbers, hyphens, underscores, and spaces only
            </small>
          </div>
          <div className="form-group" style={{ marginBottom: '1rem' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 'bold' }}>
              Description (optional)
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              placeholder="Optional description for this folder..."
              style={{
                width: '100%',
                padding: '8px',
                border: '1px solid #ddd',
                borderRadius: '4px',
                resize: 'vertical',
                boxSizing: 'border-box'
              }}
            />
          </div>
          {error && (
            <div style={{
              color: '#dc3545',
              background: '#ffe0e0',
              padding: '8px',
              borderRadius: '4px',
              marginBottom: '1rem'
            }}>
              {error}
            </div>
          )}
          <div className="modal-actions">
            <button
              type="submit"
              className="btn btn-primary"
              disabled={isCreating || !name.trim()}
            >
              {isCreating ? 'Creating...' : 'Create Folder'}
            </button>
            <button type="button" className="btn btn-secondary" onClick={onClose}>
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

interface FolderEditorProps {
  galleryName: string;
  galleryPath: string;
  folderTitle: string;
  folderDescription: string;
  markdownContent: string;
  canEdit: boolean;
}

const FolderEditor: React.FC<FolderEditorProps> = ({
  galleryName,
  galleryPath,
  folderTitle,
  folderDescription,
  markdownContent,
  canEdit,
}) => {
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Extract description without title line for editing
  // The full markdown includes "# Title\n\n..." - we show title separately
  const descriptionWithoutTitle = useMemo(() => {
    if (!markdownContent) return '';
    const lines = markdownContent.split('\n');
    const firstLine = lines[0]?.trim() || '';
    if (firstLine.startsWith('# ')) {
      // Skip title line and any following blank lines
      return lines
        .slice(1)
        .join('\n')
        .trim();
    }
    return markdownContent;
  }, [markdownContent]);

  const handleSave = useCallback(async (title: string, markdown: string) => {
    await contentEditorApi.updateFolderDescription(
      galleryName,
      galleryPath,
      markdown,
      title || undefined
    );
    // Reload page to show updated content
    window.location.reload();
  }, [galleryName, galleryPath]);

  const hasContent = folderDescription.trim().length > 0;
  const hasTitle = folderTitle.trim().length > 0;

  return (
    <>
      {/* Title with edit icon */}
      {(hasTitle || canEdit) && (
        <div className="folder-title-row">
          {hasTitle && (
            <h2 className="folder-title">{folderTitle}</h2>
          )}
          {canEdit && (
            <button
              type="button"
              className="folder-edit-icon"
              onClick={() => setIsModalOpen(true)}
              title="Edit folder"
              aria-label="Edit folder"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/>
              </svg>
            </button>
          )}
        </div>
      )}

      {/* Display description content */}
      {hasContent && (
        <div
          className="folder-description"
          dangerouslySetInnerHTML={{ __html: folderDescription }}
        />
      )}

      {/* Edit modal */}
      <EditModal
        isOpen={isModalOpen}
        modalTitle="Edit Folder"
        title={folderTitle}
        markdownContent={descriptionWithoutTitle}
        descriptionPlaceholder="Add folder description..."
        onSave={handleSave}
        onClose={() => setIsModalOpen(false)}
      />
    </>
  );
};

/**
 * Mount the folder description editor
 * Uses gallery-data as the single source of truth for folder metadata
 */
function mountFolderDescriptionEditor(galleryData: GalleryData | null) {
  const descriptionMount = document.getElementById('folder-description-mount');
  if (!descriptionMount) return;

  const canEdit = descriptionMount.getAttribute('data-can-edit') === 'true';
  const galleryName = descriptionMount.getAttribute('data-gallery-name') || '';
  const galleryPath = descriptionMount.getAttribute('data-gallery-path') || '';

  // Get the existing content from the server-rendered HTML
  const existingDescriptionEl = descriptionMount.querySelector('.folder-description');
  const htmlContent = existingDescriptionEl?.innerHTML || '';

  // Use gallery data as the single source for markdown and title
  const markdownContent = galleryData?.folder_description_markdown || '';
  const folderTitle = galleryData?.folder_title || descriptionMount.getAttribute('data-folder-title') || '';

  // Only mount if user can edit OR there's content to display (title or description)
  if (!canEdit && !htmlContent && !folderTitle) {
    return;
  }

  // Clear the mount point
  descriptionMount.innerHTML = '';

  // Mount the editor component
  const root = createRoot(descriptionMount);
  root.render(
    <FolderEditor
      galleryName={galleryName}
      galleryPath={galleryPath}
      folderTitle={folderTitle}
      folderDescription={htmlContent}
      markdownContent={markdownContent}
      canEdit={canEdit}
    />
  );
}

/**
 * Mount the "New" dropdown with Upload and New Folder options
 * This replaces the old "+ Folder" button with a dropdown
 */
function mountNewDropdown(galleryData: GalleryData | null) {
  // Look for either the new mount point or fall back to old button
  let dropdownMount = document.getElementById('new-dropdown-mount');
  const legacyButton = document.getElementById('create-folder-btn');

  if (!galleryData) return;

  // If there's no mount point but there's a legacy button, replace it
  if (!dropdownMount && legacyButton) {
    dropdownMount = document.createElement('div');
    dropdownMount.id = 'new-dropdown-mount';
    dropdownMount.style.display = 'inline-block';
    legacyButton.parentNode?.replaceChild(dropdownMount, legacyButton);
  }

  if (!dropdownMount) return;

  const galleryName = galleryData.gallery_name;
  const galleryPath = galleryData.gallery_path;

  // Create a stateful wrapper component
  const NewDropdownWrapper: React.FC = () => {
    const [showUploadModal, setShowUploadModal] = useState(false);
    const [showFolderModal, setShowFolderModal] = useState(false);

    const handleUploadSuccess = useCallback(() => {
      setShowUploadModal(false);
      window.location.reload();
    }, []);

    const handleFolderSuccess = useCallback(() => {
      setShowFolderModal(false);
      window.location.reload();
    }, []);

    return (
      <>
        <NewDropdown
          onUpload={() => setShowUploadModal(true)}
          onNewFolder={() => setShowFolderModal(true)}
        />
        {showUploadModal && (
          <UploadModal
            galleryName={galleryName}
            folderPath={galleryPath}
            onClose={() => setShowUploadModal(false)}
            onSuccess={handleUploadSuccess}
          />
        )}
        {showFolderModal && (
          <CreateFolderModal
            galleryName={galleryName}
            parentFolder={galleryPath}
            onSuccess={handleFolderSuccess}
            onClose={() => setShowFolderModal(false)}
          />
        )}
      </>
    );
  };

  // Mount the wrapper
  const root = createRoot(dropdownMount);
  root.render(<NewDropdownWrapper />);
}

/**
 * Mount the delete folder button handler
 * This works for empty folders where GalleryPage isn't mounted
 */
function mountDeleteFolderButton() {
  const deleteFolderButton = document.getElementById('delete-folder-btn');
  if (!deleteFolderButton) return;

  const galleryName = deleteFolderButton.getAttribute('data-gallery-name') || '';
  const folderPath = deleteFolderButton.getAttribute('data-folder-path') || '';
  const folderName = deleteFolderButton.getAttribute('data-folder-name') || folderPath;

  // Create a mount point for the modal
  let modalMount = document.getElementById('delete-folder-modal-mount');
  if (!modalMount) {
    modalMount = document.createElement('div');
    modalMount.id = 'delete-folder-modal-mount';
    document.body.appendChild(modalMount);
  }

  // Store reference to allow triggering modal from button click
  let showDeleteModal: (() => void) | null = null;

  // Create a stateful wrapper component
  const DeleteFolderWrapper: React.FC = () => {
    const [showModal, setShowModal] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Expose the trigger function
    useEffect(() => {
      showDeleteModal = () => setShowModal(true);
      return () => {
        showDeleteModal = null;
      };
    }, []);

    const handleDelete = async () => {
      setIsDeleting(true);
      setError(null);
      try {
        await galleryManageApi.deleteFolder(galleryName, folderPath);
        // Navigate to parent folder
        const parentPath = folderPath.includes('/')
          ? folderPath.substring(0, folderPath.lastIndexOf('/'))
          : '';
        const galleryUrlEl = document.querySelector('[data-gallery-url]');
        const galleryUrl = galleryUrlEl?.getAttribute('data-gallery-url') || '/gallery';
        window.location.href = parentPath ? `${galleryUrl}/${parentPath}` : galleryUrl;
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to delete folder');
        setIsDeleting(false);
      }
    };

    if (!showModal) return null;

    return (
      <div className="modal-overlay" onClick={() => !isDeleting && setShowModal(false)}>
        <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ textAlign: 'left', maxWidth: '400px' }}>
          <h3 style={{ marginBottom: '1rem', color: '#333' }}>Delete Folder</h3>
          <p style={{ marginBottom: '1rem', color: '#333' }}>
            Are you sure you want to delete <strong>{folderName}</strong>?
          </p>
          <p style={{ color: '#dc3545', fontSize: '14px', marginBottom: '1rem' }}>
            This action cannot be undone.
          </p>
          {error && (
            <div style={{
              color: '#dc3545',
              background: '#ffe0e0',
              padding: '8px',
              borderRadius: '4px',
              marginBottom: '1rem'
            }}>
              {error}
            </div>
          )}
          <div className="modal-actions">
            <button
              className="btn btn-secondary"
              onClick={() => setShowModal(false)}
              disabled={isDeleting}
            >
              Cancel
            </button>
            <button
              className="btn btn-danger"
              onClick={handleDelete}
              disabled={isDeleting}
            >
              {isDeleting ? 'Deleting...' : 'Delete'}
            </button>
          </div>
        </div>
      </div>
    );
  };

  // Mount the wrapper
  const root = createRoot(modalMount);
  root.render(<DeleteFolderWrapper />);

  // Wire up the button click
  deleteFolderButton.addEventListener('click', () => {
    if (showDeleteModal) {
      showDeleteModal();
    }
  });
}
