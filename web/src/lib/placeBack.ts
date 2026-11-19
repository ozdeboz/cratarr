// Where a list was, brought back (F25.4, one function for App's page and every panel step): tried until the list is long
// enough to hold it, a while at most, and stopped at once by the hand.

/** What `placeBack` scrolls: how to go to a place, where it is, how long it is, and where the hand is felt (a wheel, a touch, a
 *  pointer, a key on it stop the tries). */
export type Scroller = { scrollTo: (y: number) => void; at: () => number; long?: () => number; target?: EventTarget | null };

/** The events of the hand that stop a place coming back (F25.6 (b)): its place stands. */
const HAND = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;

/** Brings `y` back on `s`: tried at once, then every `every` ms, `tries` times at most (its content comes in a moment), until it
 *  is there; after the last try it stays where it got, and nothing moves later. The hand stops it at once. Returns its cancel
 *  (a step left, another shown). */
export function placeBack(s: Scroller, y: number, o: { every?: number; tries?: number } = {}): () => void {
  const every = o.every ?? 100;
  const tries = o.tries ?? 40;
  let n = 0;
  let t: ReturnType<typeof setTimeout> | undefined;
  let done = false;
  const cancel = () => {
    if (done) return;
    done = true;
    clearTimeout(t);
    for (const e of HAND) s.target?.removeEventListener(e, cancel);
  };
  for (const e of HAND) s.target?.addEventListener(e, cancel, { passive: true });
  const go = () => {
    if (done) return;
    s.scrollTo(y);
    if (Math.abs(s.at() - y) > 2 && n++ < tries) t = setTimeout(go, every);
    else cancel();
  };
  go();
  return cancel;
}
