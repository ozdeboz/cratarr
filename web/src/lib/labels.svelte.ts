// Icon buttons carry no text, on any width: a long-press (touch, pen) or a right-click on one shows what it does, in a
// small tip beside it (decided 2026-10-16, docs/ui-conventions.md). One listener for the whole page, so every icon
// button has it: a button or link whose only content is its icon, named by its title or aria-label. A row's own
// long-press (lib/longpress) does not start on an icon button inside it, and an icon button with a long-press of its
// own keeps that. The tap that ends a long-press is swallowed.

const HOLD_MS = 500;
const SLOP_PX = 10;
const SHOWN_MS = 1500;

/** The tip on show (lib/LabelTip): its words, and the button's middle and edge it stands by. */
export const tip = $state({ shown: false, text: '', x: 0, y: 0, below: false });

/** The icon button `target` is in, if it is in one. */
export function iconButton(target: EventTarget | null): HTMLElement | null {
  const b = target instanceof Element ? (target.closest('button, a[href], [role="button"]') as HTMLElement | null) : null;
  if (!b || b.textContent?.trim()) return null;
  return b.title || b.getAttribute('aria-label') ? b : null;
}

let hide: ReturnType<typeof setTimeout> | undefined;
function show(b: HTMLElement) {
  const r = b.getBoundingClientRect();
  // Above the button; below it near the top (the floating row), where above would cover the bar or leave the screen.
  const below = r.top < innerHeight / 4;
  Object.assign(tip, { shown: true, text: b.title || b.getAttribute('aria-label') || '', x: r.left + r.width / 2, y: below ? r.bottom : r.top, below });
  navigator.vibrate?.(8);
  clearTimeout(hide);
  hide = setTimeout(() => (tip.shown = false), SHOWN_MS);
}

/** Listens for the whole page; returns the undoing (an effect's cleanup). */
export function installLabels(): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let start: { x: number; y: number } | null = null;
  let fired = false;
  const own = (b: HTMLElement | null) => b !== null && !b.hasAttribute('data-longpress');

  const cancel = () => {
    clearTimeout(timer);
    start = null;
  };
  function down(e: PointerEvent) {
    fired = false;
    tip.shown = false;
    if (e.pointerType === 'mouse') return; // a mouse has right-click
    const b = iconButton(e.target);
    if (!own(b)) return;
    start = { x: e.clientX, y: e.clientY };
    timer = setTimeout(() => {
      fired = true;
      start = null;
      show(b!);
    }, HOLD_MS);
  }
  function move(e: PointerEvent) {
    if (start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > SLOP_PX) cancel();
  }
  function click(e: MouseEvent) {
    if (!fired) return;
    fired = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }
  // Right-click, and touch browsers' own long-press signal.
  function contextmenu(e: MouseEvent) {
    const b = iconButton(e.target);
    if (!own(b)) return;
    e.preventDefault();
    cancel();
    if (!fired) show(b!);
    fired = e.button !== 2; // a touch long-press is followed by its tap; a right-click is not
  }

  const opts = { capture: true };
  addEventListener('pointerdown', down, opts);
  addEventListener('pointermove', move, opts);
  addEventListener('pointerup', cancel, opts);
  addEventListener('pointercancel', cancel, opts);
  addEventListener('click', click, opts);
  addEventListener('contextmenu', contextmenu, opts);
  addEventListener('scroll', cancel, opts);
  return () => {
    cancel();
    removeEventListener('pointerdown', down, opts);
    removeEventListener('pointermove', move, opts);
    removeEventListener('pointerup', cancel, opts);
    removeEventListener('pointercancel', cancel, opts);
    removeEventListener('click', click, opts);
    removeEventListener('contextmenu', contextmenu, opts);
    removeEventListener('scroll', cancel, opts);
  };
}
