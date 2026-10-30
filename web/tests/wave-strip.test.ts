// A hold's waveform as one strip (lib/waveStrip): where a place of the source is drawn as the stretch shown slides and
// scales over the strip as drawn; when the strip is drawn anew, and that drawn anew it shows no step.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { coverOf, serves, placeOf, gridBars, stepOf, STRIP_SPAN, STRIP_SCALE } = await import('../src/lib/waveStrip.ts');

const COUNT = 3 * 300;

/** Where a place `p` of the source lands on the waveform (px): the strip is STRIP_SPAN waveforms wide, its left at e0. */
function screenX(d: { e0: number; e1: number; w0: number }, p: number, from: number, to: number, width: number) {
  const { x, k } = placeOf(d as never, from, to, width);
  const local = ((p - d.e0) / (d.e1 - d.e0)) * STRIP_SPAN * width;
  return k * local + x;
}

test('the stretch shown fills the waveform, edge to edge, as it slides and scales', () => {
  const d = coverOf(0.4, 0.5, COUNT);
  const W = 1000;
  for (const [from, to] of [
    [0.4, 0.5],
    [0.43, 0.53],
    [0.36, 0.44],
    [0.45, 0.57],
  ]) {
    assert.ok(Math.abs(screenX(d, from, from, to, W)) < 1e-6, `${from}..${to}: its start at the left`);
    assert.ok(Math.abs(screenX(d, to, from, to, W) - W) < 1e-6, `${from}..${to}: its end at the right`);
    const mid = (from + to) / 2;
    assert.ok(Math.abs(screenX(d, mid, from, to, W) - W / 2) < 1e-6, `${from}..${to}: its middle in the middle`);
  }
});

test('it covers the stretch drawn for, with a step of the ladder on either side, from a bar of its grid', () => {
  const d = coverOf(0.4, 0.5, COUNT);
  assert.equal(d.w0, stepOf(0.1));
  assert.ok(d.w0 / 0.1 <= Math.sqrt(STRIP_SCALE) + 1e-9 && 0.1 / d.w0 <= Math.sqrt(STRIP_SCALE) + 1e-9, 'the nearest step');
  assert.ok(d.e0 <= 0.4 - d.w0 && d.e0 > 0.4 - d.w0 - d.pitch);
  assert.ok(Math.abs(d.e1 - d.e0 - STRIP_SPAN * d.w0) < 1e-12);
  assert.ok(serves(d, 0.4, 0.5));
});

test('drawn anew only once the stretch leaves what it covers or its scale is a quarter off', () => {
  const d = coverOf(0.4, 0.5, COUNT);
  const w = d.w0;
  assert.ok(serves(d, d.e0, d.e0 + w), 'at its left end');
  assert.ok(serves(d, d.e1 - w, d.e1), 'at its right end');
  assert.ok(!serves(d, d.e1 - w + 0.01, d.e1 + 0.01), 'past its right end');
  assert.ok(!serves(d, d.e0 - 0.01, d.e0 + w - 0.01), 'past its left end');
  assert.ok(serves(d, 0.4, 0.4 + w * STRIP_SCALE * 0.999), 'nearly a quarter wider');
  assert.ok(!serves(d, 0.4, 0.4 + w * STRIP_SCALE * 1.01), 'more than a quarter wider');
  assert.ok(!serves(d, 0.4, 0.4 + (w / STRIP_SCALE) * 0.99), 'more than a quarter narrower');
});

/** A track's bins: a made-up loudness, uneven enough to show any shift. */
const BINS = Uint8Array.from({ length: 2000 }, (_, i) => Math.round(128 + 100 * Math.sin(i * 0.37) * Math.cos(i * 0.011)));

test('drawn anew as the stretch slides on, the bars where both strips have them are the same', () => {
  for (const contrast of [0, 1]) {
    for (const w of [0.02, 0.1, 0.3]) {
      const a = coverOf(0.3, 0.3 + w, COUNT);
      const b = coverOf(0.3 + w * 0.8, 0.3 + w * 1.8, COUNT);
      assert.equal(a.pitch, b.pitch, 'one grid');
      const ha = gridBars(BINS, a, COUNT, contrast, COUNT / 6);
      const hb = gridBars(BINS, b, COUNT, contrast, COUNT / 6);
      const shift = b.g0 - a.g0;
      assert.ok(shift > 0 && shift < COUNT);
      for (let i = shift; i < COUNT; i++) assert.equal(hb[i - shift], ha[i], `w ${w}, contrast ${contrast}: bar ${i}`);
    }
  }
});

test('no bars beyond the source', () => {
  const d = coverOf(-0.05, 0.05, COUNT);
  const h = gridBars(BINS, d, COUNT);
  for (let i = 0; i < COUNT; i++) {
    const at = d.e0 + (i + 1) * d.pitch;
    if (at <= 0) assert.equal(h[i], 0, `bar ${i}`);
  }
  const e = coverOf(0.95, 1.05, COUNT);
  const he = gridBars(BINS, e, COUNT);
  for (let i = 0; i < COUNT; i++) if (e.e0 + i * e.pitch >= 1) assert.equal(he[i], 0, `bar ${i}`);
});
