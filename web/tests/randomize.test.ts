// Randomize (engine/randomize.ts, decision 0057 §4d–4e): on today's settings as a v2 patch, a track's seed gives the
// values it drew before 0057 (presetDraws), pins kept; over everything, every knob within its range and the
// structure as it is; a module's draw its own.
import { test } from 'node:test';
import assert from 'node:assert/strict';
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;

const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes, SETTING_KNOB } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes } = await import('../src/lib/visuals/ports.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { drawing, seedOf, PRESET_KEYS } = await import('../src/lib/visuals/flame.ts');
const { randomize, SEEDED_BEFORE, EVERYTHING } = await import('../src/lib/visuals/engine/randomize.ts');
const { EFFECT_TYPES } = await import('../src/lib/visuals/effect-modules.ts');
type ModuleType = import('../src/lib/visuals/patch.ts').ModuleType;
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

const chain = cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS);
const types: Record<string, ModuleType> = Object.fromEntries(
  [...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...EFFECT_TYPES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]),
);
function run<T>(g: Generator<void, T>): T {
  for (;;) {
    const r = g.next();
    if (r.done) return r.value;
  }
}
const KEYS = ['subsonic/tr-10329', 'subsonic/tr-8934', 'subsonic/tr-21896', 'subsonic/tr-6487', 'cratarr'];

test("a track's seed gives what it drew before 0057 (presetDraws), value for value", () => {
  const p = toV2(structuredClone(DEFAULTS));
  for (const key of KEYS) {
    const seed = seedOf(key);
    const drawn = run(drawing(seed)).preset as Record<string, number>;
    const mine = run(randomize(p, types, { seed, scope: SEEDED_BEFORE }));
    let n = 0;
    for (const k of PRESET_KEYS) {
      const [m, knob] = SETTING_KNOB[k];
      if (!p.modules.some((x) => x.id === m)) continue;
      assert.equal(mine[m]?.[knob], drawn[k], `${key}: ${m}.${knob}`);
      n++;
    }
    assert.ok(n >= 17, `${key}: ${n} of the chain's draw`);
    // Nothing past the chain's draw.
    const typeOf = new Map(p.modules.map((x) => [x.id, x.type]));
    for (const [m, knobs] of Object.entries(mine)) for (const knob of Object.keys(knobs)) assert.ok(SEEDED_BEFORE({ id: m, type: typeOf.get(m) ?? '', knobs: {} }, { id: knob } as never), `${m}.${knob}`);
  }
});

test("pins are kept, and the chain's draw drawn around them as before (an effect module pinned whole keeps its effect)", () => {
  const s = structuredClone(DEFAULTS);
  // Effect 1 on Rotate (the kaleidoscope's third), its module pinned whole.
  const v = toV2(s);
  const p: PatchV2 = { ...v, modules: [...v.modules, { id: 'fx-rotate', type: 'fx-rotate', knobs: { amount: 0.6 } }], pins: ['flame.hue', 'module:fx-rotate'] };
  const flame = p.modules.find((m) => m.id === 'flame')!;
  const seed = seedOf('subsonic/tr-10329');
  const drawn = run(drawing(seed, { hue: flame.knobs.hue, fx1: 3, fx1Amount: 0.6, fx1K1: 0.5, fx1K2: 0.5 })).preset as Record<string, number>;
  const mine = run(randomize(p, types, { seed, scope: EVERYTHING }));
  assert.equal(mine.flame.hue, undefined);
  assert.equal(mine['fx-rotate'], undefined);
  assert.equal(mine.flame.form, drawn.form);
  assert.equal(mine.flame.twist, drawn.twist);
});

test("over everything: the chain's draw as before, every other knob within its range (steps whole), the same again, nothing but knobs", () => {
  const p = toV2(structuredClone(DEFAULTS));
  const before = JSON.stringify(p);
  const seed = seedOf('subsonic/tr-8934');
  const narrow = run(randomize(p, types, { seed, scope: SEEDED_BEFORE }));
  const all = run(randomize(p, types, { seed }));
  assert.deepEqual(run(randomize(p, types, { seed })), all);
  assert.equal(JSON.stringify(p), before);
  for (const [m, knobs] of Object.entries(narrow)) for (const [k, v] of Object.entries(knobs)) assert.equal(all[m][k], v);
  let others = 0;
  for (const [id, knobs] of Object.entries(all)) {
    const m = p.modules.find((x) => x.id === id) ?? p.modules.flatMap((x) => ('patch' in x ? (x as { patch: PatchV2 }).patch.modules.map((y) => ({ ...y, id: `${x.id}/${y.id}` })) : [])).find((x) => x.id === id);
    assert.ok(m, `${id} is a module of the patch`);
    for (const [k, v] of Object.entries(knobs)) {
      const knob = types[m!.type]?.knobs.find((x) => x.id === k.replace(/\.\d+$/, ''));
      if (!knob) continue;
      assert.ok(v >= knob.min && v <= knob.max, `${id}.${k} ${v} in ${knob.min}..${knob.max}`);
      if (knob.kind === 'step') assert.ok(Number.isInteger(v), `${id}.${k} whole`);
      if (!(id in narrow && k in narrow[id])) others++;
    }
  }
  assert.ok(others > 20, `${others} knobs past the chain's draw`);
});

