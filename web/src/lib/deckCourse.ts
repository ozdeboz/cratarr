// The app's deck as this page draws it (reports, 0.9.14-2; the owner, 2026-10-29: a held wind's picture went in
// strides, its sound smooth). A report says the deck's place, its speed (track s per s, signed) and how fast that
// changes, and from 0.9.15-2 on, when the deck was read (`deckAtMs`: ms of the app's elapsedRealtime, an epoch
// of its own). The place each moment is counted on from when the deck was read; a report's correction fades in over
// EASE_MS instead of jumping (at 128× a report's few ms of travel are seconds of track); a gap past what the speed
// explains (a seek, a stall) or another track's place is taken at once.

/** The deck's course from a report: its place (s) and when it was read on this page's clock (ms), its speed, how fast
 *  that changes (per s). */
export type Course = { pos: number; at: number; v: number; a: number };
/** A correction fading in: the place shown before it less the new course's place then (s), and from when (ms). */
export type Ease = { gap: number; at: number };

/** How long a report's correction takes to fade in. */
export const EASE_MS = 150;
/** A gap past the larger of these is not a correction but the deck elsewhere: a seek, a stall. */
const SNAP_MIN_S = 2;
const SNAP_FOR_S = 0.5;
/** How many reports the two clocks are read against. */
const CLOCK_REPORTS = 40;

/** The course's place at `now`: its place carried on by its speed and acceleration, never past a stand (its speed coming
 *  to 0: it stands there, not going back the other way). */
export function courseAt(c: Course, now: number): number {
  let t = Math.max(0, (now - c.at) / 1000);
  if (c.a !== 0 && c.v !== 0 && Math.sign(c.a) !== Math.sign(c.v)) t = Math.min(t, -c.v / c.a);
  return c.pos + c.v * t + 0.5 * c.a * t * t;
}

/** The place shown at `now`: the course's, with what is left of a correction fading in (a half cosine). */
export function shownAt(c: Course, e: Ease | null, now: number): number {
  const p = courseAt(c, now);
  if (!e) return p;
  const u = Math.max(0, (now - e.at) / EASE_MS);
  return u >= 1 ? p : p + e.gap * 0.5 * (1 + Math.cos(Math.PI * u));
}

/** Whether a correction is still fading in at `now`. */
export const easing = (e: Ease | null, now: number) => !!e && now - e.at < EASE_MS;

/** The correction a new course brings, fading in from `now` from `before` (the place shown just before it); null where
 *  it is taken at once: nothing shown before, another track, a gap past what the speed explains. */
export function easeInto(before: number | null, next: Course, now: number, sameTrack: boolean): Ease | null {
  if (before === null || !sameTrack) return null;
  const gap = before - courseAt(next, now);
  if (gap === 0 || Math.abs(gap) > Math.max(SNAP_MIN_S, Math.abs(next.v) * SNAP_FOR_S)) return null;
  return { gap, at: now };
}

/** When the deck was read, on this page's clock (ms): from the app's `deckAtMs` where a report says it, by the smallest
 *  difference between the two clocks over the last reports (the report that came quickest; `gaps` keeps them, one
 *  player's); else when the report came. */
export function readAt(gaps: number[], deckAtMs: unknown, now: number): number {
  if (typeof deckAtMs !== 'number' || !Number.isFinite(deckAtMs)) return now;
  gaps.push(now - deckAtMs);
  if (gaps.length > CLOCK_REPORTS) gaps.shift();
  return Math.min(now, deckAtMs + Math.min(...gaps));
}
