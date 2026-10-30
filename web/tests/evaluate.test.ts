// The patch's evaluator (evaluate.ts, decision 0056 step 4): swings and levels as routes.ts gives them, the envelope,
// the bands, the modulators in order (and a cycle a frame late), macros, moments, seeds, a patch changing: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { evaluate, MOMENT_GAP, newEvalState, type EvalOptions } from '../src/lib/visuals/evaluate.ts';
import { A, B, BAND_SIGNALS, compile, give, given, routeKey, T, TRACK_SIGNALS } from '../src/lib/visuals/routes.ts';
import { straight, type RouteValue } from '../src/lib/visuals/curves.ts';
import { MODULATORS, defaultsOf } from '../src/lib/visuals/modulators/modulators.ts';
import { seedOf } from '../src/lib/visuals/flame.ts';
import type { Cable, Knob, Macro, Module, ModuleType, Patch, SignalRef } from '../src/lib/visuals/patch.ts';

const BANDS = 6;
const close = (a: number, b: number, eps = 1e-5) => Math.abs(a - b) < eps;
const knob = (id: string, min: number, max: number, def: number, kind: Knob['kind']): Knob => ({ id, label: id, min, max, neutral: kind === 'swing' ? def : min, default: def, kind });
const TYPES: Record<string, ModuleType> = {
  test: {
    type: 'test',
    label: 'Test',
    place: 'module',
    knobs: [knob('swing', -10, 10, 0, 'swing'), knob('level', 0, 1, 1, 'level'), knob('step', 0, 8, 0, 'step'), knob('part.2', -1, 1, 0, 'swing')],
  },
  wide: { type: 'wide', label: 'Wide', place: 'module', knobs: [knob('swing', -100, 100, 0, 'swing'), knob('level', 0, 1, 1, 'level')] },
  ...MODULATORS,
};
const mod = (id: string, knobs: Record<string, number> = {}, type = 'test'): Module => ({ id, type, knobs });
const patchOf = (modules: Module[], modulators: Module[], cables: Cable[], macros: Macro[] = []): Patch => ({
  v: 1,
  frame: { id: 'frame', type: 'frame', knobs: {} },
  layers: [{ id: 'shapes', source: modules[0], modules: modules.slice(1), loop: [], blend: { mode: 'add', amount: 1 } }],
  loop: [],
  finish: [],
  modulators,
  cables,
  macros,
  pins: [],
});
let ids = 0;
const cable = (from: Cable['from'], module: string, k: string, value: RouteValue, mode: Cable['mode'] = 'swing'): Cable => ({ id: `c${ids++}`, from, to: { module, knob: k }, value, mode });
const OPTS: EvalOptions = { envelope: 1 };
const none = () => 0;
/** A signal held at `v` (1 = typical). */
const held = (v: Record<string, number>) => (ref: SignalRef) => v[ref.band === undefined ? ref.signal : `${ref.signal}${ref.band}`] ?? 0;

const signalsNow = (seed: number) => {
  let x = seed;
  const rnd = () => ((x = (x * 1103515245 + 12345) % 2147483648) / 2147483648);
  return {
    track: Float32Array.from(TRACK_SIGNALS, () => rnd()),
    bands: Array.from({ length: BANDS }, () => Float32Array.from(BAND_SIGNALS, () => rnd())),
  };
};

