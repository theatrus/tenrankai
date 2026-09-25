import React, { useEffect, useRef } from 'react';
import { NavigationImage } from '../../types/index.ts';

const SWIPE_THRESHOLD = 30;

interface MobileTrayProps {
  title: string;
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
  quickActions?: React.ReactNode;
  strip?: React.ReactNode;
  children: React.ReactNode;
}

function useVerticalSwipe(
  onUp: () => void,
  onDown: () => void,
  canSwipeDown: () => boolean = () => true,
) {
  const start = useRef<{ x: number; y: number } | null>(null);
  return {
    onTouchStart: (e: React.TouchEvent) => {
      const t = e.touches[0];
      start.current = { x: t.clientX, y: t.clientY };
    },
    onTouchEnd: (e: React.TouchEvent) => {
      const from = start.current;
      start.current = null;
      if (!from) return;
      const t = e.changedTouches[0];
      const dx = t.clientX - from.x;
      const dy = t.clientY - from.y;
      if (Math.abs(dy) < SWIPE_THRESHOLD || Math.abs(dy) < Math.abs(dx)) return;
      if (dy < 0) onUp();
      else if (canSwipeDown()) onDown();
    },
  };
}

export function MobileTray({
  title,
  expanded,
  onExpandedChange,
  quickActions,
  strip,
  children,
}: MobileTrayProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const open = () => onExpandedChange(true);
  const close = () => onExpandedChange(false);
  const toggle = () => onExpandedChange(!expanded);
  const headSwipe = useVerticalSwipe(open, close);
  const bodySwipe = useVerticalSwipe(
    () => {},
    close,
    () => (bodyRef.current?.scrollTop ?? 0) <= 0,
  );

  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onExpandedChange(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [expanded, onExpandedChange]);

  const handle = (
    <button
      type="button"
      className="phone-tray-handle"
      aria-expanded={expanded}
      aria-label={expanded ? 'Hide details' : 'Show details'}
      onClick={toggle}
    >
      <span />
    </button>
  );

  return (
    <>
      {expanded && <div className="phone-tray-backdrop" onClick={close} aria-hidden="true" />}
      <section className={`phone-tray${expanded ? ' expanded' : ''}`} aria-label="Image details">
        <div
          ref={bodyRef}
          className="phone-tray-body"
          aria-hidden={!expanded}
          inert={!expanded}
          {...bodySwipe}
        >
          {handle}
          {children}
        </div>
        <div className="phone-tray-head" {...headSwipe}>
          {handle}
          <div className="phone-tray-row">
            <h1 className="phone-tray-title">{title}</h1>
            {quickActions}
            <button
              type="button"
              className="phone-tray-toggle"
              aria-expanded={expanded}
              onClick={toggle}
            >
              {expanded ? 'Close' : 'Details'}
            </button>
          </div>
          {strip}
        </div>
      </section>
    </>
  );
}

interface TrayThumbnailsProps {
  current: NavigationImage;
  prevImages: NavigationImage[];
  nextImages: NavigationImage[];
  onSelect: (image: NavigationImage) => void;
}

export function TrayThumbnails({ current, prevImages, nextImages, onSelect }: TrayThumbnailsProps) {
  const currentRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    currentRef.current?.scrollIntoView({ inline: 'center', block: 'nearest' });
  }, [current.path]);

  if (prevImages.length === 0 && nextImages.length === 0) return null;

  const items = [...prevImages].reverse().concat(current, nextImages);

  return (
    <div className="phone-tray-strip" role="list">
      {items.map((img) => {
        const isCurrent = img.path === current.path;
        const url = img.thumbnail_url;
        const url2x = url.replace(/\/thumbnail$/, '/thumbnail@2x');
        return (
          <button
            key={img.path}
            ref={isCurrent ? currentRef : undefined}
            type="button"
            role="listitem"
            className={`phone-tray-thumb${isCurrent ? ' current' : ''}`}
            aria-current={isCurrent ? 'true' : undefined}
            aria-label={isCurrent ? `Current: ${img.name}` : `Go to ${img.name}`}
            onClick={() => !isCurrent && onSelect(img)}
          >
            <img src={url} srcSet={`${url} 1x, ${url2x} 2x`} alt="" loading="lazy" />
          </button>
        );
      })}
    </div>
  );
}
