// The dice's limits (engine/dice.ts; the owner, 2026-10-29: "engine purity"): the ones the rack's Calm holds the
// picture to, each kind's Ducker's threshold as set, Wild in, not a macro found by its id: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');
const { diceLimits } = await import('../src/lib/visuals/engine/dice.ts');
const { STANDING } = await import('../src/lib/visuals/engine/sidechain.ts');
const { BUDGETED } = await import('../src/lib/visuals/budget.ts');

const near = (a: number, b: number) => Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(b));

test("the dice's limits are the Calm's: at Wild ½ the standing ones, at Wild 1 twice them, as the old rule gave", () => {
  for (const wild of [0.5, 1, 0.2]) {
    const p = toV2({ ...structuredClone(DEFAULTS), wild });
    const got = diceLimits(p);
    for (const k of BUDGETED) assert.ok(near(got[k], STANDING[k] * 2 ** (2 * (wild - 0.5))), `${k} at Wild ${wild}: ${got[k]}`);
  }
});

test('a rack without a Calm: the standing limits', () => {
  const p = toV2(structuredClone(DEFAULTS));
  p.modules = p.modules.filter((m) => m.id !== 'calm');
  assert.deepEqual(diceLimits(p), { ...STANDING });
});
