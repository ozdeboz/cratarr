// The one engine against the evaluator it replaced for the extras (decision 0057, step 4): modulators, macros, a
// layer's knobs, an engine on a layer and its step moved on by its Gate's Next, frame by frame over a recorded track.
// (Moved out of the equivalence tests, which held the one engine to today's summary: retired with it in 0.10.)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/recorded.ts';
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
import type { PatchV2 } from '../src/lib/visuals/ports.ts';
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const settings = structuredClone(DEFAULTS);

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
    const mine = stepEngine(st, v2, types, m, read, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
    const theirs = evaluate(v1, types, read, ev, dt, { envelope: mine.outputs['envelope.out'], frame: { beat: m.beat } });
    for (const [knob, v] of Object.entries(theirs.knobs['shapes-1'])) {
      const d = Math.abs(v - mine.knobs['shapes-1'][knob]);
      worst = Math.max(worst, d);
      // The hits through the ADSR: its strikes in doubles, today's Hits signal stored as 32-bit floats (adsr.test.ts).
      assert.ok(d <= 1e-6, `frame ${k}: shapes-1.${knob} ${mine.knobs['shapes-1'][knob]} against ${v}`);
    }
    // An effect engine on a layer (its type also the chain's finish's) is moved by both (evaluate.ts by id since the
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
