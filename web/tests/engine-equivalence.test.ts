// The one engine against today's (decision 0057, step 2), frame by frame over recorded tracks. Until the one engine
// runs, today's against itself: the recording is whole and the same every run, on every track.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, todays, track, type Engine } from './harness/aspects-record.ts';
const { newEngineV1, stepEngineV1 } = await import('../src/lib/visuals/engine/v1.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
import type { PatchV2 } from '../src/lib/visuals/ports.ts';
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const settings = structuredClone(DEFAULTS);

test('the recorded tracks decode, the groove with its beats', () => {
  const ts = TRACKS.map(track);
  assert.ok(ts.every((t) => t.signals.series.size > 5));
  assert.ok(ts[0].beats && ts[0].beats.at.length > 100);
});

test("today's engine, recorded twice, is the same frame for frame: the harness itself", () => {
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(todays(), t, settings, 30, 10);
    const b = record(todays(), t, settings, 30, 10);
    assert.equal(a.length, 300);
    assert.ok(a[0].size > 100, `${a[0].size} numbers a frame`);
    for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} ${k} at frame ${d.frame}`);
    // It moves: the music is in it.
    const moved = [...compare(a, a.map(() => a[0]))].filter(([, d]) => d.most > 1e-3).length;
    assert.ok(moved > 10, `${id}: ${moved} fields move`);
  }
});

/** The one engine's modules, given by today's routes. */
const modules = (): Engine => {
  const st = newEngineV1();
  return { name: 'engine v1', step: (m, s, dt, opts) => stepEngineV1(st, m, s, dt, opts) };
};

test("the hidden behaviours as modules (Envelope, Clock, Track, Flash, the chain's parts) give today's picture exactly", () => {
  for (const id of TRACKS) {
    const t = track(id);
    for (const from of [0, 60]) {
      const a = record(todays(), t, settings, from, 20);
      const b = record(modules(), t, settings, from, 20);
      for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} from ${from} s: ${k} differs by ${d.most} at frame ${d.frame}`);
    }
  }
});

