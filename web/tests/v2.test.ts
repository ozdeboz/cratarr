// The v1 → v2 conversion (v2.ts, decision 0057 step 1): today's chain and extras as modules and cables, by the
// contract with the new engine: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
const { CHAINS } = await import('./harness/oracle/chains.ts');
type VisualSettings = typeof DEFAULTS;
const copy = (s: VisualSettings): VisualSettings => JSON.parse(JSON.stringify(s));

test('the routes as cables: loudness from the Loudness (unscaled), track from an ADSR on a new track, the rest from Music as the routes read it', () => {
  const s = copy(DEFAULTS);
  s.routes = { [routeKey('loudness', null, 'size')]: 0.6, [routeKey('track', null, 'trails')]: -1, [routeKey('hits', 2, 'lights')]: 1, [routeKey('beat', null, 'size')]: 0 };
  const v = toV2(s, { follow: false });
  const by = Object.fromEntries(v.cables.map((c) => [c.id, c]));
  assert.deepEqual(by[routeKey('loudness', null, 'size')].from, { module: 'envelope', port: 'level' });
  assert.equal(by[routeKey('loudness', null, 'size')].depth, null);
  assert.deepEqual(by[routeKey('track', null, 'trails')].from, { module: 'newtrack', port: 'out' });
  assert.deepEqual(by['newtrack-gate'].from, { module: 'music', port: 'newtrack' }, 'Track folded into Music: its pulse an ADSR struck by a new track');
  assert.deepEqual(by[routeKey('hits', 2, 'lights')].from, { module: 'adsr', port: 'out', channel: 2 }, 'the hits through their ADSR');
  assert.equal(by[routeKey('hits', 2, 'lights')].depth, undefined);
  assert.deepEqual(by[routeKey('hits', 2, 'lights')].to, { module: 'flame', port: 'lights' });
  assert.equal(by[routeKey('beat', null, 'size')], undefined, 'a route at nothing is no cable');
});

test('Intensity, Flash and Speed move to the modules that do them, each a rack macro on its knob; the Whole frame gone', () => {
  const s = copy(DEFAULTS);
  s.intensity = 0.3;
  s.flash = 1.4;
  s.speed = 0.7;
  s.size = 0.5;
  const v = toV2(s);
  const m = Object.fromEntries(v.modules.map((x) => [x.id, x]));
  assert.equal(m.envelope.knobs.intensity, 0.3);
  assert.equal(m.accent.knobs.accent, 1.4);
  assert.equal(m.clock.knobs.speed, 0.7);
  assert.equal(m.frame, undefined, 'the Whole frame is gone');
  const macro = Object.fromEntries(v.macros.map((x) => [x.id, x]));
  assert.deepEqual(macro.flash.targets, [{ module: 'accent', knob: 'accent', from: 0, to: 2 }]);
  assert.ok(Math.abs(macro.flash.value - 0.7) < 1e-9 && Math.abs(macro.speed.value - 0.35) < 1e-9 && macro.intensity.value === 0.3);
  assert.equal(m.flame.knobs.zoom, 0, "Show's Size is the Flame's zoom; its scale went into the cables, the knob its resting value (§7e item 2)");
  assert.equal(m.show.type, 'device');
  // The effects' slots are their effects' modules (effect-modules.ts): none here, on no effect.
  for (const id of ['music', 'envelope', 'clock', 'accent', 'flame', 'show', 'fluid', 'tunnel', 'dive', 'trails']) assert.ok(m[id], id);
});

