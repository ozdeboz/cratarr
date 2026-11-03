// A track asked of the player that plays it, elsewhere (the app's own player, or one of Cratarr's players the page
// drives): a row tapped, a queue's track jumped to. The page shows it at once; the player's reports keep coming
// meanwhile, and those sent before it took the command still say the track before (the owner, 2026-11-02: the phone
// driving the tablet, a row tapped showed it, then the track before, then it again, in the mini-player, the waveform
// and the row alike). Until a report says the player is on it, or ASKED_MS went by, a report of another is from
// before and not taken; past it the player's word stands, whatever it is (the truth principle). Pure.

export const ASKED_MS = 4000;

/** What was asked: the track (its key in the queue) at `index`, until when the page waits for it. */
export type Asked = { key: string; index: number; until: number } | null;

export const ask = (key: string | null, index: number, now: number): Asked => (key ? { key, index, until: now + ASKED_MS } : null);

/** A report's verdict: `before` (not taken, still waiting) or taken, with what is still asked after it. `keyAt` is the
 *  key of the report's track at an index (its own queue where it sends one, else the page's). */
export function heard(asked: Asked, index: unknown, keyAt: (i: number) => string | null, now: number): { asked: Asked; before: boolean } {
  if (!asked) return { asked: null, before: false };
  const on = index === asked.index && keyAt(asked.index) === asked.key;
  if (on || now > asked.until) return { asked: null, before: false };
  return { asked, before: true };
}
