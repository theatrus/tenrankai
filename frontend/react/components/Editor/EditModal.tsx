import React, { useState, useCallback, useEffect } from 'react';
import { MarkdownEditor } from './MarkdownEditor.tsx';

interface EditModalProps {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Modal title */
  modalTitle: string;
  /** The title field value */
  title: string;
  /** The markdown content */
  markdownContent: string;
  /** Placeholder for empty description */
  descriptionPlaceholder?: string;
  /** Called when saving - receives title and markdown */
  onSave: (title: string, markdown: string) => Promise<void>;
  /** Called when canceling */
  onClose: () => void;
}

export const EditModal: React.FC<EditModalProps> = ({
  isOpen,
  modalTitle,
  title: initialTitle,
  markdownContent: initialMarkdown,
  descriptionPlaceholder = 'Add description...',
  onSave,
  onClose,
}) => {
  const [title, setTitle] = useState(initialTitle);
  const [markdown, setMarkdown] = useState(initialMarkdown);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset state when modal opens with new content
  useEffect(() => {
    if (isOpen) {
      setTitle(initialTitle);
      setMarkdown(initialMarkdown);
      setError(null);
    }
  }, [isOpen, initialTitle, initialMarkdown]);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen && !isSaving) {
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, isSaving, onClose]);

  // Prevent body scroll when modal is open
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    setError(null);

    try {
      await onSave(title, markdown);
      onClose();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save';
      setError(message);
    } finally {
      setIsSaving(false);
    }
  }, [title, markdown, onSave, onClose]);

  const handleMarkdownChange = useCallback((newMarkdown: string) => {
    setMarkdown(newMarkdown);
  }, []);

  // Handle Ctrl+S in the modal
  const handleMarkdownSave = useCallback(() => {
    handleSave();
  }, [handleSave]);

  if (!isOpen) {
    return null;
  }

  return (
    <div className="edit-modal-overlay" onClick={onClose}>
      <div className="edit-modal" onClick={(e) => e.stopPropagation()}>
        <div className="edit-modal-header">
          <h2>{modalTitle}</h2>
          <button
            type="button"
            className="edit-modal-close"
            onClick={onClose}
            disabled={isSaving}
            aria-label="Close"
          >
            &times;
          </button>
        </div>

        <div className="edit-modal-body">
          <div className="edit-modal-field">
            <label htmlFor="edit-title">Title</label>
            <input
              id="edit-title"
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Enter title..."
              disabled={isSaving}
              className="edit-modal-input"
            />
          </div>

          <div className="edit-modal-field">
            <label>Description</label>
            <MarkdownEditor
              initialContent={markdown}
              placeholder={descriptionPlaceholder}
              onChange={handleMarkdownChange}
              onSave={handleMarkdownSave}
              onCancel={onClose}
              isSaving={isSaving}
              showActions={false}
              autoFocus={false}
            />
          </div>

          {error && <div className="edit-modal-error">{error}</div>}
        </div>

        <div className="edit-modal-footer">
          <button
            type="button"
            onClick={onClose}
            disabled={isSaving}
            className="edit-modal-btn edit-modal-btn-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={isSaving}
            className="edit-modal-btn edit-modal-btn-save"
          >
            {isSaving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
};

export default EditModal;
