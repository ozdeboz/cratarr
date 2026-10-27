// Holding Previous or Next winds the tape (decision 0074; the owner, 2026-10-27): a press kept past HOLD_MS starts
// `onhold`, letting go (or leaving the button) `onrelease`, and the tap that ends a hold is swallowed; a shorter press
// is the button's own tap. Null: the button is only tapped (winding off). Its label's long-press (lib/labels) gives way.

const HOLD_MS = 350;

export type Hold = { onhold: () => void; onrelease: () => void } | null;

export function holdWind(node: HTMLElement, hold: Hold) {
  let now = hold;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let held = false;

  function down(e: PointerEvent) {
    if (!now || e.button > 0) return;
    held = false;
    node.setPointerCapture?.(e.pointerId);
    const h = now;
    timer = setTimeout(() => {
      timer = undefined;
      held = true;
      navigator.vibrate?.(8);
      h.onhold();
    }, HOLD_MS);
  }
  function up() {
    clearTimeout(timer);
    timer = undefined;
    if (held) now?.onrelease();
  }
  function click(e: MouseEvent) {
    if (!held) return;
    held = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }
  const menu = (e: Event) => now && e.preventDefault();
  function mark() {
    if (now) node.setAttribute('data-longpress', '');
    else node.removeAttribute('data-longpress');
  }

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  node.style.setProperty('-webkit-touch-callout', 'none');
  node.style.userSelect = 'none';
  mark();

  return {
    update(next: Hold) {
      now = next;
      mark();
    },
    destroy() {
      if (held) now?.onrelease();
      clearTimeout(timer);
      node.removeAttribute('data-longpress');
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', up);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
    },
  };
}
