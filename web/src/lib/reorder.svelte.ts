// Finamp's reorder handle on a list's rows (the queue sheet, a playlist in edit mode): the drag starts at once on the
// handle; the row follows the finger, the rows it passes make room, the list scrolls near its edges, and `onmove(from,
// to)` runs on release. The rows read `drag` (lift the dragged one) and `offset(i)` (where each one is drawn).

export class Reorder {
  drag = $state<{ from: number; to: number; dy: number } | null>(null);
  #start = { y: 0, scroll: 0, rowH: 70 };
  #timer: ReturnType<typeof setInterval> | undefined;
  #lastY = 0;
  #list: () => HTMLElement | undefined;
  #count: () => number;
  #onmove: (from: number, to: number) => void;
  #range: ((from: number) => [number, number]) | undefined;

  /** `list`: the scrolling list; `count`: how many rows it has; `onmove`: a row was dropped elsewhere; `range`: where
   *  the row at `from` may go (first and last place), the whole list without it. The row stops at its range's edge
   *  while dragged, never past it to be put back on release (the owner, 2026-10-24: Home's top slot). */
  constructor(
    list: () => HTMLElement | undefined,
    count: () => number,
    onmove: (from: number, to: number) => void,
    range?: (from: number) => [number, number],
  ) {
    this.#list = list;
    this.#count = count;
    this.#onmove = onmove;
    this.#range = range;
  }

  /** The handle of row `i` was pressed. */
  down = (e: PointerEvent, i: number) => this.lift(e.currentTarget as HTMLElement, e.pointerId, e.clientY, i);

  /** Row `i` lifted by `el` (its handle, or the row held: the queue's long press, the owner 2026-10-26), the pointer
   *  `pointerId` at `y`. */
  lift = (el: HTMLElement, pointerId: number, y: number, i: number) => {
    const list = this.#list();
    if (!list) return;
    try {
      el.setPointerCapture(pointerId);
    } catch {
      /* the press has ended */
      return;
    }
    this.#start = { y, scroll: list.scrollTop, rowH: el.closest('li')!.getBoundingClientRect().height };
    this.drag = { from: i, to: i, dy: 0 };
    navigator.vibrate?.(8);
  };

  #to(y: number) {
    const list = this.#list();
    if (!this.drag || !list) return;
    const from = this.drag.from;
    const [lo, hi] = this.#range?.(from) ?? [0, this.#count() - 1];
    const rowH = this.#start.rowH;
    // The row itself goes no further than its range either.
    const dy = Math.min((hi - from) * rowH, Math.max((lo - from) * rowH, y - this.#start.y + (list.scrollTop - this.#start.scroll)));
    const to = Math.min(hi, Math.max(lo, from + Math.round(dy / rowH)));
    this.drag = { ...this.drag, dy, to };
  }

  move = (e: PointerEvent) => {
    const list = this.#list();
    if (!this.drag || !list) return;
    this.#lastY = e.clientY;
    this.#to(this.#lastY);
    // Near the list's top or bottom edge it scrolls, and the row goes along.
    const r = list.getBoundingClientRect();
    const speed = this.#lastY < r.top + 48 ? -8 : this.#lastY > r.bottom - 48 ? 8 : 0;
    clearInterval(this.#timer);
    if (speed)
      this.#timer = setInterval(() => {
        list.scrollTop += speed;
        this.#to(this.#lastY);
      }, 16);
  };

  end = () => {
    clearInterval(this.#timer);
    if (!this.drag) return;
    const { from, to } = this.drag;
    this.drag = null;
    navigator.vibrate?.(25);
    if (from !== to) this.#onmove(from, to);
  };

  /** Where row `j` is drawn during a drag: the dragged one under the finger, the ones it passed one row over. */
  offset = (j: number): number => {
    if (!this.drag) return 0;
    const { from, to, dy } = this.drag;
    if (j === from) return dy;
    if (from < to && j > from && j <= to) return -this.#start.rowH;
    if (to < from && j >= to && j < from) return this.#start.rowH;
    return 0;
  };
}
