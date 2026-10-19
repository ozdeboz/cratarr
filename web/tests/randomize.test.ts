// Randomize (engine/randomize.ts, decision 0057 §4d–4e): on today's settings as a v2 patch, a track's seed gives the
// look today's Another look drew for it (presetDraws), pins kept; over everything, every knob within its range and the
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
const { randomize, TODAYS_LOOK, EVERYTHING } = await import('../src/lib/visuals/engine/randomize.ts');
type ModuleType = import('../src/lib/visuals/patch.ts').ModuleType;
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

const chain = cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS);
const types: Record<string, ModuleType> = Object.fromEntries(
  [...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]),
);
const typeOf = (t: string) => types[t];
function run<T>(g: Generator<void, T>): T {
  for (;;) {
    const r = g.next();
    if (r.done) return r.value;
  }
}
const KEYS = ['subsonic/tr-10329', 'subsonic/tr-8934', 'subsonic/tr-21896', 'subsonic/tr-6487', 'cratarr'];

test("a track's seed over today's look gives what Another look drew for it, value for value", () => {
  const p = toV2(structuredClone(DEFAULTS));
  for (const key of KEYS) {
    const seed = seedOf(key);
    const drawn = run(drawing(seed)).preset as Record<string, number>;
    const mine = run(randomize(p, typeOf, { seed, scope: TODAYS_LOOK }));
    let n = 0;
    for (const k of PRESET_KEYS) {
      const [m, knob] = SETTING_KNOB[k];
      if (!p.modules.some((x) => x.id === m)) continue;
      assert.equal(mine[m]?.[knob], drawn[k], `${key}: ${m}.${knob}`);
      n++;
    }
    assert.ok(n >= 25, `${key}: ${n} of the look's keys`);
    // Nothing past today's look.
    for (const [m, knobs] of Object.entries(mine)) for (const knob of Object.keys(knobs)) assert.ok(TODAYS_LOOK({ id: m, type: '', knobs: {} }, { id: knob } as never), `${m}.${knob}`);
  }
});

test('pins are kept, and the look drawn around them as Another look drew it', () => {
  const s = structuredClone(DEFAULTS);
  const p: PatchV2 = { ...toV2(s), pins: ['flame.hue', 'fx1.effect'] };
  const flame = p.modules.find((m) => m.id === 'flame')!;
  const fx1 = p.modules.find((m) => m.id === 'fx1')!;
  const seed = seedOf('subsonic/tr-10329');
  const drawn = run(drawing(seed, { hue: flame.knobs.hue, fx1: fx1.knobs.effect })).preset as Record<string, number>;
  const mine = run(randomize(p, typeOf, { seed, scope: EVERYTHING }));
  assert.equal(mine.flame.hue, undefined);
  assert.equal(mine.fx1?.effect, undefined);
  assert.equal(mine.flame.form, drawn.form);
  assert.equal(mine.flame.twist, drawn.twist);
});

test('over everything: the look as before, every other knob within its range (steps whole), the same again, nothing but knobs', () => {
  const p = toV2(structuredClone(DEFAULTS));
  const before = JSON.stringify(p);
  const seed = seedOf('subsonic/tr-8934');
  const narrow = run(randomize(p, typeOf, { seed, scope: TODAYS_LOOK }));
  const all = run(randomize(p, typeOf, { seed }));
  assert.deepEqual(run(randomize(p, typeOf, { seed })), all);
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
  assert.ok(others > 20, `${others} knobs past the look`);
});

test("a module's draw is its own: one more module, or a narrower scope, changes no other's; reach 0 leaves them", () => {
  const p = toV2(structuredClone(DEFAULTS));
  const seed = seedOf('cratarr');
  const all = run(randomize(p, typeOf, { seed }));
  const more: PatchV2 = { ...p, modules: [{ id: 'extra-lfo', type: 'lfo', knobs: {} }, ...p.modules] };
  const withMore = run(randomize(more, typeOf, { seed }));
  for (const [m, knobs] of Object.entries(all)) assert.deepEqual(withMore[m], knobs, m);
  assert.ok(withMore['extra-lfo']);
  const onlyFluid = run(randomize(p, typeOf, { seed, scope: (m) => m.id === 'fluid' }));
  assert.deepEqual(Object.keys(onlyFluid), ['fluid']);
  assert.deepEqual(onlyFluid.fluid, all.fluid);
  const none = run(randomize(p, typeOf, { seed, reach: 0 }));
  for (const [id, knobs] of Object.entries(none)) {
    if (id === 'flame' || /^fx\d$|^loop-effect$/.test(id)) continue;
    const m = p.modules.find((x) => x.id === id);
    if (!m) continue;
    for (const [k, v] of Object.entries(knobs)) if (!TODAYS_LOOK(m, { id: k } as never)) assert.equal(v, m.knobs[k] ?? types[m.type].knobs.find((x) => x.id === k.replace(/\.\d+$/, ''))!.default, `${id}.${k}`);
  }
});
