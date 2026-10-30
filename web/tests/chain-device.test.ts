// The Cratarr chain in the patch (decision 0057 §3; the owner: "Nothing at all" can be a start, and the chain "a device
// called Cratarr"): the engine finds the chain by its Flame, wherever it sits, and takes its knobs from its modules; a
// patch without it draws nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, track, type Engine } from './harness/recorded.ts';

const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

/** The engine over `patch(s)`. */
const over = (patch: (s: typeof DEFAULTS) => PatchV2): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s) {
        from = s;
        p = patch(s);
      }
      return stepEngine(st, p!, {}, m, () => 0, dt, opts);
    },
  };
};

/** The chain's own modules (the Cratarr chain as toV2 gives it, flattened). */
const CHAIN = /^(flame|fluid|tunnel|dive|trails|frame|fx\d|loop-effect|show\/.+)$/;
/** The chain moved into a device `cratarr`, its modules named as flatten names a device's. */
function inDevice(p: PatchV2): PatchV2 {
  const to = (id: string) => (CHAIN.test(id) ? `cratarr/${id}` : id);
  return {
    ...p,
    modules: p.modules.map((m) => ({ ...m, id: to(m.id) })),
    cables: p.cables.map((c) => ({ ...c, from: { ...c.from, module: to(c.from.module) }, to: { ...c.to, module: to(c.to.module) } })),
    macros: p.macros.map((x) => ({ ...x, targets: x.targets.map((t) => ({ ...t, module: to(t.module) })) })),
  };
}

test('a blank slate: an empty patch, or one with only a modulator, runs and draws nothing', () => {
  const t = track(TRACKS[0]);
  const music = newMusicState();
  const st = newEngine();
  const empty: PatchV2 = { v: 2, modules: [], cables: [], macros: [], pins: [] };
  const lfo: PatchV2 = { ...empty, modules: [{ id: 'osc', type: 'lfo', knobs: { rate: 0.5, amount: 1 } }] };
  for (let k = 0; k < 60; k++) {
    const at = 20 + k / 30;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting });
    for (const p of [empty, lfo]) {
      const f = stepEngine(st, p, {}, m, () => 0, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
      assert.equal(f.device, null);
      if (p === lfo) assert.ok(Number.isFinite(f.outputs.osc));
    }
  }
});

test('the chain inside a device (its modules named cratarr/…) gives what it gave at the top level, frame by frame', () => {
  const p = flatten(toV2(structuredClone(DEFAULTS)));
  assert.ok(inDevice(p).modules.some((m) => m.id === 'cratarr/flame') && inDevice(p).modules.some((m) => m.id === 'cratarr/show/tonemap'));
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(over((s) => flatten(toV2(s))), t, DEFAULTS, 30, 15, false);
    const b = record(over((s) => inDevice(flatten(toV2(s)))), t, DEFAULTS, 30, 15, false);
    for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id}: ${k} differs by ${d.most} at frame ${d.frame}`);
  }
});

test("the patch is the truth: the chain's knobs turned in the patch alone give what the same settings converted give", () => {
  const set = { tunnel: 0.7, flash: 1.6, haze: 0.4 };
  const turned = (p: PatchV2): PatchV2 => ({
    ...p,
    modules: p.modules.map((m) => {
      if (m.id === 'cratarr/tunnel') return { ...m, knobs: { ...m.knobs, tunnel: set.tunnel } };
      if (m.id === 'cratarr/show/curve') return { ...m, knobs: { ...m.knobs, haze: set.haze } };
      if (m.id === 'cratarr/flame') return { ...m, knobs: { ...m.knobs, 'continuity.2': 0.2 } };
      return m;
    }),
    // Flash is the rack's macro on the Accent's knob (0..2).
    macros: p.macros.map((x) => (x.id === 'flash' ? { ...x, value: set.flash / 2 } : x)),
  });
  const continuity = [...DEFAULTS.continuity];
  continuity[2] = 0.2;
  const wanted = { ...structuredClone(DEFAULTS), ...set, continuity };
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(over((s) => inDevice(flatten(toV2(s)))), t, wanted, 30, 15, false);
    // The settings as they were (the engine must not take these from them), the knobs turned in the patch.
    const b = record(over((s) => turned(inDevice(flatten(toV2(s))))), t, DEFAULTS, 30, 15, false);
    for (const [k, d] of compare(a, b)) assert.ok(d.most <= 1e-4, `${id}: ${k} differs by ${d.most} at frame ${d.frame}`);
  }
});

test("Music's newtrack fires once as each track starts (the first frame too), its seed the same for the same track", () => {
  const t = track(TRACKS[0]);
  const st = newEngine();
  const music = newMusicState();
  const p: PatchV2 = {
    v: 2,
    modules: [{ id: 'music', type: 'music', knobs: {} }, { id: 'v', type: 'vca', knobs: { gain: 0 } }, { id: 's', type: 'vca', knobs: { gain: 0 } }],
    cables: [
      { id: 'a', from: { module: 'music', port: 'newtrack' }, to: { module: 'v', port: 'cv' }, value: 1, depth: null },
      { id: 'b', from: { module: 'music', port: 'seed' }, to: { module: 's', port: 'cv' }, value: 1, depth: null },
    ],
    macros: [],
    pins: [],
  } as PatchV2;
  const seen: { fired: number; seed: number }[] = [];
  for (let k = 0; k < 90; k++) {
    const at = 20 + k / 30;
    const key = k < 30 ? 'subsonic/a' : k < 60 ? 'subsonic/b' : 'subsonic/a';
    const m = { ...musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting }), key };
    const f = stepEngine(st, p, {}, m, () => 0, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
    seen.push({ fired: f.outputs['v.out'], seed: f.outputs['s.out'] });
  }
  assert.deepEqual(seen.flatMap((x, k) => (x.fired > 0.5 ? [k] : [])), [0, 30, 60]);
  assert.ok(seen[0].seed > 0 && seen[0].seed < 1 && seen[0].seed !== seen[30].seed && seen[0].seed === seen[60].seed);
});

test("Fluid's at: the Flame's points by default (its normal), a Grid's where one is cabled in", () => {
  const t = track(TRACKS[0]);
  const music = newMusicState();
  const flamePoints = [[0.5, 0.5], null, [0.2, 0.8], [0.9, 0.1], null, [0.4, 0.6]] as const;
  const base = flatten(toV2(structuredClone(DEFAULTS)));
  const fluid = base.modules.find((x) => x.type === 'fluid')!;
  const grid = { id: 'dots', type: 'grid', knobs: { lattice: 0, count: 4, spacing: 0.5, rotation: 0, jitter: 0 } };
  const withGrid: PatchV2 = { ...base, modules: [...base.modules, grid], cables: [...base.cables, { id: 'g', from: { module: 'dots', port: 'points' }, to: { module: fluid.id, port: 'at' }, value: 1, depth: null }] };
  const at = 20;
  const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting });
  const opts = { still: false, now: at * 1000, fluidFull: FLUID_FULL, flamePoints: flamePoints as never };
  const plain = stepEngine(newEngine(), base, {}, m, () => 0, 1 / 30, opts);
  assert.deepEqual(plain.pointsAt[`${fluid.id}.at`], flamePoints);
  const gridded = stepEngine(newEngine(), withGrid, {}, m, () => 0, 1 / 30, opts);
  const got = gridded.pointsAt[`${fluid.id}.at`]!;
  assert.equal(got.length, 4);
  for (const q of got) assert.ok(q && q[0] >= 0 && q[0] <= 1 && q[1] >= 0 && q[1] <= 1);
});
