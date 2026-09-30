import { useEffect, useRef } from 'react';

interface SwipeDragOptions {
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => unknown;
  onNext: () => unknown;
  /** Image shown beside the current one while dragging toward it */
  prevUrl?: string;
  nextUrl?: string;
  /** A single tap that did not become a drag or a double tap */
  onTap?: () => void;
  /** Dragging the image down past the threshold */
  onSwipeDown?: () => void;
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
const TAP_MAX_MS = 300;
// Must not be shorter than ImageDisplay's double-tap window (300 ms), or a
// slow double tap would also fire the single-tap action
const DOUBLE_TAP_WINDOW_MS = 300;
const DISMISS_FRACTION = 0.18;
const DISMISS_MIN_SCALE = 0.85;
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
  // pending: a committed swipe is sliding the neighbor in.
  // navigated: the new image's data has arrived; only then may another
  // touch cut the hand-off short (before that, a swipe would navigate
  // again from the old image).
  // generation: bumped on each committed swipe, so a decode that resolves
  // late cannot finish a newer hand-off or reset a drag already under way
  const handoff = useRef<{
    pending: boolean;
    navigated: boolean;
    generation: number;
    finish: () => void;
  }>({
    pending: false,
    navigated: false,
    generation: 0,
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
    let drop = 0;
    let fallback: number | undefined;
    let tapTimer: number | undefined;

    const setOffset = (px: number, ms = 0) => {
      el.style.transition = ms ? `transform ${ms}ms ease-out` : 'none';
      el.style.transform = px ? `translate3d(${px}px, 0, 0)` : '';
    };

    // Dragging down shrinks the image as it follows the finger
    const setDrop = (px: number, ms = 0) => {
      const height = el.getBoundingClientRect().height || window.innerHeight;
      const scale = Math.max(DISMISS_MIN_SCALE, 1 - px / height / 2);
      el.style.transition = ms ? `transform ${ms}ms ease-out` : 'none';
      el.style.transform = px ? `translate3d(0, ${px}px, 0) scale(${scale})` : '';
    };

    const settle = () => {
      start = null;
      axis = null;
      offset = 0;
      drop = 0;
      el.style.willChange = '';
      setOffset(0, SETTLE_MS);
    };

    const snapHome = () => {
      window.clearTimeout(fallback);
      handoff.current.pending = false;
      handoff.current.navigated = false;
      offset = 0;
      el.style.willChange = '';
      setOffset(0);
      peeks.prev.hidden = true;
      peeks.next.hidden = true;
    };
    handoff.current.finish = snapHome;

    // If the page itself is pinch-zoomed with the browser's own zoom, one
    // finger should pan the page, not drag the image. The stage claims all
    // touch input otherwise, so drags get every move and can cancel them.
    const viewport = window.visualViewport;
    const pageZoomed = () => (viewport?.scale ?? 1) > 1.01;
    const syncTouchAction = () => {
      el.style.touchAction = pageZoomed() ? 'manipulation' : '';
    };
    syncTouchAction();
    viewport?.addEventListener('resize', syncTouchAction);

    const onStart = (e: TouchEvent) => {
      if (pageZoomed()) {
        start = null;
        return;
      }
      // A second tap cancels the single-tap action (it is a double tap)
      window.clearTimeout(tapTimer);
      if (handoff.current.pending) {
        // Still waiting for the new image's data: ignore the touch
        if (!handoff.current.navigated) {
          start = null;
          return;
        }
        // Data is in but the image is still decoding: move on anyway
        snapHome();
      }
      if (e.touches.length !== 1) {
        // A second finger ends any drag, sideways or down
        if (axis) settle();
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
        if (axis === 'y' && dy > 0 && opts.current.onSwipeDown) {
          el.style.willChange = 'transform';
        }
        if (axis === 'x') {
          el.style.willChange = 'transform';
          loadPeek(peeks.prev, opts.current.canPrev ? opts.current.prevUrl : undefined);
          loadPeek(peeks.next, opts.current.canNext ? opts.current.nextUrl : undefined);
        }
      }
      if (axis === 'y') {
        if (!opts.current.onSwipeDown) return;
        e.preventDefault();
        drop = Math.max(0, dy);
        setDrop(drop);
        return;
      }
      e.preventDefault();
      const allowed = dx < 0 ? opts.current.canNext : opts.current.canPrev;
      offset = allowed ? dx : dx * EDGE_RESISTANCE;
      setOffset(offset);
    };

    const onEnd = () => {
      if (start && !axis) {
        const quick = performance.now() - start.t < TAP_MAX_MS;
        start = null;
        if (quick && opts.current.onTap) {
          tapTimer = window.setTimeout(() => opts.current.onTap?.(), DOUBLE_TAP_WINDOW_MS);
        }
        return;
      }
      if (start && axis === 'y') {
        const elapsed = Math.max(1, performance.now() - start.t);
        const height = el.getBoundingClientRect().height || window.innerHeight;
        const dismiss =
          drop > height * DISMISS_FRACTION ||
          (drop > FLICK_MIN_PX && drop / elapsed > FLICK_VELOCITY);
        start = null;
        axis = null;
        if (dismiss && opts.current.onSwipeDown) {
          opts.current.onSwipeDown();
          return;
        }
        drop = 0;
        el.style.willChange = '';
        setDrop(0, SETTLE_MS);
        return;
      }
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
      handoff.current.generation += 1;
      const travel = width + PEEK_GAP_PX;
      setOffset(goingNext ? -travel : travel, EXIT_MS);
      window.setTimeout(() => {
        if (goingNext) opts.current.onNext();
        else opts.current.onPrev();
        window.clearTimeout(fallback);
        fallback = window.setTimeout(snapHome, RESET_FALLBACK_MS);
      }, EXIT_MS);
    };

    // Safari can restore this page from its back/forward cache exactly as it
    // was left, e.g. still shrunk from dragging down to the folder
    const onPageShow = (e: PageTransitionEvent) => {
      if (e.persisted) snapHome();
    };
    window.addEventListener('pageshow', onPageShow);

    el.addEventListener('touchstart', onStart, { passive: true });
    el.addEventListener('touchmove', onMove, { passive: false });
    el.addEventListener('touchend', onEnd);
    el.addEventListener('touchcancel', settle);
    return () => {
      window.clearTimeout(fallback);
      window.clearTimeout(tapTimer);
      window.removeEventListener('pageshow', onPageShow);
      viewport?.removeEventListener('resize', syncTouchAction);
      el.style.touchAction = '';
      // Keep counting generations so an older decode can never match a new swipe
      handoff.current = {
        pending: false,
        navigated: false,
        generation: handoff.current.generation + 1,
        finish: () => {},
      };
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
    handoff.current.navigated = true;
    let done = false;
    const generation = handoff.current.generation;
    const finish = () => {
      if (done || !handoff.current.pending || handoff.current.generation !== generation) return;
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
    return () => {
      done = true;
      window.clearTimeout(timer);
    };
  }, [elementRef, options.resetKey]);
}
