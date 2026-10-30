// The effects in the walk (nodes/effect-runs.ts, decision 0057 §3): a line of effect modules drawn as one pass, up to
// three; an Echo trailing its visible Feedback (none: nothing); the pass's effects in today's order: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { effectRuns, fxOfModule, passOf, runIdle } = await import('../src/lib/visuals/nodes/effect-runs.ts');
const { EFFECTS, engineEffect } = await import('../src/lib/visuals/effects.ts');
const { effectType } = await import('../src/lib/visuals/effect-modules.ts');
type Node = Parameters<typeof effectRuns>[0][number];

const ROTATE = engineEffect('kaleidoscope', 3);
const PULSE = engineEffect('eyedo', 1);
const ECHO = engineEffect('hydra', 1);
const typeOf = (id: number) => effectType(EFFECTS.find((e) => e.id === id)!);
const pic = (from: string, port = 'picture') => ({ port, type: 'picture' as const, from: { module: from, port: 'picture' } });
// A line from `layers` into the screen through `fx` (id → effect, amount).
const line = (fx: [string, number, number][]) => {
  const nodes: Node[] = [{ id: 'layers', type: 'mix', inputs: [] }];
  for (const [id, effect] of fx) nodes.push({ id, type: typeOf(effect), inputs: [pic(nodes[nodes.length - 1].id)] });
  nodes.push({ id: 'screen', type: 'screen', inputs: [pic(nodes[nodes.length - 1].id)] });
  const knobs = new Map(fx.map(([id, , amount]) => [id, { amount }]));
  return { nodes, fx: (n: Node) => fxOfModule(n.type, knobs.get(n.id) ?? {}) };
};

test('a line of effects is one pass at its last, from its first one’s picture; an Echo with no Feedback trails nothing', () => {
  const { nodes, fx } = line([['a', ROTATE, 1], ['b', PULSE, 1], ['c', ECHO, 1]]);
  const { runs, order } = effectRuns(nodes, fx);
  assert.deepEqual([...runs], [['c', { fx: ['a', 'b', 'c'], trails: null }]]);
  assert.deepEqual(order.map((n) => [n.id, n.type, n.inputs.map((f) => f.from.module)]), [['layers', 'mix', []], ['c', 'fx-run', ['layers']], ['screen', 'screen', ['c']]]);
});

test('no Echo keeps a hidden trail, so none ends a run; a run holds three at most', () => {
  const ends = line([['c', ECHO, 1], ['b', PULSE, 1], ['a', ROTATE, 1]]);
  assert.deepEqual([...effectRuns(ends.nodes, ends.fx).runs.values()].map((r) => r.fx), [['c', 'b', 'a']]);
  const four = line([['a', ROTATE, 1], ['b', PULSE, 1], ['c', ECHO, 1], ['d', ROTATE, 1]]);
  assert.deepEqual([...effectRuns(four.nodes, four.fx).runs.values()].map((r) => r.fx), [['a', 'b', 'c'], ['d']]);
});

test('an effect read twice ends its run there', () => {
  const { nodes, fx } = line([['a', ROTATE, 1], ['b', PULSE, 1]]);
  nodes.push({ id: 'tap', type: 'glow', inputs: [pic('a')] });
  assert.deepEqual([...effectRuns(nodes, fx).runs.values()].map((r) => r.fx), [['a'], ['b']]);
});

test("a pass's effects: colours and echoes in the line's order, then the shapes the last first; idle at nothing", () => {
  const f = (id: number, amount = 1) => ({ id, amount, k: [0.5, 0.5] as [number, number] });
  assert.deepEqual(passOf([f(ROTATE), f(PULSE), f(ECHO)]).ids, [ECHO, PULSE, ROTATE]);
  assert.deepEqual(passOf([f(ECHO, 0.4)]), { ids: [ECHO, 0, 0], amounts: [0.4, 0, 0], knobs: [[0.5, 0.5], [0.5, 0.5], [0.5, 0.5]] });
  const [x, y] = EFFECTS.filter((e) => !e.holds);
  const held = EFFECTS.find((e) => e.holds)!;
  assert.ok(runIdle([f(x.id, 0), f(y.id, 0)]));
  assert.ok(!runIdle([f(x.id, 0), f(y.id, 0.1)]));
  assert.ok(!runIdle([f(held.id, 0)]), 'one resting where its time took it still draws');
});

test("an engine's module is its effect, its knobs k1 and k2; an effect's module its own knobs", () => {
  assert.deepEqual(fxOfModule('kaleidoscope-engine', { effect: 3, amount: 0.7, k1: 0.2 }), { id: ROTATE, amount: 0.7, k: [0.2, 0.5] });
  const e = EFFECTS.find((x) => x.id === PULSE)!;
  const [n1] = e.knobs.map((k) => k.label.toLowerCase());
  assert.deepEqual(fxOfModule(typeOf(PULSE), { amount: 1, [n1]: 0.9 })?.k[0], 0.9);
  assert.equal(fxOfModule('glow', {}), null);
});

test('the module observed is a barrier: its run ends at it, so its picture is its own', () => {
  const { nodes, fx } = line([['a', ROTATE, 1], ['b', PULSE, 1], ['c', ROTATE, 1]]);
  assert.deepEqual([...effectRuns(nodes, fx, 'b').runs.values()].map((r) => r.fx), [['a', 'b'], ['c']]);
  assert.deepEqual([...effectRuns(nodes, fx, null).runs.values()].map((r) => r.fx), [['a', 'b', 'c']]);
});
