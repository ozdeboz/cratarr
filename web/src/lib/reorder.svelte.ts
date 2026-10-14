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

  /** `list`: the scrolling list; `count`: how many rows it has; `onmove`: a row was dropped elsewhere. */
  constructor(list: () => HTMLElement | undefined, count: () => number, onmove: (from: number, to: number) => void) {
    this.#list = list;
    this.#count = count;
    this.#onmove = onmove;
  }

  /** The handle of row `i` was pressed. */
  down = (e: PointerEvent, i: number) => {
    const list = this.#list();
    if (!list) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    this.#start = { y: e.clientY, scroll: list.scrollTop, rowH: el.closest('li')!.getBoundingClientRect().height };
    this.drag = { from: i, to: i, dy: 0 };
    navigator.vibrate?.(8);
  };

  #to(y: number) {
    const list = this.#list();
    if (!this.drag || !list) return;
    const dy = y - this.#start.y + (list.scrollTop - this.#start.scroll);
    const to = Math.min(this.#count() - 1, Math.max(0, this.drag.from + Math.round(dy / this.#start.rowH)));
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
