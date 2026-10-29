// The shipped devices named by what they carry (device-names.ts; the owner, 2026-10-29: the panel's "Cv, In" rows):
// npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { isDevice, portsOfModule } = await import('../src/lib/visuals/ports.ts');
const { namedDevices } = await import('../src/lib/visuals/device-names.ts');
const { moduleName } = await import('../src/lib/visuals/patchgraph.ts');

const types = v2TypeRecord();
const device = (p: ReturnType<typeof toV2>, id: string) => {
  const d = p.modules.find((m) => m.id === id);
  assert.ok(d && isDevice(d), id);
  return d;
};
const inputs = (p: ReturnType<typeof toV2>, id: string) => portsOfModule(types)(device(p, id))!.in.filter((x) => !x.id.endsWith(':gain')).map((x) => x.label);

test("Loudness Follow: its VCAs and their ports by the row each follows, the loudness one input led to them all", () => {
  const p = toV2(DEFAULTS);
  const d = device(p, 'loudness-follow');
  const ins = inputs(p, 'loudness-follow');
  assert.equal(ins[0], 'Loudness');
  assert.equal(ins.filter((x) => x === 'Loudness').length, 1);
  assert.ok(ins.includes('Beat') && ins.includes('Level · Sub'), ins.join(', '));
  assert.ok(!ins.some((x) => x === 'Cv' || x === 'In'), ins.join(', '));
  // One cable in for the loudness, led inside to every VCA's cv.
  assert.equal(p.cables.filter((c) => c.to.module === d.id && c.to.port === 'loudness').length, 1);
  assert.equal(d.inputs.filter((e) => e.as === 'loudness').length, d.patch.modules.length);
  assert.deepEqual(d.patch.modules.map((m) => moduleName(m, d.patch, types)), d.patch.modules.map((m) => m.label));
});

test('Calm by the busyness each Ducker holds, Pause Dim by its parts, Show by its knobs', () => {
  const p = toV2(DEFAULTS);
  assert.deepEqual(inputs(p, 'calm'), ['Motion', 'Loudness', 'Shape churn', 'Colour churn', 'Zoom pumping', 'New looks']);
  assert.deepEqual(inputs(p, 'pause'), ['Playing']);
  assert.deepEqual(portsOfModule(types)(device(p, 'pause'))!.out.map((x) => x.label), ['Dim']);
  assert.deepEqual(inputs(p, 'show'), ['Light', 'Flow field', 'Waves', 'Smoothing', 'Glow', 'Haze', 'Saturation', 'Level']);
});

test('named again, a patch is as it was (a kept one is named as it loads)', () => {
  const p = toV2(DEFAULTS);
  const once = JSON.stringify(p);
  assert.equal(JSON.stringify(namedDevices(p, types)), once);
});

test('the rack macro on the Accent is Accent, not Flash; a kept one named so as it loads', () => {
  const p = toV2(DEFAULTS);
  assert.equal(p.macros.find((x) => x.id === 'flash')?.label, 'Accent');
  const kept = structuredClone(p);
  kept.macros.find((x) => x.id === 'flash')!.label = 'Flash';
  assert.equal(namedDevices(kept, types).macros.find((x) => x.id === 'flash')?.label, 'Accent');
});
