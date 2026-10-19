// Finamp's swipe on a list row (its Dismissible): the row follows the finger; past 65 % of its width the action runs,
// else it springs back (200 ms). A row has an action for each way it can go (the owner, 2026-10-19): `left` removes it
// from its list (the queue, a playlist, a crate), `right` plays it next; a way without one does not move. Each says what
// then happens to the row: 'away' slides it out, 'back' springs it back. What the swipe does shows in the strip the
// row uncovers as it moves (lib/SwipeBehind, beside the row in its list item): the row's place and how far it went are
// told to its parent as `--swipe-*`. Vertical moves stay the list's scrolling, and a swipe never also counts as a tap.

type Way = (() => 'away' | 'back') | null | undefined;
type Options = { enabled: boolean; left?: Way; right?: Way };

const THRESHOLD = 0.65;

export function swipe(node: HTMLElement, options: Options) {
  let opts = options;
  let from: { x: number; y: number; id: number } | null = null;
  let on = false;
  let swiped = false;

  const place = (dx: number, animate: boolean) => {
    node.style.transition = animate ? 'transform 200ms linear' : 'none';
    node.style.transform = dx ? `translateX(${dx}px)` : '';
    const p = node.parentElement;
    if (!p) return;
    if (animate) delete p.dataset.swiping;
    else p.dataset.swiping = '';
    for (const [k, v] of [
      ['--swipe-x', dx],
      ['--swipe-left', node.offsetLeft],
      ['--swipe-top', node.offsetTop],
      ['--swipe-width', node.offsetWidth],
      ['--swipe-height', node.offsetHeight],
    ] as const)
      p.style.setProperty(k, `${v}px`);
  };
  /** After a row slid out: put it back, for the list item it may now show (a list keyed by place). */
  const reset = () => {
    node.style.transition = 'none';
    node.style.transform = '';
    node.parentElement?.style.setProperty('--swipe-x', '0px');
    delete node.dataset.swiping;
  };
  /** How far the row goes: only the ways it has an action for. */
  const allowed = (dx: number) => (dx < 0 ? (opts.left ? dx : 0) : opts.right ? dx : 0);

  function down(e: PointerEvent) {
    if (!opts.enabled || (!opts.left && !opts.right)) return;
    from = { x: e.clientX, y: e.clientY, id: e.pointerId };
    on = false;
  }
  function move(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const [dx, dy] = [e.clientX - from.x, e.clientY - from.y];
    if (!on) {
      if (Math.abs(dy) > 10 && Math.abs(dy) > Math.abs(dx)) return void (from = null);
      if (Math.abs(dx) < 10 || Math.abs(dx) < Math.abs(dy) || !allowed(dx)) return;
      on = true;
      node.setPointerCapture(e.pointerId);
      node.dataset.swiping = '';
    }
    place(allowed(dx), false);
  }
  function up(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const dx = allowed(e.clientX - from.x);
    from = null;
    if (!on) return;
    on = false;
    swiped = true;
    const act = dx < 0 ? opts.left : opts.right;
    if (act && Math.abs(dx) > node.offsetWidth * THRESHOLD) {
      navigator.vibrate?.(25);
      if (act() === 'away') {
        place(Math.sign(dx) * node.offsetWidth, true);
        setTimeout(reset, 260);
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
