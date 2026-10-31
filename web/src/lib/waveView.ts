// A hold's window on the waveform (lib/SeekWave; 0.10.1, the owner: "I really do need this perfect. So pure continuous
// transitions."), pure so the tests drive it frame by frame. The window is placed from three things that each move
// continuously, so nothing in it can step:
// - Its focus: the crosshair (where letting go lands), exactly, every frame (0.13, the owner: "It should be quite fast,
//   set the crosshair in the center to keep it stable"; eased toward it, the window grew faster than its middle caught
//   up, and lagged behind it through a whole wind-up). Let go, the crosshair it aimed at while the tape is short of it,
//   then the playhead. The crosshair is of the same moment as the playhead (lib/SeekWave), so its lead on it moves on
//   continuously; where the crosshair itself moves at once (it comes to be known, it is given up), that move is the
//   offset's, eased away, never the window's in a frame.
// - Its width, eased by lib/SeekWave.
// - An offset from that place, set where the place itself moves at once (a hold beginning from the window shown, a let
//   go handing over to the track's own view, a track change then) so that what shows does not, and eased away.
// - Its walls, edges it does not pass, the focus leaving the middle toward them: a track's edge the tape stops at, and
//   on a hold through the queue the edge of the track it began in that it goes away from (the owner: "Wherever a
//   direction doesn't need the neighbour, it's not shown, and the edge never moves"). Going on through the queue the
//   way it goes there is no wall; back from a track's start the window moves on into the track before.
// - Centring by them (0.13, the owner: "the rightmost end of the window is moving too far ahead, so it's zooming out too
//   quickly"; a wall held one edge, and all the zoom's growth went to the other): held, the window is no wider than
//   twice the crosshair's distance from the nearer wall (wallCap), at the least the 1× window. So the crosshair stands
//   in the middle by a wall too, and the zoom catches up with the speed as it moves away; forward from a track's start
//   the start is the left edge only at the 1× window.
// - The queue's own ends (0.12.3, the owner: "during the continuous hold, the window shouldn't allow viewing empty
//   waveforms"): through the queue, the start of the first track a hold can wind back into and the end of the last it
//   can wind on into are walls too, as a track's edges are on its own; a window wider than what they hold is no wider.
//   Where they move (a radio or Autoplay queue growing) what shows is kept and eased, not jumped.

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

/** Where a wall lets the window go gradually (a share of its width): it leaves the wall gathering speed, not at once at
 *  the tape's (0.11.1, the owner: "we need everything a whole lot more stable, window included"; it went from standing
 *  to the full speed in a frame). Short of it, exactly against the wall. */
export const WALL_SOFT = 0.02;
/** `x` kept above `lo`: exactly `lo` below lo - z, exactly `x` above lo + z, a smooth bend between (its slope from 0 to
 *  1 without a step). */
function above(x: number, lo: number, z: number): number {
  if (!(z > 0) || x - lo >= z) return Math.max(x, lo);
  if (x - lo <= -z) return lo;
  return lo + (x - lo + z) ** 2 / (4 * z);
}

/** The window `width` wide on `focus`: in the middle, unless that passes a wall, then against it (the low one first:
 *  a window wider than the walls stands from the low one), leaving it smoothly (WALL_SOFT). */
export function place(focus: number, width: number, walls: Walls): { from: number; to: number } {
  // No softer than the room between the walls allows (a window nearly as wide as they are apart stands exactly in them).
  const z = Math.max(0, Math.min(WALL_SOFT * width, (walls.hi - walls.lo - width) / 2));
  let from = focus - width / 2;
  if (Number.isFinite(walls.hi)) from = -above(-(from + width), -walls.hi, z) - width;
  if (Number.isFinite(walls.lo)) from = above(from, walls.lo, z);
  if (walls.hi - walls.lo <= width) from = Number.isFinite(walls.lo) ? walls.lo : walls.hi - width;
  return { from, to: from + width };
}