test('swings and levels give what routes.ts gives on the same inputs: curves, bands, walks, the envelope', () => {
  const bent = { h1: [0.2, 0.5] as [number, number], h2: [0.6, -0.3] as [number, number], h: 0.4 };
  const routes: Record<string, RouteValue> = {
    [routeKey('loudness', null, 'size')]: 0.3,
    [routeKey('beat', null, 'size')]: { ...bent, walks: true },
    [routeKey('swell', null, 'size')]: -0.4,
    [routeKey('level', 1, 'size')]: 0.8,
    [routeKey('hits', 4, 'size')]: bent,
    [routeKey('swell', null, 'tunnel')]: { ...straight(0.6), walks: true },
    [routeKey('beat', null, 'tunnel')]: -0.5,
    [routeKey('level', 0, 'tunnel')]: bent,
    [routeKey('flux', 3, 'tunnel')]: 0.9,
  };
  const m = compile(routes, BANDS);
  const cables: Cable[] = Object.entries(routes).map(([key, value]) => {
    const [signal, band, aspect] = key.split('|');
    return { id: key, from: band === '-' ? { signal } : { signal, band: Number(band) }, to: { module: 'm', knob: aspect === 'size' ? 'swing' : 'level' }, value, mode: aspect === 'size' ? 'swing' : 'level' };
  });
  const p = patchOf([mod('m', { swing: 0.5, level: 0.8 }, 'wide')], [], cables);
  const walkKeys = m.walks;
  let least = 1;
  for (let seed = 1; seed < 30; seed++) {
    const { track, bands } = signalsNow(seed);
    const e = (seed % 10) / 10;
    const factors = Float32Array.from(walkKeys, (_, i) => 1 - 0.1 * ((seed + i) % 4));
    const want = give(m, track, bands, e, given(BANDS), factors);
    const signals = (ref: SignalRef) => 2 * (ref.band === undefined ? track[T[ref.signal as keyof typeof T]] : bands[ref.band][B[ref.signal as keyof typeof B]]);
    const walk = Object.fromEntries(walkKeys.map((k, i) => [k, factors[i]]));
    const got = evaluate(p, TYPES, signals, newEvalState(), 1 / 60, { envelope: e, walk }).knobs.m;
    assert.ok(close(got.swing, 0.5 + 100 * want.whole[A.size], 1e-4), `swing ${got.swing} vs ${0.5 + 100 * want.whole[A.size]}`);
    assert.ok(close(got.level, 0.8 * want.level[A.tunnel]), `level ${got.level} vs ${0.8 * want.level[A.tunnel]}`);
    least = Math.min(least, want.level[A.tunnel]);
  }
  assert.ok(least < 0.9 && walkKeys.length === 2, 'the levels held back, and both walks taken');
});

test('a swing moves by half its range at all given, and stays within it', () => {
  const p = patchOf([mod('m', { swing: 2 })], [], [cable({ signal: 'beat' }, 'm', 'swing', 1)]);
  const at = (beat: number) => evaluate(p, TYPES, held({ beat }), newEvalState(), 1 / 60, OPTS).knobs.m.swing;
  assert.equal(at(0), 2);
  assert.equal(at(1), 2 + 0.5 * 10, 'typical: half way along the cable');
  assert.equal(at(2), 10, 'all of it: half the range, held at its top');
});

test('the envelope scales every cable but the loudness’s own', () => {
  const p = patchOf(
    [mod('a'), mod('b'), mod('c')],
    [],
    [cable({ signal: 'loudness' }, 'a', 'swing', 1), cable({ signal: 'beat' }, 'b', 'swing', 1), cable({ signal: 'level', band: 2 }, 'c', 'part.2', 1)],
  );
  const got = evaluate(p, TYPES, held({ loudness: 1, beat: 1, level2: 1 }), newEvalState(), 1 / 60, { envelope: 0 }).knobs;
  assert.equal(got.a.swing, 5, 'the loudness as it is');
  assert.equal(got.b.swing, 0, 'the beat: none when quiet');
  assert.equal(got.c['part.2'], 0, 'a band’s: none when quiet');
  const half = evaluate(p, TYPES, held({ loudness: 1, beat: 1 }), newEvalState(), 1 / 60, { envelope: 0.5 }).knobs;
  assert.equal(half.a.swing, 5);
  assert.equal(half.b.swing, 2.5);
});

test('band cables into a whole knob: the swings’ mean over the bands, the levels’ geometric mean; into a band’s own, as they are', () => {
  const every = Array.from({ length: BANDS }, (_, b) => b);
  const sig = held(Object.fromEntries(every.map((b) => [`level${b}`, 1])));
  const all = patchOf([mod('m', { level: 1 })], [], [...every.map((band) => cable({ signal: 'level', band }, 'm', 'swing', 1)), ...every.map((band) => cable({ signal: 'level', band }, 'm', 'level', 1, 'level'))]);
  const a = evaluate(all, TYPES, sig, newEvalState(), 1 / 60, OPTS).knobs.m;
  assert.ok(close(a.swing, 5), `every band alike: one band’s (${a.swing})`);
  assert.ok(close(a.level, 0.5), `every band alike: one band’s share (${a.level})`);
  const two = patchOf([mod('m', { level: 1 })], [], [cable({ signal: 'level', band: 1 }, 'm', 'swing', 1), cable({ signal: 'level', band: 4 }, 'm', 'swing', 0.6), cable({ signal: 'level', band: 1 }, 'm', 'level', 1, 'level')]);
  const t = evaluate(two, TYPES, sig, newEvalState(), 1 / 60, OPTS).knobs.m;
  assert.ok(close(t.swing, ((0.5 + 0.3) / BANDS) * 10), `two bands of six: their sum over six (${t.swing})`);
  assert.ok(close(t.level, 0.5 ** (1 / BANDS)), `one band’s condition: its root over six (${t.level})`);
  const own = patchOf([mod('m')], [], [cable({ signal: 'level', band: 2 }, 'm', 'part.2', 1)]);
  assert.equal(evaluate(own, TYPES, sig, newEvalState(), 1 / 60, OPTS).knobs.m['part.2'], 0.5, 'a band’s own knob: as a track cable');
});

