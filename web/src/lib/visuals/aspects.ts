// The visuals' mapping (decision 0046): from the music now and the settings to what is drawn this frame, each aspect as
// its routes give it. Pure and stateful, with no GL and no flame: the visuals' loop steps it each frame, and the
// behaviour's precheck (recipe.ts) steps it over a track's signals to see what a recipe would do before it is shown.

import { PARTS, type Part } from './flame';
import type { Levers } from './governor';
import type { Music } from './music.svelte';
import { A, ASPECTS, B, BAND_SIGNALS, compile, give, given, T, TRACK_SIGNALS, type Given, type Matrix } from './routes';
import { BANDS, type VisualSettings } from './settings.svelte';

/** Nothing held back (the governor's levers all open). */
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };

/** A beat's turn of the transforms: once around in 32 beats. */
export const STEP = (2 * Math.PI) / 32;
/** The Flash setting's slow course: followed twice over this long (seconds). */
const SLOW_S = 0.3;
/** A moment comes again no sooner than this (ms). */
const MOMENT_GAP_MS = 2000;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

type Moment = { armed: boolean; at: number };

export type AspectsState = {
  // How intense the show is (the loudness as heard, followed over a fifth of a second; `lit`: as far as the Intensity
  // setting lets it count), and paused or not.
  intensity: number;
  lit: number;
  glow: number;
  // The turning's clock: a route's full amount turns a part once around in 16 beats (the default routes, once in 32 on
  // average), at the Speed setting; paused, it slows to a stop. The whole picture's turn.
  clock: number;
  turn: number;
  // The signals now (the loudness's as the Intensity setting lets it count), the bands' stand-ins while none are known
  // (each half way), and what the routes give each aspect.
  track: Float32Array;
  standIn: Float32Array[];
  got: Given;
  routes: Record<string, number> | null;
  matrix: Matrix | null;
  // The track playing, and when it started (ms): the Track signal.
  trackKey: string | null | undefined;
  trackAt: number;
  // Each part: how far it has turned, and its evolving aspects (shape, hue, spread) as followed over its band's
  // Evolution time.
  turned: number[];
  evolving: number[][];
  // Each part's steps: its band's last hit seen (or beat, where its hits are not known yet), what its evolving aspects
  // were given then (what a stepping part evolves to), and the turn of its steps still to come (each eased in over
  // 80 ms).
  lastHit: (number | null)[];
  held: number[][];
  pending: number[];
  rates: number[];
  // The moments: ready again once what they are given falls under a third, and not within two seconds.
  moments: { symmetry: Moment };
  // The Flash setting: each quickly changing aspect (each part's lights, the glow, size and saturation) as its slow
  // course (followed twice over a third of a second: a beat's or a hit's swing all but gone from it, a passage's change
  // kept) and the setting's share of its swing about that course.
  courses: { lights: number[][]; glow: number[]; size: number[]; saturation: number[] };
};

/** What is drawn this frame, as the music and the settings give it. */
export type AspectsNow = {
  /** What the whole track's routes give each aspect now (`ASPECTS` order): a route's full amount is 1. */
  given: Float32Array;
  /** The whole track's signals now as routed (`TRACK_SIGNALS` order: the loudness's as Intensity lets it count, the
   *  beats' halved with reduced motion), and each band's (their stand-ins while none are known). */
  track: Float32Array;
  bands: Float32Array[];
  /** The loudness as far as Intensity lets it count (0..1), and the glow (1 playing, dimmer paused). */
  lit: number;
  glow: number;
  /** The whole picture's turn (radians). */
  turn: number;
  /** Each part's turning rate (radians a second), its evolving shape as drawn (0..1.25), and its drawn aspects. */
  rates: number[];
  shape: number[];
  parts: Part[];
  /** Each part's lights (-1..3), after the Flash setting. */
  hits: number[];
  /** The picture's size (0.25..1.4), the share of each frame kept (trails), the display's gamma (depth), its glow, its
   *  saturation, the smoothing (pixels), the floor, the fluid (of the fluid's own) and its warp. */
  scale: number;
  keep: number;
  gamma: number;
  flash: number;
  saturation: number;
  smoothing: number;
  floor: number;
  fluid: number;
  warp: number;
  /** How far the look walks (drift.ts): what Drift's routes give it, but the Track row's (its move at a new track is
   *  the track's own, not a walk). */
  drift: number;
  /** The moments come this frame: another symmetry (none with reduced motion). */
  moments: { symmetry: boolean };
};

