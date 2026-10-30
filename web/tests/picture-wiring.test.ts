// The picture side of a converted patch (picture-wiring.ts, through v2.ts's hook) and the passes it is drawn with
// (picture-graph.ts `stagesOf`), decision 0057 step 3: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { compile, stagesOf, STAGES, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXAMPLES } = await import('./harness/oracle/chains.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
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
    ['trails', 'fluid', 'tunnel', 'dive', 'flame', 'show/bend', 'show/blur', 'show/tonemap', 'show/curve', 'show/saturate', 'light-feedback', 'layers', 'screen'],
  );
  assert.deepEqual(stagesOf(c), STAGES.filter((x) => x !== 'effects'), 'no effect: no effects');
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
  assert.deepEqual(stagesOf(c), STAGES.filter((x) => x !== 'effects'), 'no effect: no effects');
});

test('a module’s pass is by what it takes, not where it stands; one reading a later pass is refused, named', async () => {
  const { stageRefusal } = await import('../src/lib/visuals/picture-graph.ts');
  const c = compile(toV2(settings()), types);
  assert.deepEqual(stagesOf({ ...c, order: [...c.order].reverse() }), STAGES.filter((x) => x !== 'effects'));
  const pic = (from: string) => ({ port: 'light', type: 'light' as const, from: { module: from, port: 'picture' } });
  const bad = { order: [{ id: 's', type: 'shapes', inputs: [] }, { id: 'trails', type: 'trails', inputs: [pic('s')] }], screen: null, errors: [] };
  assert.equal(stagesOf(bad), null);
  assert.match(stageRefusal(bad) ?? '', /^trails takes s/);
});

test('Show’s own parts are what take the light and those fused straight after Tone map; elsewhere they are modules', async () => {
  const { stageOf } = await import('../src/lib/visuals/picture-graph.ts');
  const { DEVICES } = await import('../src/lib/visuals/devices.ts');
  const c = compile(toV2(settings()), types);
  const stage = (cc: typeof c, id: string) => stageOf(cc.order.find((n) => n.id === id)!);
  assert.deepEqual(['show/bend', 'show/blur', 'show/tonemap', 'show/curve', 'show/saturate'].map((id) => stage(c, id)), ['show', 'show', 'show', 'show', 'show']);
  // After the layers (between them and the Screen): each a module of the compose step.
  const after = (m: { id: string; type: string; knobs: Record<string, number> }, input = 'picture', out = 'picture') => {
    const v = toV2(settings());
    v.modules.push(m as never);
    const cab = v.cables.find((x) => x.from.module === 'layers' && x.from.port === 'picture')!;
    const to = cab.to;
    cab.to = { module: m.id, port: input };
    v.cables.push({ id: `x-${m.id}`, from: { module: m.id, port: out }, to, value: 1 } as never);
    return compile(v, types);
  };
  for (const type of ['blur', 'bend', 'curve', 'saturate']) {
    const cc = after({ id: `x-${type}`, type, knobs: {} });
    assert.deepEqual(cc.errors, [], type);
    assert.ok(stagesOf(cc), type);
    assert.equal(stage(cc, `x-${type}`), 'compose', type);
  }
  const bloom = after(JSON.parse(JSON.stringify(DEVICES.find((d) => d.id === 'bloom')!)));
  assert.deepEqual(bloom.errors, []);
  assert.ok(stagesOf(bloom), 'Bloom after the layers');
  // The cover into a Tint: Music a picture source like any other.
  const v = toV2(settings());
  v.modules.push(JSON.parse(JSON.stringify(DEVICES.find((d) => d.id === 'scope')!)));
  v.cables.push({ id: 'x1', from: { module: 'scope', port: 'picture' }, to: { module: 'layers', port: 'in2' }, value: 1 } as never, { id: 'x2', from: { module: 'music', port: 'cover' }, to: { module: 'scope', port: 'colours' }, value: 1 } as never);
  const cover = compile(v, types);
  assert.deepEqual(cover.errors, []);
  assert.ok(stagesOf(cover), 'cover → Tint');
  assert.equal(stage(cover, 'music'), 'compose');
});

