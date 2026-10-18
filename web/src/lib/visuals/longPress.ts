// A long press on an element (half a second, held still), or a right click or the context-menu key: `then` runs, and the
// click that ends the press does nothing else. The visuals' saved looks: held, one can be renamed or deleted.

export function longPress(node: HTMLElement, then: () => void) {
  let run = then;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let from: { x: number; y: number } | null = null;
  // Whether this press has run `then` (its click is swallowed); a browser that also sends a context menu for a long
  // press runs it once.
  let fired = false;
  const fire = () => {
    clearTimeout(timer);
    if (fired) return;
    fired = true;
    run();
  };
  const down = (e: PointerEvent) => {
    fired = false;
    from = { x: e.clientX, y: e.clientY };
    clearTimeout(timer);
    timer = setTimeout(fire, 500);
  };
  const move = (e: PointerEvent) => {
    if (from && Math.hypot(e.clientX - from.x, e.clientY - from.y) > 10) clearTimeout(timer);
  };
  const up = () => clearTimeout(timer);
  const click = (e: MouseEvent) => {
    if (!fired) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    fired = false;
  };
  const menu = (e: MouseEvent) => {
    e.preventDefault();
    fire();
  };
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  return {
    update(next: () => void) {
      run = next;
    },
    destroy() {
      clearTimeout(timer);
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', up);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
    },
  };
}
