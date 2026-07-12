import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { PostShare } from '../components/PostShare.tsx';
import { PostEditorModal } from '../components/Posts/PostEditorModal.tsx';
import { postsApi, PostSource } from '../api/posts.ts';

const EditPostButton: React.FC<{
  postsName: string;
  slug: string;
  indexUrl: string;
  galleries: string[];
}> = ({
  postsName,
  slug,
  indexUrl,
  galleries,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [source, setSource] = useState<PostSource | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleOpen = async () => {
    setError(null);
    try {
      setSource(await postsApi.getSource(postsName, slug));
      setIsOpen(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load post source');
    }
  };

  return (
    <>
      <button type="button" className="btn btn-secondary post-edit-btn" onClick={handleOpen}>
        Edit
      </button>
      {error && <span className="post-edit-error">{error}</span>}
      {source && (
        <PostEditorModal
          isOpen={isOpen}
          postsName={postsName}
          source={source}
          galleries={galleries}
          onClose={() => setIsOpen(false)}
          onSaved={() => window.location.reload()}
          onDeleted={() => {
            window.location.href = indexUrl;
          }}
        />
      )}
    </>
  );
};

document.addEventListener('DOMContentLoaded', () => {
  const shareMount = document.getElementById('post-share-mount');
  if (shareMount) {
    const url = shareMount.getAttribute('data-share-url') || window.location.href;
    const title = shareMount.getAttribute('data-share-title') || document.title;
    const summary = shareMount.getAttribute('data-share-summary') || '';

    createRoot(shareMount).render(<PostShare url={url} title={title} summary={summary} />);
  }

  const editMount = document.getElementById('post-edit-mount');
  if (editMount) {
    const postsName = editMount.getAttribute('data-posts-name') || '';
    const slug = editMount.getAttribute('data-slug') || '';
    const indexUrl = editMount.getAttribute('data-url-prefix') || '/';
    const galleries = (editMount.getAttribute('data-galleries') || '').split(',').filter(Boolean);
    if (postsName && slug) {
      createRoot(editMount).render(
        <EditPostButton postsName={postsName} slug={slug} indexUrl={indexUrl} galleries={galleries} />
      );
    }
  }
});
