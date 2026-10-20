// The presets (decision 0057, the switch): each a v2 patch; the saved ones with the flame; scratch empty: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { PRESETS } = await import('../src/lib/visuals/presets.ts');

test('every preset is a v2 patch; the saved ones draw the flame; Start from scratch is nothing', () => {
  for (const p of PRESETS) {
    const v = p.patch();
    assert.equal(v.v, 2, p.id);
    assert.ok(Array.isArray(v.modules) && Array.isArray(v.cables), p.id);
  }
  for (const id of ['default', 'entrance-to-zanzibar', 'glitch-adventure']) assert.ok(PRESETS.find((p) => p.id === id)!.patch().modules.some((m) => m.type === 'flame'), id);
  assert.deepEqual(PRESETS.find((p) => p.id === 'scratch')!.patch().modules, []);
});
