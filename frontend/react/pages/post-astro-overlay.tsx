import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import {
  AstroOverlay,
  AstroSolution,
  distantTransients,
  useAstroSolution,
} from '../components/ImageDetail/AstroOverlay.tsx';

/**
 * Astro object labels for gallery images embedded in posts: every embed
 * carries data-gallery/data-image-path; when the image has a plate
 * solution, a small toggle appears over the embed and the standard
 * overlay SVG renders on top of the image.
 */

const EmbedOverlay: React.FC<{ gallery: string; path: string }> = ({ gallery, path }) => {
  const solution = useAstroSolution(gallery, path);
  const [visible, setVisible] = useState(false);

  if (!solution) return null;
  const shown = (solution.objects || []).length - (visible ? 0 : distantTransients(solution).length);

  return (
    <>
      <button
        type="button"
        className="post-astro-toggle"
        onClick={(e) => {
          // The embed is a link to the detail page; the toggle must not
          // follow it
          e.preventDefault();
          e.stopPropagation();
          setVisible(!visible);
        }}
        onTouchEnd={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setVisible(!visible);
        }}
        style={{
          position: 'absolute',
          right: '0.4rem',
          bottom: '0.4rem',
          zIndex: 3,
          pointerEvents: 'auto',
          touchAction: 'manipulation',
          padding: '0.1rem 0.6rem',
          borderRadius: '999px',
          border: '1px solid rgba(255,255,255,0.4)',
          background: visible ? 'rgba(80,180,255,0.4)' : 'rgba(0,0,0,0.55)',
          color: '#fff',
          fontSize: '0.72rem',
          cursor: 'pointer',
        }}
        title="Toggle astronomical object labels"
      >
        {visible ? 'Objects ✕' : `Objects (${shown})`}
      </button>
      {visible && (
        <span
          style={{
            position: 'absolute',
            inset: 0,
            pointerEvents: 'none',
            display: 'block',
          }}
        >
          <AstroOverlay solution={solution as AstroSolution} visible allTransients={false} />
        </span>
      )}
    </>
  );
};

document.addEventListener('DOMContentLoaded', () => {
  document
    .querySelectorAll<HTMLAnchorElement>(
      'a.gallery-image-link[data-gallery], a.post-hero-link[data-gallery]',
    )
    .forEach((anchor) => {
      const gallery = anchor.dataset.gallery;
      const path = anchor.dataset.imagePath;
      if (!gallery || !path) return;

      anchor.style.position = 'relative';
      anchor.style.display = 'inline-block';
      const mount = document.createElement('span');
      mount.style.display = 'contents';
      anchor.appendChild(mount);
      createRoot(mount).render(<EmbedOverlay gallery={gallery} path={path} />);
    });
});