export function newAspects(): AspectsState {
  return {
    intensity: 0.5,
    lit: 0.5,
    glow: 1,
    clock: 0,
    turn: 0,
    track: new Float32Array(TRACK_SIGNALS.length),
    standIn: Array.from({ length: BANDS }, () => {
      const v = new Float32Array(BAND_SIGNALS.length);
      v[B.level] = 0.5;
      return v;
    }),
    got: given(BANDS),
    routes: null,
    matrix: null,
    trackKey: undefined,
    trackAt: 0,
    turned: Array<number>(PARTS).fill(0),
    evolving: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    lastHit: Array<number | null>(PARTS).fill(null),
    held: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    pending: Array<number>(PARTS).fill(0),
    rates: Array<number>(PARTS).fill(0),
    moments: { symmetry: { armed: true, at: -Infinity } },
    courses: { lights: Array.from({ length: PARTS }, () => [] as number[]), glow: [], size: [], saturation: [] },
  };
}

/** A setting of the grain (0..1) as its routes move it: a route's full amount moves one half of its range. */
export const grainOf = (given: Float32Array, setting: number, aspect: number) => clamp(setting + 0.5 * given[aspect], 0, 1);

/** Floor: none at 0, then from 0.002 of the display's scale (a stray speck's) up to 1 (the flame's mean), evenly in its
 *  log. */
export const floorOf = (v: number) => (v > 0 ? 0.002 * 500 ** v : 0);

/** One frame of the mapping, `dt` seconds on. `still`: reduced motion (a quarter of the turning, all of it steady, half
 *  the beats' and hits' share, no moments); `now`: the clock in ms (the moments' gap); `matrix`: the routes compiled
 *  (else compiled here, again whenever `s.routes` is another object); `fluidFull`: the fluid's share at Fluid's 100 %;
 *  `levers`: what the governor holds back (governor.ts; none by default). */
