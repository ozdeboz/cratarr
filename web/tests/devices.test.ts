// The devices we offer (devices.ts, decision 0057 §6b): each made of declared modules, cabled port to fitting port,
// its exposed ports and macros real, its loops through a Feedback, opened whole by flatten: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { PICTURE_MODULES, cyclesClosed, fits, portsOf } = await import('../src/lib/visuals/ports.ts');
const { DEVICES } = await import('../src/lib/visuals/devices.ts');
const { flatten } = await import('../src/lib/visuals/v2.ts');

const chainTypeOf = v2Types(cratarrTypes(RANGES, DEFAULTS));
const extra = new Map(PICTURE_MODULES.map((t) => [t.type, t]));
const typeOf = (t: string) => chainTypeOf(t) ?? extra.get(t);

test('the devices', () => {
  assert.deepEqual(
    DEVICES.map((d) => d.label),
    ['Feedback Tunnel', 'Kick', 'Breather', 'Wanderer', 'Bloom', 'Scope', 'Grid Particles', 'Echo Trails', 'Fractal Spiral'],
  );
});

test('each is made of declared modules, cabled port to fitting port, its loops through a Feedback', () => {
  for (const d of DEVICES) {
    const mods = new Map(d.patch.modules.map((m) => [m.id, m]));
    for (const m of d.patch.modules) assert.ok(typeOf(m.type), `${d.id}: ${m.type} is not declared`);
    const portOf = (id: string, port: string, dir: 'in' | 'out') => portsOf(typeOf(mods.get(id)!.type)!)[dir].find((p) => p.id === port);
    for (const c of d.patch.cables) {
      const from = portOf(c.from.module, c.from.port, 'out');
      const to = portOf(c.to.module, c.to.port, 'in');
      assert.ok(from && to, `${d.id}: ${c.from.module}.${c.from.port} → ${c.to.module}.${c.to.port}`);
      assert.ok(fits(from.type, to.type, !!(from.polymorphic || to.polymorphic)), `${d.id}: ${from.type} into ${to.type}`);
    }
    assert.ok(cyclesClosed(d.patch, typeOf), `${d.id}: a loop without a Feedback`);
    for (const e of [...d.inputs, ...d.outputs]) assert.ok(mods.has(e.module), `${d.id}: exposes ${e.module}`);
    for (const e of d.inputs) assert.ok(portOf(e.module, e.port, 'in'), `${d.id}: in ${e.module}.${e.port}`);
    for (const e of d.outputs) assert.ok(portOf(e.module, e.port, 'out'), `${d.id}: out ${e.module}.${e.port}`);
    for (const mc of d.macros)
      for (const t of mc.targets) assert.ok(typeOf(mods.get(t.module)!.type)!.knobs.some((k) => k.id === t.knob), `${d.id}: macro ${mc.id} → ${t.module}.${t.knob}`);
  }
});

test('opened whole: a cable into a device reaches every inner port its input leads to (Bloom’s picture, to both)', () => {
  const bloom = DEVICES.find((d) => d.id === 'bloom')!;
  const p = {
    v: 2 as const,
    modules: [{ id: 'flame', type: 'flame', knobs: {} }, { ...bloom, id: 'b' }],
    cables: [{ id: 'in', from: { module: 'flame', port: 'light' }, to: { module: 'b', port: 'picture' }, value: 1 }],
    macros: [],
    pins: [],
  };
  const f = flatten(p);
  const into = f.cables.filter((c) => c.from.module === 'flame').map((c) => `${c.to.module}.${c.to.port}`).sort();
  assert.deepEqual(into, ['b/bright.picture', 'b/mix.in1']);
});
