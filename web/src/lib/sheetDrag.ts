// Closing a sheet by dragging it down (up, for one that comes from the top), from anywhere on it (the owner,
// 2026-10-16): a press on its card, a button or a list scrolled to its top, moved mostly downward, drags the whole
// sheet, if the list under it was at its top when the press began (a scroll that gets there is still a scroll). Past a
// third of its height (at most 120 px) or with a flick it closes; less springs back. Text fields and
// sliders keep their own gestures, as does anything marked `data-sheet-drag="off"` (a reorder grip); a list not at its
// top scrolls; a sideways move (a row's swipe) is left alone. A drag cancels the click it began on. (Not "whatever
// captured the pointer": on a touch screen every press is captured by the element it began on.)

export type SheetDragOptions = {
  onclose: () => void;
  /** The offset to show while dragging (px, signed: down is positive); 0 when it springs back. */
  ondrag: (dy: number) => void;
  /** A sheet that comes from the top: dragged up to close. */
  top?: boolean;
  enabled?: boolean;
};

/** How far (px) the press moves before it counts as a drag or not. */
const SLOP = 8;

export function sheetDrag(node: HTMLElement, options: SheetDragOptions) {
  let opts = options;
  let press: { id: number; x: number; y: number; t: number; scroller: HTMLElement | null; edge: boolean } | null = null;
  let dragging = false;
  let offset = 0;
  /** The drag's recent moves (time, offset), for a flick's speed. */
  let recent: { t: number; o: number }[] = [];
  const dir = () => (opts.top ? -1 : 1);

  /** The nearest scrollable element between the target and the sheet. */
  const scrollerOf = (el: Element | null): HTMLElement | null => {
    for (let e = el; e && e !== node; e = e.parentElement) {
      if (e instanceof HTMLElement && e.scrollHeight > e.clientHeight + 1) {
        const y = getComputedStyle(e).overflowY;
        if (y === 'auto' || y === 'scroll') return e;
      }
    }
    return null;
  };
  /** Whether the list under the press is at the end the sheet is dragged away from (the top, for a sheet below). */
  const atEdge = (s: HTMLElement | null) =>
    !s || (opts.top ? s.scrollTop + s.clientHeight >= s.scrollHeight - 1 : s.scrollTop <= 0);

  function down(e: PointerEvent) {
    if (opts.enabled === false || !e.isPrimary || e.button !== 0) return;
    const target = e.target as Element;
    if (target.closest('input, select, textarea, [contenteditable="true"], [data-sheet-drag="off"]')) return;
    // Whether its list is at its top as the press begins (the owner, 71.1): a scroll that reaches the top on the way
    // stays a scroll; closing takes a new pull from the top.
    const scroller = scrollerOf(target);
    press = { id: e.pointerId, x: e.clientX, y: e.clientY, t: performance.now(), scroller, edge: atEdge(scroller) };
    dragging = false;
  }

  function move(e: PointerEvent) {
    if (!press || e.pointerId !== press.id) return;
    // Something on it took the press over meanwhile (a row lifted by a long press): not the sheet's.
    if (opts.enabled === false && !dragging) return void (press = null);
    const dx = e.clientX - press.x;
    const dy = (e.clientY - press.y) * dir();
    if (!dragging) {
      // Sideways, or the other way: not a drag.
      if ((Math.abs(dx) > SLOP && Math.abs(dx) > Math.abs(dy)) || dy < -SLOP) {
        press = null;
        return;
      }
      if (dy < SLOP) return;
      if (!press.edge) {
        press = null;
        return;
      }
      dragging = true;
      node.setPointerCapture(e.pointerId);
    }
    offset = Math.max(0, dy);
    const now = performance.now();
    recent = [...recent.filter((r) => now - r.t < 100), { t: now, o: offset }];
    opts.ondrag(offset * dir());
  }

  function up(e: PointerEvent) {
    if (!press || e.pointerId !== press.id) return;
    press = null;
    if (!dragging) return;
    dragging = false;
    // The click the press began on (a button under it) does not happen.
    const swallow = (c: Event) => {
      c.stopPropagation();
      c.preventDefault();
    };
    addEventListener('click', swallow, { capture: true, once: true });
    setTimeout(() => removeEventListener('click', swallow, { capture: true }), 0);
    // A flick: the speed over the last 100 ms of the drag (px/ms), not since the press began.
    const first = recent[0];
    const speed = first ? (offset - first.o) / Math.max(1, performance.now() - first.t) : 0;
    recent = [];
    const far = Math.min(120, node.offsetHeight / 3);
    if (e.type !== 'pointercancel' && (offset > far || (offset > 20 && speed > 0.7))) opts.onclose();
    else opts.ondrag(0);
    offset = 0;
  }

  // On a touch screen the browser pans a list (or the page behind) unless its move is cancelled: once the press goes
  // the closing way from a list at its edge, it is the sheet's.
  function touchMove(e: TouchEvent) {
    if (!press || !e.cancelable || (opts.enabled === false && !dragging)) return;
    const t = e.touches[0];
    if (!t) return;
    const dx = t.clientX - press.x;
    const dy = (t.clientY - press.y) * dir();
    if (dragging || (dy > 0 && Math.abs(dy) >= Math.abs(dx) && press.edge)) e.preventDefault();
  }

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
  node.addEventListener('touchmove', touchMove, { passive: false });
  return {
    update(next: SheetDragOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', up);
      node.removeEventListener('touchmove', touchMove);
    },
  };
}