test('the extras come as they are: their modules, and their cables from a modulator or from Music with their reference and speed', () => {
  const s = copy(DEFAULTS);
  const calm = CHAINS.find((c) => c.id === 'calm')!;
  s.extra = JSON.parse(JSON.stringify(calm.extra));
  const v = toV2(s, { follow: false });
  const ids = new Set(v.modules.map((m) => m.id));
  for (const m of calm.extra.modulators) assert.ok(ids.has(m.id), m.id);
  const by = Object.fromEntries(v.cables.map((c) => [c.id, c]));
  assert.deepEqual(by['calm-size'].from, { module: 'breath', port: 'out' });
  assert.deepEqual(by['calm-hue'].from, { module: 'slow-swell', port: 'level' }, 'its slow reading through an Envelope Follower');
  assert.deepEqual(by['slow-swell-in'].from, { module: 'music', port: 'swell' });
  assert.deepEqual(by['calm-in'].to, { module: 'breath', port: 'in' });
  assert.deepEqual(by['calm-size'].to, { module: 'flame', port: 'zoom' }, "a cable into Show's size goes into the Flame's zoom");
  assert.deepEqual(by['calm-saturation'].to, { module: 'show', port: 'saturation' }, "Show's saturation keeps its address");
});

test('flatten: the Show device opened up, its modules in order, a cable into its exposed knob led to the module behind', async () => {
  const { flatten } = await import('../src/lib/visuals/v2.ts');
  const s = copy(DEFAULTS);
  s.routes = { [routeKey('loudness', null, 'saturation')]: 0.8 };
  const f = flatten(toV2(s));
  const ids = f.modules.map((m) => m.id);
  for (const id of ['show/bend', 'show/blur', 'show/tonemap', 'show/curve', 'show/saturate']) assert.ok(ids.includes(id), id);
  assert.ok(!ids.includes('show'));
  const c = f.cables.find((x) => x.id === routeKey('loudness', null, 'saturation'))!;
  assert.deepEqual(c.to, { module: 'show/saturate', port: 'saturation' });
  const chain = f.cables.filter((x) => x.id.startsWith('show/show-')).map((x) => `${x.from.module}>${x.to.module}`);
  assert.deepEqual(chain, ['show/bend>show/blur', 'show/blur>show/tonemap', 'show/tonemap>show/curve', 'show/curve>show/saturate']);
});

test("the bands' Linger: Music's hold and the hits' ADSR release; a hits route from the ADSR's channel, its id kept", () => {
  const s = copy(DEFAULTS);
  s.lighting = [0.2, 0.18, 0.15, 0.13, 0.1, 0.08];
  s.routes = { [routeKey('hits', 2, 'lights')]: 1, [routeKey('level', 1, 'hue')]: 0.5 };
  const v = toV2(s, { follow: false });
  const m = Object.fromEntries(v.modules.map((x) => [x.id, x]));
  assert.equal(m.music.knobs['hold.3'], 0.13);
  assert.equal(m.adsr.knobs['release.2'], 0.15);
  assert.equal(m.adsr.knobs['attack.0'], 0.04);
  assert.ok(!('lighting.0' in m.flame.knobs), 'Linger left the Flame');
  assert.ok(!m['adsr-drums'], 'no ADSR where no cable needs one');
  const by = Object.fromEntries(v.cables.map((c) => [c.id, c]));
  assert.deepEqual(by[routeKey('hits', 2, 'lights')].from, { module: 'adsr', port: 'out', channel: 2 });
  assert.deepEqual(by['adsr-gate'].from, { module: 'music', port: 'hits' });
  assert.equal(m.adsr.knobs.reference, 2, 'the ADSR reads its strikes as the routes did');
  assert.deepEqual(by[routeKey('level', 1, 'hue')].from, { module: 'music', port: 'level', channel: 1 });
  assert.equal(m.music.knobs['reference.level'], 2, 'the row read as it is, as routes read it');
});

test("an extra cable from the hits keeps its reading: its own ADSR, the gate reading against the track's typical", () => {
  const s = copy(DEFAULTS);
  const pulse = CHAINS.find((c) => c.id === 'pulse')!;
  s.extra = JSON.parse(JSON.stringify(pulse.extra));
  s.routes = { [routeKey('hits', 0, 'lights')]: 1 };
  // The wiring as converted, before Hit Lights packs the ADSRs into a device.
  const v = toV2(s, { devices: false, follow: false });
  const by = Object.fromEntries(v.cables.map((c) => [c.id, c]));
  assert.deepEqual(by['pulse-kick-in'].from, { module: 'adsr-track', port: 'out', channel: 1 });
  assert.deepEqual(by['adsr-track-gate'].from, { module: 'music', port: 'hits' });
  assert.equal(v.modules.find((x) => x.id === 'adsr-track')!.knobs.reference, 0, 'reading against the track’s typical');
  assert.deepEqual(by[routeKey('hits', 0, 'lights')].from, { module: 'adsr', port: 'out', channel: 0 }, 'the route through the absolute one');
});

