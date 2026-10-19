// @ts-nocheck: run by Node (node:test, a .ts import), not part of the app's check.
// The modulators (modulators.ts, decision 0056): 0 at neutral, within their range, the same at any frame rate, the
// gate's hysteresis and lift, the counters' wrap.
//   node --experimental-strip-types --test src/lib/visuals/modulators/modulators.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BANDS, MODULATORS, defaultsOf, neutralOf } from './modulators.ts';

const TYPES = Object.values(MODULATORS);

/** Random numbers from a seed (mulberry32), so a failure repeats. */
function randomFrom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
/** Signals that jump about: mostly round typical, sometimes far above, now and then nothing or not a number. */
const wild = (rnd: () => number) => () => {
  const r = rnd();
  return r < 0.05 ? NaN : r < 0.1 ? 0 : r < 0.15 ? 50 * rnd() : 3 * rnd();
};

test('every modulator gives 0 with its knobs at neutral', () => {
  for (const t of TYPES) {
    const rnd = randomFrom(1);
    const m = t.create(undefined, 7);
    const k = neutralOf(t);
    for (let i = 0; i < 2000; i++) {
      const v = m.step(rnd() * 0.1, wild(rnd), k);
      assert.equal(Math.abs(v), 0, `${t.type} at frame ${i}: ${v}`);
    }
  }
});

test('every modulator stays within its range, whatever its knobs and signals', () => {
  for (const t of TYPES) {
    const [lo, hi] = t.range === 'bipolar' ? [-1, 1] : [0, 1];
    const rnd = randomFrom(2);
    const m = t.create(undefined, 3);
    let k = defaultsOf(t);
    for (let i = 0; i < 5000; i++) {
      if (i % 50 === 0)
        k = Object.fromEntries(t.knobs.map((n) => [n.id, n.min + (n.max - n.min) * (rnd() * 1.4 - 0.2)]));
      const v = m.step(rnd() < 0.01 ? -1 : rnd() * 0.2, wild(rnd), k);
      assert.ok(v >= lo && v <= hi, `${t.type} at frame ${i}: ${v}`);
    }
  }
});

test('the followers and the other smooth ones give the same at 30 and at 120 frames a second', () => {
  // The music changes every 30th of a second, each band its own way.
  const level = (seg: number, band: number) => {
    const r = randomFrom(seg * 31 + band)();
    return seg % 40 < 20 ? 2.5 * r : 0.4 * r;
  };
  const run = (type: string, fps: number, knobs: Record<string, number>) => {
    const m = MODULATORS[type].create(undefined, 5);
    const out: number[] = [];
    for (let i = 1; i <= 3 * fps; i++) {
      const seg = Math.ceil((i * 30) / fps);
      const v = m.step(1 / fps, (ref) => level(seg, ref.band ?? 0), knobs);
      if ((i * 30) % fps === 0) out.push(v);
    }
    return out;
  };
  for (const type of ['volume', 'fast', 'slow', 'average', 'peak', 'mean', 'lfo', 'drift']) {
    const t = MODULATORS[type];
    for (const knobs of [defaultsOf(t), { ...defaultsOf(t), attack: 0.3, release: 0.05, window: 0.2, fall: 0.3, horizon: 2, rate: 1.7, full: 1 }]) {
      const a = run(type, 30, knobs);
      const b = run(type, 120, knobs);
      assert.equal(a.length, 90);
      assert.equal(b.length, 90);
      for (let i = 0; i < a.length; i++) assert.ok(Math.abs(a[i] - b[i]) < 1e-9, `${type} at ${i}: ${a[i]} vs ${b[i]}`);
      assert.ok(a.some((v) => Math.abs(v) > 0.05), `${type} moves`);
    }
  }
  assert.equal(BANDS, 6);
});

/** A gate's output for each signal in turn, a 60th of a second apart. */
function gateOver(xs: number[], knobs: Record<string, number>) {
  const m = MODULATORS.gate.create();
  return xs.map((x) => m.step(1 / 60, () => x, knobs));
}
const GATE = { above: 1, hysteresis: 0.2, lift: 0, recover: 0, release: 0, amount: 1 };