/** The widest a held window may be with its middle on `focus` between `walls` (0.13, the owner: at a wall "the rightmost
 *  end of the window is moving too far ahead, so it's zooming out too quickly"; a wall held one edge, and all the zoom's
 *  growth went to the other): twice the focus's distance from the nearer wall, less the soft bend a window leaving a
 *  wall takes (WALL_SOFT, place), so that it stands in the middle exactly and both edges move as centring needs. No wall:
 *  none. Below the 1× window (ZOOM_MIN_S) the caller keeps that, the crosshair against the wall as at 1×. */
/** How gradually the cap takes hold and lets go (a share of the width): the window's edges change speed over that. */
export const CAP_SOFT = 0.1;
export function wallCap(focus: number, walls: Walls): number {
  const d = Math.min(focus - walls.lo, walls.hi - focus);
  return d === Infinity ? Infinity : Math.max(0, (2 * d) / (1 + 2 * WALL_SOFT));
}

/** `cur` eased toward `target` over `dt` ms with time constant `tau` ms: never past it, and exactly on it once within
 *  `eps`. */
export function ease(cur: number, target: number, dt: number, tau: number, eps: number): number {
  const next = target + (cur - target) * Math.exp(-Math.max(0, dt) / tau);
  return Math.abs(next - target) <= eps ? target : next;
}

/**
 * `x` moving at `v` (per ms) carried toward `target` over `dt` ms, critically damped with time constant `tau` ms: it
 * keeps its speed as it begins (a hand-over moves on as it moved, no lurch: 0.11.1; an easing that begins at its fastest
 * moved the window a track's length in a few frames), never oscillates, and is exactly on the target, at rest, once
 * within `eps`.
 */
