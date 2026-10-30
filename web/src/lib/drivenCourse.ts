// A player driven through the server (Play On) as this page counts it on between its reports (the owner, 2026-10-30:
// the tablet driving the ThinkPad "doesn't show reality" through a wind; it glided to each report and then stood).
// A report says the player's place, when it was so (`position_at`), whether it winds, and, where the player and the
// server say it, its speed. Its speed otherwise: 1 playing, 0 paused, and through a wind the speed between its last two
// places, as measured. The page then counts on as it does for the app's own deck (lib/deckCourse), each report's
// correction fading in. The same measure gives a page's own player the speed it reports while it winds.

/** A place of the player's: which track, where in it (s), when it was so (ms, on the clock it was read by). */
export type Seen = { index: number; pos: number; at: number };
/** What was last seen, and the speed made of it. */
export type Measured = { seen: Seen; v: number | null };

/** Two places further apart than this in time say nothing of the speed now (another motion, a gap in the reports). */
export const MEASURE_MAX_MS = 1500;
/** Nor two places closer than this (a repeated report, a jitter's worth). */
export const MEASURE_MIN_MS = 50;

/** The speed between two places of one track (track s per s, signed); null across a track or over too long or too
 *  short a time. */
export function speedBetween(a: Seen | null, b: Seen): number | null {
  if (!a || a.index !== b.index) return null;
  const dt = b.at - a.at;
  if (dt < MEASURE_MIN_MS || dt > MEASURE_MAX_MS) return null;
  return (b.pos - a.pos) / (dt / 1000);
}

/** The same place reported again for longer than this: it stands (a winding player reports every 250 ms). */
export const STAND_MS = 400;

/** A new place seen: the speed through a wind, measured. The same place again keeps when it was first seen (a 0.11.0
 *  server stamps a view when it sends it and does not count a winding player on, so another player's change sends the
 *  same report anew, later; from 0.11.1 it counts a winding player on at its own speed, so a view re-sent says a new
 *  place) and the speed it had, until it has stood longer than a report's interval: then it stands. */
export function measure(was: Measured | null, seen: Seen): Measured {
  if (was && was.seen.index === seen.index && was.seen.pos === seen.pos) return seen.at - was.seen.at > STAND_MS ? { seen: was.seen, v: 0 } : was;
  return { seen, v: speedBetween(was?.seen ?? null, seen) };
}

/** The speed to count on from a driven player's report: as it says it; else 1 playing and 0 paused; through a wind as
 *  measured, or none yet (the first report of a wind: shown as it says, as before). */
export function speedOf(said: unknown, rewinding: boolean, playing: boolean, measured: number | null): number | undefined {
  if (typeof said === 'number' && Number.isFinite(said)) return said;
  if (!rewinding) return playing ? 1 : 0;
  return measured ?? undefined;
}
