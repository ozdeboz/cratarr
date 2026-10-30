// Sliders a scroll passes over (the owner, 2026-10-24), as Apple's do: only the thumb grabs. A finger that lands on a
// slider's thumb (a 44 px target, Apple's least) takes it at once and keeps it, the sheet not scrolling from that
// touch, and the thumb keeps where it was grabbed (no jump to the finger). Anywhere else on the track a finger does
// nothing to the slider: it is the scroll's (no tap-to-jump); unless it is held still there half a second (a long
// press, the owner): then the thumb comes to it, with a tick, and follows it from there. A mouse, which scrolls nothing by accident, still sets
// it wherever it presses and drags it, as desktop sliders do. A range input's own touch handling would take any touch
// that lands on it, so the input lets pointers through (pointer-events: none) and its parent decides. The input keeps
// its keyboard and its own handlers: each change is told to it as its own `input` events, and a `change` at the end.
// Not for two inputs over one track (lib/TempoRange's low and high): which of them a press is for is theirs to say.

/** The thumb's half width: its centre travels this far in from either end (Chromium's 16 px thumb). */
const INSET = 8;
/** How far from the thumb's centre a finger still lands on it: half of Apple's 44 px least target. */
const REACH = 22;
/** A long press: held this long (ms), moving no further than SLOP px (as lib/longpress). */
const HOLD_MS = 500;
const SLOP = 10;

export function panSafe(input: HTMLInputElement) {
  const host = input.parentElement;
  if (!host) return {};
  input.style.pointerEvents = 'none';
  /** The pointer dragging it, and how far from the thumb's centre it took hold. */
  let held: { id: number; offset: number } | null = null;
  /** A finger on the track off the thumb, waiting to become a long press. */
  let pending: { id: number; x: number; y: number; timer: ReturnType<typeof setTimeout> } | null = null;
  const unpend = () => {
    if (pending) clearTimeout(pending.timer);
    pending = null;
  };

  const span = () => {
    const r = input.getBoundingClientRect();
    return { r, left: r.left + INSET, width: Math.max(1, r.width - 2 * INSET) };
  };
  const bounds = () => ({ min: Number(input.min || 0), max: Number(input.max || 100), step: Number(input.step || 1) });
  /** Where the thumb's centre is now (px). */
  const thumbX = () => {
    const { left, width } = span();
    const { min, max } = bounds();
    return left + ((Number(input.value) - min) / (max - min || 1)) * width;
  };
  const near = (x: number, y: number) => {
    const { r } = span();
    return y >= r.top - 16 && y <= r.bottom + 16 && x >= r.left - 4 && x <= r.right + 4;
  };
  const onThumb = (x: number, y: number) => near(x, y) && Math.abs(x - thumbX()) <= REACH;

  /** The value under `x` (the thumb's centre), by the input's own min, max and step, told to it when it changes. */
  const setFrom = (x: number) => {
    const { left, width } = span();
    const { min, max, step } = bounds();
    const p = Math.max(0, Math.min(1, (x - left) / width));
    let v = min + p * (max - min);
    if (step > 0) v = Math.min(max, min + Math.round((v - min) / step) * step);
    const s = String(Number(v.toFixed(6)));
    if (s !== input.value) {
      input.value = s;
      input.dispatchEvent(new Event('input', { bubbles: true }));
    }
  };

  // A finger on the thumb: no scroll from this touch (decided when it lands, as the browser decides it then).
  function touchstart(e: TouchEvent) {
    const t = e.touches[0];
    if (!input.disabled && e.touches.length === 1 && onThumb(t.clientX, t.clientY)) e.preventDefault();
  }
  function down(e: PointerEvent) {
    if (input.disabled || e.button > 0 || held) return;
    if (e.pointerType === 'mouse') {
      if (!near(e.clientX, e.clientY)) return;
      e.preventDefault();
      input.focus({ preventScroll: true });
      // On the thumb it keeps where it was grabbed; on the track it goes there.
      const offset = onThumb(e.clientX, e.clientY) ? e.clientX - thumbX() : 0;
      held = { id: e.pointerId, offset };
      if (!offset) setFrom(e.clientX);
    } else if (onThumb(e.clientX, e.clientY)) {
      held = { id: e.pointerId, offset: e.clientX - thumbX() };
    } else {
      // Off the thumb: the scroll's, unless it is held still.
      if (!near(e.clientX, e.clientY)) return;
      unpend();
      const id = e.pointerId, x = e.clientX;
      pending = {
        id,
        x,
        y: e.clientY,
        timer: setTimeout(() => {
          pending = null;
          held = { id, offset: 0 };
          navigator.vibrate?.(8);
          try {
            host!.setPointerCapture(id);
          } catch {
            // Lifted meanwhile.
          }
          setFrom(x);
        }, HOLD_MS),
      };
      return;
    }
    host!.setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (pending && e.pointerId === pending.id && Math.hypot(e.clientX - pending.x, e.clientY - pending.y) > SLOP) unpend();
    if (!held || e.pointerId !== held.id) return;
    e.preventDefault();
    setFrom(e.clientX - held.offset);
  }
  function end(e: PointerEvent) {
    if (pending && e.pointerId === pending.id) unpend();
    if (!held || e.pointerId !== held.id) return;
    held = null;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }

  // Once a long press holds it, the finger's moves are the slider's, not the scroll's; nor does the press open the
  // browser's own menu.
  const touchmove = (e: TouchEvent) => held && e.cancelable && e.preventDefault();
  const contextmenu = (e: Event) => (held || pending) && e.preventDefault();
  host.addEventListener('touchstart', touchstart, { passive: false });
  host.addEventListener('touchmove', touchmove, { passive: false });
  host.addEventListener('contextmenu', contextmenu);
  host.addEventListener('pointerdown', down);
  host.addEventListener('pointermove', move);
  host.addEventListener('pointerup', end);
  host.addEventListener('pointercancel', end);
  return {
    destroy() {
      unpend();
      host.removeEventListener('touchstart', touchstart);
      host.removeEventListener('touchmove', touchmove);
      host.removeEventListener('contextmenu', contextmenu);
      host.removeEventListener('pointerdown', down);
      host.removeEventListener('pointermove', move);
      host.removeEventListener('pointerup', end);
      host.removeEventListener('pointercancel', end);
      input.style.pointerEvents = '';
    },
  };
}