test('modulators step in the order their cables need: a follower into a gate into a counter, listed backwards', () => {
  const fk = { ...defaultsOf(MODULATORS.fast), full: 1 };
  const gk = { above: 0.8, hysteresis: 0.2, lift: 0, recover: 0, release: 0, amount: 1 };
  const ck = { length: 4, amount: 1 };
  const p = patchOf(
    [mod('m')],
    [mod('count', ck, 'counter'), mod('gate', gk, 'gate'), mod('follow', fk, 'fast')],
    [cable({ signal: 'beat' }, 'follow', 'in', 1), cable({ modulator: 'follow' }, 'gate', 'in', 1), cable({ modulator: 'gate' }, 'count', 'in', 1), cable({ modulator: 'count' }, 'm', 'swing', 1)],
  );
  const follow = MODULATORS.fast.create();
  const gate = MODULATORS.gate.create();
  const count = MODULATORS.counter.create();
  const st = newEvalState();
  let out = { outputs: {} as Record<string, number>, knobs: {} as Record<string, Record<string, number>> };
  for (let i = 0; i < 5 * 60; i++) {
    const beat = i % 60 < 6 ? 1.5 : 0.2;
    out = evaluate(p, TYPES, held({ beat }), st, 1 / 60, OPTS);
    const f = follow.step(1 / 60, none, { ...fk, in: beat });
    const g = gate.step(1 / 60, none, { ...gk, in: f });
    const c = count.step(1 / 60, none, { ...ck, in: g });
    assert.equal(out.outputs.follow, f);
    assert.equal(out.outputs.gate, g);
    assert.equal(out.outputs.count, c, `frame ${i}`);
  }
  assert.equal(out.outputs.count, 0.25, 'five beats on a count of four');
  assert.equal(out.knobs.m.swing, 0.25 * 10, 'the count, this frame, into the knob');
});

test('in a cycle the first modulator reads the other’s last frame', () => {
  const k = { window: 0, full: 1, amount: 1 };
  const p = patchOf([mod('m')], [mod('a', k, 'average'), mod('b', k, 'average')], [cable({ signal: 'beat' }, 'a', 'in', 1), cable({ modulator: 'b' }, 'a', 'in', 1), cable({ modulator: 'a' }, 'b', 'in', 1)]);
  const st = newEvalState();
  for (let i = 1; i <= 4; i++) {
    const out = evaluate(p, TYPES, held({ beat: 0.2 }), st, 1 / 60, OPTS);
    assert.ok(close(out.outputs.a, 0.2 * i), `a at ${i}: ${out.outputs.a}`);
    assert.equal(out.outputs.b, out.outputs.a, 'b this frame, after a');
  }
});

test('a bipolar modulator comes into a knob at half way at its rest', () => {
  const p = patchOf([mod('m')], [mod('l', { rate: 0, offset: 0, shape: 0, amount: 1 }, 'lfo')], [cable({ modulator: 'l' }, 'm', 'swing', 1)]);
  assert.equal(evaluate(p, TYPES, none, newEvalState(), 1 / 60, OPTS).knobs.m.swing, 5);
});

test('a macro sets its targets’ bases before the cables act, and is cabled itself', () => {
  const macro: Macro = { id: 'mac', on: 'shapes', label: 'Mood', value: 0.25, targets: [{ module: 'm', knob: 'swing', from: -4, to: 4 }, { module: 'm', knob: 'level', from: 0, to: 0.8 }, { module: 'm', knob: 'step', from: 0, to: 8 }] };
  const p = patchOf([mod('m', { swing: 9, level: 1, step: 3 })], [], [cable({ signal: 'beat' }, 'm', 'swing', 0.2)], [macro]);
  const quiet = evaluate(p, TYPES, none, newEvalState(), 1 / 60, OPTS);
  assert.equal(quiet.knobs.m.swing, -2);
  assert.ok(close(quiet.knobs.m.level, 0.2));
  assert.equal(quiet.knobs.m.step, 3, 'a step is not lerped');
  assert.ok(close(evaluate(p, TYPES, held({ beat: 2 }), newEvalState(), 1 / 60, OPTS).knobs.m.swing, -2 + 0.2 * 10));
  const cabled = { ...p, cables: [...p.cables, cable({ signal: 'swell' }, 'mac', 'value', 1)] };
  const out = evaluate(cabled, TYPES, held({ swell: 1 }), newEvalState(), 1 / 60, OPTS);
  assert.equal(out.macros.mac, 0.5, 'its value swung by half its range at typical');
  assert.equal(out.knobs.m.swing, 0);
});

