// Quality (decision 0050): one ceiling on the pace's own ladder, 60 % exactly the defaults before 65.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { autoResolution, ceilingOf, QUALITY } from '../src/lib/visuals/quality.ts';

const PHONE = 1080 * 2400;
const close = (a: number, b: number) => Math.abs(a - b) < 1e-9;

test('at 60 % the defaults before 65: 2^19 points, 3 steps, the screen’s default resolution', () => {
  const c = ceilingOf(QUALITY, PHONE);
  assert.equal(c.points, 19);
  assert.equal(c.steps, 3);
  assert.ok(close(c.resolution, autoResolution(PHONE)));
});

test('at its ends: 64k points, 1 step, 60 % of the default; 2M points, 6 steps, all of the screen', () => {
  const lo = ceilingOf(0, PHONE);
  assert.deepEqual([lo.points, lo.steps], [16, 1]);
  assert.ok(close(lo.resolution, 0.6 * autoResolution(PHONE)));
  const hi = ceilingOf(1, PHONE);
  assert.deepEqual([hi.points, hi.steps, hi.resolution], [21, 6, 1]);
});

test('every step up costs more, never less', () => {
  let was = ceilingOf(0, PHONE);
  for (let q = 0.05; q <= 1.0001; q += 0.05) {
    const c = ceilingOf(q, PHONE);
    assert.ok(c.points >= was.points && c.steps >= was.steps && c.resolution >= was.resolution - 1e-12, `${q}`);
    was = c;
  }
});
