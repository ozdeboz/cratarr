// A cover held (decided 2026-10-16, the queue's covers): after HOLD_MS without moving, `start`; when the finger lifts
// or the press is lost, `end`. A short tap stays a tap; the lift that ends a hold is not one. The press is the cover's
// own, so the row's long-press (its menu) and swipe do not see it; a finger that moves first is a scroll.
const HOLD_MS = 250;
const SLOP_PX = 10;

type Hold = { start: () => void; end: () => void; enabled: boolean; ms?: number };

export function hold(node: HTMLElement, initial: Hold) {
  let opts = initial;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let from: { x: number; y: number; id: number } | null = null;
  let held = false;

  const stop = () => {
    clearTimeout(timer);
    from = null;
    if (held) opts.end();
  };
  function down(e: PointerEvent) {
    if (!opts.enabled || e.button > 0) return;
    e.stopPropagation();
    held = false;
    from = { x: e.clientX, y: e.clientY, id: e.pointerId };
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (!from) return;
      held = true;
      node.setPointerCapture?.(from.id);
      opts.start();
    }, opts.ms ?? HOLD_MS);
  }
  function move(e: PointerEvent) {
    if (from && !held && Math.hypot(e.clientX - from.x, e.clientY - from.y) > SLOP_PX) {
      clearTimeout(timer);
      from = null;
    }
  }
  // The tap that ends a hold goes nowhere; the menu the browser shows for a held image neither.
  function click(e: MouseEvent) {
    if (!held) return;
    held = false;
    e.preventDefault();
    e.stopPropagation();
  }
  const menu = (e: Event) => opts.enabled && e.preventDefault();

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', stop);
  node.addEventListener('pointercancel', stop);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  return {
    update(next: Hold) {
      opts = next;
    },
    destroy() {
      if (held) opts.end();
      clearTimeout(timer);
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', stop);
      node.removeEventListener('pointercancel', stop);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
    },
  };
}
