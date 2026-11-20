// Where a list was, brought back (F25.4, one function for App's page and every panel step): tried until the list is long
// enough to hold it, a while at most, and stopped at once by the hand.

/** What `placeBack` scrolls: how to go to a place, where it is, how long it is, and where the hand is felt (a wheel, a touch, a
 *  pointer, a key on it stop the tries). `keys`: where a key is felt too, the window by default (the review of 1.2, low 13: a key
 *  goes to what has the focus, the body as often as the list, so PageDown was fought for the tries' 4 s). */
export type Scroller = { scrollTo: (y: number) => void; at: () => number; long?: () => number; target?: EventTarget | null; keys?: EventTarget | null };

/** The events of the hand that stop a place coming back (F25.6 (b)): its place stands. */
const HAND = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;

/** A place coming back: its cancel (a step left, another shown), and whether it still runs (its tries' scrolls are no place of
 *  the hand's, to be written down: lib/keepsPlace). */
export type Restore = (() => void) & { running: () => boolean };

/** Brings `y` back on `s`: tried at once, then every `every` ms, `tries` times at most (its content comes in a moment), until it
 *  is there; after the last try it stays where it got, and nothing moves later. The hand stops it at once, a key anywhere too. */
export function placeBack(s: Scroller, y: number, o: { every?: number; tries?: number } = {}): Restore {
  const every = o.every ?? 100;
  const tries = o.tries ?? 40;
  const keys = s.keys !== undefined ? s.keys : typeof window === 'undefined' ? null : window;
  let n = 0;
  let t: ReturnType<typeof setTimeout> | undefined;
  let done = false;
  const cancel = () => {
    if (done) return;
    done = true;
    clearTimeout(t);
    for (const e of HAND) s.target?.removeEventListener(e, cancel);
    keys?.removeEventListener('keydown', cancel);
  };
  for (const e of HAND) s.target?.addEventListener(e, cancel, { passive: true });
  keys?.addEventListener('keydown', cancel, { passive: true });
  const go = () => {
    if (done) return;
    s.scrollTo(y);
    if (Math.abs(s.at() - y) > 2 && n++ < tries) t = setTimeout(go, every);
    else cancel();
  };
  go();
  return Object.assign(cancel, { running: () => !done });
}
