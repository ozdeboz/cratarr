// FROZEN (0.9.19: the tests' reference only): the one engine run on today's patch (decision 0057, step 2): the routes give as they did (routes.ts give, each
// column's scale), the hidden behaviours run as the modules they become (modules.ts), the chain's own as chain.ts. What
// the equivalence harness holds frame for frame to aspects.ts, before the port evaluator gives in place of the routes.

import type { AspectsNow } from '../../../src/lib/visuals/aspects.ts';
import type { Levers } from '../../../src/lib/visuals/engine/sidechain.ts';
import type { Music } from '../../../src/lib/visuals/music-core.ts';
import { A, ASPECTS, B, BAND_SIGNALS, COLUMN_SCALES, compile, give, given, T, TRACK_SIGNALS, type Given, type Matrix, type RouteValue } from '../../../src/lib/visuals/routes.ts';
import { BANDS, type VisualSettings } from '../../../src/lib/visuals/settings.svelte.ts';
import { newChain, stepChain, type ChainState } from '../../../src/lib/visuals/engine/chain.ts';
import { newClock, newEnvelope, newTrack, stepClock, stepEnvelope, stepTrack, type ClockState, type EnvelopeState, type TrackState } from '../../../src/lib/visuals/engine/modules.ts';

/** The columns scaled by their setting (the fluid's stirring is scaled where it stirs). */
const SCALED = (Object.entries(COLUMN_SCALES) as [keyof typeof COLUMN_SCALES, (typeof COLUMN_SCALES)[keyof typeof COLUMN_SCALES]][])
  .filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl')
  .map(([a, k]) => [A[a], k] as const);
const NA = ASPECTS.length;
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };

export type EngineV1 = {
  envelope: EnvelopeState;
  clock: ClockState;
  track: TrackState;
  chain: ChainState;
  signals: Float32Array;
  standIn: Float32Array[];
  got: Given;
  routes: Record<string, RouteValue> | null;
  matrix: Matrix | null;
};

export function newEngineV1(): EngineV1 {
  return {
    envelope: newEnvelope(),
    clock: newClock(),
    track: newTrack(),
    chain: newChain(),
    signals: new Float32Array(TRACK_SIGNALS.length),
    standIn: Array.from({ length: BANDS }, () => {
      const v = new Float32Array(BAND_SIGNALS.length);
      v[B.level] = 0.5;
      return v;
    }),
    got: given(BANDS),
    routes: null,
    matrix: null,
  };
}

/** One frame, as aspects.ts's stepAspects is stepped. */
export function stepEngineV1(
  st: EngineV1,
  m: Music,
  s: VisualSettings,
  dt: number,
  opts: { still: boolean; now: number; matrix?: Matrix; fluidFull: number; levers?: Levers; routeWalk?: Float32Array },
): AspectsNow {
  const L = opts.levers ?? OPEN;
  let matrix = opts.matrix;
  if (!matrix) {
    if (st.routes !== s.routes || !st.matrix) {
      st.routes = s.routes;
      st.matrix = compile(s.routes, BANDS);
    }
    matrix = st.matrix;
  }
  // Envelope, then the Track's pulse: what the cables read and what scales them.
  const env = stepEnvelope(st.envelope, m.intensity, m.playing, s.intensity, dt);
  const track = st.signals;
  track.set(m.track);
  track[T.loudness] = env.level;
  if (opts.still) for (const t of [T.beat, T.downbeat, T.section]) track[t] *= 0.5;
  track[T.track] = stepTrack(st.track, m.key, opts.now, m.bps);
  const bands = m.bands ?? st.standIn;
  give(matrix, track, bands, env.out, st.got, opts.routeWalk);
  for (const [a, k] of SCALED) {
    const v = s[k];
    if (v === 1) continue;
    st.got.whole[a] *= v;
    for (const own of st.got.band) own[a] *= v;
  }
  const clock = stepClock(st.clock, m.bps, m.playing, s.speed, L.pace, dt);
  const trackWalk = (a: number) => track[T.track] * env.out * matrix.track[T.track * NA + a];
  return stepChain(
    st.chain,
    m,
    s,
    { given: st.got, track, bands, lit: env.level, envelope: env.out, glow: env.glow, clock, trackWalk: { look: trackWalk(A.look), behaviour: trackWalk(A.behaviour) } },
    dt,
    opts,
  );
}
