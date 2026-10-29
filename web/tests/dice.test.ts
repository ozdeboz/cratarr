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

test("the shipped face: the look's seven, Effects, then the behaviour four; the look as it was", async () => {
  const { asSet, flatten } = await import('../src/lib/visuals/v2.ts');
  const p = toV2(structuredClone(DEFAULTS));
  assert.deepEqual(p.macros.map((x) => x.label), ['Form', 'Shape', 'Symmetry', 'Lens', 'Colours', 'Hue', 'Light', 'Effects', 'Intensity', 'Accent', 'Speed', 'Wild']);
  const flat = flatten(p);
  const flame = flat.modules.find((m) => m.type === 'flame')!;
  for (const k of ['form', 'detail', 'twist', 'warp', 'symmetry', 'mirror', 'final', 'family', 'hue', 'lightness', 'vividness'])
    assert.ok(Math.abs(asSet(flat, flame, k, types)! - flame.knobs[k]) < 1e-9, `${k}: ${asSet(flat, flame, k, types)} for ${flame.knobs[k]}`);
});

test('a roll of the face draws something, whatever the seed', async () => {
  const { applyMacros, asSet, flatten } = await import('../src/lib/visuals/v2.ts');
  const { drawScore } = await import('../src/lib/visuals/flame.ts');
  const { LOOK_KNOBS } = await import('../src/lib/visuals/engine/flame-node.ts');
  const p = toV2(structuredClone(DEFAULTS));
  for (let seed = 1; seed <= 12; seed++) {
    // Run to its end, counting the seeds it tried (each after the first a yield).
    const g = rollOf(p, types, seed, 'macros');
    let n = g.next();
    let tries = 1;
    while (!n.done) {
      n = g.next();
      tries++;
    }
    const r = n.value;
    const q = structuredClone(p);
    applyMacros(q, r.macros!);
    const flat = flatten(q);
    const flame = flat.modules.find((m) => m.type === 'flame')!;
    const look = Object.fromEntries(LOOK_KNOBS.map((k) => [k, asSet(flat, flame, k, types) ?? 0]));
    // The look's own check, at the seed the roll was taken at: above ½, as a track's look always was.
    assert.ok(drawScore(look as never, seed + tries - 1) > 0.5, `seed ${seed}`);
  }
});

test("a kept rack gains the look's macros once, the look as it was", () => {
  const p = toV2(structuredClone(DEFAULTS));
  p.macros = p.macros.filter((x) => !x.id.startsWith('look-'));
  delete p.rev;
  upgraded(p, types);
  assert.equal(p.macros.filter((x) => x.id.startsWith('look-')).length, 7);
  upgraded(p, types);
  assert.equal(p.macros.filter((x) => x.id.startsWith('look-')).length, 7);
});

test('a knob a macro owns rolls through its owner: Colours, the Flame’s Randomize, Everything', () => {
  const p = toV2(structuredClone(DEFAULTS));
  // The long press's Colours: the Flame's hue and family are the Hue and Colours macros' now, so those roll.
  const colours = rolled(rollOf(p, types, 5, 'colours'));
  assert.ok(colours.macros && 'look-hue' in colours.macros && 'look-colours' in colours.macros, JSON.stringify(Object.keys(colours.macros ?? {})));
  assert.ok(!colours.values.flame || (!('hue' in colours.values.flame) && !('family' in colours.values.flame)));
  // The Flame's own Randomize: its look's macros for its owned knobs; its own for the rest.
  const flame = rolled(rollOf(p, types, 5, { module: ['flame'] }));
  for (const id of ['look-form', 'look-shape', 'look-hue']) assert.ok(flame.macros && id in flame.macros, id);
  for (const k of Object.keys(flame.values.flame ?? {})) assert.ok(!['form', 'detail', 'twist', 'warp', 'hue', 'family', 'lightness', 'vividness'].includes(k), k);
  // Everything: every macro besides the knobs no macro owns.
  const all = rolled(rollOf(p, types, 5, 'everything'));
  assert.deepEqual(Object.keys(all.macros!).sort(), p.macros.map((x) => x.id).sort());
});
