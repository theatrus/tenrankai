import { useEffect, useRef } from 'react';

interface SwipeDragOptions {
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => unknown;
  onNext: () => unknown;
  /** Image shown beside the current one while dragging toward it */
  prevUrl?: string;
  nextUrl?: string;
  disabled?: boolean;
  /** Changes when a new image is shown; puts the element back in place */
  resetKey?: string;
}

const AXIS_LOCK_PX = 10;
const COMMIT_FRACTION = 0.25;
const FLICK_MIN_PX = 40;
const FLICK_VELOCITY = 0.5;
const EDGE_RESISTANCE = 0.25;
const PEEK_GAP_PX = 16;
const SETTLE_MS = 200;
const EXIT_MS = 180;
const HANDOFF_MAX_MS = 1500;
const RESET_FALLBACK_MS = 3000;

function retinaUrl(url: string): string {
  return url.replace(/\/medium$/, '/medium@2x');
}

/**
 * Horizontal drag-to-navigate. The element follows the finger with the
 * neighboring image beside it, springs back on a short drag, resists at the
 * ends of the gallery, and on a long drag or flick slides the neighbor into
 * place. The neighbor stays in place until the new image has decoded, so
 * the swap is not visible.
 */
export function useSwipeDrag(
  elementRef: React.RefObject<HTMLElement | null>,
  options: SwipeDragOptions,
) {
  const opts = useRef(options);
  opts.current = options;
  const handoff = useRef<{ pending: boolean; finish: () => void }>({
    pending: false,
    finish: () => {},
  });

  useEffect(() => {
    const el = elementRef.current;
    if (!el || options.disabled) return;

    const makePeek = (side: 'prev' | 'next') => {
      const img = document.createElement('img');
      img.className = `swipe-peek swipe-peek-${side}`;
      img.alt = '';
      img.draggable = false;
      img.hidden = true;
      el.appendChild(img);
      return img;
    };
    const peeks = { prev: makePeek('prev'), next: makePeek('next') };

    const loadPeek = (img: HTMLImageElement, url?: string) => {
      if (!url) {
        img.hidden = true;
        return;
      }
      if (img.dataset.src !== url) {
        img.dataset.src = url;
        img.src = url;
        img.srcset = `${url} 1x, ${retinaUrl(url)} 2x`;
      }
      img.hidden = false;
    };

    let start: { x: number; y: number; t: number } | null = null;
    let axis: 'x' | 'y' | null = null;
    let offset = 0;
    let fallback: number | undefined;

    const setOffset = (px: number, ms = 0) => {
      el.style.transition = ms ? `transform ${ms}ms ease-out` : 'none';
      el.style.transform = px ? `translate3d(${px}px, 0, 0)` : '';
    };

    const settle = () => {
      start = null;
      axis = null;
      offset = 0;
      el.style.willChange = '';
      setOffset(0, SETTLE_MS);
    };

    const snapHome = () => {
      window.clearTimeout(fallback);
      handoff.current.pending = false;
      offset = 0;
      el.style.willChange = '';
      setOffset(0);
      peeks.prev.hidden = true;
      peeks.next.hidden = true;
    };
    handoff.current.finish = snapHome;

    const onStart = (e: TouchEvent) => {
      if (handoff.current.pending) return;
      if (e.touches.length !== 1) {
        if (axis === 'x') settle();
        start = null;
        return;
      }
      const t = e.touches[0];
      start = { x: t.clientX, y: t.clientY, t: performance.now() };
      axis = null;
    };

    const onMove = (e: TouchEvent) => {
      if (!start || e.touches.length !== 1) return;
      const t = e.touches[0];
      const dx = t.clientX - start.x;
      const dy = t.clientY - start.y;
      if (!axis) {
        if (Math.abs(dx) < AXIS_LOCK_PX && Math.abs(dy) < AXIS_LOCK_PX) return;
        axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
        if (axis === 'x') {
          el.style.willChange = 'transform';
          loadPeek(peeks.prev, opts.current.canPrev ? opts.current.prevUrl : undefined);
          loadPeek(peeks.next, opts.current.canNext ? opts.current.nextUrl : undefined);
        }
      }
      if (axis !== 'x') return;
      e.preventDefault();
      const allowed = dx < 0 ? opts.current.canNext : opts.current.canPrev;
      offset = allowed ? dx : dx * EDGE_RESISTANCE;
      setOffset(offset);
    };

    const onEnd = () => {
      if (!start || axis !== 'x') {
        start = null;
        return;
      }
      const elapsed = Math.max(1, performance.now() - start.t);
      const velocity = Math.abs(offset) / elapsed;
      const width = el.getBoundingClientRect().width || window.innerWidth;
      const goingNext = offset < 0;
      const allowed = goingNext ? opts.current.canNext : opts.current.canPrev;
      const far = Math.abs(offset) > width * COMMIT_FRACTION;
      const flick = Math.abs(offset) > FLICK_MIN_PX && velocity > FLICK_VELOCITY;
      start = null;
      axis = null;

      if (!allowed || !(far || flick)) {
        settle();
        return;
      }

      handoff.current.pending = true;
      const travel = width + PEEK_GAP_PX;
      setOffset(goingNext ? -travel : travel, EXIT_MS);
      window.setTimeout(() => {
        if (goingNext) opts.current.onNext();
        else opts.current.onPrev();
        window.clearTimeout(fallback);
        fallback = window.setTimeout(snapHome, RESET_FALLBACK_MS);
      }, EXIT_MS);
    };

    el.addEventListener('touchstart', onStart, { passive: true });
    el.addEventListener('touchmove', onMove, { passive: false });
    el.addEventListener('touchend', onEnd);
    el.addEventListener('touchcancel', settle);
    return () => {
      window.clearTimeout(fallback);
      handoff.current = { pending: false, finish: () => {} };
      el.removeEventListener('touchstart', onStart);
      el.removeEventListener('touchmove', onMove);
      el.removeEventListener('touchend', onEnd);
      el.removeEventListener('touchcancel', settle);
      peeks.prev.remove();
      peeks.next.remove();
      el.style.transition = '';
      el.style.transform = '';
      el.style.willChange = '';
    };
  }, [elementRef, options.disabled]);

  // A new image is showing. After a swipe, keep the neighbor in view until
  // the new image has decoded, then snap back without animation.
  useEffect(() => {
    const el = elementRef.current;
    if (!el) return;
    if (!handoff.current.pending) {
      el.style.transition = 'none';
      el.style.transform = '';
      return;
    }
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      handoff.current.finish();
    };
    const img = el.querySelector<HTMLImageElement>('.image-container img');
    const timer = window.setTimeout(finish, HANDOFF_MAX_MS);
    if (img && typeof img.decode === 'function') {
      img.decode().then(finish, finish);
    } else {
      finish();
    }
    return () => window.clearTimeout(timer);
  }, [elementRef, options.resetKey]);
}
