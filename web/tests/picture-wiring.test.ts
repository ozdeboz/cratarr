// The picture side of a converted patch (picture-wiring.ts, through v2.ts's hook) and the passes it is drawn with
// (picture-graph.ts `stagesOf`), decision 0057 step 3: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { compile, stagesOf, STAGES, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXAMPLES } = await import('../src/lib/visuals/chains.ts');
type VisualSettings = typeof DEFAULTS;

const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
const extraOf = (id: string) => EXAMPLES.find((c) => c.id === id)!.extra;
const settings = (extra?: VisualSettings['extra']): VisualSettings => ({ ...JSON.parse(JSON.stringify(DEFAULTS)), ...(extra ? { extra } : {}) });
// The window example's layer as a plain one (its Glow off: a Glow on a layer is the walk's, tested below).
const layered = () => ({ ...extraOf('example-kaleido-layer'), layers: [...extraOf('example-kaleido-layer').layers, ...extraOf('example-window').layers.map((l) => ({ ...l, modules: [] }))], loop: extraOf('example-feedback').loop });

test("today's chain converted is whole: the flame's light loop, Show, the layers, the effects, the screen", () => {
  const c = compile(toV2(settings()), types);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(
    c.order.map((n) => n.id),
    ['trails', 'fluid', 'tunnel', 'loop-effect', 'dive', 'flame', 'show/bend', 'show/blur', 'show/tonemap', 'show/curve', 'show/saturate', 'light-feedback', 'layers', 'fx1', 'fx2', 'fx3', 'screen'],
  );
  assert.deepEqual(stagesOf(c), [...STAGES]);
  // Bend and Blur on the light, Tone map its only converter.
  const bend = c.order.find((n) => n.id === 'show/bend')!;
  assert.deepEqual(bend.inputs.map((f) => [f.from.module, f.type]), [['flame', 'light']]);
});

test('layers and the picture’s own loop: mixed in their order, the loop’s Feedback after its readers', () => {
  const v = toV2(settings(layered()));
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  const ids = c.order.map((n) => n.id);
  const at = (id: string) => ids.indexOf(id);
  assert.ok(at('shapes-kaleido') < at('layers') && at('window-source') < at('layers'));
  assert.ok(at('example-keep') < at('loop-mix') && at('loop-mix') < at('loop-feedback') && at('loop-mix') < at('screen'));
  assert.deepEqual(c.order.find((n) => n.id === 'layers')!.inputs.map((f) => f.from.module), ['show/saturate', 'shapes-kaleido', 'window-source']);
  const mix = v.modules.find((m) => m.id === 'layers')!;
  assert.deepEqual([mix.knobs.mode1, mix.knobs.mode2, mix.knobs.mode3], [0, 1, 4], 'add, screen, window');
  assert.deepEqual(stagesOf(c), [...STAGES]);
});

test('passes out of the order this version draws them in are refused', () => {
  const c = compile(toV2(settings()), types);
  assert.equal(stagesOf({ ...c, order: [...c.order].reverse() }), null);
});

test("the effects in a line drawing today's one pass: colours and echoes in slot order, then the shapes reversed", async () => {
  const { effectsInOrder } = await import('../src/lib/visuals/picture-wiring.ts');
  const fx = (id: string, type: string, effect: number) => ({ id, type, knobs: { effect, amount: 1 } });
  // Rotate (a shape), Pulse (a shape), Echo; Posterize (a colour) between shapes.
  assert.deepEqual(effectsInOrder([fx('fx1', 'kaleidoscope-engine', 3), fx('fx2', 'eyedo-engine', 1), fx('fx3', 'hydra-engine', 1)]).map((m) => m.id), ['fx3', 'fx2', 'fx1']);
  assert.deepEqual(effectsInOrder([fx('fx1', 'kaleidoscope-engine', 1), fx('fx3', 'hydra-engine', 7)]).map((m) => m.id), ['fx3', 'fx1']);
});

test('the effects run as one pass, but a run ends at an Echo that shows', async () => {
  const { effectRuns } = await import('../src/lib/visuals/picture-graph.ts');
  const ids = [10, 16, 9]; // Rotate, Pulse, Echo
  assert.deepEqual(effectRuns([0, 1, 2], ids, [1, 1, 1]), [[10, 16, 9]], 'Echo last: one pass');
  assert.deepEqual(effectRuns([2, 1, 0], ids, [1, 1, 1]), [[0, 0, 9], [10, 16, 0]]);
  assert.deepEqual(effectRuns([2, 1, 0], ids, [1, 1, 0]), [[10, 16, 9]], 'an Echo at nothing ends none');
});

