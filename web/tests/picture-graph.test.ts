// The picture graph (picture-graph.ts, decision 0057 step 3): what reaches the Screen and in what order, the lazy
// rules, the pool and a Feedback's adopted target: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { compile, plan, walk, newWalkState, outKey, Pool } = await import('../src/lib/visuals/picture-graph.ts');
const { FEEDBACK, MIX, SCREEN } = await import('../src/lib/visuals/ports.ts');
type Ports = import('../src/lib/visuals/ports.ts').Ports;
type CableV2 = import('../src/lib/visuals/ports.ts').CableV2;
type PictureNode = import('../src/lib/visuals/picture-graph.ts').PictureNode;

const light = (id: string) => ({ id, label: id, type: 'light' as const });
const pic = (id: string) => ({ id, label: id, type: 'picture' as const });
const poly = (id: string) => ({ ...pic(id), polymorphic: true });
const lit = (type: string, ports: Ports) => ({ type, label: type, place: 'module' as const, knobs: [], ports });
const TYPES = new Map<string, Parameters<typeof compile>[1] extends (t: string) => infer R ? NonNullable<R> : never>([
  ['shapes', { type: 'shapes', label: 'Shapes', place: 'source', knobs: [] }],
  ['kaleid', { type: 'kaleid', label: 'Kaleid', place: 'module', knobs: [] }],
  ['mix', MIX],
  ['feedback', FEEDBACK],
  ['screen', SCREEN],
  ['flame', lit('flame', { in: [light('light')], out: [light('light')] })],
  ['tunnel', lit('tunnel', { in: [light('light')], out: [light('light')] })],
  ['tonemap', lit('tonemap', { in: [light('light')], out: [pic('picture')] })],
  ['bend', lit('bend', { in: [poly('picture')], out: [poly('picture')] })],
]);
// The flame's loop needs a Feedback of light: here one with polymorphic ports.
const LIGHT_TYPES = new Map(TYPES).set('feedback', lit('feedback', { in: [poly('picture')], out: [poly('picture')] }));
const typeOf = (t: string) => TYPES.get(t);

let n = 0;
const cable = (from: string, to: string, fromPort = 'picture', toPort = 'picture'): CableV2 => ({
  id: `c${n++}`,
  from: { module: from, port: fromPort },
  to: { module: to, port: toPort },
  value: 1,
  mode: 'level',
});
const patch = (modules: [string, string][], cables: CableV2[]) => ({ v: 2 as const, modules: modules.map(([id, type]) => ({ id, type, knobs: {} })), cables, macros: [], pins: [] });
const ids = (ns: PictureNode[]) => ns.map((x) => x.id);

test('what reaches the Screen, in order; anything else is not drawn', () => {
  const c = compile(patch([['screen', 'screen'], ['k', 'kaleid'], ['s', 'shapes'], ['lone', 'shapes']], [cable('s', 'k'), cable('k', 'screen')]), typeOf);
  assert.deepEqual(ids(c.order), ['s', 'k', 'screen']);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(compile(patch([['s', 'shapes']], []), typeOf).errors, ['Nothing reaches a Screen: there is none.']);
});