test("the effects in a line drawing today's one pass: colours and echoes in slot order, then the shapes reversed", async () => {
  const { effectsInOrder } = await import('../src/lib/visuals/picture-wiring.ts');
  const fx = (id: string, type: string, effect: number) => ({ id, type, knobs: { effect, amount: 1 } });
  // Rotate (a shape), Pulse (a shape), Echo; Posterize (a colour) between shapes.
  assert.deepEqual(effectsInOrder([fx('fx1', 'kaleidoscope-engine', 3), fx('fx2', 'eyedo-engine', 1), fx('fx3', 'hydra-engine', 1)]).map((m) => m.id), ['fx3', 'fx2', 'fx1']);
  assert.deepEqual(effectsInOrder([fx('fx1', 'kaleidoscope-engine', 1), fx('fx3', 'hydra-engine', 7)]).map((m) => m.id), ['fx3', 'fx1']);
});

test('an Echo is a visible loop: Feedback → Echo → Mix (Lighten) beside the picture, one pass with any shapes after it', async () => {
  const { effectRuns, fxOfModule } = await import('../src/lib/visuals/nodes/effect-runs.ts');
  // Rotate, Pulse, Echo (the effects recording's): each its effect's module.
  const v = toV2({ ...settings(), fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8 });
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  const ids = c.order.map((n) => n.id);
  assert.deepEqual(ids.slice(ids.indexOf('layers')).filter((id) => id !== 'fx-echo-feedback'), ['layers', 'fx-echo-mix', 'fx-pulse', 'fx-rotate', 'screen']);
  assert.ok(ids.indexOf('fx-echo-feedback') > ids.indexOf('fx-rotate'), 'after its input and its reader');
  assert.ok(ids.indexOf('fx-echo') < ids.indexOf('fx-echo-mix'));
  const by = (id: string) => c.order.find((n) => n.id === id)!;
  assert.deepEqual(by('fx-echo').inputs.map((f) => f.from.module), ['fx-echo-feedback']);
  assert.deepEqual(by('fx-echo-mix').inputs.map((f) => [f.port, f.from.module]), [['in1', 'layers'], ['in2', 'fx-echo']]);
  assert.deepEqual(by('fx-echo-feedback').inputs.map((f) => f.from.module), ['fx-rotate']);
  assert.deepEqual(stagesOf(c), [...STAGES]);
  // The walk draws the line as one pass, its Echo trailing the Feedback.
  const knobs = (id: string) => v.modules.find((m) => m.id === id)?.knobs ?? {};
  const { runs, order } = effectRuns(c.order, (n) => fxOfModule(n.type, knobs(n.id)));
  assert.deepEqual([...runs], [['fx-rotate', { fx: ['fx-echo', 'fx-pulse', 'fx-rotate'], trails: 'fx-echo-feedback' }]]);
  assert.deepEqual(order.find((n) => n.id === 'fx-rotate')!.inputs.map((f) => [f.port, f.from.module]), [['picture', 'layers'], ['trails', 'fx-echo-feedback']]);
  // Without shapes an Echo is in its visible loop too (no hidden trail): one pass, trailing its Feedback.
  const alone = toV2({ ...settings(), fx3: 1, fx3Amount: 0.8 });
  const plain = compile(alone, types);
  assert.ok(plain.order.some((n) => n.id === 'fx-echo-feedback') && plain.order.some((n) => n.id === 'fx-echo'));
  const aloneKnobs = (id: string) => alone.modules.find((m) => m.id === id)?.knobs ?? {};
  assert.deepEqual([...effectRuns(plain.order, (n) => fxOfModule(n.type, aloneKnobs(n.id))).runs], [['fx-echo-mix', { fx: ['fx-echo'], trails: 'fx-echo-feedback' }]]);
  // The Echo observed: its own pass, the Mix drawn as a Mix.
  assert.deepEqual([...effectRuns(plain.order, (n) => fxOfModule(n.type, aloneKnobs(n.id)), 'fx-echo').runs], [['fx-echo', { fx: ['fx-echo'], trails: null }]]);
});

