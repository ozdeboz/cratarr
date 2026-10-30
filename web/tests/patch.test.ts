// The patch model's step 1 (decision 0056, patch.ts): today's settings as the Cratarr chain and back, losing nothing;
// every setting and every route column a knob; every knob's range and neutral sound: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

// settings.svelte.ts makes its state with Svelte's rune at load; outside Svelte it is the value itself.
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { ASPECT_KNOB, SETTING_KNOB, cratarrPatch, cratarrTypes, modulesOf, settingsOf } = await import('../src/lib/visuals/patch.ts');
const { ASPECTS, BAND_SIGNALS, TRACK_SIGNALS, routeKey } = await import('../src/lib/visuals/routes.ts');
type VisualSettings = typeof DEFAULTS;

const copy = (s: VisualSettings): VisualSettings => JSON.parse(JSON.stringify(s));

test('every setting and every route column has a knob', () => {
  for (const [k, v] of Object.entries(DEFAULTS)) {
    // The flame layer's share is that layer's blend amount, not a module's knob.
    if (typeof v === 'number' && k !== 'flameLayer') assert.ok(SETTING_KNOB[k], `setting ${k} has no knob`);
  }
  for (const a of ASPECTS) assert.ok(ASPECT_KNOB[a], `column ${a} has no knob`);
});

test('the defaults go to the Cratarr chain and back unchanged', () => {
  const s = copy(DEFAULTS);
  s.pins = ['detail', 'lighting:2', `route:${routeKey('level', 3, 'shape')}`, 'flash'];
  assert.deepEqual(settingsOf(cratarrPatch(s), DEFAULTS), s);
});

test('any settings go there and back unchanged: every column, curves, pins of all kinds', () => {
  // A small seeded generator, so a failure repeats.
  let seed = 7;
  const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  for (let round = 0; round < 20; round++) {
    const s = copy(DEFAULTS);
    for (const [k, v] of Object.entries(s)) {
      if (typeof v !== 'number') continue;
      const [lo, hi] = RANGES[k as keyof typeof RANGES];
      (s as Record<string, unknown>)[k] = lo + (hi - lo) * rnd();
    }
    for (const k of ['lighting', 'evolution', 'continuity'] as const) s[k] = s[k].map(() => RANGES[k][0] + (RANGES[k][1] - RANGES[k][0]) * rnd());
    s.routes = {};
    for (const a of ASPECTS) {
      const t = TRACK_SIGNALS[Math.floor(rnd() * TRACK_SIGNALS.length)];
      s.routes[routeKey(t, null, a)] = Math.round((rnd() * 2 - 1) * 100) / 100;
      const b = BAND_SIGNALS[Math.floor(rnd() * BAND_SIGNALS.length)];
      s.routes[routeKey(b, Math.floor(rnd() * 6), a)] = { h1: [0.2, rnd()], h2: [0.7, rnd()], h: rnd() * 2 - 1 };
    }
    const keys = Object.keys(s.routes);
    s.pins = ['form', 'hue', 'evolution:5', 'fluid', `route:${keys[round % keys.length]}`, 'continuity:0'];
    assert.deepEqual(settingsOf(cratarrPatch(s), DEFAULTS), s, `round ${round}`);
  }
});

test('the module types: every knob within its range, its neutral too, the cables reach knobs there are', () => {
  const types = cratarrTypes(RANGES, DEFAULTS);
  const knobs = new Map(types.flatMap((t) => t.knobs.map((k) => [`${t.type}.${k.id}`, k] as const)));
  for (const t of types)
    for (const k of t.knobs) {
      assert.ok(k.min <= k.default && k.default <= k.max, `${t.type}.${k.id}: default ${k.default} outside ${k.min}..${k.max}`);
      assert.ok(k.min <= k.neutral && k.neutral <= k.max, `${t.type}.${k.id}: neutral outside its range`);
    }
  assert.equal(knobs.get('flame.symmetry')?.kind, 'step');
  assert.equal(knobs.get('fluid.fluid')?.neutral, 0);
  assert.equal(knobs.get('frame.quality')?.device, true);
  const p = cratarrPatch(copy(DEFAULTS));
  const mods = modulesOf(p);
  for (const c of p.cables) {
    const m = mods.get(c.to.module);
    assert.ok(m, `cable ${c.id} to a module there is not`);
    const type = types.find((t) => t.type === m.type);
    assert.ok(type?.knobs.some((k) => k.id === c.to.knob), `cable ${c.id} to a knob there is not: ${m.type}.${c.to.knob}`);
  }
  // Today's chain: one layer (the flame, its show, its own loop), finish's three effects, nothing yet in the picture's
  // own loop, no modulators.
  assert.equal(p.layers.length, 1);
  assert.deepEqual(p.layers[0].loop.map((x) => x.id), ['fluid', 'tunnel', 'dive', 'trails', 'loop-effect']);
  assert.deepEqual(p.finish.map((x) => x.type), ['kaleidoscope-engine', 'eyedo-engine', 'hydra-engine']);
});