test('the gate opens above typical and, by its hysteresis, closes only well below it', () => {
  //                      shut open stays  stays shuts  stays-shut opens
  assert.deepEqual(gateOver([0.5, 1.1, 0.9, 0.85, 0.7, 0.95, 1.05], GATE), [0, 1, 1, 1, 0, 0, 1]);
  // Without hysteresis it flickers with the signal round its threshold.
  assert.deepEqual(gateOver([0.5, 1.1, 0.95, 1.05, 0.99], { ...GATE, hysteresis: 0 }), [0, 1, 0, 1, 0]);
});

test('the gate lifts its threshold after each opening and lets it down over Recover', () => {
  const knobs = { ...GATE, lift: 0.5, recover: 0.5 };
  const xs = [1.2, 0, 1.2, ...Array(120).fill(0), 1.2];
  const out = gateOver(xs, knobs);
  assert.equal(out[0], 1);
  assert.equal(out[2], 0, 'just after a beat, the same height does not open it again');
  assert.equal(out.at(-1), 1, 'two seconds later it does');
});

test('the gate falls over its release once shut', () => {
  const out = gateOver([2, 0, ...Array(59).fill(0)], { ...GATE, release: 0.25 });
  assert.equal(out[0], 1);
  assert.ok(out[1] < 1 && out[1] > 0.9);
  assert.ok(Math.abs(out[60] - Math.exp(-1 / 0.25)) < 1e-9, `a second on: ${out[60]}`);
});

/** A counter's output after each of `n` beats (a frame up, a frame down). */
function counted(type: string, n: number, knobs: Record<string, number>) {
  const m = MODULATORS[type].create();
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    out.push(m.step(1 / 60, () => 2, knobs));
    m.step(1 / 60, () => 2, knobs);
    m.step(1 / 60, () => 0, knobs);
  }
  return out;
}

test('the counters count each beat once and wrap', () => {
  assert.deepEqual(counted('beats', 6, { length: 4, amount: 1 }), [0.25, 0.5, 0.75, 0, 0.25, 0.5]);
  assert.deepEqual(counted('bars', 5, { length: 2, amount: 1 }), [0.5, 0, 0.5, 0, 0.5]);
  for (const length of [3, 7, 16]) {
    const out = counted('beats', length * 5, { length, amount: 1 });
    for (let i = 0; i < out.length; i++) assert.ok(Math.abs(out[i] - ((i + 1) % length) / length) < 1e-12, `${length}: ${i}`);
  }
  assert.deepEqual(counted('beats', 4, { length: 1, amount: 1 }), [0, 0, 0, 0]);
});

test('the counters read their own signals', () => {
  assert.equal(MODULATORS.beats.input?.signal, 'beat');
  assert.equal(MODULATORS.bars.input?.signal, 'downbeat');
});

test('sample and hold keeps what it took until the next sample', () => {
  const m = MODULATORS.hold.create();
  const knobs = { rate: 4, glide: 0, full: 1, amount: 1 };
  const xs: number[] = [];
  let x = 0;
  for (let i = 1; i <= 60; i++) {
    x = i / 100;
    xs.push(m.step(1 / 60, () => x, knobs));
  }
  assert.equal(xs[0], 0, 'nothing taken yet');
  assert.equal(xs[14], 0.15, 'taken at a quarter second');
  assert.equal(xs[20], 0.15, 'held');
  assert.equal(xs[29], 0.3);
});

test('every knob is continuous, with its neutral and default in its range', () => {
  for (const t of TYPES) {
    assert.equal(t.place, 'modulator');
    for (const k of t.knobs) {
      assert.notEqual(k.kind, 'step', `${t.type}.${k.id}`);
      assert.ok(k.min < k.max && k.neutral >= k.min && k.neutral <= k.max && k.default >= k.min && k.default <= k.max, `${t.type}.${k.id}`);
    }
  }
});
