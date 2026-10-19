// The sidechain compressor (engine/compressor.ts; Giannoulis, Massberg, Reiss 2012): its static curve, its knee, its
// ballistics as time constants, a limiter's ceiling, the same at any frame rate.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gainComputer, newCompressor, stepCompressor, type CompressorKnobs } from '../src/lib/visuals/engine/compressor.ts';

const near = (a: number, b: number, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} is not near ${b}`);
const K = (k: Partial<CompressorKnobs> = {}): CompressorKnobs => ({ threshold: 1, ratio: 4, knee: 0, attack: 0, release: 0, makeup: 0, ...k });

test('the static curve: as it is under the threshold, 1/ratio of what is over above it; a limiter flat', () => {
  near(gainComputer(-10, 0, 4, 0), -10);
  near(gainComputer(8, 0, 4, 0), 2);
  near(gainComputer(8, 0, Infinity, 0), 0);
  near(gainComputer(8, 0, 1, 0), 8);
});

test('the knee: continuous at both its ends, and through its middle', () => {
  for (const [r, w] of [[4, 6], [Infinity, 10], [2, 1]] as const) {
    near(gainComputer(-w / 2, 0, r, w), -w / 2, 1e-12);
    near(gainComputer(w / 2, 0, r, w), r === Infinity ? 0 : w / 2 / r, 1e-12);
    assert.ok(gainComputer(0, 0, r, w) < 0, 'below the line at the threshold');
  }
});

test('instant ballistics: the key 12 dB over at ratio 4 holds the input back 9 dB', () => {
  const st = newCompressor();
  const r = stepCompressor(st, 1, 10 ** (12 / 20), K(), 1 / 60);
  near(r.reduction, 9);
  near(r.out, 10 ** (-9 / 20));
  near(stepCompressor(newCompressor(), 2, 0.5, K(), 1 / 60).out, 2, 1e-12);
});

test('attack and release as time constants: 63 % after one, at any frame rate', () => {
  for (const fps of [30, 120]) {
    const st = newCompressor();
    const k = K({ attack: 0.5, release: 2 });
    let r = { reduction: 0 };
    for (let i = 0; i < Math.round(0.5 * fps); i++) r = stepCompressor(st, 1, 10 ** (12 / 20), k, 1 / fps);
    near(r.reduction / 9, 1 - Math.exp(-1), 0.03);
    // The release: the detector's peak (9 dB asked) falls to 1/e of it in one time constant; the attack smooths after.
    for (let i = 0; i < Math.round(2 * fps); i++) r = stepCompressor(st, 1, 0.01, k, 1 / fps);
    near(st.peak / 9, Math.exp(-1), 0.03);
    assert.ok(r.reduction > st.peak, 'the output follows the peak down a little behind it');
  }
});

test("a limiter keyed by its own output's measure keeps it at the ceiling; makeup adds", () => {
  const st = newCompressor();
  let out = 0;
  for (let i = 0; i < 10; i++) out = stepCompressor(st, 4, 4, K({ threshold: 1, ratio: Infinity }), 1 / 60).out;
  near(out, 1, 1e-9);
  near(stepCompressor(newCompressor(), 1, 0.1, K({ makeup: 6 }), 1 / 60).gain, 10 ** (6 / 20));
});

test('nothing and not-a-number are the floor, never more than none held back', () => {
  for (const key of [0, Number.NaN, -3, Number.POSITIVE_INFINITY]) {
    const r = stepCompressor(newCompressor(), 1, key, K(), 1 / 60);
    assert.ok(Number.isFinite(r.out) && r.reduction >= 0, `${key}: ${JSON.stringify(r)}`);
  }
});
