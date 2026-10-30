// A hold's window on the waveform (lib/SeekWave; 0.10.1, the owner: "I really do need this perfect. So pure continuous
// transitions."), pure so the tests drive it frame by frame. The window is placed from three things that each move
// continuously, so nothing in it can step:
// - Its focus: the crosshair (where letting go lands), converged on so that it settles exactly in the middle (the owner,
//   2026-10-30: "it should converge on the crosshair being exactly in the center that aims the easiest"). The focus is
//   the playhead plus a lead eased toward the crosshair's lead on it: the tape's own motion followed exactly, however
//   fast, only the lead eased; never past it, and exactly on it once within a pixel's fraction.
// - Its width, eased by lib/SeekWave.
// - An offset from that place, set where the place itself moves at once (a hold beginning from the window shown, a let
//   go handing over to the track's own view, a track change then) so that what shows does not, and eased away.
// - Its walls, edges it does not pass, the focus leaving the middle toward them: a track's edge the tape stops at, and
//   on a hold through the queue the edge of the track it began in that it goes away from (the owner: "Wherever a
//   direction doesn't need the neighbour, it's not shown, and the edge never moves"). Going on through the queue the
//   way it goes there is no wall: forward from a track's start the start stays the left edge, the crosshair moving
//   right from it until it can stand in the middle; back from there the window moves on into the track before.

/** The direction the tape goes: forward, backward, or not known. */
export type WaveDir = 1 | -1 | 0;
/** Edges the window does not pass, in the units of its place (`-Infinity`/`Infinity`: none). */
export type Walls = { lo: number; hi: number };

/** A hold's walls, the track it began in running from `start` to `end`: both its edges, but on a hold `through` the
 *  queue only the one it goes away from. */
export function holdWalls(dir: WaveDir, through: boolean, start: number, end: number): Walls {
  if (through && dir > 0) return { lo: start, hi: Infinity };
  if (through && dir < 0) return { lo: -Infinity, hi: end };
  return { lo: start, hi: end };
}

/** The window `width` wide on `focus`: in the middle, unless that passes a wall, then against it (the low one first:
 *  a window wider than the walls stands from the low one). */
export function place(focus: number, width: number, walls: Walls): { from: number; to: number } {
  let from = focus - width / 2;
  if (from + width > walls.hi) from = walls.hi - width;
  if (from < walls.lo) from = walls.lo;
  return { from, to: from + width };
}

/** `cur` eased toward `target` over `dt` ms with time constant `tau` ms: never past it, and exactly on it once within
 *  `eps`. */
export function ease(cur: number, target: number, dt: number, tau: number, eps: number): number {
  const next = target + (cur - target) * Math.exp(-Math.max(0, dt) / tau);
  return Math.abs(next - target) <= eps ? target : next;
}

/** How close counts as there: a fraction of a pixel on a window `width` wide (512 bars at the most, lib/Waveform). */
export const settleOf = (width: number) => Math.abs(width) * 1e-4;

/** The two edges' offsets from a window's place: what it showed, less where it is placed now. */
export type Offset = { a: number; b: number };
export const offsetOf = (shown: { from: number; to: number }, placed: { from: number; to: number }): Offset => ({ a: shown.from - placed.from, b: shown.to - placed.to });
/** The offset eased away: exactly none once both are within `eps`. */
export function easeOffset(o: Offset, dt: number, tau: number, eps: number): Offset | null {
  const a = ease(o.a, 0, dt, tau, eps);
  const b = ease(o.b, 0, dt, tau, eps);
  return a === 0 && b === 0 ? null : { a, b };
}
/** What shows: the window placed, moved by the offset, never past a wall. */
export function shownOf(placed: { from: number; to: number }, o: Offset | null, walls: Walls): { from: number; to: number } {
  if (!o) return placed;
  return { from: Math.max(walls.lo, placed.from + o.a), to: Math.min(walls.hi, placed.to + o.b) };
}

/** A wide view's colours, track by track (lib/Waveform's `wide.colours`; the owner, 2026-10-30: "The color should be
 *  stable"): null, the player's own colour as the waveform has it anyway, for the track the player shows and for all
 *  where the player does not take its colours from the covers; each other track its cover's, else the theme's. */
export function waveColours(keys: (string | null)[], shown: string | null, own: boolean, accentOf: (key: string) => string | null): (string | null)[] {
  return keys.map((k) => (!own || !k || k === shown ? null : (accentOf(k) ?? 'var(--accent)')));
}

