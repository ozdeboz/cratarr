// Where the signals have been lately, for the curve editor (history.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BINS, densityOf, HISTORY, newHistory, remember } from '../src/lib/visuals/history.ts';

test('a signal’s density peaks where it has been, the most 1, and nothing is known before anything is remembered', () => {
  const h = newHistory(2, 3, 6);
  assert.equal(densityOf(h, { track: 0 }), null);
  const bands = Array.from({ length: 6 }, (_, b) => [0.1 * b, 0.9, 0]);
  for (let i = 0; i < 50; i++) remember(h, [0.25, 1], bands);
  const d = densityOf(h, { track: 0 })!;
  const top = d.indexOf(Math.max(...d));
  assert.equal(top, Math.floor(0.25 * BINS));
  assert.equal(d[top], 1);
  assert.ok(d[top + 1] > 0 && d[top + 3] === 0, 'smoothed two bins either way, no further');
  // A full signal falls in the last bin.
  assert.equal(densityOf(h, { track: 1 })![BINS - 1], 1);
  // One band's, and all six's.
  assert.equal(densityOf(h, { signal: 0, band: 3 })!.indexOf(1), Math.floor(0.3 * BINS));
  const all = densityOf(h, { signal: 0, band: null })!;
  for (let b = 0; b < 6; b++) assert.ok(all[Math.floor(0.1 * b * BINS)] > 0);
});

test('only the last half minute is kept', () => {
  const h = newHistory(1, 1, 1);
  for (let i = 0; i < HISTORY; i++) remember(h, [0.1], [[0]]);
  for (let i = 0; i < HISTORY; i++) remember(h, [0.8], [[0]]);
  const d = densityOf(h, { track: 0 })!;
  assert.equal(d[Math.floor(0.1 * BINS)], 0);
  assert.equal(d[Math.floor(0.8 * BINS)], 1);
});
