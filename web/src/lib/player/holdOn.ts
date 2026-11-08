// Holding on server loss (the owner, 2026-11-06): with Cratarr out of reach a player stops at the place reached, keeps the
// queue and carries on by itself when Cratarr answers. What a track that could not be asked for is held at, and when it is
// asked for again: sooner at first, as Cratarr restarts in about 15 s, then no more than a knock now and then. Pure, so
// any head that fetches a track's sound can hold by it (lib/localPlayer's stream today, the tape's next).

/** The track held for: where in the queue, from where in it (s), asked from its top all the same (`whole`); how many asks have
 *  failed since it began, and when the next one is due (ms). A new stream ends it. */
export type HeldFor = { index: number; from: number; whole: boolean; tries: number; askAt: number };

/** The first wait, doubled at each ask that fails, up to the last. */
export const HOLD_FIRST_MS = 2_000;
export const HOLD_MOST_MS = 30_000;

/** A failed ask for `at` at `now`: held, or held on, for as long as a refusal asks (`refused`, ms: lib/reach's). */
export function heldAt(was: HeldFor | null, at: Pick<HeldFor, 'index' | 'from' | 'whole'>, now: number, refused = 0): HeldFor {
  const tries = was && was.index === at.index ? was.tries + 1 : 0;
  return { ...at, tries, askAt: now + Math.max(Math.min(HOLD_FIRST_MS * 2 ** tries, HOLD_MOST_MS), refused) };
}

/** A queue changed from place `at` on: what it holds for there is whatever is at `at` now, from its start; none left, nothing. */
export function holdAfterCut(h: HeldFor | null, at: number, length: number): HeldFor | null {
  if (!h || at > h.index) return h;
  return at < length ? { ...h, index: at, from: 0, whole: false, tries: 0 } : null;
}
