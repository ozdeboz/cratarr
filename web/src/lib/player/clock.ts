// The one clock for where the playing track is (0.10; the architecture audit, 2026-10-29: four clocks, each the
// position its own way). This device's player says it to the frame itself (lib/localPlayer positionNow). Another
// player says it now and then, through the server: the clock holds where it was at a moment (the anchor) and runs on
// from it, and every view (the player's bar, the visuals' Music, the band glass) reads that one anchor. A report that
// disagrees much (another track, paused or not, a tape's rewind, or more than JUMP off: a seek) is taken as it is; a
// small difference is eased in, half at a time, so the music's beat is neither played twice nor skipped. Pure.

export type Anchor = { id: string; pos: number; at: number; paused: boolean };
/** A player's report: its track, its place (s), whether it stands. */
export type Heard = { id: string; pos: number; paused: boolean; rewinding?: boolean };

/** How far off (s) a report is taken as it is (a seek, a restart). */
export const JUMP = 1.5;

/** Where the anchor puts the track at `at` (ms, performance.now()), no further than its length. */
export function placeOf(a: Anchor | null, at: number, duration: number | null = null): number {
  if (!a) return 0;
  const p = a.paused ? a.pos : a.pos + Math.max(0, at - a.at) / 1000;
  return duration ? Math.min(p, duration) : p;
}

/** The anchor after a report heard at `now`. A tape's rewind is taken report by report (it goes back on purpose). */
export function heardAt(a: Anchor | null, h: Heard, now: number): Anchor {
  const still = h.paused || !!h.rewinding;
  if (!a || a.id !== h.id || a.paused !== still || h.rewinding) return { id: h.id, pos: h.pos, at: now, paused: still };
  const off = h.pos - placeOf(a, now);
  if (Math.abs(off) > JUMP) return { id: h.id, pos: h.pos, at: now, paused: still };
  if (!off) return a;
  // Eased: the line moved half the way toward the report.
  return { ...a, pos: a.pos + off / 2 };
}
