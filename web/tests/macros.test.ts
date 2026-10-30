// Macros (decision 0077: macros are the rack's face; one level down): a device's own macros reach the engine, a macro
// maps a device's macro, nothing reaches past a device's edge: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/recorded.ts';
const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { DEVICES } = await import('../src/lib/visuals/devices.ts');
const { asSet, flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { macrosOneLevel } = await import('../src/lib/visuals/macro-levels.ts');
const { isDevice } = await import('../src/lib/visuals/ports.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

const types = v2TypeRecord();
const t = track(TRACKS[0]);
const step = (p: PatchV2) => stepEngine(newEngine(), flatten(p), types, musicOf(t.signals, t.beats, 20, true, { state: newMusicState(), at: 20000, lighting: DEFAULTS.lighting }), () => 0, 1 / 30, { now: 20000, fluidFull: FLUID_FULL });

test("a device's own macro moves what it maps: Bloom's Threshold from 0.9 to 0.4", () => {
  for (const [value, want] of [[0, 0.9], [1, 0.4], [0.5, 0.65]] as const) {
    const bloom = structuredClone(DEVICES.find((d) => d.id === 'bloom')!);
    bloom.macros.find((x) => x.id === 'threshold')!.value = value;
    const f = step({ v: 2, modules: [bloom], cables: [], macros: [], pins: [] });
    assert.ok(Math.abs(f.knobs['bloom/bright'].threshold - want) < 1e-9, `${value}: ${f.knobs['bloom/bright'].threshold}`);
  }
});

test("one level down: the rack's Wild maps Calm's own Wild, which maps its five thresholds, as before", () => {
  for (const wild of [0.2, 0.5, 0.9]) {
    const p = toV2({ ...structuredClone(DEFAULTS), wild });
    const rack = p.macros.find((x) => x.id === 'wild')!;
    assert.deepEqual(rack.targets, [{ module: 'calm', knob: 'macro:wild', from: 0, to: 1 }]);
    const calm = p.modules.find((m) => m.id === 'calm');
    assert.ok(calm && isDevice(calm));
    const own = calm.macros.find((x) => x.id === 'wild')!;
    assert.equal(own.targets.length, 5);
    assert.ok(own.targets.every((x) => !x.module.includes('/') && x.knob === 'threshold'));
    // As the rack's Wild reached the thresholds directly.
    const direct = structuredClone(p);
    const d = direct.modules.find((m) => m.id === 'calm')!;
    if (isDevice(d)) d.macros = [];
    direct.macros.find((x) => x.id === 'wild')!.targets = own.targets.map((x) => ({ ...x, module: `calm/${x.module}` }));
    // The thresholds as set (the Ducker's base, Wild's along its taper) and what each Ducker gives, both alike.
    const fa = flatten(p);
    const fb = flatten(direct);
    const a = step(p);
    const b = step(direct);
    for (const x of own.targets) {
      const id = `calm/${x.module}`;
      const ta = asSet(fa, fa.modules.find((m) => m.id === id), 'threshold', types)!;
      const tb = asSet(fb, fb.modules.find((m) => m.id === id), 'threshold', types)!;
      assert.ok(Math.abs(ta - tb) < 1e-9 * tb, `${x.module} at Wild ${wild}: ${ta} against ${tb}`);
      assert.equal(a.outputs[`${id}.out`], b.outputs[`${id}.out`]);
    }
  }
});

test('moved one level down again, a patch is as it was', () => {
  const p = toV2(structuredClone(DEFAULTS));
  const once = JSON.stringify(p);
  assert.equal(JSON.stringify(macrosOneLevel(p)), once);
});

test('a mapped knob shows its macro and the value it sets, in the graph as in the panel (knobViews)', async () => {
  const { graphOfV2 } = await import('../src/lib/visuals/patchgraph.ts');
  const p = toV2({ ...structuredClone(DEFAULTS), intensity: 0.8 });
  const g = graphOfV2({ patch: p, types, open: new Set(['envelope']) });
  const env = g.nodes.find((n) => n.id === 'envelope')!.data as { knobs: { id: string; value: number; macro?: { label: string } }[] };
  const k = env.knobs.find((x) => x.id === 'intensity')!;
  assert.equal(k.macro?.label, 'Intensity');
  const flat = flatten(p);
  assert.equal(k.value, asSet(flat, flat.modules.find((m) => m.id === 'envelope'), 'intensity', types));
  // Inside Calm: its own Wild mapped by the rack's.
  const inCalm = graphOfV2({ patch: p, types, at: ['calm'], open: new Set(['duck-motion']) });
  const duck = inCalm.nodes.find((n) => n.id === 'duck-motion')!.data as { knobs: { id: string; macro?: { label: string } }[] };
  assert.equal(duck.knobs.find((x) => x.id === 'threshold')?.macro?.label, 'Wild');
  const top = g.nodes.find((n) => n.id === 'calm')!.data as { knobs: { id: string; macro?: { label: string } }[] };
  assert.equal(top.knobs.find((x) => x.id === 'macro:wild')?.macro?.label, 'Wild');
});

test('making macros (patch-edits.ts): mapped without a jump, one a knob, let go and removed where they were', async () => {
  const { applyEdit } = await import('../src/lib/visuals/patch-edits.ts');
  const p = toV2(structuredClone(DEFAULTS));
  const set = (q: PatchV2, module: string, knob: string) => asSet(flatten(q), flatten(q).modules.find((m) => m.id === module), knob, types)!;
  const hueBefore = set(p, 'flame', 'hue');
  applyEdit(p, { kind: 'newMacro', scope: [], id: 'macro-13', label: 'Macro 13' }, types);
  assert.ok(p.macros.some((x) => x.id === 'macro-13' && x.label === 'Macro 13'));
  // The Flame's Hue mapped to it: taken from Hue's macro, where it was.
  applyEdit(p, { kind: 'mapKnob', scope: [], id: 'macro-13', module: 'flame', knob: 'hue' }, types);
  assert.equal(p.macros.filter((x) => x.targets.some((t) => t.module === 'flame' && t.knob === 'hue')).map((x) => x.id).join(), 'macro-13');
  assert.ok(Math.abs(set(p, 'flame', 'hue') - hueBefore) < 1e-9);
  // Its range, then let go: where it was set.
  applyEdit(p, { kind: 'macroRange', scope: [], id: 'macro-13', module: 'flame', knob: 'hue', from: 0, to: 1 }, types);
  applyEdit(p, { kind: 'macro', scope: [], id: 'macro-13', value: 0.25 }, types);
  assert.ok(Math.abs(set(p, 'flame', 'hue') - 0.25) < 1e-9);
  applyEdit(p, { kind: 'unmapKnob', scope: [], id: 'macro-13', module: 'flame', knob: 'hue' }, types);
  assert.ok(Math.abs(p.modules.find((m) => m.id === 'flame')!.knobs.hue - 0.25) < 1e-9);
  // A macro removed: each knob it set where it was, its pin gone.
  const lightness = set(p, 'flame', 'lightness');
  p.pins.push('macro:look-light');
  applyEdit(p, { kind: 'removeMacro', scope: [], id: 'look-light' }, types);
  assert.ok(!p.macros.some((x) => x.id === 'look-light') && !p.pins.includes('macro:look-light'));
  assert.ok(Math.abs(p.modules.find((m) => m.id === 'flame')!.knobs.lightness - lightness) < 1e-9);
  // A device's macros are its own: one made inside Calm.
  applyEdit(p, { kind: 'newMacro', scope: ['calm'], id: 'macro-2', label: 'Macro 2' }, types);
  const calm = p.modules.find((m) => m.id === 'calm');
  assert.ok(calm && isDevice(calm) && calm.macros.some((x) => x.id === 'macro-2'));
});