test('an Echo with shapes after it is a visible loop: Feedback → Echo → Mix (Lighten) beside the picture, one pass', async () => {
  const { effectRuns } = await import('../src/lib/visuals/picture-graph.ts');
  // Rotate, Pulse, Echo (the effects recording's).
  const v = toV2({ ...settings(), fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8 });
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  const ids = c.order.map((n) => n.id);
  assert.deepEqual(ids.slice(ids.indexOf('layers')).filter((id) => id !== 'fx3-feedback'), ['layers', 'fx3-mix', 'fx2', 'fx1', 'screen']);
  assert.ok(ids.indexOf('fx3-feedback') > ids.indexOf('fx1'), 'after its input and its reader');
  assert.ok(ids.indexOf('fx3') < ids.indexOf('fx3-mix'));
  const by = (id: string) => c.order.find((n) => n.id === id)!;
  assert.deepEqual(by('fx3').inputs.map((f) => f.from.module), ['fx3-feedback']);
  assert.deepEqual(by('fx3-mix').inputs.map((f) => [f.port, f.from.module]), [['in1', 'layers'], ['in2', 'fx3']]);
  assert.deepEqual(by('fx3-feedback').inputs.map((f) => f.from.module), ['fx1']);
  assert.deepEqual(stagesOf(c), [...STAGES]);
  assert.deepEqual(effectRuns([2, 1, 0], [10, 16, 9], [1, 0.8, 0.8], new Set([2])), [[10, 16, 9]], 'the loop in the one pass');
  // Without shapes an Echo stays a plain module.
  const plain = compile(toV2({ ...settings(), fx3: 1, fx3Amount: 0.8 }), types);
  assert.ok(!plain.order.some((n) => n.id === 'fx3-feedback'));
});

test('lazy: the flame’s layer at nothing cuts the flame and its own loop, unless its morph brings it back', async () => {
  const { plan } = await import('../src/lib/visuals/picture-graph.ts');
  const c = compile(toV2(settings()), types);
  const live = (share: number, to: number) =>
    plan(c, { knobs: (id): Record<string, number> => (id === 'layers' ? { mode1: 0, amount1: share } : {}), target: (id, k) => (id === 'layers' && k === 'amount1' ? to : 1) }).live.map((n) => n.id);
  assert.ok(live(1, 1).includes('flame'));
  const cut = live(0, 0);
  for (const id of ['flame', 'trails', 'fluid', 'show/tonemap', 'light-feedback']) assert.ok(!cut.includes(id), id);
  assert.ok(cut.includes('screen'));
  assert.ok(live(0, 1).includes('flame'), 'coming back: still gathering');
});

test('the compose step: its modules, from Show’s picture to what goes on', async () => {
  const { composeOf } = await import('../src/lib/visuals/picture-graph.ts');
  const c = compile(toV2(settings(layered())), types);
  const cs = composeOf(c)!;
  assert.equal(cs.from, 'show/saturate');
  assert.equal(cs.to, 'loop-mix');
  for (const id of ['shapes-source', 'shapes-kaleido', 'window-source', 'layers', 'example-warp', 'example-keep', 'loop-mix', 'loop-feedback']) assert.ok(cs.ids.has(id), id);
  assert.ok(!cs.ids.has('fx1') && !cs.ids.has('show/saturate'));
  // A Glow on a layer: in the compose step.
  const v = toV2(settings(layered()));
  const glowed = { ...v, modules: [...v.modules, { id: 'glow-1', type: 'glow', knobs: { glow: 0.5 } }], cables: v.cables.map((x) => (x.from.module === 'window-source' ? { ...x, from: { module: 'glow-1', port: 'picture' } } : x)).concat({ id: 'g', from: { module: 'window-source', port: 'picture' }, to: { module: 'glow-1', port: 'picture' }, value: 1, mode: 'swing' }) };
  const g = compile(glowed, types);
  assert.deepEqual(g.errors, []);
  const gs = composeOf(g)!;
  assert.ok(gs.ids.has('glow-1'));
});

test('the walk draws the picture’s own loop as one pass: its Feedback, its Warps and Keeps, a Mix keeping the brighter', async () => {
  const { composeOf } = await import('../src/lib/visuals/picture-graph.ts');
  const { loopsOf } = await import('../src/lib/visuals/nodes/walker.ts');
  const v = toV2(settings(layered()));
  const c = compile(v, types);
  const ids = composeOf(c)!.ids;
  const knobs = (id: string) => v.modules.find((m) => m.id === id)?.knobs ?? {};
  const loops = loopsOf(c.order.filter((n) => ids.has(n.id)), knobs);
  assert.deepEqual([...loops.entries()], [['loop-mix', { feedback: 'loop-feedback', chain: [{ id: 'example-warp', type: 'warp' }, { id: 'example-keep', type: 'keep' }] }]]);
  // Not lightened: drawn module by module.
  assert.equal(loopsOf(c.order.filter((n) => ids.has(n.id)), (id) => (id === 'loop-mix' ? { ...knobs(id), mode2: 0 } : knobs(id))).size, 0);
});

test('every preset is drawn whole: no errors, its stages; Start from scratch has no Screen', async () => {
  const { PRESETS } = await import('../src/lib/visuals/presets.ts');
  for (const p of PRESETS) {
    const c = compile(p.patch(), types);
    if (p.id === 'scratch') assert.deepEqual(c.errors, ['Nothing reaches a Screen: there is none.']);
    else assert.ok(!c.errors.length && stagesOf(c), `${p.id}: ${c.errors.join(' · ')}`);
  }
});
