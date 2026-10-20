// The Cratarr chain in the patch (decision 0057 §3; the owner: "Nothing at all" can be a start, and the chain "a device
// called Cratarr"): the engine finds the chain by its Flame, wherever it sits, and takes its knobs from its modules; a
// patch without it draws nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, todays, track, type Engine } from './harness/aspects-record.ts';

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
      return stepEngine(st, p!, {}, m, () => 0, s, dt, opts).aspects!;
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
      const f = stepEngine(st, p, {}, m, () => 0, DEFAULTS, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
      assert.equal(f.aspects, null);
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

test("the patch is the truth: the chain's knobs turned in the patch alone give what today's engine gives with those settings", () => {
  const set = { tunnel: 0.7, flash: 1.6, haze: 0.4 };
  const turned = (p: PatchV2): PatchV2 => ({
    ...p,
    modules: p.modules.map((m) => {
      if (m.id === 'cratarr/tunnel') return { ...m, knobs: { ...m.knobs, tunnel: set.tunnel } };
      if (m.id === 'accent') return { ...m, knobs: { ...m.knobs, accent: set.flash } };
      if (m.id === 'cratarr/show/curve') return { ...m, knobs: { ...m.knobs, haze: set.haze } };
      if (m.id === 'cratarr/flame') return { ...m, knobs: { ...m.knobs, 'continuity.2': 0.2 } };
      return m;
    }),
  });
  const continuity = [...DEFAULTS.continuity];
  continuity[2] = 0.2;
  const wanted = { ...structuredClone(DEFAULTS), ...set, continuity };
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(todays(), t, wanted, 30, 15, false);
    // The settings as they were (the engine must not take these from them), the knobs turned in the patch.
    const b = record(over((s) => turned(inDevice(flatten(toV2(s))))), t, DEFAULTS, 30, 15, false);
    for (const [k, d] of compare(a, b)) assert.ok(d.most <= 1e-4, `${id}: ${k} differs by ${d.most} at frame ${d.frame}`);
  }
});
