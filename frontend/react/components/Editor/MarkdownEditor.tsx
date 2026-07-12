import React, { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import Image from '@tiptap/extension-image';
import Placeholder from '@tiptap/extension-placeholder';
import TurndownService from 'turndown';
import { marked } from 'marked';
import {
  GalleryImagePicker,
  GalleryImageSelection,
} from '../Posts/GalleryImagePicker.tsx';

// Configure marked for synchronous operation
marked.use({ async: false });

type EditorMode = 'rich' | 'markdown';

interface MarkdownEditorProps {
  /** Initial markdown content */
  initialContent: string;
  /** Placeholder text when empty */
  placeholder?: string;
  /** Called when content changes (debounced) */
  onChange?: (markdown: string) => void;
  /** Called when save is triggered (Ctrl+S or button) */
  onSave?: (markdown: string) => void;
  /** Called when cancel is triggered (Escape or button) */
  onCancel?: () => void;
  /** Whether the editor is in a saving state */
  isSaving?: boolean;
  /** Whether to show action buttons */
  showActions?: boolean;
  /** Auto-focus the editor on mount */
  autoFocus?: boolean;
  /** Gallery names for the gallery image picker; omit to hide the button */
  galleries?: string[];
}

export const MarkdownEditor: React.FC<MarkdownEditorProps> = ({
  initialContent,
  placeholder = 'Start typing...',
  onChange,
  onSave,
  onCancel,
  isSaving = false,
  showActions = true,
  autoFocus = true,
  galleries,
}) => {
  const [mode, setMode] = useState<EditorMode>('rich');
  const [markdownText, setMarkdownText] = useState(initialContent);
  const [pickerOpen, setPickerOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Create turndown instance for HTML -> Markdown conversion
  const turndown = useMemo(() => {
    const service = new TurndownService({
      headingStyle: 'atx',
      codeBlockStyle: 'fenced',
    });
    return service;
  }, []);

  // Convert markdown to HTML for initial editor content
  const initialHtml = useMemo(() => {
    if (!initialContent) return '';
    try {
      const html = marked.parse(initialContent) as string;
      return html;
    } catch (e) {
      console.error('Failed to parse markdown:', e);
      return `<p>${initialContent}</p>`;
    }
  }, [initialContent]);

  // Get current markdown from editor
  const getMarkdown = useCallback(
    (html: string) => {
      if (!html || html === '<p></p>') return '';
      return turndown.turndown(html);
    },
    [turndown]
  );

  const editor = useEditor({
    shouldRerenderOnTransaction: true,
    extensions: [
      StarterKit.configure({
        heading: {
          levels: [1, 2, 3],
        },
        link: false,
      }),
      Link.configure({
        openOnClick: false,
        HTMLAttributes: {
          rel: 'noopener noreferrer',
        },
      }),
      // Keeps <img> nodes (including gallery references) intact in rich mode
      // instead of silently dropping them
      Image,
      Placeholder.configure({
        placeholder,
      }),
    ],
    content: initialHtml,
    autofocus: autoFocus && mode === 'rich' ? 'end' : false,
    onUpdate: ({ editor }) => {
      if (onChange && mode === 'rich') {
        const markdown = getMarkdown(editor.getHTML());
        onChange(markdown);
      }
    },
    editorProps: {
      attributes: {
        class: 'markdown-editor-content',
      },
      handleKeyDown: (_view, event) => {
        // Ctrl+S or Cmd+S to save
        if ((event.ctrlKey || event.metaKey) && event.key === 's') {
          event.preventDefault();
          if (onSave && editor) {
            const markdown = getMarkdown(editor.getHTML());
            onSave(markdown);
          }
          return true;
        }
        // Escape to cancel
        if (event.key === 'Escape') {
          event.preventDefault();
          if (onCancel) {
            onCancel();
          }
          return true;
        }
        return false;
      },
    },
  });

  // Update content when initialContent changes
  useEffect(() => {
    if (editor && initialHtml !== editor.getHTML()) {
      editor.commands.setContent(initialHtml);
    }
    setMarkdownText(initialContent);
  }, [editor, initialHtml, initialContent]);

  // Focus textarea when switching to markdown mode
  useEffect(() => {
    if (mode === 'markdown' && textareaRef.current && autoFocus) {
      textareaRef.current.focus();
    }
  }, [mode, autoFocus]);

  // Handle mode switch
  const handleModeSwitch = useCallback((e: React.MouseEvent) => {
    // Prevent event from bubbling up to modal overlay
    e.stopPropagation();
    e.preventDefault();

    if (mode === 'rich' && editor) {
      // Switching to markdown mode - get current content from rich editor
      const markdown = getMarkdown(editor.getHTML());
      setMarkdownText(markdown);
      setMode('markdown');
    } else if (mode === 'markdown' && editor) {
      // Switching to rich mode - convert markdown to HTML
      try {
        const html = marked.parse(markdownText) as string;
        editor.commands.setContent(html);
      } catch (e) {
        console.error('Failed to parse markdown:', e);
      }
      setMode('rich');
    }
  }, [mode, editor, getMarkdown, markdownText]);

  // Get current content based on mode
  const getCurrentMarkdown = useCallback(() => {
    if (mode === 'markdown') {
      return markdownText;
    }
    if (editor) {
      return getMarkdown(editor.getHTML());
    }
    return '';
  }, [mode, markdownText, editor, getMarkdown]);

  const handleSave = useCallback(() => {
    if (onSave) {
      onSave(getCurrentMarkdown());
    }
  }, [onSave, getCurrentMarkdown]);

  // Handle markdown textarea changes
  const handleMarkdownChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const value = e.target.value;
      setMarkdownText(value);
      if (onChange) {
        onChange(value);
      }
    },
    [onChange]
  );

  // Handle keyboard shortcuts in textarea
  const handleTextareaKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Ctrl+S or Cmd+S to save
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        if (onSave) {
          onSave(markdownText);
        }
      }
      // Escape to cancel
      if (e.key === 'Escape') {
        e.preventDefault();
        if (onCancel) {
          onCancel();
        }
      }
    },
    [onSave, onCancel, markdownText]
  );

  const insertGalleryImage = useCallback(
    (selection: GalleryImageSelection) => {
      setPickerOpen(false);
      const hint = selection.details ? `${selection.size},details` : selection.size;

      if (mode === 'rich' && editor) {
        // Round-trips through turndown as ![gallery:...](size,details)
        editor.chain().focus().setImage({ src: hint, alt: selection.reference }).run();
        if (onChange) onChange(getMarkdown(editor.getHTML()));
        return;
      }

      const markdown = `![${selection.reference}](${hint})`;
      const textarea = textareaRef.current;
      const start = textarea ? textarea.selectionStart : markdownText.length;
      const end = textarea ? textarea.selectionEnd : markdownText.length;
      const next = `${markdownText.slice(0, start)}${markdown}${markdownText.slice(end)}`;
      setMarkdownText(next);
      if (onChange) onChange(next);
      requestAnimationFrame(() => {
        if (textarea) {
          textarea.focus();
          textarea.selectionStart = start + markdown.length;
          textarea.selectionEnd = start + markdown.length;
        }
      });
    },
    [mode, editor, getMarkdown, markdownText, onChange],
  );

  const setLink = useCallback(() => {
    if (!editor) return;

    const previousUrl = editor.getAttributes('link').href;
    const url = window.prompt('URL', previousUrl);

    if (url === null) return; // Cancelled

    if (url === '') {
      editor.chain().focus().extendMarkRange('link').unsetLink().run();
      return;
    }

    editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
  }, [editor]);

  if (!editor) {
    return null;
  }

  return (
    <div className="markdown-editor">
      {/* Mode toggle */}
      <div className="editor-mode-toggle">
        {galleries && galleries.length > 0 && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setPickerOpen(true);
            }}
            className="editor-mode-btn editor-gallery-btn"
            title="Insert gallery image"
          >
            🖼 Gallery image
          </button>
        )}
        <button
          type="button"
          onClick={handleModeSwitch}
          className="editor-mode-btn"
          title={mode === 'rich' ? 'Switch to Markdown' : 'Switch to Rich Text'}
        >
          {mode === 'rich' ? 'Markdown' : 'Rich Text'}
        </button>
      </div>

      {galleries && galleries.length > 0 && (
        <GalleryImagePicker
          isOpen={pickerOpen}
          galleries={galleries}
          withOptions
          onClose={() => setPickerOpen(false)}
          onSelect={insertGalleryImage}
        />
      )}

      {/* Rich text editor - hidden when in markdown mode but kept mounted to avoid DOM errors */}
      <div style={{ display: mode === 'rich' ? 'block' : 'none' }}>
        {/* Fixed toolbar */}
        <div className="editor-toolbar">
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleBold().run()}
            className={`editor-toolbar-btn ${editor.isActive('bold') ? 'is-active' : ''}`}
            title="Bold (Ctrl+B)"
          >
            <strong>B</strong>
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleItalic().run()}
            className={`editor-toolbar-btn ${editor.isActive('italic') ? 'is-active' : ''}`}
            title="Italic (Ctrl+I)"
          >
            <em>I</em>
          </button>
          <span className="editor-toolbar-divider" />
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
            className={`editor-toolbar-btn ${editor.isActive('heading', { level: 1 }) ? 'is-active' : ''}`}
            title="Title (H1)"
          >
            H1
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
            className={`editor-toolbar-btn ${editor.isActive('heading', { level: 2 }) ? 'is-active' : ''}`}
            title="Heading (H2)"
          >
            H2
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
            className={`editor-toolbar-btn ${editor.isActive('heading', { level: 3 }) ? 'is-active' : ''}`}
            title="Heading (H3)"
          >
            H3
          </button>
          <span className="editor-toolbar-divider" />
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleBulletList().run()}
            className={`editor-toolbar-btn ${editor.isActive('bulletList') ? 'is-active' : ''}`}
            title="Bullet List"
          >
            •
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleOrderedList().run()}
            className={`editor-toolbar-btn ${editor.isActive('orderedList') ? 'is-active' : ''}`}
            title="Numbered List"
          >
            1.
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleBlockquote().run()}
            className={`editor-toolbar-btn ${editor.isActive('blockquote') ? 'is-active' : ''}`}
            title="Quote"
          >
            "
          </button>
          <button
            type="button"
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}
            className={`editor-toolbar-btn ${editor.isActive('codeBlock') ? 'is-active' : ''}`}
            title="Code Block"
          >
            {'</>'}
          </button>
          <span className="editor-toolbar-divider" />
          <button
            type="button"
            onClick={setLink}
            className={`editor-toolbar-btn ${editor.isActive('link') ? 'is-active' : ''}`}
            title="Link"
          >
            🔗
          </button>
        </div>

        {/* Rich text editor content */}
        <EditorContent editor={editor} />
      </div>

      {/* Markdown textarea - shown when in markdown mode */}
      {mode === 'markdown' && (
        <textarea
          ref={textareaRef}
          value={markdownText}
          onChange={handleMarkdownChange}
          onKeyDown={handleTextareaKeyDown}
          placeholder={placeholder}
          className="markdown-editor-textarea"
          spellCheck={false}
        />
      )}

      {/* Action buttons */}
      {showActions && (
        <div className="editor-actions">
          <button
            type="button"
            onClick={onCancel}
            disabled={isSaving}
            className="editor-cancel-btn"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={isSaving}
            className="editor-save-btn"
          >
            {isSaving ? 'Saving...' : 'Save'}
          </button>
          <span className="editor-shortcuts">
            Ctrl+S to save, Esc to cancel
          </span>
        </div>
      )}
    </div>
  );
};

export default MarkdownEditor;
