// Instance and Fold's points (points-pictures.ts, decision 0057 §4c), and the Grid Particles device in the compose
// step with its Instance: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { MOST, nearest, packed, spacingOf } = await import('../src/lib/visuals/points-pictures.ts');
const { gridOf } = await import('../src/lib/visuals/engine/grid.ts');
const { DEVICES } = await import('../src/lib/visuals/devices.ts');
const { compile, composeOf, stageOf, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { flatten } = await import('../src/lib/visuals/v2.ts');

test("each point's cell is the nearest neighbour's distance; a lone point the whole short side", () => {
  assert.equal(spacingOf([{ x: 0, y: 0 }]), 2);
  assert.equal(spacingOf([{ x: 0, y: 0 }, { x: 0.3, y: 0.4 }, { x: 1, y: 1 }]), 0.5);
  const g = gridOf({ lattice: 0, count: 16, spacing: 0.25, rotation: 0, jitter: 0 }, 1);
  assert.ok(Math.abs(spacingOf(g.points) - 0.25) < 1e-9, 'a square grid: its spacing');
});

test('points packed up to the most, and the nearest found', () => {
  const out = new Float32Array(MOST * 2);
  const many = Array.from({ length: 80 }, (_, i) => ({ x: i, y: -i }));
  assert.equal(packed(many, out), MOST);
  assert.deepEqual([out[2], out[3]], [1, -1]);
  assert.equal(packed(null, out), 0);
  assert.equal(nearest([{ x: -1, y: 0 }, { x: 1, y: 0 }], { x: 0.4, y: 0.9 }), 1);
});

test('Grid Particles: its Instance in the compose step, its Shapes into it, the Grid outside the picture graph', () => {
  const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
  const dev = DEVICES.find((d) => d.id === 'grid-particles')!;
  const p = { v: 2 as const, macros: [], pins: [], modules: [{ ...dev, id: 'gp' }, { id: 'screen', type: 'screen', knobs: {} }], cables: [{ id: 'a', from: { module: 'gp', port: 'picture' }, to: { module: 'screen', port: 'picture' }, value: 1 }] };
  const c = compile(p as never, types);
  assert.deepEqual(c.errors, []);
  const each = c.order.find((n) => n.type === 'instance')!;
  assert.equal(stageOf(each), 'compose');
  assert.deepEqual(each.inputs.map((f) => [f.port, f.from.module]), [['picture', 'gp/dot']]);
  assert.ok(composeOf(c)!.ids.has(each.id));
  // Its points' cable, as render.ts finds it.
  const flat = flatten(p as never);
  assert.ok(flat.cables.some((x) => x.to.module === each.id && x.to.port === 'points' && x.from.module === 'gp/grid'));
});