export function spring(x: number, v: number, target: number, dt: number, tau: number, eps: number): [number, number] {
  const w = 2 / tau;
  const t = Math.max(0, dt);
  const e = x - target;
  const k = Math.exp(-w * t);
  const c = v + w * e;
  const nx = target + (e + c * t) * k;
  const nv = (v - w * c * t) * k;
  return Math.abs(nx - target) <= eps && Math.abs(nv) * tau <= eps ? [target, 0] : [nx, nv];
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
 *  stable"; 0.12: "Colour or brightness flips" during a hold): each track its cover's, the same whichever track the
 *  player shows (the player's own colour follows its cover a moment after a track change, and the track turning shown
 *  took that colour on for those frames); null, the player's own, for the shown track while its cover's is not known
 *  and for all where the player does not take its colours from the covers; else the theme's. */
export function waveColours(keys: (string | null)[], shown: string | null, own: boolean, accentOf: (key: string) => string | null): (string | null)[] {
  return keys.map((k) => (!own || !k ? null : (accentOf(k) ?? (k === shown ? null : 'var(--accent)'))));
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
/** An offset eased away; as a hold begins, from the whole track to its crosshair in the middle, quickly (0.13, the
 *  owner: "it should be quite fast, set the crosshair in the center"). */
export const OFFSET_MS = 400;
export const HOLD_IN_MS = 150;
/** A crosshair moving further in a frame than this share of the window (a quarter of a bar of 130) moved at once (it
 *  came to be known, or was given up): its move is taken up by the offset, eased away. Its own run, a wind-up to the
 *  top speed included, moves it a twentieth of that a frame. */
export const AIM_JUMP = 0.002;

/** The crosshair's lead on the playhead the window is centred by: held, where letting go lands (not known: as it was);
 *  let go, where it aimed while the tape is short of it the way it goes, then none. */
export function leadOf(holding: boolean, dir: WaveDir, pos: number, aim: number | null, had: number): number {
  if (aim === null) return holding ? had : 0;
  const d = aim - pos;
  if (holding) return d;
  return dir !== 0 && d * dir > 0 ? d : 0;
}

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
  /** Through the queue, how far a hold can go: the start of the first track it can wind back into and the end of the
   *  last it can wind on into (none: `-Infinity`/`Infinity`). */
  ends?: Walls;
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
  /** How fast the width and the offset move (per ms): each eased on as it moved (lib/waveView's spring). `rel`: the
   *  crosshair's lead on the playhead (leadOf), the window's middle. */
  wv: number;
  offv: Offset;
  /** The offset's easing: quick as a hold begins (HOLD_IN_MS), else OFFSET_MS. */
  offMs: number;
  /** How fast what shows moved last frame (per ms), each edge: a hand-over goes on from where it was heading, at that
   *  speed. Its focus (the playhead plus the lead), for where it would have been placed by the new rules. */
  sv: Offset;
  focus: number;
  /** The walls the window keeps to: the hold's own (`hold`) within the queue's ends. */
  walls: Walls;
  hold: Walls;
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
  let hold: Walls = { lo: 0, hi: i.len };
  let rebase = false;
  if (s && i.moved !== null) {
    // Where it was heading: a hand-over does not stand still a frame (0.11.1).
    last = { from: s.shown.from + i.moved + s.sv.a * dt, to: s.shown.to + i.moved + s.sv.b * dt };
    hold = { lo: s.hold.lo + i.moved, hi: s.hold.hi + i.moved };
    // Let go and on into another track: that one's own view, from what shows.
    if (i.moved !== 0 && !i.holding) rebase = true;
  }
  const starting = i.holding && (!s || !s.held || i.moved === null);
  if (starting) {
    hold = holdWalls(i.dir, i.through, 0, i.len);
    rebase = true;
  }
  if (!i.holding) hold = { lo: 0, hi: i.len };
  const ends = i.through && i.ends ? i.ends : NO_WALLS;
  const walls: Walls = i.holding ? { lo: Math.max(hold.lo, ends.lo), hi: Math.min(hold.hi, ends.hi) } : hold;
  // The queue's ends moved under a hold (a queue growing, a track added or taken out): from what showed, eased.
  if (s && s.held && i.holding && !starting && i.moved !== null && (moves(s.walls.lo + i.moved, walls.lo) || moves(s.walls.hi + i.moved, walls.hi))) rebase = true;
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
  // Let go, the width springs back from rest (held, it is the speed's at once; its change as a hold begins is the
  // offset's), the lead toward the crosshair's as it moved, the offset away from rest: each change begins without a
  // lurch (0.11.1).
  let w = target;
  let wv = 0;
  if (!i.holding) [w, wv] = spring(w0, s && !s.held ? s.wv : 0, target, dt, target > w0 ? ZOOM_OUT_MS : ZOOM_EASE_MS, settleOf(target));
  // The middle on the crosshair exactly; a crosshair that moved at once (AIM_JUMP) is the offset's to ease away.
  const rel = leadOf(i.holding, i.dir, i.pos, i.aim, s?.rel ?? 0);
  // Held, no wider than centring by its walls allows (wallCap), the 1× window at the least: the crosshair stays in the
  // middle and the far end never runs ahead. Each bound bends in over CAP_SOFT of what it bounds (above), so an edge's
  // pace changes over a few frames as the cap takes hold or lets go, not in one; never wider than the cap but in that
  // bend at the 1× window. And no wider than the walls hold: nothing past the queue's ends ever shows.
  if (i.holding) {
    const least = Math.min(w, ZOOM_MIN_S);
    const capped = above(wallCap(i.pos + rel, walls), least, CAP_SOFT * least);
    w = Math.min(-above(-w, -capped, CAP_SOFT * w), walls.hi - walls.lo);
  }
  const jumped = !!s && !starting && i.moved !== null && Math.abs(rel - s.rel) > AIM_JUMP * w;
  const placed = place(i.pos + rel, w, walls);
  let off: Offset | null = null;
  let offv: Offset = { a: 0, b: 0 };
  // Let go, the crosshair stands where it aimed and the middle with it: no hand-over of the hold's speed to the window.
  const lettingGo = !!s?.held && !i.holding;
  let offMs = s?.offMs ?? OFFSET_MS;
  // Let go with its crosshair kept (where it aimed), the middle stays on it; let go with none (the middle back to the
  // playhead at once), the window goes on as it was going, the rebase's.
  if ((jumped && !rebase) || (rebase && lettingGo && !jumped)) {
    // What showed kept, the window placed by the crosshair moving as it does, the change eased away (an offset's own
    // easing goes on as it went). Let go, from where it stood: the crosshair stops where it aimed, and the window with
    // it, not on at the hold's speed a frame.
    const here = lettingGo && s && i.moved !== null ? { from: s.shown.from + i.moved, to: s.shown.to + i.moved } : last;
    const o = offsetOf(here, placed);
    off = o.a === 0 && o.b === 0 ? null : o;
    if (off && s?.off) offv = s.offv;
    if (!s?.off) offMs = OFFSET_MS;
  } else if (rebase) {
    const o = offsetOf(last, placed);
    off = o.a === 0 && o.b === 0 ? null : o;
    // A hold beginning comes to its crosshair quickly (HOLD_IN_MS); other changes ease (OFFSET_MS).
    offMs = starting ? HOLD_IN_MS : OFFSET_MS;
    // On at the speed it showed: the offset takes the difference from how fast the window placed by the new rules
    // moves (against a wall, standing), and springs away from there.
    if (off && s && i.moved !== null && dt > 0) {
      const before = place(s.focus + i.moved, w, walls);
      offv = { a: s.sv.a - (placed.from - before.from) / dt, b: s.sv.b - (placed.to - before.to) / dt };
    }
  } else if (s?.off) {
    const eps = settleOf(w);
    const [a, va] = spring(s.off.a, s.offv.a, 0, dt, offMs, eps);
    const [b, vb] = spring(s.off.b, s.offv.b, 0, dt, offMs, eps);
    off = a === 0 && b === 0 && va === 0 && vb === 0 ? null : { a, b };
    offv = { a: va, b: vb };
  }
  // Handing over, what shows may be beyond the new walls (a let go after the next track showed): through the queue it
  // eases in from there; on one track never past its edges.
  const shown = shownOf(placed, off, i.through ? ends : { lo: 0, hi: i.len });
  if (!i.holding && since! >= ZOOM_KEEP_MS && !off && rel === 0 && w === i.len && shown.from === 0 && shown.to === i.len) return null;
  const sv =
    s && i.moved !== null && dt > 0 ? { a: (shown.from - (s.shown.from + i.moved)) / dt, b: (shown.to - (s.shown.to + i.moved)) / dt } : { a: 0, b: 0 };
  return { w, span, rel, off, wv, offv, offMs, sv, focus: i.pos + rel, walls, hold, held: i.holding, since, shown };
}

/**
 * How much a track boundary's marker shows, at `x` across the waveform (0 its left edge, 1 its right) with `n` bars
 * across it (0.12.3, the owner: "when track boundaries are snapped to the waveform viewport edges, they should not be
 * shown", as the plain waveform shows none at its edges): none within a bar of an edge, whole from two bars in, between
 * them fading, so a boundary leaving a wall comes in without a pop and one reaching an edge goes the same way.
 */
export function markShows(x: number, n: number): number {
  if (!(n > 0) || !Number.isFinite(x)) return 0;
  return Math.max(0, Math.min(1, Math.min(x, 1 - x) * n - 1));
}

const NO_WALLS: Walls = { lo: -Infinity, hi: Infinity };
/** Whether a wall moved between frames (one gone or come counts; none either time does not). */
const moves = (a: number, b: number) => (Number.isFinite(a) || Number.isFinite(b)) && !(Math.abs(a - b) <= 1e-9);