test("a module's draw is its own: one more module, or a narrower scope, changes no other's; reach 0 leaves them", () => {
  const p = toV2(structuredClone(DEFAULTS));
  const seed = seedOf('cratarr');
  const all = run(randomize(p, types, { seed }));
  const more: PatchV2 = { ...p, modules: [{ id: 'extra-lfo', type: 'lfo', knobs: {} }, ...p.modules] };
  const withMore = run(randomize(more, types, { seed }));
  for (const [m, knobs] of Object.entries(all)) assert.deepEqual(withMore[m], knobs, m);
  assert.ok(withMore['extra-lfo']);
  const onlyFluid = run(randomize(p, types, { seed, scope: (m) => m.id === 'fluid' }));
  assert.deepEqual(Object.keys(onlyFluid), ['fluid']);
  assert.deepEqual(onlyFluid.fluid, all.fluid);
  const none = run(randomize(p, types, { seed, reach: 0 }));
  for (const [id, knobs] of Object.entries(none)) {
    if (id === 'flame' || /^fx\d$|^loop-effect$/.test(id)) continue;
    const m = p.modules.find((x) => x.id === id);
    if (!m) continue;
    for (const [k, v] of Object.entries(knobs)) if (!SEEDED_BEFORE(m, { id: k } as never)) assert.equal(v, m.knobs[k] ?? types[m.type].knobs.find((x) => x.id === k.replace(/\.\d+$/, ''))!.default, `${id}.${k}`);
  }
});

test('pinned whole: a module (module:<id>), a category (category:<name>), a polyphonic knob all its channels; the same seed the same values whatever was rolled before', async () => {
  const { categoryOf } = await import('../src/lib/visuals/rack.ts');
  const p = toV2(structuredClone(DEFAULTS));
  const seed = seedOf('subsonic/tr-21896');
  const all = run(randomize(p, types, { seed }));
  const mod = p.modules.find((m) => m.id in all && m.id !== 'flame')!;
  const noMod = run(randomize({ ...p, pins: [`module:${mod.id}`] }, types, { seed }));
  assert.equal(noMod[mod.id], undefined);
  const cat = categoryOf(types[mod.type]);
  const noCat = run(randomize({ ...p, pins: [`category:${cat}`] }, types, { seed }));
  for (const m of p.modules) if (types[m.type] && categoryOf(types[m.type]) === cat) assert.equal(noCat[m.id], undefined, m.id);
  const poly = p.modules.find((m) => types[m.type]?.knobs.some((k) => k.channels && all[m.id]?.[`${k.id}.0`] !== undefined));
  if (poly) {
    const knob = types[poly.type].knobs.find((k) => k.channels && all[poly.id]?.[`${k.id}.0`] !== undefined)!;
    const held = run(randomize({ ...p, pins: [`${poly.id}.${knob.id}`] }, types, { seed }));
    for (let c = 0; c < knob.channels!; c++) assert.equal(held[poly.id]?.[`${knob.id}.${c}`], undefined);
    const one = run(randomize({ ...p, pins: [`${poly.id}.${knob.id}.1`] }, types, { seed }));
    assert.equal(one[poly.id]?.[`${knob.id}.1`], undefined);
    assert.equal(one[poly.id]?.[`${knob.id}.0`], all[poly.id][`${knob.id}.0`]);
  }
  // Rolled in between (a new seed applied), then the track's own seed again: what it gave first.
  const rolled = run(randomize(p, types, { seed: 12345 }));
  const after: PatchV2 = { ...p, modules: p.modules.map((m) => (rolled[m.id] ? { ...m, knobs: { ...m.knobs, ...rolled[m.id] } } : m)) };
  assert.deepEqual(run(randomize(after, types, { seed })), all);
});

