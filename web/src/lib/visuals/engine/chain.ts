// The Flame's own stepper in the one engine (decision 0057, §3; graph.ts runs it): its parts' turning, evolution,
// presence, lights and steps on their band's hits, from what the cables give them (`Given`, routes.ts's shape) and the
// Envelope's, Clock's and Track's outputs (modules.ts). (The chain's own stepper, the old engine's other half, went
// with the summary it made in 0.10.)

import { PARTS, type Part } from '../flame';
import type { Levers } from './sidechain';
import type { Music } from '../music-core';
import { A, AMOUNT_ASPECTS, KNOB_ASPECTS, MOMENT_ASPECTS, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { courseStep, newAccent, stepAccent, type AccentState } from './modules';

/** Nothing held back (the sidechaining's levers all open). */
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
type StepAspect = (typeof MOMENT_ASPECTS)[number];

/** What a Flame keeps from frame to frame (its node's, step 5): the whole picture's turn; each part's turn, evolving
 *  aspects, steps and presence; each part's lights' Accent course. */
export type FlameState = {
  turn: number;
  turned: number[];
  evolving: number[][];
  lastHit: (number | null)[];
  held: number[][];
  pending: number[];
  rates: number[];
  presence: number[];
  lights: AccentState[];
};

export function newFlame(): FlameState {
  return {
    turn: 0,
    turned: Array<number>(PARTS).fill(0),
    evolving: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    lastHit: Array<number | null>(PARTS).fill(null),
    held: Array.from({ length: PARTS }, () => [0.5, 0, 0]),
    pending: Array<number>(PARTS).fill(0),
    rates: Array<number>(PARTS).fill(0),
    presence: Array<number>(PARTS).fill(0),
    lights: Array.from({ length: PARTS }, newAccent),
  };
}

/** A Flame's part aspects, as its knobs name them. */
export type FlameKnob = 'shape' | 'rotation' | 'lights' | 'hue' | 'spread' | 'presence';

/** What a Flame's node draws with this frame. */
export type FlameNow = { turn: number; rates: number[]; shape: number[]; parts: Part[]; hits: number[] };

/** One frame of a Flame, `dt` seconds on: its parts (their turning, evolution, presence, steps on their band's hits) and
 *  lights, from what its knobs give (`at`: a part aspect's, the whole track's, or with band `band`'s own added), the
 *  Clock's rate, its continuity and evolution per band, the Accent's setting, the sidechains' levers. `still`: reduced
 *  motion, as aspects.ts had it (decision 0057: kept on purpose): every part steady, its morph slower, its lights half. */
export function stepFlame(
  st: FlameState,
  m: Music,
  at: (knob: FlameKnob, band: number | null) => number,
  x: { clock: number; continuity: readonly number[]; evolution: readonly number[]; accent: number; still?: boolean },
  dt: number,
  L: Levers,
): FlameNow {
  const follow2 = courseStep(dt, L.swing);
  const b = m.playing ? m.beat : null;
  st.turn += x.clock * 0.6 * at('rotation', null) * dt;
  const hits: number[] = [];
  const shapes: number[] = [];
  const parts: Part[] = [];
  for (let i = 0; i < PARTS; i++) {
    const k = Math.floor((i * BANDS) / PARTS);
    const c = x.still ? 1 : x.continuity[k];
    const wants = [at('shape', k), at('hue', k) * L.hue, at('spread', k)];
    const rate = x.clock * 2 * at('rotation', k);
    st.rates[i] = rate;
    const step = m.playing && m.hits ? m.hits[k] : null;
    const hitAt = step ? step.index : b ? b.index : null;
    if (hitAt !== st.lastHit[i]) {
      const was = st.lastHit[i];
      if (hitAt !== null && was !== null && hitAt > was && hitAt - was <= 3) {
        st.held[i] = wants;
        const often = Math.max(0.5, step ? step.perSecond : m.bps);
        st.pending[i] += ((1 - c) * rate * (step ? step.weight : 1)) / often;
      }
      st.lastHit[i] = hitAt;
    }
    const eased = st.pending[i] * Math.min(1, dt / 0.08);
    st.turned[i] += c * rate * dt + eased;
    st.pending[i] -= eased;
    const follow = Math.min(1, dt / (x.evolution[k] * L.evolution));
    const ev = st.evolving[i];
    for (let e = 0; e < 3; e++) ev[e] += (c * wants[e] + (1 - c) * st.held[i][e] - ev[e]) * follow;
    const [shape, hue, spread] = ev;
    const grown = clamp(shape, 0, 1.25);
    shapes.push(grown);
    const wantP = clamp(at('presence', k), -2, 2);
    st.presence[i] += (wantP - st.presence[i]) * Math.min(1, dt / ((wantP > st.presence[i] ? 0.1 : 0.4) * L.evolution));
    parts.push({
      turn: st.turned[i],
      size: 0.94 + 0.12 * grown,
      morph: (x.still ? 0.15 : 0.35) * grown,
      hue,
      spread: 2 ** clamp(spread, -2, 2),
      weight: 2 ** (1.5 * st.presence[i]),
    });
    hits.push(clamp(stepAccent(st.lights[i], clamp(at('lights', k), -1, 3) * (x.still ? 0.5 : 1), follow2, x.accent, L.swing), -1, 3));
  }
  return { turn: st.turn, rates: st.rates, shape: shapes, parts, hits };
}