export function stepAspects(
  st: AspectsState,
  m: Music,
  s: VisualSettings,
  dt: number,
  opts: { still: boolean; now: number; matrix?: Matrix; fluidFull: number; levers?: Levers },
): AspectsNow {
  const { still, now } = opts;
  const L = opts.levers ?? OPEN;
  let matrix = opts.matrix;
  if (!matrix) {
    if (st.routes !== s.routes || !st.matrix) {
      st.routes = s.routes;
      st.matrix = compile(s.routes, BANDS);
    }
    matrix = st.matrix;
  }
  // The show's intensity: the loudness as heard, followed over a fifth of a second, as far as the Intensity setting
  // lets it count. It is the loudness's signal, and the envelope of every other one's routes (none while paused).
  // Dimmer while paused.
  st.intensity += (m.intensity - st.intensity) * Math.min(1, dt * 5);
  st.lit = 1 - s.intensity * (1 - st.intensity);
  st.glow += ((m.playing ? 1 : 0.55) - st.glow) * Math.min(1, dt * 2);
  const track = st.track;
  track.set(m.track);
  track[T.loudness] = st.lit;
  if (still) for (const t of [T.beat, T.downbeat, T.section]) track[t] *= 0.5;
  // Track: all of it as a track starts (the visuals opening on one too), gone over its first beat.
  if (m.key !== st.trackKey) {
    st.trackKey = m.key;
    st.trackAt = now;
  }
  track[T.track] = Math.max(0, 1 - ((now - st.trackAt) / 1000) * m.bps);
  const bands = m.bands ?? st.standIn;
  const envelope = m.playing ? st.lit : 0;
  give(matrix, track, bands, envelope, st.got);
  const W = st.got.whole;
  // The courses: followed more slowly while the governor holds the flashes back, so they no longer follow each pulse.
  const follow2 = 1 - Math.exp((-dt * L.swing) / SLOW_S);
  // The swing about the course: the Flash setting's share, as much of it as the governor lets through (and of the
  // size's, as much as its pumping is let).
  const flashed = (v: number, course: number[], held = 1) => {
    if (!course.length) course.push(v, v);
    course[0] += (v - course[0]) * follow2;
    course[1] += (course[0] - course[1]) * follow2;
    return course[1] + s.flash * L.swing * held * (v - course[1]);
  };
  // The turning's clock, at the track's tempo (its beats' where they are known, 90 BPM without one); the whole picture
  // turns with the whole track's routes to Rotation.
  const b = m.playing ? m.beat : null;
  const want = m.playing ? STEP * m.bps * (still ? 0.25 : 1) * s.speed * L.pace : 0;
  st.clock += (want - st.clock) * Math.min(1, dt * 1.5);
  st.turn += st.clock * 0.6 * W[A.rotation] * dt;
  // Each part, given the whole track's routes and its band's: Shape evolves it (larger, its variations given over to its
  // band's own), Hue moves its colour round, Spread draws it out from the middle or in (all three followed over its
  // band's Evolution time), Rotation turns it, Lights light its colour. Its band's Continuity shares its motion between
  // steady (turning at its rate, evolving as it is given) and steps on its band's own hits (or the beats, where those
  // are not known yet): a step turns it by as much as it would have turned steadily since the band's last, on average
  // (a strong hit further, a weak one less), and evolves it to what it was given at that moment, held until the next.
  const hits: number[] = [];
  const shapes: number[] = [];
  const parts: Part[] = [];
  for (let i = 0; i < PARTS; i++) {
    const k = Math.floor((i * BANDS) / PARTS);
    const own = st.got.band[k];
    const c = still ? 1 : s.continuity[k];
    const wants = [W[A.shape] + own[A.shape], (W[A.hue] + own[A.hue]) * L.hue, W[A.spread] + own[A.spread]];
    const rate = st.clock * 2 * (W[A.rotation] + own[A.rotation]);
    st.rates[i] = rate;
    const step = m.playing && m.hits ? m.hits[k] : null;
    const at = step ? step.index : b ? b.index : null;
    if (at !== st.lastHit[i]) {
      const was = st.lastHit[i];
      // Backwards or by more than three: a seek or another track, not stepped for.
      if (at !== null && was !== null && at > was && at - was <= 3) {
        st.held[i] = wants;
        const often = Math.max(0.5, step ? step.perSecond : m.bps);
        st.pending[i] += ((1 - c) * rate * (step ? step.weight : 1)) / often;
      }
      st.lastHit[i] = at;
    }
    const eased = st.pending[i] * Math.min(1, dt / 0.08);
    st.turned[i] += c * rate * dt + eased;
    st.pending[i] -= eased;
    const follow = Math.min(1, dt / (s.evolution[k] * L.evolution));
    const ev = st.evolving[i];
    for (let e = 0; e < 3; e++) ev[e] += (c * wants[e] + (1 - c) * st.held[i][e] - ev[e]) * follow;
    const [shape, hue, spread] = ev;
    const grown = clamp(shape, 0, 1.25);
    shapes.push(grown);
    parts.push({ turn: st.turned[i], size: 0.94 + 0.12 * grown, morph: (still ? 0.15 : 0.35) * grown, hue, spread: 2 ** clamp(spread, -2, 2) });
    // Lights: its colour's points lit up, or dimmed where its routes take it below nothing.
    hits.push(clamp(flashed(clamp(W[A.lights] + own[A.lights], -1, 3) * (still ? 0.5 : 1), st.courses.lights[i]), -1, 3));
  }
  // The moments: what their routes give rising past a half.
  const moment = (mo: Moment, v: number) => {
    if (v < 1 / 3) mo.armed = true;
    if (!(v > 0.5 && mo.armed && now - mo.at > MOMENT_GAP_MS * L.gap)) return false;
    mo.armed = false;
    mo.at = now;
    return true;
  };
  const moments = { symmetry: !still && moment(st.moments.symmetry, W[A.symmetry]) };
  const drift = (W[A.drift] - track[T.track] * envelope * matrix.track[T.track * ASPECTS.length + A.drift]) / L.evolution;
  const fluid = opts.fluidFull * grainOf(W, s.fluid, A.fluid);
  return {
    given: W,
    track,
    bands,
    lit: st.lit,
    glow: st.glow,
    turn: st.turn,
    rates: st.rates,
    shape: shapes,
    parts,
    hits,
    // Size: half its size at rest, all of it given all (by default the loudness: smaller when quiet, and a beat's
    // swell).
    scale: clamp(flashed(0.5 + 0.5 * W[A.size], st.courses.size, L.size), 0.25, 1.4),
    // Trails: each frame keeps this much of the last (longer trails given more, shorter given less).
    keep: clamp(1 - 0.07 * 2 ** (-1.5 * W[A.trails]), 0.5, 0.985),
    // Depth: the display's gamma, 1.5 to 6 (3 as before at its middle); higher lifts more of the faint detail.
    gamma: 1.5 * 4 ** grainOf(W, s.depth, A.depth),
    // Glow: as bright at rest (dimmer only while paused), brighter given more (by default a beat's flash).
    flash: st.glow * clamp(flashed(1 + 0.5 * W[A.brightness], st.courses.glow), 0.15, 3),
    // Saturation: a third of its colour at rest, all of it given all (by default the loudness: paler when quiet).
    saturation: clamp(flashed(0.35 + 0.65 * W[A.saturation], st.courses.saturation), 0, 1.25),
    // Smoothing: up to 12 pixels (3 as before at its middle). Floor: from none to about the flame's mean density.
    smoothing: 12 * grainOf(W, s.smoothing, A.smoothing) ** 2,
    floor: floorOf(grainOf(W, s.floor, A.floor)),
    // Fluid: the light carried along a flow (none at 0), read back along it a little as it shows.
    fluid,
    warp: 0.12 * fluid,
    drift,
    moments,
  };
}