// The hold's window frame by frame (lib/SeekWave runs it; the tests drive it): where it stands in s from the playing
// track's start, from the hold's beginning to the track's own view again.
/** A hold's window: the stretch the tape covers in ZOOM_S at its speed, at least ZOOM_MIN_S (the owner, 2026-10-28). */
export const ZOOM_S = 15;
export const ZOOM_MIN_S = 20;
/** The width's easing in, and out (slower, for the eye to follow; the owner, 2026-10-28), once let go. */
export const ZOOM_EASE_MS = 250;
export const ZOOM_OUT_MS = 700;
/** Let go, the window stays this long before it eases back to the whole track (the owner, 2026-10-28). */
export const ZOOM_KEEP_MS = 2000;
/** The crosshair's lead eased toward where it is; an offset eased away. */
export const FOCUS_MS = 300;
export const OFFSET_MS = 400;

/** What a frame of the window goes by. Places in s from the playing track's start. */
export type HoldWindowIn = {
  /** The playing track's length, and the playhead in it. */
  len: number;
  pos: number;
  /** Where the last frame's track's start is, from this one's (0: the same track; null: not known). */
  moved: number | null;
  /** A hold, its finger down; whether it winds on through the queue, which way, its speed and its top speed. */
  holding: boolean;
  through: boolean;
  dir: WaveDir;
  speed: number;
  top: number;
  /** The crosshair: where letting go lands (null: not known now). */
  aim: number | null;
};
/** The window between frames. */
export type HoldWindow = {
  /** Its width, eased toward `span` while held: the widest the hold's speed asked for, so it never narrows in a hold
   *  and stands still once the top speed is reached (the owner, 2026-10-30: at the top speed "the zoom window in
   *  actual time should not change anymore"). */
  w: number;
  span: number;
  rel: number;
  off: Offset | null;
  walls: Walls;
  held: boolean;
  /** How long ago it was let go (ms; null: held). */
  since: number | null;
  shown: { from: number; to: number };
};

/**
 * The window's next frame, `dt` ms on; null: none (the track's own view, whole). Held, its walls are the hold's, set as
 * it begins; let go, the track's own both edges. Each change of those, and a track change once let go, keeps what shows
 * by an offset eased away, so no frame jumps.
 */
export function stepWindow(s: HoldWindow | null, i: HoldWindowIn, dt: number): HoldWindow | null {
  // No track, or none held; let go and on into a track of a place not known: its own view at once.
  if (!(i.len > 0) || (!s && !i.holding) || (s && i.moved === null && !i.holding)) return null;
  let last = { from: 0, to: i.len };
  let walls: Walls = { lo: 0, hi: i.len };
  let rebase = false;
  if (s && i.moved !== null) {
    last = { from: s.shown.from + i.moved, to: s.shown.to + i.moved };
    walls = { lo: s.walls.lo + i.moved, hi: s.walls.hi + i.moved };
    // Let go and on into another track: that one's own view, from what shows.
    if (i.moved !== 0 && !i.holding) rebase = true;
  }
  const starting = i.holding && (!s || !s.held || i.moved === null);
  if (starting) {
    walls = holdWalls(i.dir, i.through, 0, i.len);
    rebase = true;
  }
  if (!i.holding) walls = { lo: 0, hi: i.len };
  if (s?.held && !i.holding) rebase = true;
  const since = i.holding ? null : s?.held || s?.since == null ? 0 : s.since + dt;
  let span = starting ? 0 : (s?.span ?? 0);
  let target: number;
  if (i.holding) {
    span = Math.max(span, ZOOM_MIN_S, Math.min(i.speed, i.top) * ZOOM_S);
    // Through the queue no track's length bounds it; on this track alone, the track.
    target = i.through ? span : Math.min(i.len, span);
  } else target = since! < ZOOM_KEEP_MS ? (s?.w ?? i.len) : i.len;
  // Held, the speed's own width, no easing behind it (the speed itself runs up smoothly; the change from what showed as
  // the hold begins is the offset's): at the top speed it stands at once. Let go, eased.
  const w0 = s?.w ?? i.len;
  const w = i.holding ? target : ease(w0, target, dt, target > w0 ? ZOOM_OUT_MS : ZOOM_EASE_MS, settleOf(target));
  const lead = i.holding ? (i.aim !== null ? i.aim - i.pos : (s?.rel ?? 0)) : 0;
  const rel = starting || !s ? lead : ease(s.rel, lead, dt, FOCUS_MS, settleOf(w));
  const placed = place(i.pos + rel, w, walls);
  let off: Offset | null;
  if (rebase) {
    const o = offsetOf(last, placed);
    off = o.a === 0 && o.b === 0 ? null : o;
  } else off = s?.off ? easeOffset(s.off, dt, OFFSET_MS, settleOf(w)) : null;
  // Handing over, what shows may be beyond the new walls (a let go after the next track showed): through the queue it
  // eases in from there; on one track never past its edges.
  const shown = shownOf(placed, off, i.through ? { lo: -Infinity, hi: Infinity } : { lo: 0, hi: i.len });
  if (!i.holding && since! >= ZOOM_KEEP_MS && !off && rel === 0 && w === i.len && shown.from === 0 && shown.to === i.len) return null;
  return { w, span, rel, off, walls, held: i.holding, since, shown };
}
