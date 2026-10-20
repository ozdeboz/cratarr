// The modulators (modulators.ts, decision 0056): 0 at neutral, within their range, the same at any frame rate, the
// gate's hysteresis and lift, the counter's wrap, chaining through their ports.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BANDS, MODULATORS, defaultsOf, neutralOf } from '../src/lib/visuals/modulators/modulators.ts';

const TYPES = Object.values(MODULATORS);
const none = () => 0;

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
/** Values that jump about: mostly round typical, sometimes far above or below, now and then nothing or not a number. */
const wild = (rnd: () => number) => {
  const r = rnd();
  return r < 0.05 ? NaN : r < 0.1 ? 0 : r < 0.15 ? 50 * rnd() : r < 0.2 ? -2 * rnd() : 3 * rnd();
};

test('every modulator gives 0 with its knobs at neutral, whatever comes in', () => {
  // The Envelope Follower is the Loudness too: at Intensity 0 it holds at 1 (the loudness not counting), as it must.
  for (const t of TYPES.filter((x) => x.type !== 'envelope')) {
    const rnd = randomFrom(1);
    const m = t.create(7);
    const k = neutralOf(t);
    for (let i = 0; i < 2000; i++) {
      const v = m.step(rnd() * 0.1, () => wild(rnd), { ...k, in: wild(rnd) });
      assert.equal(Math.abs(v), 0, `${t.type} at frame ${i}: ${v}`);
    }
  }
});

test('every modulator stays within its range, whatever its knobs and inputs', () => {
  for (const t of TYPES) {
    const [lo, hi] = t.range === 'bipolar' ? [-1, 1] : [0, 1];
    const rnd = randomFrom(2);
    const m = t.create(3);
    let k = defaultsOf(t);
    for (let i = 0; i < 5000; i++) {
      if (i % 50 === 0) k = Object.fromEntries(t.knobs.map((n) => [n.id, n.min + (n.max - n.min) * (rnd() * 1.4 - 0.2)]));
      const v = m.step(rnd() < 0.01 ? -1 : rnd() * 0.2, () => wild(rnd), { ...k, in: wild(rnd) });
      assert.ok(v >= lo && v <= hi, `${t.type} at frame ${i}: ${v}`);
    }
  }
});

test('with nothing cabled in, a modulator reads its input as 0', () => {
  for (const t of TYPES.filter((t) => t.inputs)) {
    const a = t.create();
    const b = t.create();
    const k = defaultsOf(t);
    for (let i = 0; i < 120; i++) assert.equal(a.step(1 / 60, none, k), b.step(1 / 60, none, { ...k, in: 0 }), t.type);
  }
  assert.deepEqual(MODULATORS.fast.inputs, ['in']);
  assert.equal(MODULATORS.volume.inputs, undefined, 'volume reads the bands itself');
});

