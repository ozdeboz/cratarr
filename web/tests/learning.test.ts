// What the governor learns from the thumbs (learning.ts): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { drifted, fresh, limitsOf, thumbed, LEAST_STEP } from '../src/lib/visuals/learning.ts';

const base = { motion: 0.9, shapeChurn: 0.7, colourChurn: 0.4, zoomPumping: 0.4, looksPerMinute: 4 };
const calm = { relative: { motion: 0.2, shapeChurn: 0.1, colourChurn: 0.02, zoomPumping: 0.05, looksPerMinute: 0 } };

test('down with nothing standing out changes nothing', () => {
  const r = thumbed(fresh(base), base, 'down', calm, null);
  assert.equal(r.changed, null);
  assert.deepEqual(limitsOf(r.learned), base);
});

test('down tightens the one that stands out most, under what it was, at once', () => {
  const w = { relative: { ...calm.relative, motion: 0.8, colourChurn: 0.06 } };
  const r = thumbed(fresh(base), base, 'down', w, null);
  assert.equal(r.changed?.meter, 'motion');
  assert.ok(Math.abs(r.changed!.to - 0.8 * 0.9) < 1e-9, `to ${r.changed!.to}`);
  assert.equal(r.learned.colourChurn.value, 0.4);
});

test('stands out only above the defaults typical, in the governor terms', () => {
  assert.equal(thumbed(fresh(base), base, 'down', { relative: { ...calm.relative, motion: 0.4 } }, null).changed?.meter, 'motion');
  assert.equal(thumbed(fresh(base), base, 'down', { relative: { ...calm.relative, motion: 0.25 } }, null).changed, null);
});

test('a kind that ranges widely does not outweigh one far past its own usual range', () => {
  // The lab's case: Speed 2 and strong turning (motion past the defaults' most), colour churn high but within its range.
  const r = thumbed(fresh(base), base, 'down', { relative: { ...calm.relative, motion: 0.542, colourChurn: 0.25 } }, null);
  assert.equal(r.changed?.meter, 'motion');
});

test('up loosens only the held limit; with nothing held, nothing', () => {
  assert.equal(thumbed(fresh(base), base, 'up', calm, null).changed, null);
  const r = thumbed(fresh(base), base, 'up', calm, 'motion');
  assert.ok(Math.abs(r.learned.motion.value - 0.99) < 1e-9);
});

test('a reversal halves the step, down to the least', () => {
  let l = fresh(base);
  const w = { relative: { ...calm.relative, motion: 0.9 } };
  l = thumbed(l, base, 'down', w, null).learned;
  l = thumbed(l, base, 'up', calm, 'motion').learned;
  assert.equal(l.motion.step, 0.05);
  for (let k = 0; k < 6; k++) l = thumbed(l, base, k % 2 ? 'up' : 'down', w, k % 2 ? 'motion' : null).learned;
  assert.equal(l.motion.step, LEAST_STEP);
});

test('never under half nor over twice where it started', () => {
  let l = fresh(base);
  for (let k = 0; k < 40; k++) l = thumbed(l, base, 'down', { relative: { ...calm.relative, motion: 5 } }, null).learned;
  assert.ok(Math.abs(l.motion.value - 0.45) < 1e-9, `low ${l.motion.value}`);
  l = fresh(base);
  for (let k = 0; k < 40; k++) l = thumbed(l, base, 'up', calm, 'motion').learned;
  assert.ok(Math.abs(l.motion.value - 1.8) < 1e-9, `high ${l.motion.value}`);
});

test('drifts half way back in thirty days', () => {
  const l = { ...fresh(base), motion: { value: 0.45, step: 0.1, dir: -1 } };
  const d = drifted(l, base, 30).motion.value;
  assert.ok(Math.abs(d - 0.9 * Math.sqrt(0.5)) < 1e-9, `${d}`);
  assert.equal(drifted(l, base, 0).motion.value, 0.45);
});
