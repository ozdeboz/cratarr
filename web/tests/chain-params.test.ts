// The chain's values by node (chain-params.ts, decision 0057 step 3): exactly the look Visuals.svelte gave the
// renderer before (its literal, kept here as it was), and the nodes' values back into one look: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { chainNow, chainOfShown, shownOf } = await import('../src/lib/visuals/chain-params.ts');
const { effectsNow } = await import('../src/lib/visuals/effects.ts');
const { DIVE_MOST, TUNNEL_MOST } = await import('../src/lib/visuals/zoom.ts');
type AspectsNow = import('../src/lib/visuals/aspects.ts').AspectsNow;
type ChainContext = import('../src/lib/visuals/chain-params.ts').ChainContext;

/** Visuals.svelte's look for the renderer as it was before C4 (its own names in the context). */
function before(d: AspectsNow, c: ChainContext) {
  const { turn, hits } = d;
  const governor = { levers: c.levers };
  const frame = c.coverage === null ? null : { coverage: c.coverage };
  return {
    brightness: 1,
    keep: d.keep,
    gamma: d.gamma,
    vibrancy: 1,
    turn,
    scale: d.scale,
    zoom: 1,
    flash: d.flash,
    hits,
    ceiling: c.ceiling,
    saturation: d.saturation,
    exposure: frame ? Math.min(1.5, Math.max(0.4, 0.33 / Math.max(0.05, frame.coverage))) : 1,
    smoothing: d.smoothing,
    floor: d.floor,
    fire: c.fire,
    fluid: d.fluid,
    tunnel: c.still ? 0 : TUNNEL_MOST * d.tunnel * governor.levers.pace,
    effects: effectsNow(c.how, d.effects, c.position, c.beat, governor.levers, c.still, d.routed),
    facets: { amount: d.facets, hex: Math.round(c.how.lattice) === 1 },
    dive: c.still ? 0 : DIVE_MOST * d.dive * governor.levers.pace,
    forces: c.forces,
    flow: c.flow,
    dt: c.dt * c.speed * governor.levers.pace * (c.still ? 0.25 : 1),
    warp: d.warp,
  };
}

const aspects = (k: number) =>
  ({
    turn: 0.3 * k,
    scale: 0.7 + 0.01 * k,
    dive: 0.4,
    keep: 0.93,
    fluid: 0.05 * k,
    tunnel: 0.6,
    facets: 0.25,
    warp: 0.006 * k,
    gamma: 4.1,
    flash: 1.3,
    hits: [0.1, 0.4, -0.2, 0, 1.2, 0.05].map((h) => h * k),
    saturation: 0.8,
    smoothing: 3 + k,
    floor: 0.02,
    effects: 0.9,
    routed: { amounts: [1, 0.5, 0.8, 1], knobs: [0.1, -0.2, 0, 0, 0.3, 0, 0, 0.1] },
  }) as unknown as AspectsNow;

const contexts = (): ChainContext[] =>
  [false, true].flatMap((still) =>
    [null, 0.02, 0.4].map((coverage, i) => ({
      how: { ...DEFAULTS, fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8, fxLoop: 2, fxLoopAmount: 0.5, lattice: i % 2 },
      levers: { swing: 1, pace: 0.7 + 0.1 * i, evolution: 1, hue: 1, size: 1, gap: 1 },
      still,
      ceiling: 0.9,
      coverage,
      fire: 0.3 * i,
      position: 12.5 + i,
      beat: i ? { index: 7, since: 0.1, length: 0.5, strength: 0.8 } : null,
      forces: [{ x: 0.5, y: 0.5, dx: 0.1, dy: 0, radius: 0.1, strength: 1 }],
      flow: { energy: 0.5, swirl: 0.2, wave: { amount: 0.3, angle: 1, phase: 2 } },
      dt: 1 / 30,
      speed: 1.2,
    })),
  );

test("the nodes' values are the look Visuals gave the renderer before, exactly", () => {
  for (const k of [0, 1, 2])
    for (const c of contexts()) {
      const d = aspects(k);
      assert.deepStrictEqual(shownOf(chainNow(d, c)), before(d, c));
    }
});

test('a look split by node and put together again is the same look', () => {
  const look = shownOf(chainNow(aspects(1), contexts()[4]));
  assert.deepStrictEqual(shownOf(chainOfShown(look)), look);
});