test('an input takes one picture, of its own type', () => {
  const c = compile(patch([['a', 'shapes'], ['b', 'shapes'], ['screen', 'screen']], [cable('a', 'screen'), cable('b', 'screen')]), typeOf);
  assert.deepEqual(ids(c.order), ['a', 'screen']);
  assert.equal(c.errors.length, 1);
  const t = compile(patch([['f', 'flame'], ['screen', 'screen']], [cable('f', 'screen', 'light')]), typeOf);
  assert.match(t.errors[0], /takes a picture, not f's light/);
});

test('a cycle must pass through a Feedback; a Feedback comes after its input and after what reads it', () => {
  const bad = compile(patch([['s', 'shapes'], ['a', 'kaleid'], ['b', 'kaleid'], ['m', 'mix'], ['screen', 'screen']], [cable('s', 'm', 'picture', 'in1'), cable('a', 'b'), cable('b', 'a'), cable('a', 'm', 'picture', 'in2'), cable('m', 'screen')]), typeOf);
  assert.match(bad.errors[0], /through a Feedback/);
  assert.deepEqual(ids(bad.order), ['s', 'm', 'screen'], 'the cycle left out, the rest drawn');
  // Today's picture loop: Feedback → a move → Mix with the new picture → Feedback and the Screen.
  const c = compile(
    patch(
      [['fb', 'feedback'], ['screen', 'screen'], ['m', 'mix'], ['warp', 'kaleid'], ['s', 'shapes']],
      [cable('s', 'm', 'picture', 'in1'), cable('fb', 'warp'), cable('warp', 'm', 'picture', 'in2'), cable('m', 'fb'), cable('m', 'screen')],
    ),
    typeOf,
  );
  assert.deepEqual(c.errors, []);
  const at = (id: string) => ids(c.order).indexOf(id);
  assert.ok(at('warp') < at('fb') && at('m') < at('fb'), 'after its reader and its input');
  assert.ok(at('s') < at('m') && at('warp') < at('m') && at('m') < at('screen'));
  assert.deepEqual(c.order.find((x) => x.id === 'm')!.inputs.map((f) => f.port), ['in1', 'in2']);
});

test('light: one flame’s own loop, Feedback → carry → points, then on through Bend to Tone map; nothing else takes light', () => {
  const lightOf = (t: string) => LIGHT_TYPES.get(t);
  const loop = [cable('fb', 't', 'picture', 'light'), cable('t', 'flame', 'light', 'light'), cable('flame', 'fb', 'light', 'picture'), cable('flame', 'bend', 'light', 'picture'), cable('bend', 'show', 'picture', 'light'), cable('show', 'screen')];
  const mods: [string, string][] = [['fb', 'feedback'], ['t', 'tunnel'], ['flame', 'flame'], ['bend', 'bend'], ['show', 'tonemap'], ['screen', 'screen']];
  const c = compile(patch(mods, loop), lightOf);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(ids(c.order), ['t', 'flame', 'fb', 'bend', 'show', 'screen']);
  assert.deepEqual(c.order.find((x) => x.id === 'bend')!.inputs.map((f) => f.type), ['light'], 'Bend on light, its output light');
  // A polymorphic module on a picture gives a picture: Tone map will not take it.
  const onPicture = compile(patch([['s', 'shapes'], ['bend', 'bend'], ['show', 'tonemap'], ['screen', 'screen']], [cable('s', 'bend'), cable('bend', 'show', 'picture', 'light'), cable('s', 'screen')]), lightOf);
  assert.match(onPicture.errors[0], /show.light takes a light, not bend's picture/);
  // Without its Feedback the flame's light does not come round: refused, with what it fed.
  const open = compile(patch(mods.filter(([id]) => id !== 'fb'), loop.filter((x) => x.from.module !== 'fb' && x.to.module !== 'fb')), lightOf);
  assert.match(open.errors[0], /must come round through a Feedback/);
  assert.deepEqual(ids(open.order), ['screen']);
  const short = loop.filter((x) => x.to.module !== 'bend' && x.from.module !== 'bend').concat(cable('flame', 'show', 'light', 'light'));
  // A second Show of the same light is fine; a carry fed from the flame's out (outside its loop) is not.
  const two = compile(
    patch(
      [...mods, ['show2', 'tonemap'], ['show3', 'tonemap'], ['t2', 'tunnel'], ['m', 'mix']],
      [
        ...short.slice(0, 3),
        cable('flame', 'show', 'light', 'light'),
        cable('flame', 'show2', 'light', 'light'),
        cable('flame', 't2', 'light', 'light'),
        cable('t2', 'show3', 'light', 'light'),
        cable('show', 'm', 'picture', 'in1'),
        cable('show2', 'm', 'picture', 'in2'),
        cable('show3', 'm', 'picture', 'in3'),
        cable('m', 'screen'),
      ],
    ),
    lightOf,
  );
  assert.ok(ids(two.order).includes('show2'));
  assert.ok(!ids(two.order).includes('t2') && !ids(two.order).includes('show3'));
  assert.ok(two.errors.some((e) => /t2 cannot take light/.test(e)));
  assert.deepEqual(two.order.find((x) => x.id === 'm')!.inputs.map((f) => f.port), ['in1', 'in2'], 'what read the refused goes on without it');
});

// A Mix of two shapes over a picture loop, for the lazy rules.
const lazy = () =>
  compile(
    patch(
      [['a', 'shapes'], ['b', 'shapes'], ['k', 'kaleid'], ['k2', 'kaleid'], ['m', 'mix'], ['screen', 'screen']],
      [cable('a', 'k'), cable('k', 'k2'), cable('k2', 'm', 'picture', 'in1'), cable('b', 'm', 'picture', 'in2'), cable('m', 'screen')],
    ),
    typeOf,
  );
const knobs = (over: Record<string, Record<string, number>> = {}) => (id: string) => ({ mode1: 2, amount1: 1, mode2: 1, amount2: 1, ...(over[id] ?? {}) });

test('lazy: a Mix input at amount 0 is cut with what feeds only it, unless its amount is coming back', () => {
  const c = lazy();
  assert.deepEqual(ids(plan(c, { knobs: knobs() }).live), ['a', 'b', 'k', 'k2', 'm', 'screen']);
  const cut = plan(c, { knobs: knobs({ m: { amount1: 0 } }) });
  assert.ok(!ids(cut.live).some((id) => ['a', 'k', 'k2'].includes(id)));
  // The one left is screened, not over: drawn.
  assert.ok(ids(cut.live).includes('m'));
  const coming = plan(c, { knobs: knobs({ m: { amount1: 0 } }), target: (id, k) => (id === 'm' && k === 'amount1' ? 1 : 0) });
  assert.ok(ids(coming.live).includes('a'));
});

test('lazy: a module at rest is its input; one coming back keeps running; a Mix of one picture over at 1 is it', () => {
  const c = lazy();
  const rest = plan(c, { knobs: knobs({ m: { amount2: 0 } }), idle: (id) => id === 'k' || id === 'k2' });
  assert.deepEqual(ids(rest.live), ['a', 'screen'], 'both at rest and the Mix of one picture over: the Screen reads the source');
  assert.deepEqual(rest.reads.get('screen')!.get('picture'), { module: 'a', port: 'picture' });
  assert.equal(rest.readers.get(outKey({ module: 'a', port: 'picture' })), 1);
  const back = plan(c, { knobs: knobs({ m: { amount2: 0 } }), idle: (id) => id === 'k', returning: (id) => id === 'k' });
  assert.deepEqual(ids(back.live), ['a', 'k', 'k2', 'screen']);
  const dim = plan(c, { knobs: knobs({ m: { amount1: 0.5, amount2: 0 } }) });
  assert.ok(ids(dim.live).includes('m'), 'less than whole: drawn (dimmed)');
});

test('readers: an output read twice counts twice', () => {
  const c = compile(patch([['a', 'shapes'], ['k', 'kaleid'], ['m', 'mix'], ['screen', 'screen']], [cable('a', 'k'), cable('a', 'm', 'picture', 'in1'), cable('k', 'm', 'picture', 'in2'), cable('m', 'screen')]), typeOf);
  const p = plan(c, { knobs: knobs() });
  assert.equal(p.readers.get('a.picture'), 2);
  assert.equal(p.readers.get('m.picture'), 1);
});

// Fake targets: each holds what was last drawn into it, so a reuse too soon shows.
type T = { id: number; holds: string | null };
function harness(clock = { t: 0 }) {
  let made = 0;
  const released: T[] = [];
  const pool = new Pool<T>(() => ({ id: made++, holds: null }), (t) => released.push(t), () => clock.t);
  const copies: [T, T][] = [];
  const own = new Map<string, T>();
  const io = (owners: Set<string> = new Set()) => ({
    spec: (n: PictureNode) => (n.type === 'screen' ? null : owners.has(n.id) ? ('own' as const) : { w: 4, h: 4, format: 'rgba8' as const }),
    draw: (n: PictureNode, inputs: Map<string, T | null>, out: T | null) => {
      for (const [port, t] of inputs) {
        const f = n.inputs.find((x) => x.port === port)!;
        if (t && f.from.module !== 'fb') assert.equal(t.holds, f.from.module, `${n.id} reads ${port}: ${t.holds} is there, not ${f.from.module}`);
      }
      if (out) {
        assert.ok(![...inputs.values()].includes(out), `${n.id} draws into its own input`);
        out.holds = n.id;
      }
      if (owners.has(n.id)) {
        if (!own.has(n.id)) own.set(n.id, { id: -1, holds: n.id });
        return own.get(n.id)!;
      }
    },
    copy: (from: T, to: T) => {
      copies.push([from, to]);
      to.holds = from.holds;
    },
  });
  return { pool, io, copies, released, clock };
}

test('the pool: a line of modules needs two targets; one read twice is kept till its last reader', () => {
  const line = compile(patch([['a', 'shapes'], ['k1', 'kaleid'], ['k2', 'kaleid'], ['k3', 'kaleid'], ['screen', 'screen']], [cable('a', 'k1'), cable('k1', 'k2'), cable('k2', 'k3'), cable('k3', 'screen')]), typeOf);
  const h = harness();
  walk(plan(line, { knobs: knobs() }), h.pool, h.io(), newWalkState<T>());
  assert.equal(h.pool.size, 2);
  const fan = compile(
    patch([['a', 'shapes'], ['k1', 'kaleid'], ['k2', 'kaleid'], ['m', 'mix'], ['screen', 'screen']], [cable('a', 'k1'), cable('k1', 'k2'), cable('a', 'm', 'picture', 'in2'), cable('k2', 'm', 'picture', 'in1'), cable('m', 'screen')]),
    typeOf,
  );
  const f = harness();
  walk(plan(fan, { knobs: knobs() }), f.pool, f.io(), newWalkState<T>());
  assert.equal(f.pool.size, 3, "a's kept while k1 and k2 draw");
});

test('the observed module’s target is pinned: out of the pool until the caller has read it, then given back', () => {
  const line = compile(patch([['a', 'shapes'], ['k1', 'kaleid'], ['k2', 'kaleid'], ['k3', 'kaleid'], ['screen', 'screen']], [cable('a', 'k1'), cable('k1', 'k2'), cable('k2', 'k3'), cable('k3', 'screen')]), typeOf);
  const h = harness();
  const state = newWalkState<T>();
  const p = plan(line, { knobs: knobs() });
  walk(p, h.pool, h.io(), state, new Map(), 'k1');
  assert.equal(state.pinned?.holds, 'k1', "k1's picture, not drawn over by k3");
  assert.equal(h.pool.size, 3, 'one target more while pinned');
  h.pool.give(state.pinned!);
  walk(p, h.pool, h.io(), state);
  assert.equal(state.pinned, null, 'nothing pinned: nothing kept');
  assert.equal(h.pool.size, 3, 'no target leaks');
  // The walk's last picture is read after it anyway: not the caller's to give back.
  walk(p, h.pool, h.io(), state, new Map(), 'screen');
  assert.equal(state.pinned, null);
});

test('a Feedback adopts its input’s target (no copy), gives back the last; copies one that is not the pool’s', () => {
  const loop = compile(
    patch(
      [['fb', 'feedback'], ['screen', 'screen'], ['m', 'mix'], ['warp', 'kaleid'], ['s', 'shapes']],
      [cable('s', 'm', 'picture', 'in1'), cable('fb', 'warp'), cable('warp', 'm', 'picture', 'in2'), cable('m', 'fb'), cable('m', 'screen')],
    ),
    typeOf,
  );
  const h = harness();
  const state = newWalkState<T>();
  const p = plan(loop, { knobs: knobs() });
  const sizes: number[] = [];
  let last: T | null = null;
  for (let i = 0; i < 5; i++) {
    walk(p, h.pool, h.io(), state);
    const held = state.held.get('fb')!.t;
    assert.equal(held.holds, 'm', "the Mix's picture, kept");
    assert.notEqual(held, last, 'a new one each frame, the old given back');
    last = held;
    sizes.push(h.pool.size);
  }
  assert.equal(h.copies.length, 0);
  assert.equal(new Set(sizes.slice(1)).size, 1, 'no target leaks');
  // A Feedback of what keeps its own picture (the flame's light): kept as it is, never copied.
  const own = compile(
    patch([['fb', 'feedback'], ['s', 'shapes'], ['k', 'kaleid'], ['m', 'mix'], ['screen', 'screen']], [cable('s', 'fb'), cable('fb', 'k'), cable('s', 'm', 'picture', 'in1'), cable('k', 'm', 'picture', 'in2'), cable('m', 'screen')]),
    typeOf,
  );
  const o = harness();
  const os = newWalkState<T>();
  const io = o.io(new Set(['s', 'fb']));
  walk(plan(own, { knobs: knobs() }), o.pool, io, os);
  assert.equal(os.held.get('fb')!.t.holds, 's');
  assert.equal(os.held.get('fb')!.pooled, false);
  assert.equal(o.copies.length, 0);
  // One whose Feedback has a target of its own: copied into it.
  const c = harness();
  const cs = newWalkState<T>();
  walk(plan(own, { knobs: knobs() }), c.pool, c.io(new Set(['s'])), cs);
  assert.equal(c.copies.length, 1);
  assert.equal(cs.held.get('fb')!.pooled, true);
});

test('the pool lets go of what was not taken for ten seconds, and of everything at a resize', () => {
  const h = harness();
  const a = h.pool.take({ w: 2, h: 2, format: 'rgba8' });
  const b = h.pool.take({ w: 2, h: 2, format: 'rgba16f' });
  h.pool.give(a);
  h.clock.t = 9_000;
  h.pool.give(b);
  h.clock.t = 10_500;
  h.pool.sweep();
  assert.deepEqual(h.released, [a]);
  assert.equal(h.pool.take({ w: 2, h: 2, format: 'rgba16f' }), b, 'by size and format');
  h.pool.clear();
  assert.equal(h.pool.size, 0);
});

test('devices are opened before compiling (v2.ts flatten), nested ones too', () => {
  const device = (id: string, inner: ReturnType<typeof patch>, inputs: [string, string, string][], outputs: [string, string, string][]) => ({
    id,
    type: 'device',
    label: id,
    knobs: {},
    patch: inner,
    inputs: inputs.map(([as, module, port]) => ({ as, module, port, label: as })),
    outputs: outputs.map(([as, module, port]) => ({ as, module, port, label: as })),
    macros: [],
  });
  const e = device('e', patch([['k', 'kaleid']], []), [['in', 'k', 'picture']], [['out', 'k', 'picture']]);
  const d = device('d', { ...patch([['k2', 'kaleid']], [cable('e', 'k2', 'out')]), modules: [e, { id: 'k2', type: 'kaleid', knobs: {} }] }, [['in', 'e', 'in']], [['out', 'k2', 'picture']]);
  const outer = { ...patch([['s', 'shapes'], ['screen', 'screen']], [cable('s', 'd', 'picture', 'in'), cable('d', 'screen', 'out')]), modules: [{ id: 's', type: 'shapes', knobs: {} }, d, { id: 'screen', type: 'screen', knobs: {} }] };
  const c = compile(outer, typeOf);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(ids(c.order), ['s', 'd/e/k', 'd/k2', 'screen']);
});

test('a patch without the flame: its compose step starts from nothing (no Show), and gives on what reaches the Screen', async () => {
  const { composeOf } = await import('../src/lib/visuals/picture-graph.ts');
  const c = compile(patch([['s', 'shapes'], ['k', 'kaleid'], ['screen', 'screen']], [cable('s', 'k'), cable('k', 'screen')]), typeOf);
  const cs = composeOf(c)!;
  assert.equal(cs.from, null);
  assert.equal(cs.to, 'k');
  assert.deepEqual([...cs.ids].sort(), ['k', 's']);
  assert.equal(composeOf(compile(patch([['screen', 'screen']], []), typeOf)), null, 'nothing reaches the Screen');
});

test('the light loop’s effect is the effect module on the light, in the carry stage (6b: found by its wiring, not its id)', async () => {
  (globalThis as Record<string, unknown>).$state ??= <T>(v: T) => v;
  await import('../src/lib/visuals/picture-wiring.ts');
  const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
  const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
  const { toV2, flatten } = await import('../src/lib/visuals/v2.ts');
  const { v2Types, stageOf } = await import('../src/lib/visuals/picture-graph.ts');
  const { fxOfModule } = await import('../src/lib/visuals/nodes/effect-runs.ts');
  const p = flatten(toV2({ ...DEFAULTS, fxLoop: 2, fxLoopAmount: 0.6 }));
  const c = compile(p, v2Types(cratarrTypes(RANGES, DEFAULTS)));
  const onLight = c.order.filter((n) => stageOf(n) === 'carry' && fxOfModule(n.type, {}));
  assert.equal(onLight.length, 1, JSON.stringify(c.order.map((n) => `${n.id}:${stageOf(n)}`)));
  assert.ok(onLight[0].inputs.some((f) => f.type === 'light'));
});
