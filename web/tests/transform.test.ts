// Transform (transform.ts, decision 0057 §7b3): one map in the points' camera's square space, for the light and a
// picture alike; idle at its neutral (its picture its input's), on the light drawn with the points: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { apply, chain, IDENTITY, inverse, toScreen, transformIdle, transformOf } = await import('../src/lib/visuals/transform.ts');
const { compile, plan, stageOf, stagesOf, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
await import('../src/lib/visuals/picture-wiring.ts');

const near = (a: number[], b: number[]) => a.forEach((x, i) => assert.ok(Math.abs(x - b[i]) < 1e-9, `${a} vs ${b}`));
// A point in the screen's halves through the map (square space between).
const onScreen = (k: Record<string, number>, aspect: number, p: [number, number]) => {
  const s = toScreen(aspect);
  const q = apply(transformOf(k, aspect), [p[0] / s[0], p[1] / s[1]]);
  return [q[0] * s[0], q[1] * s[1]];
};

test('at its neutral it moves nothing, and is idle', () => {
  const k = { x: 0, y: 0, scale: 1, rotate: 0 };
  assert.ok(transformIdle(k) && transformIdle({}));
  assert.ok(!transformIdle({ ...k, rotate: 0.1 }));
  const t = transformOf(k, 16 / 9);
  near([...t.m, ...t.at], [1, 0, 0, 1, 0, 0]);
});

test('Across and Up in the screen’s halves; Scale and Turn about the middle, square whatever the aspect', () => {
  near(onScreen({ x: 1 }, 16 / 9, [0, 0]), [1, 0]);
  near(onScreen({ y: -1 }, 9 / 16, [0, 0]), [0, -1]);
  near(onScreen({ scale: 2 }, 16 / 9, [0.25, 0.25]), [0.5, 0.5]);
  // A quarter turn counter-clockwise: the right edge's middle (square: the short side's) goes up.
  const s = toScreen(16 / 9);
  near(onScreen({ rotate: 0.25 }, 16 / 9, [s[0], 0]), [0, 1]);
});

test('maps chain and undo', () => {
  const a = transformOf({ x: 0.3, rotate: 0.1, scale: 1.5 }, 1.5);
  const b = transformOf({ y: -0.2, scale: 0.5 }, 1.5);
  const p: [number, number] = [0.4, -0.7];
  near(apply(chain(a, b), p), apply(b, apply(a, p)));
  near(apply(inverse(a), apply(a, p)), p);
  assert.deepEqual(chain(IDENTITY, a), a);
});

test('on a picture: in the compose step, at rest aliased; on the light: with the points', () => {
  const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
  const v = toV2(JSON.parse(JSON.stringify(DEFAULTS)));
  // One after the layers, before the effects; one on the flame's light after Dive.
  const into = v.cables.find((c) => c.from.module === 'layers')!;
  const dive = v.cables.find((c) => c.from.module === 'dive')!;
  const p = {
    ...v,
    modules: [...v.modules, { id: 'move', type: 'transform', knobs: { x: 0, y: 0, scale: 1, rotate: 0 } }, { id: 'frame', type: 'transform', knobs: { x: 0.2 } }],
    cables: [
      ...v.cables.filter((c) => c !== into && c !== dive),
      { id: 'a', from: into.from, to: { module: 'move', port: 'in' }, value: 1 },
      { id: 'b', from: { module: 'move', port: 'out' }, to: into.to, value: 1 },
      { id: 'c', from: dive.from, to: { module: 'frame', port: 'in' }, value: 1 },
      { id: 'd', from: { module: 'frame', port: 'out' }, to: dive.to, value: 1 },
    ],
  };
  const c = compile(p, types);
  assert.deepEqual(c.errors, []);
  assert.ok(stagesOf(c));
  const by = (id: string) => c.order.find((n) => n.id === id)!;
  assert.equal(stageOf(by('move')), 'compose');
  assert.equal(stageOf(by('frame')), 'points');
  const knobs = (id: string) => p.modules.find((m) => m.id === id)?.knobs ?? {};
  const lazy = plan(c, { knobs, idle: (id) => id === 'move' && transformIdle(knobs(id)) });
  assert.deepEqual(lazy.alias.get('move'), { module: 'layers', port: 'picture' });
});
