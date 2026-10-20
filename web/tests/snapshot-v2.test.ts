// A snapshot as the switch keeps it: the patch whole, back as it was; an older one converted: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const state = <T>(v: T) => v;
state.raw = <T>(v: T) => v;
state.snapshot = <T>(v: T) => structuredClone(v);
(globalThis as Record<string, unknown>).$state = state;
const { snapshotOfPatch, patchOfSnapshot } = await import('../src/lib/visuals/snapshot-v2.ts');

test('a patch kept and back; a settings snapshot converted', () => {
  const p = { v: 2 as const, modules: [{ id: 'noise-1', type: 'noise', knobs: { scale: 0.3 } }], cables: [], macros: [], pins: ['category:sources'] };
  assert.deepEqual(patchOfSnapshot(snapshotOfPatch(p)), p);
  const old = patchOfSnapshot({ v: 3, hue: 0.2 });
  assert.equal(old.v, 2);
  assert.equal(old.modules.find((m) => m.type === 'flame')!.knobs.hue, 0.2);
});