test("Dice: rolls as its trigger rises, seeded by its seed input (Music's: the track's own), its scope from its knob", async () => {
  const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
  const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
  const { TRACKS, track } = await import('./harness/recorded.ts');
  const { FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
  const t = track(TRACKS[0]);
  const st = newEngine();
  const music = newMusicState();
  const p: PatchV2 = {
    v: 2,
    modules: [{ id: 'music', type: 'music', knobs: {} }, { id: 'look', type: 'dice', knobs: { scope: 0 } }],
    cables: [
      { id: 'a', from: { module: 'music', port: 'newtrack' }, to: { module: 'look', port: 'trigger' }, value: 1, depth: null },
      { id: 'b', from: { module: 'music', port: 'seed' }, to: { module: 'look', port: 'seed' }, value: 1, depth: null },
    ],
    macros: [],
    pins: [],
  } as PatchV2;
  const rolls: { k: number; seed: number; scope: string; done: number }[] = [];
  for (let k = 0; k < 90; k++) {
    const at = 20 + k / 30;
    const key = k < 40 ? 'subsonic/tr-10329' : 'subsonic/tr-8934';
    const m = { ...musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting }), key };
    const f = stepEngine(st, p, {}, m, () => 0, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
    for (const r of f.dice) rolls.push({ k, seed: r.seed, scope: r.scope, done: f.outputs['look.done'] });
  }
  assert.deepEqual(rolls, [
    { k: 0, seed: seedOf('subsonic/tr-10329'), scope: 'everything', done: 1 },
    { k: 40, seed: seedOf('subsonic/tr-8934'), scope: 'everything', done: 1 },
  ]);
});

test('the Cables roll: new amounts for the cables into knobs, none of the plumbing, pins kept, the same again, a curve its shape', async () => {
  const { randomizeCables } = await import('../src/lib/visuals/engine/randomize.ts');
  const { flatten } = await import('../src/lib/visuals/v2.ts');
  const p = flatten(toV2(structuredClone(DEFAULTS)));
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const rolled = randomizeCables(p, types, { seed: 4242 });
  const { followsOf } = await import('../src/lib/visuals/engine/follow.ts');
  const follows = followsOf(p);
  const ids = Object.keys(rolled);
  assert.ok(ids.length > 20, `${ids.length} cables rolled`);
  const PLUMBING = ['vca', 'merge', 'gate', 'shaper', 'ducker', 'meter', 'screen', 'dice'];
  for (const id of ids) {
    const c = p.cables.find((x) => x.id === id)!;
    // A cable through the Loudness Follow is the cable from the signal behind it.
    const from = follows.get(c.from.module)?.from.module ?? c.from.module;
    assert.ok(!PLUMBING.includes(typeOf.get(from)!) && (c.to.port === 'in' || !PLUMBING.includes(typeOf.get(c.to.module)!)), `${id} is no plumbing`);
    const v = rolled[id];
    const h = typeof v === 'number' ? v : v.h;
    assert.ok(h >= -1 && h <= 1, `${id}: ${h}`);
    if (typeof c.value !== 'number' && typeof v !== 'number' && c.value.h !== 0) assert.ok(Math.abs(v.h1[1] / v.h - c.value.h1[1] / c.value.h) < 1e-9, `${id} keeps its shape`);
  }
  assert.deepEqual(randomizeCables(p, types, { seed: 4242 }), rolled);
  const pinned = randomizeCables({ ...p, pins: [`cable:${ids[0]}`] }, types, { seed: 4242 });
  assert.equal(pinned[ids[0]], undefined);
  assert.equal(pinned[ids[1]], rolled[ids[1]]);
  const none = randomizeCables(p, types, { seed: 4242, reach: 0 });
  for (const id of Object.keys(none)) {
    const c = p.cables.find((x) => x.id === id)!;
    assert.equal(typeof none[id] === 'number' ? none[id] : (none[id] as { h: number }).h, typeof c.value === 'number' ? c.value : c.value.h);
  }
});

test("the rack as it is: an effect module keeps its effect, the draw drawn around it, its Amount and controls drawn", () => {
  const v = toV2(structuredClone(DEFAULTS));
  // Effect 1 on Rotate (the kaleidoscope's third: its Speed and Way).
  const p: PatchV2 = { ...v, modules: [...v.modules, { id: 'fx-rotate', type: 'fx-rotate', knobs: { amount: 0.6, speed: 0.5, way: 0.5 } }] };
  for (const key of KEYS) {
    const seed = seedOf(key);
    const drawn = run(drawing(seed, { fx1: 3 })).preset as Record<string, number>;
    const mine = run(randomize(p, types, { seed, scope: EVERYTHING }));
    assert.deepEqual(mine['fx-rotate'], { amount: drawn.fx1Amount, speed: drawn.fx1K1, way: drawn.fx1K2 }, key);
    for (const id of Object.keys(mine)) assert.ok(p.modules.some((m) => m.id === id) || id.includes('/'), `${key}: ${id} is the rack's`);
  }
});