test('a Drift converts to a Random at its smooth end', () => {
  const s = copy(DEFAULTS);
  s.extra = JSON.parse(JSON.stringify(CHAINS.find((c) => c.id === 'calm')!.extra));
  const wander = toV2(s).modules.find((m) => m.id === 'wander')!;
  assert.equal(wander.type, 'random');
  assert.equal(wander.knobs.shape, 0);
});

test("a column's scale goes into its cables' amounts (curves' heights too), the column knob at rest (0); the fluid's stirring left", () => {
  const s = copy(DEFAULTS);
  s.size = 0.5;
  s.rotation = 2;
  s.stir = 0.3;
  s.routes = {
    [routeKey('loudness', null, 'size')]: 0.6,
    [routeKey('beat', null, 'rotation')]: { h1: [1 / 3, 0.1], h2: [2 / 3, 0.4], h: 0.8 },
    [routeKey('beat', null, 'stir')]: 1,
  };
  const v = toV2(s, { follow: false });
  const by = Object.fromEntries(v.cables.map((c) => [c.id, c]));
  assert.equal(by[routeKey('loudness', null, 'size')].value, 0.3);
  assert.deepEqual(by[routeKey('beat', null, 'rotation')].value, { h1: [1 / 3, 0.2], h2: [2 / 3, 0.8], h: 1.6 });
  assert.equal(by[routeKey('beat', null, 'stir')].value, 1);
  const m = Object.fromEntries(v.modules.map((x) => [x.id, x]));
  assert.equal(m.flame.knobs.zoom, 0, 'the column knob its resting value, the cables adding to it');
  assert.equal(m.flame.knobs.rotation, 0);
});

test('the common patterns as devices: Calm, Pause Dim, Hit Lights; a Dice for a new track’s look', async () => {
  const v = toV2(copy(DEFAULTS));
  const device = (id: string) => v.modules.find((m) => m.id === id) as { label?: string; patch?: { modules: { id: string }[] } } | undefined;
  assert.equal(device('calm')?.label, 'Calm');
  assert.ok(device('calm')!.patch!.modules.some((m) => m.id === 'duck-motion'));
  assert.equal(device('pause')?.label, 'Pause Dim');
  assert.ok(!v.modules.some((m) => m.id.startsWith('duck-') || m.id.startsWith('pause-')), 'none left loose');
  assert.ok(v.modules.some((m) => m.type === 'dice'));
  assert.ok(v.cables.some((c) => c.from.module === 'music' && c.from.port === 'newtrack' && c.to.module === 'dice'));
});

test("Effects, a rack macro on each effect's Amount; its routes into each Amount beside the Amount's own", () => {
  const s = copy(DEFAULTS);
  Object.assign(s, { effects: 0.6, fx1: 3, fx1Amount: 0.8, fx2: 1, fx2Amount: 0.5 });
  s.routes = { ...s.routes, [routeKey('loudness', null, 'effects')]: 0.5 };
  const v = toV2(s, { devices: false });
  const macro = v.macros.find((x) => x.id === 'effects')!;
  assert.equal(macro.on, 'rack');
  assert.equal(macro.value, 0.6);
  assert.deepEqual(macro.targets, [
    { module: 'fx-rotate', knob: 'amount', from: 0, to: 0.8 },
    { module: 'fx-pulse', knob: 'amount', from: 0, to: 0.5 },
  ]);
  for (const fx of ['fx-rotate', 'fx-pulse']) assert.ok(v.cables.some((c) => c.to.module === fx && c.to.port === 'amount:gain'), `${fx}: the Effects route's VCA into its Amount's gain`);
  assert.ok(!v.cables.some((c) => c.to.module === 'frame'));
});