test('a slot becomes its effect’s module: its knobs by their own names, its cables with it; none on no effect or at 0', () => {
  const s = { ...settings(), fx1: 3, fx1Amount: 1, fx1K1: 0.2, fx1K2: 0.9, fx2: 0, fx2Amount: 1, fx3: 1, fx3Amount: 0, fxLoop: 1, fxLoopAmount: 0.6, fxLoopK1: 0.3 };
  s.routes = { ...s.routes, [routeKey('loudness', null, 'fx1Amount')]: 0.5 };
  const v = toV2(s);
  const m = Object.fromEntries(v.modules.map((x) => [x.id, x]));
  for (const id of ['fx1', 'fx2', 'fx3', 'loop-effect']) assert.ok(!m[id], id);
  assert.deepEqual(m['fx-rotate'], { id: 'fx-rotate', type: 'fx-rotate', knobs: { amount: 1, speed: 0.2, way: 0.9 } });
  assert.ok(!v.modules.some((x) => x.type === 'fx-pulse' || x.type === 'fx-echo'), 'none on no effect, none at 0');
  assert.deepEqual(m['loop-kaleid'].knobs, { amount: 0.6, sides: 0.3, spin: 0.5 });
  assert.ok(v.cables.some((c) => c.to.module === 'fx-rotate' && c.to.port.startsWith('amount')), 'the route into Amount on the module');
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(c.order.find((n) => n.id === 'dive')!.inputs.map((f) => [f.from.module, f.type]), [['loop-kaleid', 'light']], 'the loop’s effect in the flame’s own loop, on its light');
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
  assert.equal(cs.to, 'loop-mix', 'no effect at nothing: the loop gives on');
  for (const id of ['shapes-source', 'shapes-kaleido', 'window-source', 'layers', 'example-warp', 'example-keep', 'loop-mix', 'loop-feedback']) assert.ok(cs.ids.has(id), id);
  assert.ok(!cs.ids.has('show/saturate'));
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
  assert.deepEqual([...loops.entries()], [['loop-mix', { feedback: 'loop-feedback', chain: [{ id: 'example-warp', type: 'warp' }, { id: 'example-keep', type: 'keep' }], now: 'in2' }]]);
  // Not lightened: drawn module by module.
  assert.equal(loopsOf(c.order.filter((n) => ids.has(n.id)), (id) => (id === 'loop-mix' ? { ...knobs(id), mode2: 0 } : knobs(id))).size, 0);
  // One of its Warps or Keeps observed: a barrier, drawn module by module while it is.
  assert.equal(loopsOf(c.order.filter((n) => ids.has(n.id)), knobs, 'example-keep').size, 0);
  assert.equal(loopsOf(c.order.filter((n) => ids.has(n.id)), knobs, 'loop-mix').size, 1);
});

test('a device’s loop (its Feedback on the Mix’s second input) is one pass too, while both inputs are at full', async () => {
  const { composeOf } = await import('../src/lib/visuals/picture-graph.ts');
  const { loopsOf } = await import('../src/lib/visuals/nodes/walker.ts');
  const { DEVICES } = await import('../src/lib/visuals/devices.ts');
  const { flatten } = await import('../src/lib/visuals/v2.ts');
  const v = toV2(settings());
  v.modules.push(JSON.parse(JSON.stringify(DEVICES.find((d) => d.id === 'feedback-tunnel')!)));
  const cab = v.cables.find((x) => x.from.module === 'layers' && x.from.port === 'picture')!;
  const to = cab.to;
  cab.to = { module: 'feedback-tunnel', port: 'picture' };
  v.cables.push({ id: 'x-ft', from: { module: 'feedback-tunnel', port: 'picture' }, to, value: 1 } as never);
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  const ids = composeOf(c)!.ids;
  const flat = flatten(v);
  const knobs = (id: string) => flat.modules.find((m) => m.id === id)?.knobs ?? {};
  const order = c.order.filter((n) => ids.has(n.id));
  assert.deepEqual([...loopsOf(order, knobs).values()], [{ feedback: 'feedback-tunnel/feedback', chain: [{ id: 'feedback-tunnel/warp', type: 'warp' }, { id: 'feedback-tunnel/keep', type: 'keep' }], now: 'in1' }]);
  assert.equal(loopsOf(order, (id) => (id === 'feedback-tunnel/mix' ? { ...knobs(id), amount1: 0.5 } : knobs(id))).size, 0, 'the first at less than full: module by module');
});

test('every chain and example converted is drawn whole: no errors, its stages', async () => {
  const { chainRoutes, CHAINS, EXAMPLES } = await import('./harness/oracle/chains.ts');
  for (const c0 of [...CHAINS, ...EXAMPLES]) {
    const v = toV2({ ...settings(), ...(c0.settings ?? {}), routes: chainRoutes(c0, 6), extra: JSON.parse(JSON.stringify(c0.extra)) } as never);
    const c = compile(v, types);
    assert.ok(!c.errors.length && stagesOf(c), `${c0.id}: ${c.errors.join(' · ')}`);
  }
});
