// The morph (morph.ts, decision 0056 step 6): knob by knob over a time, the same at 30 and 120 frames a second, steps
// only at neutral, modules fading in and out on a chain change: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { askStep, FALLBACK_BEAT, morphTo, newMorph, stepMorph, type MorphFrame } from '../src/lib/visuals/morph.ts';
import type { Knob, Module, ModuleType } from '../src/lib/visuals/patch.ts';

const knob = (id: string, min: number, max: number, neutral: number, kind: Knob['kind'], device = false): Knob => ({ id, label: id, min, max, neutral, default: neutral, kind, ...(device ? { device } : {}) });
const TYPES: Record<string, ModuleType> = {
  kaleido: { type: 'kaleido', label: 'Kaleidoscope', place: 'module', knobs: [knob('amount', 0, 1, 0, 'level'), knob('angle', -1, 1, 0, 'swing'), knob('segments', 0, 8, 0, 'step'), knob('quality', 0, 2, 0, 'level', true)] },
  waves: { type: 'waves', label: 'Waves', place: 'source', knobs: [knob('height', 0, 1, 0, 'level'), knob('count', 1, 6, 1, 'step')] },
  drift: { type: 'drift', label: 'Drift', place: 'module', knobs: [knob('turn', -1, 1, 0, 'swing'), knob('order', 0, 4, 0, 'step')] },
};
const mod = (id: string, type: string, knobs: Record<string, number>): Module => ({ id, type, knobs });
const close = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) < eps;

test('a morph goes knob by knob from one state to the other over its time, eased', () => {
  const m = newMorph([mod('k', 'kaleido', { amount: 0.2, angle: -0.5, segments: 3, quality: 1 })], TYPES);
  morphTo(m, [mod('k', 'kaleido', { amount: 0.8, angle: 0.5, segments: 3, quality: 2 })], 2, TYPES);
  let f = stepMorph(m, 1, null, TYPES);
  assert.ok(close(f.knobs.k.amount, 0.5) && close(f.knobs.k.angle, 0), 'half way at half time');
  assert.equal(f.knobs.k.quality, 2, 'the device’s knob at once');
  assert.equal(f.still, false);
  f = stepMorph(m, 0.5, null, TYPES);
  assert.ok(f.knobs.k.amount > 0.5 && f.knobs.k.amount < 0.8);
  f = stepMorph(m, 0.5, null, TYPES);
  assert.deepEqual(f.knobs.k, { amount: 0.8, angle: 0.5, segments: 3, quality: 2 });
  assert.equal(f.still, true);
});

test('a new morph mid-way starts from where it is shown, without a jump', () => {
  const m = newMorph([mod('k', 'kaleido', { amount: 0, angle: 0, segments: 0, quality: 0 })], TYPES);
  morphTo(m, [mod('k', 'kaleido', { amount: 1, angle: 1, segments: 0, quality: 0 })], 1, TYPES);
  const was = stepMorph(m, 0.3, null, TYPES).knobs.k.amount;
  morphTo(m, [mod('k', 'kaleido', { amount: 0, angle: 1, segments: 0, quality: 0 })], 1, TYPES);
  const now = stepMorph(m, 1e-6, null, TYPES).knobs.k.amount;
  assert.ok(close(now, was, 1e-6), `${now} vs ${was}`);
});

/** A chain change with a step change, a module added and one taken away, a moment asked for on the way, run at `fps`;
 *  the frames at each 30th of a second. */
function run(fps: number): MorphFrame[] {
  const m = newMorph([mod('k', 'kaleido', { amount: 0.6, angle: 0.2, segments: 2, quality: 1 }), mod('d', 'drift', { turn: 0.5, order: 1 })], TYPES);
  morphTo(m, [mod('k', 'kaleido', { amount: 0.9, angle: -0.4, segments: 5, quality: 1 }), mod('w', 'waves', { height: 0.7, count: 3 })], 1.5, TYPES);
  const out: MorphFrame[] = [];
  for (let i = 1; i <= 2.5 * fps; i++) {
    const f = stepMorph(m, 1 / fps, 0.5, TYPES);
    if ((i * 30) % fps === 0) out.push(f);
    if (i === fps) askStep(m, 'k', 'segments', TYPES);
  }
  return out;
}

test('the same at 30 and at 120 frames a second', () => {
  const a = run(30);
  const b = run(120);
  assert.equal(a.length, b.length);
  let bottoms = 0;
  for (let i = 0; i < a.length; i++) {
    assert.deepEqual(Object.keys(a[i].knobs), Object.keys(b[i].knobs));
    // A dip's bottom falls between 30ths of a second here (half a beat is 0.25 s): the frame reaching it shows it.
    if (i > 0 && a[i].knobs.k.segments !== a[i - 1].knobs.k.segments) {
      bottoms++;
      assert.equal(a[i].knobs.k.amount, 0);
      continue;
    }
    for (const id of Object.keys(a[i].knobs))
      for (const [k, v] of Object.entries(a[i].knobs[id])) assert.ok(close(v, b[i].knobs[id][k], 1e-9), `${id}.${k} at ${i}: ${v} vs ${b[i].knobs[id][k]}`);
    assert.deepEqual(a[i].leaving.map((x) => x.id), b[i].leaving.map((x) => x.id), `leaving at ${i}`);
    for (let j = 0; j < a[i].leaving.length; j++)
      for (const [k, v] of Object.entries(a[i].leaving[j].knobs)) assert.ok(close(v, b[i].leaving[j].knobs[k]), `leaving ${k} at ${i}`);
  }
  assert.equal(bottoms, 2);
  const last = a[a.length - 1].knobs.k;
  assert.equal(last.segments, 6, 'the morph’s 5, then the moment’s next');
  assert.ok(close(last.amount, 0.9), 'back up after both dips');
});

