// Only what is near is made (2026-10-16: the whole Library at once hung the app; the owner, 2026-10-25: Discover's
// More for you the same way): a long list cut into blocks, each made while it is within a screen of what shows, else an
// empty box as tall as it was when last made (an estimate before that), so the page keeps its length and the scroll
// does not jump. Watched against the box that scrolls (a margin reaches no further than the box that clips it). The
// Library's albums (lib/AlbumShelf) and Discover's More for you.

const scrollerOf = (node: Element): Element | null => {
  for (let el = node.parentElement; el; el = el.parentElement) {
    const o = getComputedStyle(el).overflowY;
    if (o === 'auto' || o === 'scroll') return el;
  }
  return null;
};

export class NearBlocks {
  /** Blocks within reach (made), by key. */
  near = $state<Record<string, boolean>>({});
  /** Each block's height when last made, by key. */
  heights = $state<Record<string, number>>({});
  /** Each block's rows, as last watched. */
  rowsOf: Record<string, number> = {};
  #seen: IntersectionObserver | null = null;
  #sized = new ResizeObserver((entries) => {
    for (const e of entries) {
      const key = (e.target as HTMLElement).dataset.key!;
      if (this.near[key] && e.contentRect.height > 0) this.heights[key] = e.contentRect.height;
    }
  });

  /** Whether a block is made: near, or the first one before anything is known. */
  made = (key: string, first: boolean) => this.near[key] ?? first;

  /** Watches a block (an attachment): made while near, measured while made. */
  watch = (key: string, rows = 1) => (node: HTMLElement) => {
    this.rowsOf[key] = rows;
    this.#seen ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) this.near[(e.target as HTMLElement).dataset.key!] = e.isIntersecting;
      },
      { root: scrollerOf(node), rootMargin: '100% 0px' },
    );
    node.dataset.key = key;
    this.#seen.observe(node);
    this.#sized.observe(node);
    return () => {
      this.#seen?.unobserve(node);
      this.#sized.unobserve(node);
    };
  };

  /** Stops watching (the list's component gone). */
  dispose() {
    this.#seen?.disconnect();
    this.#sized.disconnect();
  }
}