test('the patch beyond today\'s chain: kept, checked, there and back', async () => {
  const { extraFrom } = await import('../src/lib/visuals/patch.ts');
  const { extraType } = await import('../src/lib/visuals/registry.ts');
  const s = copy(DEFAULTS);
  const kept = {
    layers: [
      {
        id: 'shapes-1',
        source: { id: 'shapes-1-src', type: 'shapes', knobs: { count: 5, nonsense: 3 } },
        modules: [{ id: 'kal-1', type: 'kaleidoscope-engine', knobs: { amount: 2 } }, { id: 'bad', type: 'no-such-type', knobs: {} }],
        loop: [],
        blend: { mode: 'screen', amount: 0.7 },
      },
      { id: 'flame', source: { id: 'x', type: 'shapes', knobs: {} } },
    ],
    loop: [{ id: 'warp-1', type: 'warp', knobs: { zoom: 0.3 } }, { id: 'keep-1', type: 'keep', knobs: {} }, { id: 'warp-1', type: 'warp', knobs: {} }],
    modulators: [{ id: 'drift-1', type: 'drift', knobs: {} }],
    cables: [
      { id: 'c1', from: { modulator: 'drift-1' }, to: { module: 'kal-1', knob: 'k1' }, value: 0.5, mode: 'swing' },
      { id: 'c2', from: { signal: 'level', band: 2, reference: 'lately' }, to: { module: 'flame', knob: 'hue' }, value: { h1: [0.2, 0.1], h2: [0.7, 0.4], h: 0.9 }, mode: 'swing' },
      { id: 'c3', from: { signal: 'beat' }, to: { module: 'nowhere', knob: 'x' }, value: 1, mode: 'swing' },
      { id: 'c4', from: { modulator: 'gone' }, to: { module: 'warp-1', knob: 'turn' }, value: 1, mode: 'swing' },
    ],
    macros: [{ id: 'm1', on: 'shapes-1', label: 'Spin', value: 0.4, targets: [{ module: 'shapes-1-src', knob: 'spin', from: 0, to: 1 }, { module: 'x', knob: 'y', from: 0, to: 1 }] }],
  };
  s.extra = extraFrom(kept, extraType);
  assert.deepEqual(s.extra.layers.map((l) => l.id), ['shapes-1'], 'a layer named flame is not kept');
  assert.deepEqual(s.extra.layers[0].modules.map((m) => m.id), ['kal-1'], 'a module of a type there is not is left out');
  assert.equal(s.extra.layers[0].modules[0].knobs.amount, 1, 'a knob brought into its range');
  assert.ok(!('nonsense' in s.extra.layers[0].source.knobs), 'a knob the type has not is left out');
  assert.deepEqual(s.extra.loop.map((m) => m.id), ['warp-1', 'keep-1'], 'an id taken twice is kept once');
  assert.deepEqual(s.extra.cables.map((c) => c.id), ['c1', 'c2'], 'cables to nowhere and from a modulator not there are left out');
  assert.equal(s.extra.macros[0].targets.length, 1);
  // Through the patch and back: the extras unchanged, the routes as they were.
  const back = settingsOf(cratarrPatch(s), DEFAULTS);
  assert.deepEqual(back, s);
  const p = cratarrPatch(s);
  assert.equal(p.layers.length, 2);
  assert.ok(p.cables.some((c) => c.id === 'c2'));
});
