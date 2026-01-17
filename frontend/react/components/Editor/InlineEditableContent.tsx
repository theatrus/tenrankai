import React, { useState, useCallback, useMemo } from 'react';
import { MarkdownEditor } from './MarkdownEditor.tsx';
import TurndownService from 'turndown';

interface InlineEditableContentProps {
  /** The HTML content to display (rendered markdown) */
  htmlContent?: string;
  /** The raw markdown content for editing */
  markdownContent?: string;
  /** Placeholder shown when content is empty and editable */
  emptyPlaceholder?: string;
  /** Whether the user has permission to edit */
  canEdit: boolean;
  /** Called when saving - should return a promise */
  onSave: (markdown: string) => Promise<void>;
  /** Optional callback after successful save */
  onSaveSuccess?: () => void;
  /** CSS class for the content container */
  className?: string;
}

export const InlineEditableContent: React.FC<InlineEditableContentProps> = ({
  htmlContent,
  markdownContent = '',
  emptyPlaceholder = 'Add description...',
  canEdit,
  onSave,
  onSaveSuccess,
  className = '',
}) => {
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const hasContent = Boolean(htmlContent?.trim());

  // Create turndown instance for HTML -> Markdown fallback conversion
  const turndown = useMemo(() => {
    return new TurndownService({
      headingStyle: 'atx',
      codeBlockStyle: 'fenced',
    });
  }, []);

  // Get the markdown content to edit - use provided markdown, or convert HTML as fallback
  const editableMarkdown = useMemo(() => {
    if (markdownContent && markdownContent.trim()) {
      return markdownContent;
    }
    // Fallback: convert HTML to markdown if no markdown source is available
    if (htmlContent && htmlContent.trim()) {
      try {
        return turndown.turndown(htmlContent);
      } catch (e) {
        console.error('Failed to convert HTML to markdown:', e);
        return '';
      }
    }
    return '';
  }, [markdownContent, htmlContent, turndown]);

  const handleStartEdit = useCallback(() => {
    if (canEdit) {
      setIsEditing(true);
      setError(null);
    }
  }, [canEdit]);

  const handleCancel = useCallback(() => {
    setIsEditing(false);
    setError(null);
  }, []);

  const handleSave = useCallback(
    async (markdown: string) => {
      setIsSaving(true);
      setError(null);

      try {
        await onSave(markdown);
        setIsEditing(false);
        if (onSaveSuccess) {
          onSaveSuccess();
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to save';
        setError(message);
      } finally {
        setIsSaving(false);
      }
    },
    [onSave, onSaveSuccess]
  );

  // Editing mode
  if (isEditing) {
    return (
      <div className={`inline-editable inline-editable-editing ${className}`}>
        <MarkdownEditor
          initialContent={editableMarkdown}
          placeholder={emptyPlaceholder}
          onSave={handleSave}
          onCancel={handleCancel}
          isSaving={isSaving}
          autoFocus
        />
        {error && <div className="editor-error">{error}</div>}
      </div>
    );
  }

  // View mode - empty state with placeholder
  if (!hasContent && canEdit) {
    return (
      <div
        className={`inline-editable inline-editable-empty ${className}`}
        onClick={handleStartEdit}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            handleStartEdit();
          }
        }}
        role="button"
        tabIndex={0}
        title="Click to add description"
      >
        <span className="inline-editable-placeholder">{emptyPlaceholder}</span>
      </div>
    );
  }

  // View mode - has content
  if (hasContent) {
    const contentElement = (
      <div
        className="inline-editable-content"
        dangerouslySetInnerHTML={{ __html: htmlContent || '' }}
      />
    );

    if (canEdit) {
      return (
        <div
          className={`inline-editable inline-editable-clickable ${className}`}
          onClick={handleStartEdit}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              handleStartEdit();
            }
          }}
          role="button"
          tabIndex={0}
          title="Click to edit"
        >
          {contentElement}
        </div>
      );
    }

    return (
      <div className={`inline-editable ${className}`}>
        {contentElement}
      </div>
    );
  }

  // No content and can't edit - render nothing
  return null;
};

export default InlineEditableContent;
