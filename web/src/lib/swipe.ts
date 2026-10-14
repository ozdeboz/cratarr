// Finamp's swipe on a list row (its Dismissible, right to left): the row follows the finger; past 65 % of its width the
// action runs, else it springs back (200 ms). `onswipe` says what then happens to the row: 'away' slides it out (the
// queue's removal), 'back' springs it back (the playlist view's Play next). Vertical moves stay the list's scrolling,
// and a swipe never also counts as a tap.

type Options = { enabled: boolean; onswipe: () => 'away' | 'back' };

const THRESHOLD = 0.65;

export function swipe(node: HTMLElement, options: Options) {
  let opts = options;
  let from: { x: number; y: number; id: number } | null = null;
  let on = false;
  let swiped = false;

  const place = (dx: number, animate: boolean) => {
    node.style.transition = animate ? 'transform 200ms linear' : 'none';
    node.style.transform = dx ? `translateX(${dx}px)` : '';
  };

  function down(e: PointerEvent) {
    if (!opts.enabled) return;
    from = { x: e.clientX, y: e.clientY, id: e.pointerId };
    on = false;
  }
  function move(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const [dx, dy] = [e.clientX - from.x, e.clientY - from.y];
    if (!on) {
      if (Math.abs(dy) > 10 && Math.abs(dy) > Math.abs(dx)) return void (from = null);
      if (dx > -10 || Math.abs(dx) < Math.abs(dy)) return;
      on = true;
      node.setPointerCapture(e.pointerId);
      node.dataset.swiping = '';
    }
    place(Math.min(0, dx), false);
  }
  function up(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const dx = e.clientX - from.x;
    from = null;
    if (!on) return;
    on = false;
    swiped = true;
    if (-dx > node.offsetWidth * THRESHOLD) {
      navigator.vibrate?.(25);
      if (opts.onswipe() === 'away') {
        place(-node.offsetWidth, true);
        return;
      }
    }
    place(0, true);
    setTimeout(() => delete node.dataset.swiping, 200);
  }
  function click(e: MouseEvent) {
    if (!swiped) return;
    swiped = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }

  node.style.touchAction = 'pan-y';
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
  node.addEventListener('click', click, true);
  return {
    update(next: Options) {
      opts = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', up);
      node.removeEventListener('click', click, true);
    },
  };
}
