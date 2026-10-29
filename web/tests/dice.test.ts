// The dice (engine/dice.ts; decision 0077, the owner: "keep the dice coherent too"): one roll, the face of where it is,
// from a seed: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { DEVICES } = await import('../src/lib/visuals/devices.ts');
const { rollOf } = await import('../src/lib/visuals/engine/dice.ts');
const { upgraded, REV } = await import('../src/lib/visuals/upgrade.ts');
const { RANDOMIZE_SCOPES } = await import('../src/lib/visuals/ports.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

const types = v2TypeRecord();
/** A roll run to its end (its yields the frames it spreads over). */
function rolled(g: Generator<void, { values: Record<string, Record<string, number>>; cables: Record<string, unknown>; macros?: Record<string, number> }>) {
  let n = g.next();
  while (!n.done) n = g.next();
  return n.value;
}

test("the face: the rack's macros rolled, a macro mapped one level up (Calm's Wild) its owner's", () => {
  const p = toV2(structuredClone(DEFAULTS));
  const r = rolled(rollOf(p, types, 7, 'macros'));
  assert.deepEqual(Object.keys(r.macros!).sort(), p.macros.map((x) => x.id).sort());
  assert.ok(!('calm/macro:wild' in r.macros!));
  assert.deepEqual(r.values, {});
  for (const v of Object.values(r.macros!)) assert.ok(v >= 0 && v <= 1);
});

test('a pinned macro is kept; Reach 0 moves nothing', () => {
  const p = toV2(structuredClone(DEFAULTS));
  p.pins.push('macro:speed');
  const r = rolled(rollOf(p, types, 7, 'macros'));
  assert.ok(!('speed' in r.macros!));
  const still = rolled(rollOf(p, types, 7, 'macros', 0));
  for (const x of p.macros) if (x.id in still.macros!) assert.ok(Math.abs(still.macros![x.id] - x.value) < 1e-12);
});

test("a device's Randomize rolls its face (its macros); a module's its knobs", () => {
  const bloom = structuredClone(DEVICES.find((d) => d.id === 'bloom')!);
  const p: PatchV2 = { v: 2, modules: [bloom, { id: 'glow', type: 'glow', knobs: { glow: 0.4 } }], cables: [], macros: [], pins: [] };
  const face = rolled(rollOf(p, types, 3, { module: ['bloom'] }));
  assert.deepEqual(Object.keys(face.macros!).sort(), bloom.macros.map((x) => `bloom/macro:${x.id}`).sort());
  const own = rolled(rollOf(p, types, 3, { module: ['glow'] }));
  assert.deepEqual(Object.keys(own.values), ['glow']);
  assert.equal(own.macros, undefined);
});

test('a kept rack: its new-track Dice moves from Everything to the face once; a later choice stays', () => {
  const p = toV2(structuredClone(DEFAULTS));
  const dice = p.modules.find((m) => m.type === 'dice')!;
  assert.equal(dice.knobs.scope, RANDOMIZE_SCOPES.indexOf('macros'));
  // As kept before 0077.
  delete p.rev;
  dice.knobs.scope = RANDOMIZE_SCOPES.indexOf('everything');
  upgraded(p, types);
  assert.equal(dice.knobs.scope, RANDOMIZE_SCOPES.indexOf('macros'));
  assert.equal(p.rev, REV);
  // Set back to Everything since: it stays.
  dice.knobs.scope = RANDOMIZE_SCOPES.indexOf('everything');
  upgraded(p, types);
  assert.equal(dice.knobs.scope, RANDOMIZE_SCOPES.indexOf('everything'));
});
