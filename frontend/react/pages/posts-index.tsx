import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { PostEditorModal } from '../components/Posts/PostEditorModal.tsx';

const NewPostButton: React.FC<{ postsName: string }> = ({ postsName }) => {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <>
      <button type="button" className="btn btn-primary post-new-btn" onClick={() => setIsOpen(true)}>
        + New post
      </button>
      <PostEditorModal
        isOpen={isOpen}
        postsName={postsName}
        source={null}
        onClose={() => setIsOpen(false)}
        onSaved={(url) => {
          window.location.href = url;
        }}
      />
    </>
  );
};

document.addEventListener('DOMContentLoaded', () => {
  const mount = document.getElementById('post-new-mount');
  if (!mount) return;

  const postsName = mount.getAttribute('data-posts-name') || '';
  if (!postsName) return;

  const root = createRoot(mount);
  root.render(<NewPostButton postsName={postsName} />);
});
