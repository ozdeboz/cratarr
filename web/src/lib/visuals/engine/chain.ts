// The Cratarr chain's own modules in the one engine (decision 0057, §3): the Flame's parts (their turning, evolution,
// presence, lights and steps on their band's hits) and the Show's and the loops' levels, from what the cables give
// them (`Given`, routes.ts's shape) and the Envelope's, Clock's and Track's outputs (modules.ts). The same arithmetic
// as aspects.ts, which the equivalence harness holds it to; what aspects.ts made out of sight comes in as inputs.

import type { AspectsNow } from '../aspects';
import { hazeOf, levelOf } from '../aspects';
import { PARTS, type Part } from '../flame';
import type { Levers } from './sidechain';
import type { Music } from '../music-core';
import { A, AMOUNT_ASPECTS, KNOB_ASPECTS, MOMENT_ASPECTS, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { courseStep, newFlash, newMoment, stepFlash, stepMoment, type FlashState, type MomentState } from './modules';

/** Nothing held back (the sidechaining's levers all open). */
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
type MomentAspect = (typeof MOMENT_ASPECTS)[number];

/** What the chain keeps from frame to frame: the whole picture's turn; each part's turn, evolving aspects, steps and
 *  presence; the moments; the Flash courses (each part's lights, the glow, the size, the saturation). */
export type ChainState = {
  turn: number;
  turned: number[];
  evolving: number[][];
  lastHit: (number | null)[];
  held: number[][];
  pending: number[];
  rates: number[];
  presence: number[];
  moments: Record<MomentAspect, MomentState>;
  courses: { lights: FlashState[]; glow: FlashState; size: FlashState; saturation: FlashState };
};

export function newChain(): ChainState {
  return {
    turn: 0,
    turned: Array<number>(PARTS).fill(0),
    evolving: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    lastHit: Array<number | null>(PARTS).fill(null),
    held: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    pending: Array<number>(PARTS).fill(0),
    rates: Array<number>(PARTS).fill(0),
    presence: Array<number>(PARTS).fill(0),
    moments: Object.fromEntries(MOMENT_ASPECTS.map((a) => [a, newMoment()])) as Record<MomentAspect, MomentState>,
    courses: { lights: Array.from({ length: PARTS }, newFlash), glow: newFlash(), size: newFlash(), saturation: newFlash() },
  };
}

/** What the chain's modules take this frame besides the given: the Envelope's level and depth and glow, the Clock's
 *  rate, the track's signals as routed (the loudness the Envelope's level, Track's pulse in them), each band's (or their
 *  stand-ins), and the Track row's share of the walks (its move at a new track is the track's own, not a walk). */
export type ChainInputs = {
  given: Given;
  track: Float32Array;
  bands: Float32Array[];
  lit: number;
  envelope: number;
  glow: number;
  clock: number;
  /** What the Track pulse gives the Look and the Behaviour walks (aspects.ts's walkOf subtracts it). */
  trackWalk: { look: number; behaviour: number };
};

/** One frame of the chain, `dt` seconds on: what aspects.ts's stepAspects returned. */
export function stepChain(
  st: ChainState,
  m: Music,
  s: VisualSettings,
  x: ChainInputs,
  dt: number,
  opts: { still: boolean; now: number; fluidFull: number; levers?: Levers },
): AspectsNow {
  const { still, now } = opts;
  const L = opts.levers ?? OPEN;
  const got = x.given;
  const W = got.whole;
  const follow2 = courseStep(dt, L.swing);
  const flashed = (v: number, course: FlashState, held = 1) => stepFlash(course, v, follow2, s.flash, L.swing, held);
  const b = m.playing ? m.beat : null;
  st.turn += x.clock * 0.6 * W[A.rotation] * dt;
  const hits: number[] = [];
  const shapes: number[] = [];
  const parts: Part[] = [];
  for (let i = 0; i < PARTS; i++) {
    const k = Math.floor((i * BANDS) / PARTS);
    const own = got.band[k];
    const c = still ? 1 : s.continuity[k];
    const wants = [W[A.shape] + own[A.shape], (W[A.hue] + own[A.hue]) * L.hue, W[A.spread] + own[A.spread]];
    const rate = x.clock * 2 * (W[A.rotation] + own[A.rotation]);
    st.rates[i] = rate;
    const step = m.playing && m.hits ? m.hits[k] : null;
    const at = step ? step.index : b ? b.index : null;
    if (at !== st.lastHit[i]) {
      const was = st.lastHit[i];
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
    const wantP = clamp(W[A.presence] + own[A.presence], -2, 2);
    st.presence[i] += (wantP - st.presence[i]) * Math.min(1, dt / ((wantP > st.presence[i] ? 0.1 : 0.4) * L.evolution));
    parts.push({
      turn: st.turned[i],
      size: 0.94 + 0.12 * grown,
      morph: (still ? 0.15 : 0.35) * grown,
      hue,
      spread: 2 ** clamp(spread, -2, 2),
      weight: 2 ** (1.5 * st.presence[i]),
    });
    hits.push(clamp(flashed(clamp(W[A.lights] + own[A.lights], -1, 3) * (still ? 0.5 : 1), st.courses.lights[i]), -1, 3));
  }
  const moments = Object.fromEntries(MOMENT_ASPECTS.map((a) => [a, !still && stepMoment(st.moments[a], W[A[a]], now, L.gap)])) as Record<MomentAspect, boolean>;
  const walkOf = (a: number, track: number) => (W[a] - track) / L.evolution;
  const fluid = opts.fluidFull * levelOf(got, s.fluid, A.fluid);
  const haze = hazeOf(clamp(s.haze * got.level[A.haze], -1, 1));
  return {
    given: W,
    track: x.track,
    bands: x.bands,
    lit: x.lit,
    glow: x.glow,
    turn: st.turn,
    rates: st.rates,
    shape: shapes,
    parts,
    hits,
    scale: clamp(flashed(0.5 + 0.5 * W[A.size], st.courses.size, L.size), 0.25, 1.4),
    keep: clamp(1 - 0.07 * 2 ** (-1.5 * W[A.trails]), 0.5, 0.985),
    gamma: haze.gamma,
    flash: x.glow * clamp(flashed(1 + 0.5 * W[A.brightness], st.courses.glow), 0.15, 3),
    saturation: clamp(flashed(0.35 + 0.65 * W[A.saturation], st.courses.saturation), 0, 1.25),
    smoothing: 12 * levelOf(got, s.smoothing, A.smoothing) ** 2,
    floor: haze.floor,
    levels: { fluid: got.level[A.fluid], smoothing: got.level[A.smoothing], haze: got.level[A.haze], tunnel: got.level[A.tunnel], dive: got.level[A.dive] },
    fluid,
    warp: 0.12 * fluid,
    tunnel: levelOf(got, s.tunnel, A.tunnel),
    effects: levelOf(got, s.effects, A.effects),
    facets: levelOf(got, s.facets, A.facets),
    dive: levelOf(got, s.dive, A.dive),
    look: walkOf(A.look, x.trackWalk.look),
    behaviour: walkOf(A.behaviour, x.trackWalk.behaviour),
    moments,
    routed: { amounts: AMOUNT_ASPECTS.map((a) => got.level[A[a]]), knobs: KNOB_ASPECTS.map((a) => clamp(W[A[a]], -1, 1)) },
  };
}

