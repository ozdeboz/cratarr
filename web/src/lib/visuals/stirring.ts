// How the music stirs the fluid (decision 0047, amended): as timed as the flames, with no new settings. The owner asked
// how; the answer, all here:
//
// - Each hit and beat is a kick, short and strong, not a push for as long as a light rings; and a kick lands about 60 ms
//   before its moment where the moment is known ahead (the server's signals), so the motion peaks on it, not after.
// - The flow breathes with the pulse: it runs freer just after each beat and is held tighter between.
// - A downbeat shoves harder, from the middle; a new section sets a whirl going round it.
// - Each band's Continuity steers its part: stepped (0) kicks only on the band's own hits, steady (1) stirs all the
//   time with its level, between them some of each.
// - The beats' pushes turn a little each beat, so their pattern rotates in tempo.
// - What each push is worth is a route's (64.1; the owner asked that nothing moving the fluid be hidden): Hits, Beat,
//   Downbeat, Section and Level into Stir, Loudness and Beat into Flow, Swell and Section into Swirl. Across a route's
//   curve is the push's own strength as it always was (a hit's, a beat's, a band's level, the loudness, the pulse, the
//   swell, the whirl; a downbeat and a section are 1); straight at 1, the defaults, each gives it unchanged. Not
//   scaled by the loudness, as the routes the flame's aspects take are.
//
// Pure and deterministic (no randomness: every device of a sync group stirs alike). `stir` each frame the fluid steps.

import type { Flow, Force } from './fluid';
import type { Signal } from './routes';

/** A route into the fluid's `aspect` at `x` (routes.ts `heightOn`); none given: straight at 1, `x` itself. */
export type Gain = (signal: Signal, band: number | null, aspect: 'stir' | 'flow' | 'swirl', x: number) => number;
const AS_EVER: Gain = (_signal, _band, _aspect, x) => x;

/** Kicks land this long before a moment known ahead (s). */
export const LEAD = 0.06;
/** A kick fades over this long (s), and is gone after three of them. */
const KICK_S = 0.05;
/** A kick's strength against a light's push: about a ringing light's whole push, given at once. */
const KICK = 1.5;
/** How fast a kick moves the fluid (texture coordinates a second at strength 1), and how far it reaches. */
const KICK_SPEED = 0.5;
const KICK_RADIUS = 0.12;
/** The beats' pushes: four, this far out from the middle, turning this far each beat (radians). */
const BEAT_OUT = 0.12;
const BEAT_TURN = 0.15;
/** A steady stir's strength at a band's full level. */
const STIR = 0.25;

export type StirInput = {
  t: number;
  dt: number;
  amount: number;
  parts: readonly (readonly [number, number] | null)[];
  turns: number[];
  lights: number[];
  level: number[];
  continuity: number[];
  hits: ({ index: number; strength: number; nextIn: number | null } | null)[];
  beat: { index: number; since: number; length: number; strength: number; nextIn: number | null } | null;
  downbeat: number;
  section: number;
  loudness: number;
  swell: number;
  bps: number;
  /** The Waves column now (its routes, scaled), and the track's own time (seconds): the waves' height, way and phase. */
  waves?: number;
  position?: number;
  center: [number, number];
  gain?: Gain;
};

type Kick = { x: number; y: number; dx: number; dy: number; radius: number; strength: number; born: number };

export type Stirring = {
  kicks: Kick[];
  /** Per band, the hit (index) last kicked for: the next one ahead, or the one just come. */
  kickedHit: number[];
  kickedBeat: number;
  /** Whether the downbeat's and the section's pulses were up last frame (a kick on each rise; up when first seen is no
   *  rise, as below). */
  downUp: boolean;
  sectionUp: boolean;
  /** The last section's whirl, for the swirl it adds (its time). */
  whirlAt: number;
};

export function newStirring(): Stirring {
  return { kicks: [], kickedHit: [], kickedBeat: NaN, downUp: true, sectionUp: true, whirlAt: -Infinity };
}

const unit = (x: number, y: number): [number, number] => {
  const l = Math.hypot(x, y);
  return l > 1e-6 ? [x / l, y / l] : [1, 0];
};

