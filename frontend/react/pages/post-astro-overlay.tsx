import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import {
  AstroOverlay,
  AstroSolution,
  CatalogMenu,
  distantTransients,
  catalogGroup,
  useAstroSolution,
} from '../components/ImageDetail/AstroOverlay.tsx';

/**
 * Astro object labels for gallery images embedded in posts (inline
 * embeds and gallery-backed hero images). The overlay mounts inside a
 * wrapper around the image itself, so the pill and SVG align with the
 * image box regardless of the surrounding anchor's layout — and the
 * anchor (which also hosts the details hover card) keeps its normal
 * line-height.
 */

const HIDDEN_GROUPS_KEY = 'astro-hidden-catalogs';

function loadHiddenGroups(): string[] {
  try {
    return JSON.parse(localStorage.getItem(HIDDEN_GROUPS_KEY) || '[]');
  } catch {
    return [];
  }
}

const EmbedOverlay: React.FC<{ gallery: string; path: string }> = ({ gallery, path }) => {
  const solution = useAstroSolution(gallery, path);
  const [visible, setVisible] = useState(false);
  const [hiddenGroups, setHiddenGroupsState] = useState<string[]>(loadHiddenGroups);
  const setHiddenGroups = (groups: string[]) => {
    setHiddenGroupsState(groups);
    try {
      localStorage.setItem(HIDDEN_GROUPS_KEY, JSON.stringify(groups));
    } catch {
      /* private mode */
    }
  };

  if (!solution) return null;
  const kept = (solution.objects || []).filter((o) => !hiddenGroups.includes(catalogGroup(o)));
  const shown = kept.length - distantTransients(solution).filter((o) => kept.includes(o)).length;

  const stop = (e: React.SyntheticEvent) => {
    // The embed is a link to the detail page; controls must not follow it
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <>
      <span
        style={{
          position: 'absolute',
          right: '0.4rem',
          bottom: '0.4rem',
          zIndex: 3,
          display: 'flex',
          gap: '0.3rem',
          lineHeight: 'normal',
          pointerEvents: 'auto',
        }}
      >
        {visible && (
          <CatalogMenu
            solution={solution}
            hiddenGroups={hiddenGroups}
            onHiddenGroupsChange={setHiddenGroups}
            compact
          />
        )}
        <button
          type="button"
          className="post-astro-toggle"
          onClick={(e) => {
            stop(e);
            setVisible(!visible);
          }}
          onTouchEnd={(e) => {
            stop(e);
            setVisible(!visible);
          }}
          style={{
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
      </span>
      {visible && (
        <span
          style={{
            position: 'absolute',
            inset: 0,
            pointerEvents: 'none',
            display: 'block',
            lineHeight: 'normal',
          }}
        >
          <AstroOverlay
            solution={solution as AstroSolution}
            visible
            allTransients={false}
            hiddenGroups={hiddenGroups}
          />
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
      const img = anchor.querySelector('img');
      if (!gallery || !path || !img) return;

      // Wrap only the image: the wrapper is the overlay's containing
      // block, sized exactly to the image (line-height 0 kills the
      // inline baseline gap without leaking into the hover card, which
      // mounts on the anchor and needs its normal line-height)
      const wrapper = document.createElement('span');
      wrapper.style.position = 'relative';
      wrapper.style.display = 'inline-block';
      wrapper.style.lineHeight = '0';
      // The image's own margins move to the wrapper so the wrapper box
      // (and the overlay spanning it) coincides with the image exactly
      const computed = getComputedStyle(img);
      wrapper.style.margin = `${computed.marginTop} ${computed.marginRight} ${computed.marginBottom} ${computed.marginLeft}`;
      img.parentNode?.insertBefore(wrapper, img);
      wrapper.appendChild(img);
      img.style.display = 'block';
      img.style.margin = '0';

      const mount = document.createElement('span');
      mount.style.display = 'contents';
      wrapper.appendChild(mount);
      createRoot(mount).render(<EmbedOverlay gallery={gallery} path={path} />);
    });
});
