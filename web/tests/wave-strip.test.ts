// A hold's waveform as one strip (lib/waveStrip): where a place of the source is drawn as the stretch shown slides and
// scales over the strip as drawn; when the strip is drawn anew.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { coverOf, serves, placeOf, STRIP_SPAN, STRIP_SCALE } = await import('../src/lib/waveStrip.ts');

/** Where a place `p` of the source lands on the waveform (px): the strip is STRIP_SPAN waveforms wide, its left at e0. */
function screenX(d: { e0: number; e1: number; w0: number }, p: number, from: number, to: number, width: number) {
  const { x, k } = placeOf(d, from, to, width);
  const local = ((p - d.e0) / (d.e1 - d.e0)) * STRIP_SPAN * width;
  return k * local + x;
}

test('the stretch shown fills the waveform, edge to edge, as it slides and scales', () => {
  const d = coverOf(0.4, 0.5);
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

test('drawn anew only once the stretch leaves what it covers or its scale is a quarter off', () => {
  const d = coverOf(0.4, 0.5);
  // It covers 0.3..0.6.
  assert.ok(serves(d, 0.4, 0.5));
  assert.ok(serves(d, 0.5, 0.6), 'slid on by a whole stretch, to its right end');
  assert.ok(!serves(d, 0.52, 0.62), 'past its right end');
  assert.ok(!serves(d, 0.29, 0.39), 'past its left end');
  assert.ok(serves(d, 0.4, 0.4 + 0.1 * STRIP_SCALE * 0.999), 'nearly a quarter wider');
  assert.ok(!serves(d, 0.4, 0.4 + 0.1 * STRIP_SCALE * 1.01), 'more than a quarter wider');
  assert.ok(!serves(d, 0.4, 0.4 + (0.1 / STRIP_SCALE) * 0.99), 'more than a quarter narrower');
});