test('the same with reduced motion, and under each preset chain', async () => {
  const { CHAINS, chainRoutes } = await import('../src/lib/visuals/chains.ts');
  const variants: [string, typeof settings, boolean][] = [];
  for (const c of CHAINS) {
    if (c.extra.cables.length || c.extra.modulators.length) continue;
    variants.push([c.id, { ...settings, ...(c.settings ?? {}), routes: chainRoutes(c) } as typeof settings, false]);
  }
  for (const id of [TRACKS[0], TRACKS[3]]) {
    const t = track(id);
    for (const [name, s, still] of variants) {
      const a = record(todays(), t, s, 30, 15, still);
      const b = record(modules(), t, s, 30, 15, still);
      for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} ${name}: ${k} differs by ${d.most} at frame ${d.frame}`);
    }
  }
});

/** The one engine over the cables of today's settings as a v2 patch (a5's toV2), its devices opened up (flatten). */
const graph = (): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s) {
        from = s;
        p = flatten(toV2(s));
      }
      return stepEngine(st, p!, {}, m, () => 0, s, dt, opts).aspects;
    },
  };
};

test('the one engine over the cables of a v2 patch gives what the routes gave', async () => {
  const { CHAINS, chainRoutes } = await import('../src/lib/visuals/chains.ts');
  // Every column's scale off 1 (folded into the cables' amounts by toV2, 0057 §7e): Size above all, which is no base.
  const scaled = { ...settings, shape: 1.4, rotation: 0.6, lights: 1.3, glow: 0.7, size: 0.55, saturation: 1.2, trails: 0.8, presence: 1.5, waves: 0.65 } as typeof settings;
  const variants: [string, typeof settings, boolean][] = [['the Cratarr chain', settings, false], ['every scale off 1', scaled, false]];
  for (const c of CHAINS) if (!c.extra.cables.length && !c.extra.modulators.length) variants.push([c.id, { ...settings, ...(c.settings ?? {}), routes: chainRoutes(c) } as typeof settings, false]);
  let worst = 0;
  for (const id of TRACKS) {
    const t = track(id);
    for (const [name, s, still] of variants) {
      const a = record(todays(), t, s, 30, 15, still);
      const b = record(graph(), t, s, 30, 15, still);
      for (const [k, d] of compare(a, b)) {
        worst = Math.max(worst, d.most);
        assert.ok(d.most <= 1e-4, `${id} ${name}: ${k} differs by ${d.most} at frame ${d.frame}`);
      }
    }
  }
});

test("the extras (modulators, macros, a layer's knobs) as evaluate.ts moved them; an engine on a layer moved too, its step moved on by its Gate's Next", async () => {
  const { evaluate, newEvalState } = await import('../src/lib/visuals/evaluate.ts');
  const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
  const { defaultsOf } = await import('../src/lib/visuals/modulators/modulators.ts');
  const { MODULATORS } = await import('../src/lib/visuals/modulators/modulators.ts');
  const { newNormalState, signalsFor } = await import('../src/lib/visuals/normal.ts');
  const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
  const { FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
  type Patch = import('../src/lib/visuals/patch.ts').Patch;
  type Cable = import('../src/lib/visuals/patch.ts').Cable;
  const types = Object.fromEntries(EXTRA_TYPES.map((t) => [t.type, t]));
  const shapes = { id: 'shapes-1', type: 'shapes', knobs: { count: 4, opacity: 0.85, sides: 5, radius: 0.22, thickness: 0.012 } };
  const modulators = [
    { id: 'fol', type: 'fast', knobs: defaultsOf(MODULATORS.fast) },
    { id: 'osc', type: 'lfo', knobs: { ...defaultsOf(MODULATORS.lfo), rate: 0.5 } },
  ];
  const cables: Cable[] = [
    // A row no route reads (a row's reading is the routes' where they read it: 0057 §7e).
    { id: 'c1', from: { signal: 'flux', band: 1 }, to: { module: 'fol', knob: 'in' }, value: 1, mode: 'swing' },
    { id: 'c2', from: { modulator: 'fol' }, to: { module: 'shapes-1', knob: 'radius' }, value: 0.5, mode: 'swing' },
    { id: 'c3', from: { modulator: 'osc' }, to: { module: 'shapes-1', knob: 'opacity' }, value: 0.8, mode: 'level' },
    { id: 'c4', from: { signal: 'hits', band: 0 }, to: { module: 'shapes-1', knob: 'sides' }, value: 1, mode: 'swing' },
    { id: 'c6', from: { signal: 'hits', band: 0 }, to: { module: 'kal', knob: 'effect' }, value: 1, mode: 'swing' },
    { id: 'c5', from: { signal: 'centroid', reference: 'lately' }, to: { module: 'shapes-1', knob: 'thickness' }, value: 0.4, mode: 'swing' },
  ];
  const macros = [{ id: 'mac', on: 'shapes-1', label: 'M', value: 0.3, targets: [{ module: 'shapes-1', knob: 'count', from: 2, to: 8 }] }];
  const flame = { id: 'flame', type: 'flame', knobs: {} };
  const kal = { id: 'kal', type: 'kaleidoscope-engine', knobs: { effect: 1, amount: 1, k1: 0.5, k2: 0.5 } };
  const v1 = {
    v: 1,
    frame: { id: 'frame', type: 'frame', knobs: {} },
    layers: [
      { id: 'flame', source: flame, modules: [], loop: [], blend: { mode: 'add', amount: 1 } },
      { id: 'l2', source: shapes, modules: [kal], loop: [], blend: { mode: 'screen', amount: 1 } },
    ],
    loop: [],
    finish: [],
    modulators,
    cables,
    macros,
    pins: [],
  } as unknown as Patch;
  // The same extras as today's settings carry them, converted by toV2 (its ADSR for the hits, the rows' readings, a VCA
  // into the opacity's gain for the level cable) and opened up.
  const withExtras = { ...settings, extra: { layers: [v1.layers[1]], loop: [], modulators, cables, macros } } as typeof settings;
  const v2: PatchV2 = flatten(toV2(withExtras));
  const t = track(TRACKS[0]);
  const st = newEngine();
  const ev = newEvalState();
  const normal = newNormalState();
  const music = newMusicState();
  const dt = 1 / 30;
  let worst = 0;
  let moments = 0;
  const theirFrames: number[] = [];
  const myFrames: number[] = [];
  for (let k = 0; k < 600; k++) {
    const at = 20 + k * dt;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: settings.lighting });
    const read = signalsFor(normal, m, at * 1000);
    const mine = stepEngine(st, v2, types, m, read, settings, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
    const theirs = evaluate(v1, types, read, ev, dt, { envelope: mine.outputs['envelope.out'], frame: { beat: m.beat } });
    for (const [knob, v] of Object.entries(theirs.knobs['shapes-1'])) {
      const d = Math.abs(v - mine.knobs['shapes-1'][knob]);
      worst = Math.max(worst, d);
      // The hits through the ADSR: its strikes in doubles, today's Hits signal stored as 32-bit floats (adsr.test.ts).
      assert.ok(d <= 1e-6, `frame ${k}: shapes-1.${knob} ${mine.knobs['shapes-1'][knob]} against ${v}`);
    }
    // An effect engine on a layer (its type also the chain's finish's) is moved by both (evaluate.ts by id since a5's
    // fix), and its Effect (a step) moves on as the Gate toV2 made fires its Next.
    assert.ok(mine.knobs.kal && mine.knobs.kal.effect === 1);
    for (const [knob, v] of Object.entries(theirs.knobs.kal)) assert.ok(Math.abs(v - mine.knobs.kal[knob]) <= 1e-6, `frame ${k}: kal.${knob}`);
    for (const id of ['fol', 'osc']) assert.ok(Math.abs(theirs.outputs[id] - mine.outputs[id]) <= 1e-9, `frame ${k}: ${id}`);
    assert.ok(Math.abs(theirs.macros.mac - mine.macros.mac) <= 1e-9);
    assert.ok(mine.nexts.every((x) => x.module === 'kal' && x.knob === 'effect'));
    if (theirs.moments.length) theirFrames.push(k);
    if (mine.nexts.length) myFrames.push(k);
    moments += mine.nexts.length;
  }
  // The Gate's gap is "more than two seconds" (the chain's moments' rule); evaluate.ts's was "two or more", so a moment
  // exactly two seconds after the last comes a frame later (0057 §7e item 3).
  assert.equal(myFrames.length, theirFrames.length);
  theirFrames.forEach((f, i) => assert.ok(myFrames[i] === f || myFrames[i] === f + 1, `moment ${i}: frame ${myFrames[i]} against ${f}`));
  assert.ok(moments > 0 && moments < 600 / 30 / 2 + 1, `the hits asked ${moments} moments, two seconds apart at most`);
});

test("each chain module's own knobs and node values (step 5, the one engine) are the chain's: the same, by module", async () => {
  const { A } = await import('../src/lib/visuals/routes.ts');
  const { BANDS } = await import('../src/lib/visuals/settings.svelte.ts');
  const swing: [keyof typeof A, string, string][] = [['brightness', 'show/tonemap', 'glow'], ['size', 'flame', 'zoom'], ['saturation', 'show/saturate', 'saturation'], ['trails', 'trails', 'trails'], ['waves', 'show/bend', 'waves'], ['stir', 'fluid', 'stir'], ['flow', 'fluid', 'flow'], ['swirl', 'fluid', 'swirl']];
  const parts: [keyof typeof A, string][] = [['shape', 'shape'], ['rotation', 'rotation'], ['lights', 'lights'], ['hue', 'hue'], ['spread', 'spread'], ['presence', 'presence']];
  const levels: [keyof typeof A, string, string, number][] = [['smoothing', 'show/blur', 'smoothing', 0], ['haze', 'show/curve', 'haze', -1], ['fluid', 'fluid', 'fluid', 0], ['tunnel', 'tunnel', 'tunnel', 0], ['dive', 'dive', 'dive', 0], ['facets', 'fluid', 'facets', 0]];
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  let checked = 0;
  let effected = 0;
  const { slotModules } = await import('../src/lib/visuals/engine/effect-slots.ts');
  const { settingsOfPatch } = await import('../src/lib/visuals/engine/settings-view.ts');
  const { effectsNow } = await import('../src/lib/visuals/effects.ts');
  const { fxOfModule } = await import('../src/lib/visuals/nodes/effect-runs.ts');
  const checking = (): Engine => {
    const st = newEngine();
    let p: PatchV2 | null = null;
    let from: unknown = null;
    return {
      name: 'engine, checked',
      step: (m, s, dt, opts) => {
        if (from !== s) [from, p] = [s, flatten(toV2(s))];
        const f = stepEngine(st, p!, {}, m, () => 0, s, dt, opts);
        const g = st.got;
        const mod = (id: string) => p!.modules.find((x) => x.id === id);
        for (const [a, id, knob] of swing) if (mod(id)) assert.ok(Math.abs(f.knobs[id][knob] - g.whole[A[a]]) < 1e-9, `${a}: ${f.knobs[id][knob]} vs ${g.whole[A[a]]}`);
        for (const [a, knob] of parts) for (let b = 0; b < BANDS; b++) assert.ok(Math.abs(f.knobs.flame[`${knob}.${b}`] - (g.whole[A[a]] + g.band[b][A[a]])) < 1e-9, `${a}.${b}`);
        for (const [a, id, knob, lo] of levels) if (mod(id)) assert.ok(Math.abs(f.knobs[id][knob] - clamp(mod(id)!.knobs[knob] * g.level[A[a]], lo, 1)) < 1e-9, `${a}`);
        // The Show's parts' own node values (5b): what the chain gave.
        const d = f.aspects!;
        const near = (a: number | undefined, b: number, what: string, within = 1e-9) => assert.ok(a !== undefined && Math.abs(a - b) < within, `${what}: ${a} vs ${b}`);
        near(f.params['show/tonemap']?.flash, d.flash, 'flash');
        near(f.params['show/saturate']?.saturation, d.saturation, 'saturation');
        near(f.params['show/curve']?.gamma, d.gamma, 'gamma');
        near(f.params['show/curve']?.floor, d.floor, 'floor');
        near(f.params['show/blur']?.smoothing, d.smoothing, 'smoothing');
        near(f.params['show/bend']?.waves, d.given[A.waves], 'waves');
        // The light loop's and the camera's (5c).
        if (mod('trails')) near(f.params.trails?.keep, d.keep, 'keep');
        if (mod('fluid')) {
          near(f.params.fluid?.fluid, d.fluid, 'fluid');
          near(f.params.fluid?.warp, d.warp, 'warp');
          near(f.params.fluid?.facets, d.facets, 'facets');
        }
        if (mod('tunnel')) near(f.params.tunnel?.tunnel, d.tunnel, 'tunnel');
        if (mod('dive')) near(f.params.dive?.dive, d.dive, 'dive');
        near(f.params.flame?.scale, d.scale, 'scale');
        // The Flame's own state (5d): its parts, turn, rates, shapes, lights.
        const own = f.params.flame!.flame!;
        near(own.turn, d.turn, 'turn');
        own.rates.forEach((v, i) => near(v, d.rates[i], `rate ${i}`));
        own.shape.forEach((v, i) => near(v, d.shape[i], `shape ${i}`));
        own.hits.forEach((v, i) => near(v, d.hits[i], `lights ${i}`));
        own.parts.forEach((q, i) => (['turn', 'size', 'morph', 'hue', 'spread', 'weight'] as const).forEach((key) => near(q[key], d.parts[i][key], `part ${i} ${key}`)));
        // Each effect module's own (5e): what its slot was given (effectsNow over the patch as settings).
        for (const [slot, m] of slotModules(p!.modules)) {
          if (slot === 'loop-effect') continue;
          const i = ['fx1', 'fx2', 'fx3'].indexOf(slot);
          const old = effectsNow(settingsOfPatch(p!, s) as never, d.effects, 0, null, undefined, d.routed);
          const own = fxOfModule(m.type, f.knobs[m.id]);
          near(own?.amount, old.amounts[i], `${slot} amount`, 1e-6); // its columns are 32-bit floats
          near(own?.k[0], old.knobs[i][0], `${slot} k1`, 1e-6); // its columns are 32-bit floats
          near(own?.k[1], old.knobs[i][1], `${slot} k2`, 1e-6); // its columns are 32-bit floats
          effected++;
        }
        checked++;
        return f.aspects;
      },
    };
  };
  const { CHAINS, chainRoutes } = await import('../src/lib/visuals/chains.ts');
  const scaled = { ...settings, shape: 1.4, rotation: 0.6, lights: 1.3, glow: 0.7, size: 0.55, saturation: 1.2, trails: 0.8, presence: 1.5, waves: 0.65, fluid: 0.6, tunnel: 0.4, dive: 0.3, haze: -0.4 } as typeof settings;
  const effected_ = { ...settings, fx1: 2, fx1Amount: 0.7, fx1K1: 0.3, fx1K2: 0.8, fx2: 1, fx2Amount: 0.5, fx2K1: 0.6, fx2K2: 0.2, fx3: 3, fx3Amount: 0.4 } as typeof settings;
  const variants = [settings, scaled, effected_, ...CHAINS.filter((c) => !c.extra.cables.length && !c.extra.modulators.length).map((c) => ({ ...settings, ...(c.settings ?? {}), routes: chainRoutes(c) }) as typeof settings)];
  for (const id of [TRACKS[0], TRACKS[3]]) for (const s of variants) record(checking(), track(id), s, 30, 5);
  assert.ok(checked >= 600, `${checked} frames`);
  assert.ok(effected >= 300, `${effected} effect frames`);
});