/** This frame's forces on the fluid and how it flows. */
export function stir(s: Stirring, i: StirInput): { forces: Force[]; flow: Flow } {
  // The flow breathes: freer just after a beat, held tighter between (its energy, how long its motion lasts).
  const pulse = i.beat ? Math.exp(-i.beat.since / Math.max(0.05, 0.3 * i.beat.length)) : 0;
  const whirl = Math.exp(-(i.t - s.whirlAt) / 2);
  const g = i.gain ?? AS_EVER;
  const flow: Flow = {
    energy: Math.min(1, Math.max(0, g('loudness', null, 'flow', i.loudness) * (0.7 + 0.6 * g('beat', null, 'flow', pulse)))),
    swirl: Math.max(0, 1 + 2 * g('swell', null, 'swirl', Math.max(0, i.swell)) + 2 * g('section', null, 'swirl', whirl)),
    // The waves: once round their cycle in eight seconds, their way turning once in two minutes, both with the track
    // (each kept within a turn: a sine of a large number loses its precision on some GPUs).
    ...(i.waves ? { wave: { amount: i.waves, angle: (((i.position ?? i.t) / 120) % 1) * 2 * Math.PI, phase: (((i.position ?? i.t) / 8) % 1) * 2 * Math.PI } } : {}),
  };
  if (!(i.amount > 0)) {
    s.kicks = [];
    return { forces: [], flow };
  }
  const kick = (x: number, y: number, dx: number, dy: number, strength: number, radius = KICK_RADIUS) =>
    s.kicks.push({ x, y, dx, dy, radius, strength, born: i.t });

  // Each band's hits: a kick at its part, out from the middle and round the way it turns, as strong as the hit and as
  // its band is stepped; ahead of the hit where it is known, else as it comes.
  i.hits.forEach((h, b) => {
    const at = i.parts[b];
    if (!h || !at) return;
    const stepped = 1 - Math.min(1, Math.max(0, i.continuity[b] ?? 0.5));
    // First seen: what came before is not kicked for.
    if (s.kickedHit[b] === undefined) {
      s.kickedHit[b] = h.index;
      return;
    }
    let due: number | null = null;
    if (h.nextIn !== null && h.nextIn <= LEAD && s.kickedHit[b] !== h.index + 1) due = h.index + 1;
    else if (s.kickedHit[b] !== h.index && s.kickedHit[b] !== h.index + 1) due = h.index;
    if (due === null) return;
    s.kickedHit[b] = due;
    if (stepped <= 0) return;
    const worth = g('hits', b, 'stir', 0.3 + 0.7 * Math.min(1, Math.max(0, h.strength)));
    if (!worth) return;
    const [ox, oy] = unit(at[0] - i.center[0], at[1] - i.center[1]);
    const round = Math.sign(i.turns[b] ?? 0) * 0.6;
    const [dx, dy] = unit(ox - oy * round, oy + ox * round);
    kick(at[0], at[1], dx * KICK_SPEED, dy * KICK_SPEED, KICK * stepped * worth);
  });

  // The beats: four pushes out from the middle, their pattern turned a little further each beat; a downbeat's harder.
  const b = i.beat;
  if (b && Number.isNaN(s.kickedBeat)) s.kickedBeat = b.index;
  else if (b) {
    let due: number | null = null;
    if (b.nextIn !== null && b.nextIn <= LEAD && s.kickedBeat !== b.index + 1) due = b.index + 1;
    else if (s.kickedBeat !== b.index && s.kickedBeat !== b.index + 1) due = b.index;
    if (due !== null) {
      s.kickedBeat = due;
      const strength = KICK * 0.6 * g('beat', null, 'stir', 0.35 + 0.65 * Math.min(1, Math.max(0, b.strength)));
      if (strength) for (let k = 0; k < 4; k++) {
        const a = due * BEAT_TURN + (k * Math.PI) / 2;
        const [cx, cy] = [Math.cos(a), Math.sin(a)];
        kick(i.center[0] + BEAT_OUT * cx, i.center[1] + BEAT_OUT * cy, cx * 0.4, cy * 0.4, strength, 0.1);
      }
    }
  }
  const downUp = i.downbeat > 0.5;
  const down = downUp && !s.downUp ? KICK * 1.2 * g('downbeat', null, 'stir', 1) : 0;
  if (down) {
    for (let k = 0; k < 4; k++) {
      const a = (k * Math.PI) / 2 + Math.PI / 4;
      const [cx, cy] = [Math.cos(a), Math.sin(a)];
      kick(i.center[0] + BEAT_OUT * cx, i.center[1] + BEAT_OUT * cy, cx * 0.6, cy * 0.6, down, 0.14);
    }
  }
  s.downUp = downUp;
  // A new section: six kicks round the middle, all the same way round, and the whirls kept stronger for a while.
  const sectionUp = i.section > 0.5;
  if (sectionUp && !s.sectionUp) {
    s.whirlAt = i.t;
    const worth = KICK * g('section', null, 'stir', 1);
    if (worth)
      for (let k = 0; k < 6; k++) {
        const a = (k * Math.PI) / 3;
        const [cx, cy] = [Math.cos(a), Math.sin(a)];
        kick(i.center[0] + 0.2 * cx, i.center[1] + 0.2 * cy, -cy * 0.5, cx * 0.5, worth, 0.12);
      }
  }
  s.sectionUp = sectionUp;

  // What is pushed this frame: each kick fading (a frame's share, so the same at any frame rate), and each steady
  // band's stir with its level.
  const frames = i.dt * 30;
  s.kicks = s.kicks.filter((k) => i.t - k.born < 3 * KICK_S);
  const forces: Force[] = s.kicks.map((k) => ({
    x: k.x,
    y: k.y,
    dx: k.dx,
    dy: k.dy,
    radius: k.radius,
    strength: k.strength * i.amount * Math.exp(-(i.t - k.born) / KICK_S) * frames,
  }));
  i.parts.forEach((at, p) => {
    if (!at) return;
    const steady = Math.min(1, Math.max(0, i.continuity[p] ?? 0.5));
    const level = steady > 0 ? g('level', p, 'stir', Math.min(1, Math.max(0, i.level[p] ?? 0))) : 0;
    if (!level) return;
    const [ox, oy] = unit(at[0] - i.center[0], at[1] - i.center[1]);
    const round = Math.sign(i.turns[p] ?? 0) * 0.6;
    const [dx, dy] = unit(ox - oy * round, oy + ox * round);
    forces.push({ x: at[0], y: at[1], dx: dx * KICK_SPEED, dy: dy * KICK_SPEED, radius: KICK_RADIUS, strength: STIR * steady * level * i.amount * frames });
  });
  return { forces, flow };
}
