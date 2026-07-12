import React, { useState, useEffect, useCallback } from 'react';
import { MarkdownEditor } from '../Editor/MarkdownEditor.tsx';
import { GalleryImagePicker } from './GalleryImagePicker.tsx';
import { postsApi, PostSource } from '../../api/posts.ts';

interface PostEditorModalProps {
  isOpen: boolean;
  postsName: string;
  /** Existing post source when editing; null when creating a new post */
  source: PostSource | null;
  /** Gallery names available for image picking */
  galleries?: string[];
  onClose: () => void;
  /** Called with the post URL after a successful save */
  onSaved: (url: string) => void;
  /** Called after a successful delete (edit mode only) */
  onDeleted?: () => void;
}

function slugFromTitle(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export const PostEditorModal: React.FC<PostEditorModalProps> = ({
  isOpen,
  postsName,
  source,
  galleries,
  onClose,
  onSaved,
  onDeleted,
}) => {
  const isNew = source === null;
  const [heroPickerOpen, setHeroPickerOpen] = useState(false);

  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [slugTouched, setSlugTouched] = useState(false);
  const [summary, setSummary] = useState('');
  const [categories, setCategories] = useState('');
  const [heroImage, setHeroImage] = useState('');
  const [content, setContent] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setTitle(source?.title || '');
    setSlug(source?.slug || '');
    setSlugTouched(!!source);
    setSummary(source?.summary || '');
    setCategories(source?.categories.join(', ') || '');
    setHeroImage(source?.hero_image || '');
    setContent(source?.content || '');
    setConfirmingDelete(false);
    setError(null);
  }, [isOpen, source]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen && !isSaving) onClose();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, isSaving, onClose]);

  useEffect(() => {
    document.body.style.overflow = isOpen ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const handleTitleChange = (value: string) => {
    setTitle(value);
    if (isNew && !slugTouched) {
      setSlug(slugFromTitle(value));
    }
  };

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    setError(null);

    const post = {
      title: title.trim(),
      summary: summary.trim(),
      categories: categories
        .split(',')
        .map((c) => c.trim())
        .filter(Boolean),
      hero_image: heroImage.trim() || undefined,
      content,
    };

    try {
      const response = isNew
        ? await postsApi.createPost(postsName, slug.trim(), post)
        : await postsApi.updatePost(postsName, source!.slug, post);
      onSaved(response.url);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save post');
      setIsSaving(false);
    }
  }, [isNew, postsName, slug, source, title, summary, categories, heroImage, content, onSaved]);

  const handleDelete = useCallback(async () => {
    if (!source || !onDeleted) return;
    setIsSaving(true);
    setError(null);
    try {
      await postsApi.deletePost(postsName, source.slug);
      onDeleted();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete post');
      setIsSaving(false);
      setConfirmingDelete(false);
    }
  }, [postsName, source, onDeleted]);

  if (!isOpen) return null;

  const canSave = title.trim() && summary.trim() && (!isNew || slug.trim());

  return (
    <div className="edit-modal-overlay" onClick={() => !isSaving && onClose()}>
      <div className="edit-modal post-editor-modal" onClick={(e) => e.stopPropagation()}>
        <div className="edit-modal-header">
          <h2>{isNew ? 'New Post' : 'Edit Post'}</h2>
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
            <label htmlFor="post-title">Title</label>
            <input
              id="post-title"
              type="text"
              value={title}
              onChange={(e) => handleTitleChange(e.target.value)}
              placeholder="Post title..."
              disabled={isSaving}
              className="edit-modal-input"
              autoFocus={isNew}
            />
          </div>

          {isNew && (
            <div className="edit-modal-field">
              <label htmlFor="post-slug">Slug</label>
              <input
                id="post-slug"
                type="text"
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugTouched(true);
                }}
                placeholder="my-new-post"
                disabled={isSaving}
                className="edit-modal-input"
              />
              <small className="post-editor-hint">
                Letters, digits, hyphens; use / for subdirectories. Becomes the post URL.
              </small>
            </div>
          )}

          <div className="edit-modal-field">
            <label htmlFor="post-summary">Summary</label>
            <textarea
              id="post-summary"
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              placeholder="A short summary shown on the index and in social previews..."
              disabled={isSaving}
              className="edit-modal-input post-editor-summary"
              rows={2}
            />
          </div>

          <div className="edit-modal-field">
            <label htmlFor="post-categories">Categories</label>
            <input
              id="post-categories"
              type="text"
              value={categories}
              onChange={(e) => setCategories(e.target.value)}
              placeholder="Travel, Photo Gear (comma separated)"
              disabled={isSaving}
              className="edit-modal-input"
            />
          </div>

          <div className="edit-modal-field">
            <label htmlFor="post-hero">Hero image</label>
            <div className="post-editor-hero-row">
              <input
                id="post-hero"
                type="text"
                value={heroImage}
                onChange={(e) => setHeroImage(e.target.value)}
                placeholder="gallery:main:folder/image.jpg or https://... (optional)"
                disabled={isSaving}
                className="edit-modal-input"
              />
              {galleries && galleries.length > 0 && (
                <button
                  type="button"
                  className="edit-modal-btn post-editor-browse-btn"
                  onClick={() => setHeroPickerOpen(true)}
                  disabled={isSaving}
                >
                  Browse…
                </button>
              )}
            </div>
            {galleries && galleries.length > 0 && (
              <GalleryImagePicker
                isOpen={heroPickerOpen}
                galleries={galleries}
                onClose={() => setHeroPickerOpen(false)}
                onSelect={(selection) => {
                  setHeroImage(selection.reference);
                  setHeroPickerOpen(false);
                }}
              />
            )}
          </div>

          <div className="edit-modal-field">
            <label>Content</label>
            <MarkdownEditor
              initialContent={source?.content || ''}
              placeholder="Write your post in markdown..."
              onChange={setContent}
              onSave={handleSave}
              onCancel={onClose}
              isSaving={isSaving}
              showActions={false}
              autoFocus={false}
              galleries={galleries}
            />
          </div>

          {error && <div className="edit-modal-error">{error}</div>}
        </div>

        <div className="edit-modal-footer">
          {!isNew && onDeleted && (
            <button
              type="button"
              onClick={() => (confirmingDelete ? handleDelete() : setConfirmingDelete(true))}
              disabled={isSaving}
              className="edit-modal-btn post-editor-btn-delete"
            >
              {confirmingDelete ? 'Really delete?' : 'Delete'}
            </button>
          )}
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
            disabled={isSaving || !canSave}
            className="edit-modal-btn edit-modal-btn-save"
          >
            {isSaving ? 'Saving...' : isNew ? 'Create' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
};

export default PostEditorModal;
