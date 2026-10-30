// The visuals' patch store (decision 0057, the switch S1): made from the settings where none is kept, changed with
// undo and redo, a new object each change: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const state = <T>(v: T) => v;
state.raw = <T>(v: T) => v;
state.snapshot = <T>(v: T) => structuredClone(v);
(globalThis as Record<string, unknown>).$state = state;
const { visualsPatch, patchFrom } = await import('../src/lib/visuals/visuals-patch.svelte.ts');

test('none kept: the patch is made from the settings, the Cratarr chain in it', () => {
  const p = visualsPatch.patch;
  assert.equal(p.v, 2);
  assert.ok(p.modules.some((m) => m.type === 'flame'));
  assert.ok(p.cables.length > 0);
});

test('a change is a new object, one undo step a key while it runs; undo and redo', () => {
  const before = visualsPatch.patch;
  const flame = before.modules.find((m) => m.type === 'flame')!;
  visualsPatch.change((p) => void (p.modules.find((m) => m.id === flame.id)!.knobs.hue = 0.1), `knob:${flame.id}.hue`);
  visualsPatch.change((p) => void (p.modules.find((m) => m.id === flame.id)!.knobs.hue = 0.2), `knob:${flame.id}.hue`);
  const after = visualsPatch.patch;
  assert.notEqual(after, before);
  assert.equal(after.modules.find((m) => m.id === flame.id)!.knobs.hue, 0.2);
  assert.equal(before.modules.find((m) => m.id === flame.id)!.knobs.hue, flame.knobs.hue, 'the patch before is untouched');
  visualsPatch.undo();
  assert.equal(visualsPatch.patch, before, 'the drag was one step');
  visualsPatch.redo();
  assert.equal(visualsPatch.patch.modules.find((m) => m.id === flame.id)!.knobs.hue, 0.2);
});

test('a performance (a moment, a track’s look) is no undo step', () => {
  const flame = visualsPatch.patch.modules.find((m) => m.type === 'flame')!;
  visualsPatch.change((p) => void (p.modules.find((m) => m.id === flame.id)!.knobs.hue = 0.3), 'knob:hue-a');
  const edited = visualsPatch.patch;
  visualsPatch.perform((p) => void (p.modules.find((m) => m.id === flame.id)!.knobs.symmetry = 3));
  assert.equal(visualsPatch.patch.modules.find((m) => m.id === flame.id)!.knobs.symmetry, 3);
  assert.notEqual(visualsPatch.patch, edited);
  visualsPatch.undo();
  assert.notEqual(visualsPatch.patch.modules.find((m) => m.id === flame.id)!.knobs.hue, 0.3, 'the undo is the edit’s');
});

test('only a v2 patch is taken as kept', () => {
  assert.equal(patchFrom({ v: 1, modules: [], cables: [] }), null);
  assert.equal(patchFrom(null), null);
  assert.deepEqual(patchFrom({ v: 2, modules: [], cables: [] }), { v: 2, modules: [], cables: [], macros: [], pins: [] });
});
