import { useEffect, useRef } from 'react';

interface SwipeDragOptions {
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => unknown;
  onNext: () => unknown;
  disabled?: boolean;
  /** Changes when a new image is shown; puts the element back in place */
  resetKey?: string;
}

const AXIS_LOCK_PX = 10;
const COMMIT_FRACTION = 0.25;
const FLICK_MIN_PX = 40;
const FLICK_VELOCITY = 0.5;
const EDGE_RESISTANCE = 0.25;
const SETTLE_MS = 200;
const EXIT_MS = 180;
const RESET_FALLBACK_MS = 2000;

/**
 * Horizontal drag-to-navigate: the element follows the finger, springs back
 * on a short drag, resists at the ends of the gallery, and slides out before
 * navigating on a long drag or a flick.
 */
export function useSwipeDrag(
  elementRef: React.RefObject<HTMLElement | null>,
  options: SwipeDragOptions,
) {
  const opts = useRef(options);
  opts.current = options;

  useEffect(() => {
    const el = elementRef.current;
    if (!el || options.disabled) return;

    let start: { x: number; y: number; t: number } | null = null;
    let axis: 'x' | 'y' | null = null;
    let offset = 0;
    let fallback: number | undefined;

    const setOffset = (px: number, ms = 0) => {
      el.style.transition = ms ? `transform ${ms}ms ease-out` : 'none';
      el.style.transform = px ? `translate3d(${px}px, 0, 0)` : '';
    };

    const reset = () => {
      start = null;
      axis = null;
      offset = 0;
      el.style.willChange = '';
      setOffset(0, SETTLE_MS);
    };

    const onStart = (e: TouchEvent) => {
      if (e.touches.length !== 1) {
        if (axis === 'x') reset();
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
        if (axis === 'x') el.style.willChange = 'transform';
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
        reset();
        return;
      }

      setOffset(goingNext ? -width : width, EXIT_MS);
      window.setTimeout(() => {
        if (goingNext) opts.current.onNext();
        else opts.current.onPrev();
        window.clearTimeout(fallback);
        fallback = window.setTimeout(reset, RESET_FALLBACK_MS);
      }, EXIT_MS);
    };

    el.addEventListener('touchstart', onStart, { passive: true });
    el.addEventListener('touchmove', onMove, { passive: false });
    el.addEventListener('touchend', onEnd);
    el.addEventListener('touchcancel', reset);
    return () => {
      window.clearTimeout(fallback);
      el.removeEventListener('touchstart', onStart);
      el.removeEventListener('touchmove', onMove);
      el.removeEventListener('touchend', onEnd);
      el.removeEventListener('touchcancel', reset);
      el.style.transition = '';
      el.style.transform = '';
      el.style.willChange = '';
    };
  }, [elementRef, options.disabled]);

  useEffect(() => {
    const el = elementRef.current;
    if (!el) return;
    el.style.transition = 'none';
    el.style.transform = '';
    el.style.willChange = '';
  }, [elementRef, options.resetKey]);
}