test('the followers and the other smooth ones give the same at 30 and at 120 frames a second', () => {
  // What comes in changes every 30th of a second, each band its own way.
  const level = (seg: number, band: number) => {
    const r = randomFrom(seg * 31 + band)();
    return seg % 40 < 20 ? 2.5 * r : 0.4 * r;
  };
  const run = (type: string, fps: number, knobs: Record<string, number>) => {
    const m = MODULATORS[type].create(5);
    const out: number[] = [];
    for (let i = 1; i <= 3 * fps; i++) {
      const seg = Math.ceil((i * 30) / fps);
      const v = m.step(1 / fps, (ref) => level(seg, ref.band ?? 0), { ...knobs, in: level(seg, 0) });
      if ((i * 30) % fps === 0) out.push(v);
    }
    return out;
  };
  for (const type of ['volume', 'fast', 'envelope', 'average', 'peak', 'mean', 'lfo', 'random']) {
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

/** A gate's output for each input in turn, a 60th of a second apart. */
function gateOver(xs: number[], knobs: Record<string, number>) {
  const m = MODULATORS.gate.create();
  return xs.map((x) => m.step(1 / 60, none, { ...knobs, in: x }));
}
const GATE = { above: 1, hysteresis: 0.2, lift: 0, recover: 0, release: 0, amount: 1 };

test('the gate opens above typical and, by its hysteresis, closes only well below it', () => {
  //                      shut open stays  stays shuts  stays-shut opens
  assert.deepEqual(gateOver([0.5, 1.1, 0.9, 0.85, 0.7, 0.95, 1.05], GATE), [0, 1, 1, 1, 0, 0, 1]);
  // Without hysteresis it flickers with its input round its threshold.
  assert.deepEqual(gateOver([0.5, 1.1, 0.95, 1.05, 0.99], { ...GATE, hysteresis: 0 }), [0, 1, 0, 1, 0]);
});

test('the gate lifts its threshold after each opening and lets it down over Recover', () => {
  const out = gateOver([1.2, 0, 1.2, ...Array(120).fill(0), 1.2], { ...GATE, lift: 0.5, recover: 0.5 });
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

/** The counter's output after each of `n` pulses of height `up` (a frame up, a frame down). */
function counted(n: number, knobs: Record<string, number>, up = 1) {
  const m = MODULATORS.counter.create();
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    out.push(m.step(1 / 60, none, { ...knobs, in: up }));
    m.step(1 / 60, none, { ...knobs, in: up });
    m.step(1 / 60, none, { ...knobs, in: 0 });
  }
  return out;
}

test('the counter counts each rise above ½ once, and wraps', () => {
  assert.deepEqual(counted(6, { length: 4, amount: 1 }), [0.25, 0.5, 0.75, 0, 0.25, 0.5]);
  assert.deepEqual(counted(5, { length: 2, amount: 1 }, 0.6), [0.5, 0, 0.5, 0, 0.5]);
  assert.deepEqual(counted(3, { length: 4, amount: 1 }, 0.45), [0, 0, 0], 'not up to ½: no count');
  for (const length of [3, 7, 16]) {
    const out = counted(length * 5, { length, amount: 1 });
    for (let i = 0; i < out.length; i++) assert.ok(Math.abs(out[i] - ((i + 1) % length) / length) < 1e-12, `${length}: ${i}`);
  }
  assert.deepEqual(counted(4, { length: 1, amount: 1 }), [0, 0, 0, 0]);
});

test('modulators chain: a follower into a gate into a counter', () => {
  const follow = MODULATORS.fast.create();
  const gate = MODULATORS.gate.create();
  const count = MODULATORS.counter.create();
  const fk = { ...defaultsOf(MODULATORS.fast), full: 1 };
  const gk = { ...GATE, above: 0.8 };
  const ck = { length: 4, amount: 1 };
  let c = 0;
  // Five beats of a second each: up for a tenth, then quiet.
  for (let i = 0; i < 5 * 60; i++) {
    const f = follow.step(1 / 60, none, { ...fk, in: i % 60 < 6 ? 1.5 : 0.2 });
    const g = gate.step(1 / 60, none, { ...gk, in: f });
    c = count.step(1 / 60, none, { ...ck, in: g });
  }
  assert.equal(c, 0.25, 'five beats on a count of four');
});

test('sample and hold keeps what it took until the next sample', () => {
  const m = MODULATORS.hold.create();
  const knobs = { rate: 4, glide: 0, full: 1, amount: 1 };
  const xs: number[] = [];
  for (let i = 1; i <= 60; i++) xs.push(m.step(1 / 60, none, { ...knobs, in: i / 100 }));
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

test("random: Shape 0 glides the whole step (Drift's), 1 jumps and holds, between glides then holds; Drift's name still loads", async () => {
  const { modulatorOf } = await import('../src/lib/visuals/modulators/modulators.ts');
  assert.equal(modulatorOf('drift'), MODULATORS.random);
  const run = (shape: number) => {
    const r = MODULATORS.random.create(7);
    return Array.from({ length: 30 * 6 }, () => r.step(1 / 30, none as never, { rate: 1, shape, amount: 1 }));
  };
  const smooth = run(0);
  const stepped = run(1);
  const half = run(0.5);
  for (let k = 1; k < smooth.length; k++) assert.ok(Math.abs(smooth[k] - smooth[k - 1]) < 0.2, `glides: frame ${k}`);
  for (let step = 0; step < 5; step++) {
    // Frames 30·step+1 … 30·step+29 lie inside one step (the 30th is the next's start, give or take a sum's last bit).
    const inside = (a: number[]) => a.slice(30 * step + 1, 30 * step + 29);
    assert.ok(inside(stepped).every((v) => v === inside(stepped)[0]), `stepped holds through step ${step}`);
    // Where the glide ends, at the next point: the smooth one arrives there at the step's end.
    assert.ok(Math.abs(stepped[30 * step + 1] - smooth[30 * step + 29]) < 0.05, `the same next point, step ${step}`);
    assert.ok(half.slice(30 * step + 16, 30 * step + 29).every((v) => Math.abs(v - stepped[30 * step + 1]) < 1e-12), `half: holds the second half of step ${step}`);
  }
});
