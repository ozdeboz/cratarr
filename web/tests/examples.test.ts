// The example racks (examples.ts; the owner, 2026-10-29: example snapshots for each technique): every one a rack the
// page draws as it is: known modules, every cable into a port of its type, its picture reaching the Screen, its loop
// drawn in one pass, its macros on knobs that exist: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { compile, stagesOf, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { fits, portsOfModule } = await import('../src/lib/visuals/ports.ts');
const { loopsOf } = await import('../src/lib/visuals/nodes/walker.ts');
const { upgraded } = await import('../src/lib/visuals/upgrade.ts');
const { EXAMPLES, exampleOf } = await import('../src/lib/visuals/examples.ts');

const PICTURE_TYPES = v2Types(cratarrTypes(RANGES, DEFAULTS));
const RECORD = v2TypeRecord();

test('there are examples, each with an id of its own that no snapshot can have', () => {
  assert.ok(EXAMPLES.length >= 8);
  const ids = new Set(EXAMPLES.map((x) => x.id));
  assert.equal(ids.size, EXAMPLES.length);
  for (const x of EXAMPLES) {
    assert.match(x.id, /^example:[a-z-]+$/);
    assert.equal(exampleOf(x.id), x);
    assert.doesNotMatch(x.label, /^Example/, 'under the library’s Examples: no prefix');
    assert.match(x.line, /\.$/, 'its line a sentence');
  }
});

/** Racks let off the one-pass loop, each with its reason. "The middle, still" is a test rack (the owner's, 2026-10-30):
 *  every knob at the middle of its range, its Mix's amounts at a half too, so its loop is blended module by module, as
 *  any loop with a part-way Mix is; it is there to be looked at, not played. */
const NOT_ONE_PASS: Readonly<Record<string, string>> = {
  'example:wandering-kaleido-middle': 'its Mix at a half, every knob at its middle',
};

// The technique racks; Fire is the Cratarr device (below).
for (const x of EXAMPLES.filter((e) => !e.patch.modules.some((m) => m.type === 'flame'))) {
  test(`${x.label}: known modules, cables that fit, the Screen reached, the loop in one pass`, () => {
    const p = x.patch;
    const ports = portsOfModule(RECORD);
    const byId = new Map(p.modules.map((m) => [m.id, m]));
    for (const m of p.modules) assert.ok(RECORD[m.type], `${m.id}: no such module type ${m.type}`);
    for (const c of p.cables) {
      const from = byId.get(c.from.module);
      const to = byId.get(c.to.module);
      assert.ok(from && to, `${c.id}: both ends are modules`);
      const o = ports(from)!.out.find((q) => q.id === c.from.port);
      const i = ports(to)!.in.find((q) => q.id === c.to.port);
      assert.ok(o, `${c.id}: ${c.from.module} gives ${c.from.port}`);
      assert.ok(i, `${c.id}: ${c.to.module} takes ${c.to.port}`);
      assert.ok(fits(o.type, i.type, i.polymorphic ?? false) || fits(o.type, i.type, o.polymorphic ?? false), `${c.id}: ${o.type} into ${i.type}`);
      if (c.from.channel !== undefined) assert.ok((o.channels ?? 1) > c.from.channel, `${c.id}: a channel it has`);
    }
    for (const mc of p.macros) {
      for (const t of mc.targets) {
        const m = byId.get(t.module);
        assert.ok(m, `${mc.id}: ${t.module} is in the rack`);
        assert.ok(RECORD[m.type].knobs.some((k) => k.id === t.knob), `${mc.id}: ${m.type} has ${t.knob}`);
        // The Grid is laid out from its stored knobs (graph.ts): a macro on its layout would move nothing.
        assert.ok(m.type !== 'grid' || ['drift', 'return'].includes(t.knob), `${mc.id}: no macro on a Grid's ${t.knob}`);
      }
    }
    const c = compile(p, PICTURE_TYPES);
    assert.deepEqual(c.errors, [], 'drawn without errors');
    assert.ok(c.screen, 'a Screen');
    assert.ok(stagesOf(c), 'drawn in its order');
    assert.ok(c.order.some((n) => n.id === 'mix'), 'its picture reaches the Screen');
    const knobs = (id: string) => byId.get(id)?.knobs ?? {};
    // A loop of Warps and Keeps only is drawn in one pass (compose.ts LOOP); one with anything else in it (a Fold, a
    // Displace) module by module, its Feedback in the order.
    const onePass = p.cables.filter((x) => x.from.module === 'feedback').every((x) => ['warp', 'keep'].includes(byId.get(x.to.module)?.type ?? ''));
    if (onePass && !NOT_ONE_PASS[x.id]) assert.equal(loopsOf(c.order, knobs).size, 1, 'its loop drawn in one pass (compose.ts LOOP)');
    else assert.ok(c.order.some((n) => n.type === 'feedback'), 'its loop drawn module by module');
    // As a kept patch made current (upgrade.ts), it is the same rack.
    const up = upgraded(structuredClone(p), RECORD);
    assert.equal(up.modules.length, p.modules.length);
  });
}

test('Fire: the Cratarr device in the Fire colours, its Fluid on, open, lifting and cooling, no flashes', () => {
  const x = exampleOf('example:fire');
  assert.ok(x, 'in the library');
  const flame = x.patch.modules.find((m) => m.type === 'flame');
  const fluid = x.patch.modules.find((m) => m.type === 'fluid');
  assert.equal(flame?.knobs.family, 5, 'the Fire colours');
  assert.ok((fluid?.knobs.fluid ?? 0) > 0, 'its Fluid on');
  assert.deepEqual([fluid?.knobs.lift, fluid?.knobs.open, fluid?.knobs.cooling], [2, 1, 3]);
  assert.equal(x.patch.modules.find((m) => m.type === 'accent')?.knobs.accent, 0, 'no flashes');
  const fluidType = cratarrTypes(RANGES, DEFAULTS).find((t) => t.type === 'fluid')!;
  for (const k of ['lift', 'open', 'cooling']) assert.ok(fluidType.knobs.some((d: { id: string }) => d.id === k), `the Fluid has ${k}`);
  // As a kept patch made current (upgrade.ts), its Fire stays.
  const up = upgraded(structuredClone(x.patch), RECORD);
  assert.equal(up.modules.find((m) => m.type === 'fluid')?.knobs.open, 1);
});
