// The panel's edits of a v2 patch (decision 0057, the switch S3): a cable's amount, depth and channel; Reroute; a
// module removed with what hangs on it; a pin; the graph's edits handed on: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { applyEdit, panelKey } = await import('../src/lib/visuals/patch-edits.ts');
type P = import('../src/lib/visuals/ports.ts').PatchV2;

const patch = (): P => ({
  v: 2,
  modules: [
    { id: 'lfo-1', type: 'lfo', knobs: {} },
    { id: 'flame', type: 'flame', knobs: { hue: 0.5 } },
    { id: 'env', type: 'envelope', knobs: {} },
  ],
  cables: [
    { id: 'c1', from: { module: 'lfo-1', port: 'out' }, to: { module: 'flame', port: 'hue' }, value: 0.5 },
    { id: 'c2', from: { module: 'env', port: 'out' }, to: { module: 'flame', port: 'spread' }, value: 1, depth: { module: 'lfo-1', port: 'out' } },
  ],
  macros: [{ id: 'm', on: 'flame', label: 'M', value: 0, targets: [{ module: 'lfo-1', knob: 'rate', from: 0, to: 1 }] }] as never,
  pins: ['lfo-1.rate', 'cable:c1', 'flame.hue'],
});

test('a cable’s amount, depth and channel', () => {
  const p = applyEdit(patch(), { kind: 'cable', scope: [], id: 'c1', value: 0.8, depth: { module: 'env', port: 'out' }, channel: 2 });
  const c = p.cables.find((x) => x.id === 'c1')!;
  assert.deepEqual([c.value, c.depth, c.from.channel], [0.8, { module: 'env', port: 'out' }, 2]);
  const q = applyEdit(p, { kind: 'cable', scope: [], id: 'c1', channel: null, depth: null });
  assert.deepEqual([q.cables[0].from, q.cables[0].depth], [{ module: 'lfo-1', port: 'out' }, null]);
});

test('Reroute moves one end', () => {
  const p = applyEdit(patch(), { kind: 'reroute', scope: [], id: 'c1', end: 'to', to: { module: 'flame', port: 'twist' } });
  assert.deepEqual(p.cables[0].to, { module: 'flame', port: 'twist' });
});

test("a module removed takes its cables, its pins and its macro targets, as the graph removes one (a cable's depth from it let go, the cable kept: 0.10, one reducer)", () => {
  const p = applyEdit(patch(), { kind: 'removeModule', scope: [], id: 'lfo-1' });
  assert.deepEqual(p.modules.map((m) => m.id), ['flame', 'env']);
  assert.deepEqual(p.cables.map((c) => [c.id, c.depth ?? null]), [['c2', null]]);
  assert.deepEqual(p.pins, ['flame.hue']);
  assert.deepEqual((p.macros[0] as { targets: unknown[] }).targets, []);
});

test('a pin, on and off; the graph’s edits handed on; a knob’s and a cable’s key', () => {
  let p = applyEdit(patch(), { kind: 'pinOne', scope: [], pin: 'module:env', on: true });
  assert.ok(p.pins.includes('module:env'));
  p = applyEdit(p, { kind: 'pinOne', scope: [], pin: 'flame.hue', on: false });
  assert.ok(!p.pins.includes('flame.hue'));
  p = applyEdit(p, { kind: 'knob', module: ['flame'], knob: 'hue', value: 0.25 });
  assert.equal(p.modules.find((m) => m.id === 'flame')!.knobs.hue, 0.25);
  assert.equal(panelKey({ kind: 'knob', module: ['show', 'blur'], knob: 'smoothing', value: 0 }), 'knob:show/blur.smoothing');
  assert.equal(panelKey({ kind: 'cable', scope: ['show'], id: 'c9' }), 'cable:show/c9');
});

test("the visuals' own values (a roll, a step's Next) through the same reducer: knobs, cables, macros", () => {
  const p = applyEdit(patch(), { kind: 'values', knobs: { env: { intensity: 0.75 } }, cables: { c1: 0.2 }, macros: { m: 0.6 } });
  assert.equal(p.modules.find((m) => m.id === 'env')!.knobs.intensity, 0.75);
  assert.equal(p.cables.find((c) => c.id === 'c1')!.value, 0.2);
  assert.equal((p.macros[0] as { value: number }).value, 0.6);
  // A pinned knob's value stays as set (applyKnobs: the visuals' own changes never move a pinned knob).
  const q = applyEdit(patch(), { kind: 'values', knobs: { 'lfo-1': { rate: 0.9 } } });
  assert.equal(q.modules.find((m) => m.id === 'lfo-1')!.knobs.rate, undefined);
});