test('a macro cabled from a modulator, and a modulator its target, go in order', () => {
  const macro: Macro = { id: 'mac', on: 'modulators', label: 'Depth', value: 0, targets: [{ module: 'b', knob: 'amount', from: 0, to: 1 }] };
  const k = { window: 0, full: 1, amount: 1 };
  const p = patchOf([mod('m')], [mod('b', k, 'average'), mod('a', k, 'average')], [cable({ signal: 'beat' }, 'a', 'in', 1), cable({ modulator: 'a' }, 'mac', 'value', 1), cable({ signal: 'beat' }, 'b', 'in', 1)], [macro]);
  const out = evaluate(p, TYPES, held({ beat: 0.4 }), newEvalState(), 1 / 60, OPTS);
  assert.ok(close(out.macros.mac, 0.2), 'a this frame, half its range');
  assert.ok(close(out.outputs.b, 0.4 * 0.2), 'b’s amount from the macro this frame');
});

test('a step is not moved; a moment is asked for on a rising edge only, ready again under a third and after the gap', () => {
  const p = patchOf([mod('m', { step: 2 })], [], [cable({ signal: 'beat' }, 'm', 'step', 1)]);
  const st = newEvalState();
  const frame = (beat: number, dt = 1 / 60) => evaluate(p, TYPES, held({ beat }), st, dt, OPTS);
  const asked: number[] = [];
  const beats = [0, 0.8, 1.2, 1.6, 1.6, 1.2, 0.9, 1.5, 1.6, 0.4];
  beats.forEach((b, i) => {
    const out = frame(b, 1);
    assert.equal(out.knobs.m.step, 2);
    if (out.moments.length) {
      assert.deepEqual(out.moments, [{ module: 'm', knob: 'step' }]);
      asked.push(i);
    }
  });
  assert.deepEqual(asked, [2], 'once as it rises past a half; not again while it stays up or above a third');
  // Down under a third, and up again: once more, but not within the gap.
  frame(0.4, 1);
  frame(0.4, 1);
  assert.equal(frame(1.2, 1).moments.length, 1);
  frame(0.2, 0.1);
  assert.equal(frame(1.2, 0.1).moments.length, 0, `within ${MOMENT_GAP} s`);
  assert.equal(frame(1.2, MOMENT_GAP).moments.length, 1, 'held up past the gap: taken then, as aspects.ts takes it');
  assert.equal(frame(1.2, 0.1).moments.length, 0, 'still up: no edge');
});

test('a modulator’s seed is its id’s hash: the same id walks alike, another id its own way', () => {
  const k = { rate: 2, amount: 1 };
  const p = patchOf([mod('m')], [mod('drift-a', k, 'drift'), mod('drift-b', k, 'drift')], []);
  const one = newEvalState();
  const two = newEvalState();
  const own = MODULATORS.random.create(seedOf('drift-a'));
  let apart = 0;
  for (let i = 0; i < 120; i++) {
    const x = evaluate(p, TYPES, none, one, 1 / 30, OPTS).outputs;
    const y = evaluate(p, TYPES, none, two, 1 / 30, OPTS).outputs;
    assert.equal(x['drift-a'], y['drift-a']);
    assert.equal(x['drift-a'], own.step(1 / 30, none, k));
    apart = Math.max(apart, Math.abs(x['drift-a'] - x['drift-b']));
  }
  assert.ok(apart > 0.1, 'two ids, two walks');
});