test('a step flips only while its module’s levels are at neutral, at any frame rate', () => {
  let seed = 3;
  const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  const m = newMorph([mod('k', 'kaleido', { amount: 0.7, angle: 0.3, segments: 1, quality: 1 }), mod('d', 'drift', { turn: 0.6, order: 0 })], TYPES);
  let was = stepMorph(m, 0, 0.6, TYPES);
  let flips = 0;
  for (let i = 0; i < 4000; i++) {
    const r = rnd();
    if (r < 0.01) askStep(m, 'k', 'segments', TYPES);
    else if (r < 0.015) askStep(m, 'd', 'order', TYPES);
    else if (r < 0.018) morphTo(m, [mod('k', 'kaleido', { amount: rnd(), angle: rnd() * 2 - 1, segments: Math.floor(rnd() * 9), quality: 1 }), mod('d', 'drift', { turn: rnd() * 2 - 1, order: Math.floor(rnd() * 5) })], rnd() * 2, TYPES);
    const f = stepMorph(m, 0.005 + 0.04 * rnd(), rnd() < 0.2 ? null : 0.3 + rnd(), TYPES);
    if (f.knobs.k.segments !== was.knobs.k.segments) {
      flips++;
      assert.equal(f.knobs.k.amount, 0, `frame ${i}: the kaleidoscope at neutral as it flips`);
    }
    if (f.knobs.d.order !== was.knobs.d.order) {
      flips++;
      assert.equal(f.knobs.d.turn, 0, `frame ${i}: no level, so its swing at neutral as it flips`);
    }
    was = f;
  }
  assert.ok(flips > 20, `${flips} flips`);
});

test('a dip takes half a beat down and half back; 90 BPM without a tempo', () => {
  const at = (beat: number | null) => {
    const m = newMorph([mod('k', 'kaleido', { amount: 1, angle: 0, segments: 0, quality: 0 })], TYPES);
    askStep(m, 'k', 'segments', TYPES, 4);
    stepMorph(m, 0, beat, TYPES);
    const dt = 1 / 240;
    let flipped = -1;
    let back = -1;
    for (let i = 1; i <= 480; i++) {
      const f = stepMorph(m, dt, beat, TYPES);
      if (flipped < 0 && f.knobs.k.segments === 4) flipped = i * dt;
      if (flipped >= 0 && back < 0 && f.still) back = i * dt;
    }
    return { flipped, back };
  };
  const tempo = at(0.5);
  assert.ok(close(tempo.flipped, 0.25, 1e-6) && close(tempo.back, 0.5, 1e-6), JSON.stringify(tempo));
  const none = at(null);
  assert.ok(close(none.flipped, FALLBACK_BEAT / 2, 1e-6) && close(none.back, FALLBACK_BEAT, 1e-6), JSON.stringify(none));
});

test('a chain change: shared modules morph, added ones fade in from neutral, removed ones out to it and are dropped', () => {
  const m = newMorph([mod('k', 'kaleido', { amount: 0.5, angle: 0.5, segments: 2, quality: 1 }), mod('d', 'drift', { turn: 0.8, order: 3 }), mod('x', 'drift', { turn: -0.6, order: 2 })], TYPES);
  // `x` comes back as another type: the drift fades out, the waves in.
  morphTo(m, [mod('k', 'kaleido', { amount: 0.5, angle: -0.5, segments: 2, quality: 1 }), mod('w', 'waves', { height: 0.6, count: 4 }), mod('x', 'waves', { height: 1, count: 2 })], 1, TYPES);
  const first = stepMorph(m, 1e-4, null, TYPES);
  assert.ok(first.knobs.w.height < 1e-3, 'added: from its neutral');
  assert.equal(first.knobs.w.count, 4, 'added: its step at once, at neutral');
  assert.ok(close(first.knobs.k.amount, 0.5) && first.knobs.k.angle > 0.49, 'shared: from its state');
  assert.deepEqual(first.leaving.map((l) => `${l.id}:${l.type}`), ['d:drift', 'x:drift']);
  assert.ok(first.leaving[0].knobs.turn > 0.79 && first.leaving[0].knobs.order === 3, 'removed: from its state, its step kept');
  assert.ok('x' in first.knobs && first.knobs.x.height < 1e-3);
  const mid = stepMorph(m, 0.5 - 1e-4, null, TYPES);
  assert.ok(close(mid.knobs.w.height, 0.3) && close(mid.leaving[0].knobs.turn, 0.4) && close(mid.knobs.k.angle, 0));
  const end = stepMorph(m, 0.5, null, TYPES);
  assert.ok(close(end.knobs.w.height, 0.6) && close(end.knobs.x.height, 1));
  assert.ok(close(end.leaving[0].knobs.turn, 0) && close(end.leaving[1].knobs.turn, 0), 'removed: at neutral on the last frame');
  assert.deepEqual(stepMorph(m, 1 / 60, null, TYPES).leaving, [], 'then dropped');
  assert.equal(m.entries.length, 3);
});
