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
  const variants: [string, typeof settings, boolean][] = [['reduced motion', settings, true]];
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
  const variants: [string, typeof settings, boolean][] = [['the Cratarr chain', settings, false], ['reduced motion', settings, true]];
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

test("the extras (modulators, macros, a layer's knobs) as evaluate.ts moved them; an engine on a layer moved too, its steps asking for moments", async () => {
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
    { id: 'c1', from: { signal: 'level', band: 1 }, to: { module: 'fol', knob: 'in' }, value: 1, mode: 'swing' },
    { id: 'c2', from: { modulator: 'fol' }, to: { module: 'shapes-1', knob: 'radius' }, value: 0.5, mode: 'swing' },
    { id: 'c3', from: { modulator: 'osc' }, to: { module: 'shapes-1', knob: 'opacity' }, value: 0.8, mode: 'level' },
    { id: 'c4', from: { signal: 'hits', band: 0 }, to: { module: 'shapes-1', knob: 'sides' }, value: 1, mode: 'swing' },
    { id: 'c6', from: { signal: 'hits', band: 0 }, to: { module: 'kal', knob: 'effect' }, value: 1, mode: 'swing' },
    { id: 'c5', from: { signal: 'beat', reference: 'lately' }, to: { module: 'shapes-1', knob: 'thickness' }, value: 0.4, mode: 'swing' },
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
  // The hits through an ADSR at Linger, its gate reading them against the track's typical (an extra cable's default),
  // as toV2 has them.
  const adsr = { id: 'adsr-track', type: 'adsr', knobs: { retrigger: 0, amount: 1, ...Object.fromEntries(settings.lighting.flatMap((r: number, b: number) => [[`attack.${b}`, 0.04], [`decay.${b}`, 0], [`sustain.${b}`, 0], [`release.${b}`, r]])) } };
  const v2: PatchV2 = {
    v: 2,
    modules: [...['music', 'track', 'envelope', 'clock'].map((id) => ({ id, type: id, knobs: {} })), adsr, ...modulators, shapes, kal],
    cables: [
      ...cables.map((c) => ({
        id: c.id,
        from:
          'modulator' in c.from
            ? { module: c.from.modulator, port: 'out' }
            : c.from.signal === 'hits'
              ? { module: 'adsr-track', port: 'out', channel: c.from.band }
              : { module: 'music', port: c.from.signal, channel: c.from.band, reference: c.from.reference, speed: c.from.speed },
        to: { module: c.to.module, port: c.to.knob },
        value: c.value,
        mode: c.mode,
      })),
      { id: 'adsr-track-gate', from: { module: 'music', port: 'hits', reference: 'track' }, to: { module: 'adsr-track', port: 'gate' }, value: 1, mode: 'swing' },
    ],
    macros,
    pins: [],
  };
  const t = track(TRACKS[0]);
  const st = newEngine();
  const ev = newEvalState();
  const normal = newNormalState();
  const music = newMusicState();
  const dt = 1 / 30;
  let worst = 0;
  let moments = 0;
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
    // fix), and its Effect (a step) asks for its moments.
    assert.ok(mine.knobs.kal && mine.knobs.kal.effect === 1);
    for (const [knob, v] of Object.entries(theirs.knobs.kal)) assert.ok(Math.abs(v - mine.knobs.kal[knob]) <= 1e-6, `frame ${k}: kal.${knob}`);
    for (const id of ['fol', 'osc']) assert.ok(Math.abs(theirs.outputs[id] - mine.outputs[id]) <= 1e-9, `frame ${k}: ${id}`);
    assert.ok(Math.abs(theirs.macros.mac - mine.macros.mac) <= 1e-9);
    assert.deepEqual(theirs.moments, mine.moments);
    assert.ok(mine.moments.every((x) => x.module === 'kal' && x.knob === 'effect'));
    moments += mine.moments.length;
  }
  assert.ok(moments > 0 && moments < 600 / 30 / 2 + 1, `the hits asked ${moments} moments, two seconds apart at most`);
});