test('modules added and removed mid-run: the new ones start, the others carry on, the gone ones are dropped', () => {
  const k = { window: 1, full: 1, amount: 1 };
  const a = mod('a', k, 'average');
  const b = mod('b', k, 'average');
  const to = (from: string) => cable({ signal: 'beat' }, from, 'in', 1);
  const first = patchOf([mod('m')], [a], [to('a')]);
  const both = patchOf([mod('m'), mod('n')], [a, b], [to('a'), to('b'), cable({ modulator: 'b' }, 'n', 'swing', 1)]);
  const st = newEvalState();
  const whole = newEvalState();
  const sig = held({ beat: 0.8 });
  for (let i = 0; i < 30; i++) {
    evaluate(first, TYPES, sig, st, 1 / 60, OPTS);
    evaluate(first, TYPES, sig, whole, 1 / 60, OPTS);
  }
  const added = evaluate(both, TYPES, sig, st, 1 / 60, OPTS);
  assert.equal(added.outputs.a, evaluate(first, TYPES, sig, whole, 1 / 60, OPTS).outputs.a, 'a carries on');
  assert.equal(added.outputs.b, MODULATORS.average.create().step(1 / 60, sig, { ...k, in: 0.8 }), 'b from its start');
  assert.ok('n' in added.knobs);
  const gone = patchOf([mod('m'), mod('n')], [b], [to('b'), cable({ modulator: 'a' }, 'n', 'swing', 1)]);
  const out = evaluate(gone, TYPES, sig, st, 1 / 60, OPTS);
  assert.deepEqual(Object.keys(st.modulators), ['b']);
  assert.ok(!('a' in st.values) && !('a' in out.outputs));
  assert.equal(out.knobs.n.swing, 0, 'a cable from a gone modulator gives nothing');
  // Another type under the same id starts anew.
  const swapped = patchOf([mod('m')], [mod('b', k, 'peak')], []);
  evaluate(swapped, TYPES, sig, st, 1 / 60, OPTS);
  assert.equal(st.modulators.b.type, 'peak');
  // A step’s moment state goes with its module.
  const stepper = patchOf([mod('s')], [], [cable({ signal: 'beat' }, 's', 'step', 1)]);
  evaluate(stepper, TYPES, held({ beat: 2 }), st, 1 / 60, OPTS);
  assert.ok('s|step' in st.moments);
  evaluate(first, TYPES, sig, st, 1 / 60, OPTS);
  assert.ok(!('s|step' in st.moments));
});

test('today’s pipeline is left to aspects.ts', () => {
  const p = patchOf([mod('flame', {}, 'flame'), mod('m')], [], []);
  const out = evaluate(p, TYPES, none, newEvalState(), 1 / 60, OPTS);
  assert.deepEqual(Object.keys(out.knobs), ['m']);
});

test('beats into a port count exactly from the frame’s beat index: a weak beat still counts', () => {
  const p = patchOf([mod('m')], [mod('count', { length: 8, amount: 1 }, 'counter')], [cable({ signal: 'beat' }, 'count', 'in', 1)]);
  // Six beats of half a second at 60 frames a second; the third one weak, its normalised pulse under the count’s half.
  const strength = [1.5, 1.5, 0.3, 1.5, 1.5, 1.5];
  const run = (exact: boolean) => {
    const st = newEvalState();
    let out = 0;
    // A quarter second before the first beat, none known.
    for (let i = -15; i < 6 * 30; i++) {
      const b = Math.floor(i / 30);
      const pulse = i >= 0 && i % 30 < 3 ? strength[b] : 0;
      const frame = { beat: i < 0 ? null : { index: 10 + b, since: (i % 30) / 60, length: 0.5, strength: strength[b] } };
      out = evaluate(p, TYPES, held({ beat: pulse }), st, 1 / 60, exact ? { ...OPTS, frame } : OPTS).outputs.count;
    }
    return out;
  };
  assert.equal(run(true), 6 / 8, 'every beat');
  assert.equal(run(false), 5 / 8, 'the normalised pulse misses the weak one');
});

test('an effect engine on a layer is the patch’s to move: cables into it act (by id, not type)', async () => {
  const { evaluate, newEvalState } = await import('../src/lib/visuals/evaluate.ts');
  const { ENGINE_TYPES } = await import('../src/lib/visuals/registry.ts');
  const t = ENGINE_TYPES.find((x) => x.type === 'kaleidoscope-engine')!;
  const types = { [t.type]: t, shapes: { type: 'shapes', label: 'Shapes', place: 'source', knobs: [] } } as never;
  const k1 = t.knobs.find((k) => k.id === 'k1')!;
  const engine = { id: 'on-layer', type: 'kaleidoscope-engine', knobs: Object.fromEntries(t.knobs.map((k) => [k.id, k.default])) };
  const patch = {
    v: 1, frame: { id: 'frame', type: 'frame', knobs: {} }, finish: [], loop: [], modulators: [], macros: [], pins: [],
    layers: [{ id: 'l', source: { id: 'src', type: 'shapes', knobs: {} }, modules: [engine], loop: [], blend: { mode: 'add', amount: 1 } }],
    cables: [{ id: 'c', from: { signal: 'loudness' }, to: { module: 'on-layer', knob: 'k1' }, value: 1, mode: 'swing' }],
  } as never;
  const out = evaluate(patch, types, () => 2, newEvalState(), 1 / 60, { envelope: 1 });
  assert.notEqual(out.knobs['on-layer']?.k1, engine.knobs.k1, `k1 moved from ${k1.default}`);
});
